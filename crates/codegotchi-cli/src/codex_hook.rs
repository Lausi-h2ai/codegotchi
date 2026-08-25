use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use chrono::Utc;
use codegotchi_domain::{ActivityKind, AgentEvent, AgentEventKind, EventMetadata, EventSource};
use thiserror::Error;
use uuid::Uuid;

use crate::classify::{
    activity_for_command, classify_command, command_category_name, executable_name,
};
use crate::protocol::{
    DebugRequest, EventIngestRequest, EventIngestResponse, HookInput, HookOutput, ModeRequest,
    PermissionContext, RuntimeMetadataV1, SnapshotMutationResponse,
};
use crate::runtime_metadata::read_metadata;

pub const CODEGOTCHI_SESSION_FILE: &str = "CODEGOTCHI_SESSION_FILE";
pub const EVENT_INGEST_PATH: &str = "/api/v1/events";
pub const MAX_HOOK_INPUT_BYTES: usize = 1024 * 1024;
const HOOK_IO_TIMEOUT: Duration = Duration::from_millis(250);
const HOOK_TOTAL_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum HookTransportError {
    #[error("invalid loopback URL")]
    InvalidUrl,
    #[error("loopback URL is not local")]
    NonLoopback,
    #[error("HTTP transport failed: {0}")]
    Io(#[source] io::Error),
    #[error("HTTP response was not successful")]
    Status,
    #[error("HTTP response was invalid")]
    Response,
}

/// Translates one accepted Codex event into the domain event boundary.
pub fn translate_hook(input: &HookInput, metadata: &RuntimeMetadataV1) -> Option<AgentEvent> {
    let event_name = input.hook_event_name.as_str();
    let session_id = input.parsed_session_id().unwrap_or(metadata.runtime_id);
    let (kind, activity, event_metadata) = match event_name {
        "SessionStart" => (
            AgentEventKind::SessionStarted,
            Some(ActivityKind::Idle),
            EventMetadata::default(),
        ),
        "SessionEnd" => (
            AgentEventKind::SessionEnded,
            Some(ActivityKind::Idle),
            EventMetadata::default(),
        ),
        "UserPromptSubmit" => (
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Thinking),
            EventMetadata::default(),
        ),
        "Stop" => (
            AgentEventKind::TurnCompleted,
            Some(ActivityKind::Waiting),
            EventMetadata::default(),
        ),
        "PreToolUse" | "PostToolUse" => {
            let tool_name = input.tool_name.as_deref().unwrap_or_default();
            let command = input.command().unwrap_or_default();
            let exit_status = input.exit_status();
            let known_tool = tool_name.eq_ignore_ascii_case("bash")
                || tool_name.eq_ignore_ascii_case("apply_patch");
            let classification = classify_command(tool_name, command);
            let activity = if known_tool {
                activity_for_command(tool_name, command, exit_status)
            } else {
                ActivityKind::UnknownWork
            };
            let event_metadata = EventMetadata::new(
                executable_name(tool_name, command),
                Some(command_category_name(classification.category()).to_owned()),
                exit_status,
                input.duration_ms(),
                false,
            );
            (
                if event_name == "PreToolUse" {
                    AgentEventKind::ToolStarted
                } else {
                    AgentEventKind::ToolCompleted
                },
                Some(activity),
                event_metadata,
            )
        }
        _ => return None,
    };

    let id = deterministic_event_id(
        metadata,
        session_id,
        input.stable_event_identity(),
        event_name,
        kind,
        activity,
        &event_metadata,
    );
    Some(AgentEvent::new(
        id,
        session_id,
        metadata.repository_root.to_string_lossy(),
        EventSource::Codex,
        kind,
        activity,
        Utc::now(),
        event_metadata,
    ))
}

pub fn translate_hook_json(payload: &[u8], metadata: &RuntimeMetadataV1) -> Option<AgentEvent> {
    HookInput::from_json(payload)
        .ok()
        .and_then(|input| translate_hook(&input, metadata))
}

/// Produces the minimum structured policy context for a PreToolUse request.
/// Raw hook values are consumed only inside this adapter.
pub fn permission_context_for_hook(input: &HookInput) -> Option<PermissionContext> {
    if input.hook_event_name != "PreToolUse" {
        return None;
    }
    let tool_name = input.tool_name.as_deref().unwrap_or_default();
    let command = input.command().unwrap_or_default();
    Some(PermissionContext::from_classification(classify_command(
        tool_name, command,
    )))
}

pub fn hook_output_for_payload(payload: &[u8], metadata: &RuntimeMetadataV1) -> HookOutput {
    if payload.len() > MAX_HOOK_INPUT_BYTES {
        return HookOutput::allow();
    }
    translate_hook_json(payload, metadata)
        .map(|_| HookOutput::allow())
        .unwrap_or_else(HookOutput::allow)
}

pub fn run_hook_from_environment() -> HookOutput {
    let payload = match read_bounded_stdin() {
        Ok(payload) => payload,
        Err(_) => return HookOutput::allow(),
    };
    if payload.len() > MAX_HOOK_INPUT_BYTES {
        return HookOutput::allow();
    }

    let metadata_path = match std::env::var_os(CODEGOTCHI_SESSION_FILE) {
        Some(path) => path,
        None => return HookOutput::allow(),
    };
    let metadata = match read_metadata(std::path::Path::new(&metadata_path)) {
        Ok(metadata) => metadata,
        Err(_) => return HookOutput::allow(),
    };
    if !runtime_metadata_is_active(&metadata) {
        return HookOutput::allow();
    }
    let input = match HookInput::from_json(&payload) {
        Ok(input) => input,
        Err(_) => return HookOutput::allow(),
    };
    let event = match translate_hook(&input, &metadata) {
        Some(event) => event,
        None => return HookOutput::allow(),
    };
    let request = match permission_context_for_hook(&input) {
        Some(permission) => EventIngestRequest {
            event,
            permission: Some(permission),
        },
        None => EventIngestRequest::new(event),
    };
    let response = match send_event_to_runtime(&metadata, &request) {
        Ok(response) => response,
        Err(_) => return HookOutput::allow(),
    };

    if input.hook_event_name == "PreToolUse" && response.is_strict_denial() {
        return HookOutput::deny(
            response
                .denial_reason()
                .unwrap_or("Safe development work is blocked until care is restored."),
        );
    }
    HookOutput::allow()
}

fn read_bounded_stdin() -> Result<Vec<u8>, io::Error> {
    let mut input = io::stdin().lock();
    let mut payload = Vec::with_capacity(MAX_HOOK_INPUT_BYTES.min(4096));
    input
        .by_ref()
        .take((MAX_HOOK_INPUT_BYTES as u64).saturating_add(1))
        .read_to_end(&mut payload)?;
    Ok(payload)
}

/// Returns false for a missing/stale owner process so hooks never contact an
/// unrelated service using an abandoned metadata file.
pub fn runtime_metadata_is_active(metadata: &RuntimeMetadataV1) -> bool {
    if metadata.owning_pid == 0 || metadata.bearer_token.is_empty() {
        return false;
    }
    if metadata.owning_pid == std::process::id() {
        return true;
    }
    process_exists(metadata.owning_pid)
}

/// Portable Unix liveness check. Signal zero performs permission and process
/// existence checks without delivering a signal, including when the hook runs
/// as a child process on macOS.
fn process_exists(pid: u32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

fn deterministic_event_id(
    metadata: &RuntimeMetadataV1,
    session_id: Uuid,
    event_identity: Option<&str>,
    event_name: &str,
    kind: AgentEventKind,
    activity: Option<ActivityKind>,
    event_metadata: &EventMetadata,
) -> Uuid {
    let canonical = if let Some(event_identity) = event_identity {
        format!(
            "codegotchi-hook-v3-stable|{}|{}|{}|{}|{}",
            metadata.runtime_id,
            session_id,
            event_name,
            kind_name(kind),
            event_identity,
        )
    } else {
        format!(
            "codegotchi-hook-v2|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            metadata.runtime_id,
            session_id,
            "",
            event_name,
            kind_name(kind),
            activity_name(activity),
            event_metadata
                .executable_name
                .as_deref()
                .unwrap_or_default(),
            event_metadata
                .command_category
                .as_deref()
                .unwrap_or_default(),
            event_metadata
                .exit_status
                .map_or_else(String::new, |value| value.to_string()),
            event_metadata
                .duration_ms
                .map_or_else(String::new, |value| value.to_string()),
            event_metadata.blocked,
            metadata.repository_root.to_string_lossy(),
            metadata.loopback_base_url,
        )
    };
    Uuid::new_v5(&Uuid::NAMESPACE_URL, canonical.as_bytes())
}

fn kind_name(kind: AgentEventKind) -> &'static str {
    match kind {
        AgentEventKind::SessionStarted => "session_started",
        AgentEventKind::SessionEnded => "session_ended",
        AgentEventKind::TurnStarted => "turn_started",
        AgentEventKind::TurnCompleted => "turn_completed",
        AgentEventKind::WaitingForUser => "waiting_for_user",
        AgentEventKind::OutputActivity => "output_activity",
        AgentEventKind::ToolStarted => "tool_started",
        AgentEventKind::ToolCompleted => "tool_completed",
        AgentEventKind::CommandStarted => "command_started",
        AgentEventKind::CommandCompleted => "command_completed",
        AgentEventKind::Interrupted => "interrupted",
        AgentEventKind::IntegrationError => "integration_error",
    }
}

fn activity_name(activity: Option<ActivityKind>) -> &'static str {
    match activity {
        None => "none",
        Some(ActivityKind::Idle) => "idle",
        Some(ActivityKind::Thinking) => "thinking",
        Some(ActivityKind::Reading) => "reading",
        Some(ActivityKind::Searching) => "searching",
        Some(ActivityKind::Editing) => "editing",
        Some(ActivityKind::Testing) => "testing",
        Some(ActivityKind::Building) => "building",
        Some(ActivityKind::Installing) => "installing",
        Some(ActivityKind::GitOperation) => "git_operation",
        Some(ActivityKind::DockerOperation) => "docker_operation",
        Some(ActivityKind::WebResearch) => "web_research",
        Some(ActivityKind::Waiting) => "waiting",
        Some(ActivityKind::Celebrating) => "celebrating",
        Some(ActivityKind::Error) => "error",
        Some(ActivityKind::Blocked) => "blocked",
        Some(ActivityKind::UnknownWork) => "unknown_work",
    }
}

pub fn send_event_to_runtime(
    metadata: &RuntimeMetadataV1,
    request: &EventIngestRequest,
) -> Result<EventIngestResponse, HookTransportError> {
    let body = serde_json::to_vec(request).map_err(|_| HookTransportError::Response)?;
    let body = send_json_request(metadata, EVENT_INGEST_PATH, &body)?;
    serde_json::from_slice(&body).map_err(|_| HookTransportError::Response)
}

pub fn send_mode_to_runtime(
    metadata: &RuntimeMetadataV1,
    mode: codegotchi_domain::EnforcementMode,
) -> Result<SnapshotMutationResponse, HookTransportError> {
    let body =
        serde_json::to_vec(&ModeRequest { mode }).map_err(|_| HookTransportError::Response)?;
    let body = send_json_request(metadata, "/api/v1/mode", &body)?;
    serde_json::from_slice(&body).map_err(|_| HookTransportError::Response)
}

pub fn send_debug_neglect_to_runtime(
    metadata: &RuntimeMetadataV1,
) -> Result<SnapshotMutationResponse, HookTransportError> {
    send_debug_request(metadata, "/api/v1/debug/neglect")
}

pub fn send_debug_restock_to_runtime(
    metadata: &RuntimeMetadataV1,
) -> Result<SnapshotMutationResponse, HookTransportError> {
    send_debug_request(metadata, "/api/v1/debug/restock")
}

pub fn send_debug_generate_poop_to_runtime(
    metadata: &RuntimeMetadataV1,
) -> Result<SnapshotMutationResponse, HookTransportError> {
    send_debug_request(metadata, "/api/v1/debug/generate-poop")
}

fn send_debug_request(
    metadata: &RuntimeMetadataV1,
    path: &str,
) -> Result<SnapshotMutationResponse, HookTransportError> {
    let body =
        serde_json::to_vec(&DebugRequest::default()).map_err(|_| HookTransportError::Response)?;
    let body = send_json_request(metadata, path, &body)?;
    serde_json::from_slice(&body).map_err(|_| HookTransportError::Response)
}

fn send_json_request(
    metadata: &RuntimeMetadataV1,
    path: &str,
    body: &[u8],
) -> Result<Vec<u8>, HookTransportError> {
    let deadline = Instant::now() + HOOK_TOTAL_TIMEOUT;
    let endpoint = parse_loopback_endpoint(&metadata.loopback_base_url)?;
    let connect_timeout = remaining_hook_timeout(deadline).map_err(HookTransportError::Io)?;
    let mut stream =
        TcpStream::connect_timeout(&endpoint, connect_timeout).map_err(HookTransportError::Io)?;
    let io_timeout = remaining_hook_timeout(deadline).map_err(HookTransportError::Io)?;
    stream
        .set_read_timeout(Some(io_timeout))
        .map_err(HookTransportError::Io)?;
    stream
        .set_write_timeout(Some(io_timeout))
        .map_err(HookTransportError::Io)?;
    let debug_header = if path.starts_with("/api/v1/debug/") {
        "X-CodeGotchi-Debug: 1\r\n"
    } else {
        ""
    };
    let request_head = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n{debug_header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        endpoint,
        metadata.bearer_token,
        body.len()
    );
    stream
        .set_write_timeout(Some(
            remaining_hook_timeout(deadline).map_err(HookTransportError::Io)?,
        ))
        .map_err(HookTransportError::Io)?;
    stream
        .write_all(request_head.as_bytes())
        .map_err(HookTransportError::Io)?;
    stream
        .set_write_timeout(Some(
            remaining_hook_timeout(deadline).map_err(HookTransportError::Io)?,
        ))
        .map_err(HookTransportError::Io)?;
    stream.write_all(body).map_err(HookTransportError::Io)?;
    let response = read_http_response(&mut stream, deadline)?;
    parse_http_response_body(&response)
}

fn read_http_response(
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<Vec<u8>, HookTransportError> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        stream
            .set_read_timeout(Some(
                remaining_hook_timeout(deadline).map_err(HookTransportError::Io)?,
            ))
            .map_err(HookTransportError::Io)?;
        let count = match stream.read(&mut buffer) {
            Ok(count) => count,
            Err(error)
                if !response.is_empty()
                    && matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
            {
                if Instant::now() >= deadline {
                    return Err(HookTransportError::Io(hook_deadline_error()));
                }
                break;
            }
            Err(error) => return Err(HookTransportError::Io(error)),
        };
        if count == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..count]);
        if response.len() > MAX_RESPONSE_BYTES {
            return Err(HookTransportError::Response);
        }
        if response_body_complete(&response)? {
            break;
        }
    }
    Ok(response)
}

fn remaining_hook_timeout(deadline: Instant) -> io::Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(hook_deadline_error())
    } else {
        Ok(remaining.min(HOOK_IO_TIMEOUT))
    }
}

fn hook_deadline_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "CodeGotchi hook request exceeded its deadline",
    )
}

fn response_body_complete(response: &[u8]) -> Result<bool, HookTransportError> {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(false);
    };
    let headers =
        std::str::from_utf8(&response[..header_end]).map_err(|_| HookTransportError::Response)?;
    let body_start = header_end + 4;
    let mut content_length = None;
    let mut chunked = false;
    for line in headers.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| HookTransportError::Response)?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"));
        }
    }
    if let Some(content_length) = content_length {
        return Ok(response.len() >= body_start.saturating_add(content_length));
    }
    if chunked {
        return chunked_body_complete(
            response
                .get(body_start..)
                .ok_or(HookTransportError::Response)?,
        );
    }
    Ok(false)
}

fn chunked_body_complete(mut bytes: &[u8]) -> Result<bool, HookTransportError> {
    loop {
        let Some(line_end) = bytes.windows(2).position(|window| window == b"\r\n") else {
            return Ok(false);
        };
        let size = std::str::from_utf8(&bytes[..line_end])
            .map_err(|_| HookTransportError::Response)?
            .split(';')
            .next()
            .ok_or(HookTransportError::Response)?
            .trim();
        let size = usize::from_str_radix(size, 16).map_err(|_| HookTransportError::Response)?;
        bytes = bytes
            .get(line_end + 2..)
            .ok_or(HookTransportError::Response)?;
        if size == 0 {
            return Ok(bytes.starts_with(b"\r\n")
                || bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .is_some());
        }
        let Some(chunk_and_crlf) = bytes.get(size..) else {
            return Ok(false);
        };
        let Some(rest) = chunk_and_crlf.strip_prefix(b"\r\n") else {
            return if chunk_and_crlf.len() < 2 {
                Ok(false)
            } else {
                Err(HookTransportError::Response)
            };
        };
        bytes = rest;
    }
}

fn parse_loopback_endpoint(base_url: &str) -> Result<SocketAddr, HookTransportError> {
    let authority = base_url
        .strip_prefix("http://")
        .ok_or(HookTransportError::InvalidUrl)?
        .split('/')
        .next()
        .ok_or(HookTransportError::InvalidUrl)?;
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or(HookTransportError::InvalidUrl)?;
    if !matches!(host, "127.0.0.1" | "localhost") {
        return Err(HookTransportError::NonLoopback);
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| HookTransportError::InvalidUrl)?;
    let mut addresses = (host, port)
        .to_socket_addrs()
        .map_err(HookTransportError::Io)?;
    addresses
        .find(|address| address.ip().is_loopback())
        .ok_or(HookTransportError::NonLoopback)
}

fn parse_http_response_body(response: &[u8]) -> Result<Vec<u8>, HookTransportError> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(HookTransportError::Response)?;
    let headers =
        std::str::from_utf8(&response[..header_end]).map_err(|_| HookTransportError::Response)?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or(HookTransportError::Response)?;
    if !(200..300).contains(&status) {
        return Err(HookTransportError::Status);
    }
    let mut content_length = None;
    let mut chunked = false;
    for line in headers.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| HookTransportError::Response)?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"));
        }
    }

    let body_start = header_end + 4;
    let body = if chunked {
        decode_chunked_body(
            response
                .get(body_start..)
                .ok_or(HookTransportError::Response)?,
        )?
    } else if let Some(content_length) = content_length {
        let body_end = body_start
            .checked_add(content_length)
            .ok_or(HookTransportError::Response)?;
        response
            .get(body_start..body_end)
            .ok_or(HookTransportError::Response)?
            .to_vec()
    } else {
        response
            .get(body_start..)
            .ok_or(HookTransportError::Response)?
            .to_vec()
    };
    Ok(body)
}

fn decode_chunked_body(mut bytes: &[u8]) -> Result<Vec<u8>, HookTransportError> {
    let mut body = Vec::new();
    loop {
        let line_end = bytes
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or(HookTransportError::Response)?;
        let size = std::str::from_utf8(&bytes[..line_end])
            .map_err(|_| HookTransportError::Response)?
            .split(';')
            .next()
            .ok_or(HookTransportError::Response)?
            .trim();
        let size = usize::from_str_radix(size, 16).map_err(|_| HookTransportError::Response)?;
        bytes = bytes
            .get(line_end + 2..)
            .ok_or(HookTransportError::Response)?;
        if size == 0 {
            if bytes.starts_with(b"\r\n")
                || bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .is_some()
            {
                return Ok(body);
            }
            return Err(HookTransportError::Response);
        }
        let chunk = bytes.get(..size).ok_or(HookTransportError::Response)?;
        body.extend_from_slice(chunk);
        bytes = bytes.get(size..).ok_or(HookTransportError::Response)?;
        if !bytes.starts_with(b"\r\n") {
            return Err(HookTransportError::Response);
        }
        bytes = &bytes[2..];
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{
        HookTransportError, RuntimeMetadataV1, read_http_response, runtime_metadata_is_active,
    };

    #[test]
    fn response_read_has_an_outer_deadline_when_peer_dribbles_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept hook connection");
            let response = b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
            for byte in response.iter().copied().cycle().take(100) {
                if stream.write_all(&[byte]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let mut stream = TcpStream::connect(address).expect("connect loopback listener");
        let started = Instant::now();

        let error = read_http_response(&mut stream, Instant::now() + Duration::from_millis(150))
            .expect_err("dribbled response must hit the outer deadline");

        assert!(matches!(
            error,
            HookTransportError::Io(error) if error.kind() == std::io::ErrorKind::TimedOut
        ));
        assert!(started.elapsed() < Duration::from_millis(600));
        stream
            .shutdown(Shutdown::Both)
            .expect("close hook connection");
        server.join().expect("dribble server exits");
    }

    #[test]
    fn runtime_liveness_uses_portable_unix_process_probe() {
        let metadata = RuntimeMetadataV1::new(
            uuid::Uuid::new_v4(),
            std::path::PathBuf::from("."),
            "http://127.0.0.1:1",
            "token",
            std::process::id(),
        );
        assert!(runtime_metadata_is_active(&metadata));

        let mut stale = metadata.clone();
        stale.owning_pid = 1;
        assert!(!runtime_metadata_is_active(&stale));
    }
}

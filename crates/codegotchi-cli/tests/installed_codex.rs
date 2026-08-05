use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use codegotchi_cli::runtime_metadata::{remove_metadata, write_metadata};
use codegotchi_cli::{EventIngestRequest, RuntimeMetadataV1, TemporaryCodexProfile};
use codegotchi_domain::{AgentEvent, AgentEventKind, EventMetadata, EventSource};
use uuid::Uuid;

const MANUAL_DENIAL_REASON: &str = "manual Codex strict denial";

#[test]
fn receiver_decision_requires_a_canonical_started_cargo_event() {
    let canonical = event_request(AgentEventKind::ToolStarted, Some("cargo"));
    let completed = event_request(AgentEventKind::ToolCompleted, Some("cargo"));
    let other_executable = event_request(AgentEventKind::ToolStarted, Some("cargo-test"));
    let no_executable = event_request(AgentEventKind::ToolStarted, None);

    assert!(should_deny_request(
        &serde_json::to_vec(&canonical).unwrap()
    ));
    assert!(!should_deny_request(
        &serde_json::to_vec(&completed).unwrap()
    ));
    assert!(!should_deny_request(
        &serde_json::to_vec(&other_executable).unwrap()
    ));
    assert!(!should_deny_request(
        &serde_json::to_vec(&no_executable).unwrap()
    ));
    assert!(!should_deny_request(br#"{"prompt":"cargo"}"#));
}

fn event_request(kind: AgentEventKind, executable_name: Option<&str>) -> EventIngestRequest {
    EventIngestRequest::new(AgentEvent::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "/tmp/codegotchi-test",
        EventSource::Codex,
        kind,
        None,
        chrono::Utc::now(),
        EventMetadata::new(
            executable_name.map(str::to_owned),
            Some("development".to_owned()),
            None,
            None,
            false,
        ),
    ))
}

fn should_deny_request(body: &[u8]) -> bool {
    serde_json::from_slice::<EventIngestRequest>(body)
        .ok()
        .is_some_and(|request| {
            request.event.kind == AgentEventKind::ToolStarted
                && request.event.metadata.executable_name.as_deref() == Some("cargo")
        })
}

/// This is an intentionally manual gate. It needs a real authenticated Codex
/// session and leaves trust approval to the operator instead of bypassing it.
#[test]
#[ignore = "manual installed Codex 0.146.0 trust/coexistence gate; run with --ignored --nocapture"]
fn installed_codex_0146_real_trust_and_coexistence_gate() {
    let codex_program =
        env::var_os("CODEGOTCHI_CODEX_BIN").unwrap_or_else(|| std::ffi::OsString::from("codex"));
    let version = Command::new(&codex_program)
        .arg("--version")
        .output()
        .expect("CODEGOTCHI_CODEX_BIN or codex must be installed");
    assert!(version.status.success());
    let version_text = String::from_utf8_lossy(&version.stdout);
    assert!(
        version_text.contains("codex-cli 0.146.0"),
        "manual gate requires Codex 0.146.0, got {version_text}"
    );

    let api_key = env::var_os("OPENAI_API_KEY")
        .or_else(|| env::var_os("CODEX_API_KEY"))
        .expect("run the manual gate with OPENAI_API_KEY or CODEX_API_KEY set");

    let temporary = TestDirectory::new("codegotchi-installed-codex");
    let home = temporary.path().join("codex-home");
    let repository = temporary.path().join("repository");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&repository).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .status()
            .expect("git must be installed for the disposable repository")
            .success()
    );

    let marker = temporary.path().join("existing-hook.marker");
    let existing_hook = temporary.path().join("existing-hook.sh");
    let fake_bin = temporary.path().join("fake-bin");
    let fake_cargo = fake_bin.join("cargo");
    let cargo_sentinel = temporary.path().join("fake-cargo-executed");
    fs::create_dir_all(&fake_bin).unwrap();
    write_fake_cargo(&fake_cargo, &cargo_sentinel);
    write_existing_hook(&existing_hook, &marker);
    let base_config = home.join("config.toml");
    fs::write(&base_config, render_base_config(&existing_hook, &marker)).unwrap();
    let base_before = fs::read(&base_config).unwrap();
    let credentials = home.join("auth.json");
    assert!(
        !credentials.exists(),
        "the manual gate must not create credentials"
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let bearer_token = format!("manual-{}", Uuid::new_v4());
    let capture = Arc::new(Mutex::new(Capture::default()));
    let mut receiver = Receiver::start(listener, bearer_token.clone(), Arc::clone(&capture));

    let metadata_path = temporary.path().join("runtime-metadata.json");
    let metadata = RuntimeMetadataV1::new(
        Uuid::new_v4(),
        &repository,
        format!("http://127.0.0.1:{}", address.port()),
        &bearer_token,
        std::process::id(),
    );
    write_metadata(&metadata_path, &metadata).unwrap();

    let binary = PathBuf::from(env!("CARGO_BIN_EXE_codegotchi"));
    assert_eq!(
        binary.file_name().and_then(|name| name.to_str()),
        Some("codegotchi")
    );
    let mut profile = TemporaryCodexProfile::create(
        &home,
        "codegotchi-manual-installed-codex",
        &metadata_path,
        "codegotchi hook",
    )
    .unwrap();
    let profile_path = profile.config_path().to_path_buf();

    let direct_denial = invoke_hook(
        &binary,
        &metadata_path,
        br#"{"session_id":"00000000-0000-0000-0000-000000000001","turn_id":"manual-turn","hook_event_name":"PreToolUse","tool_name":"Bash","tool_use_id":"manual-tool","tool_input":{"command":"cargo --version"}}"#,
    );
    assert!(direct_denial.status.success());
    assert_eq!(
        direct_denial.stdout,
        format!(
            "{{\"hookSpecificOutput\":{{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"{MANUAL_DENIAL_REASON}\"}}}}\n"
        )
        .as_bytes()
    );
    let direct_denials = capture.lock().unwrap().denied_requests;
    assert!(
        direct_denials >= 1,
        "the authenticated loopback must receive the denial"
    );

    let binary_directory = binary.parent().expect("binary has a parent");
    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let mut path_entries = vec![binary_directory.to_path_buf(), fake_bin.clone()];
    path_entries.extend(env::split_paths(&inherited_path));
    let path = env::join_paths(path_entries).unwrap();
    let mut codex = profile.codex_command(&codex_program);
    codex
        .args([
            "exec",
            "--ephemeral",
            "--json",
            "--cd",
        ])
        .arg(&repository)
        .arg("Create MANUAL_CODEGOTCHI_MARKER.txt with apply_patch, then run exactly cargo --version. Stop when the cargo command is denied by the hook.")
        .env("PATH", path)
        .env("OPENAI_API_KEY", &api_key)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    assert!(
        codex
            .get_args()
            .all(|argument| argument != "--dangerously-bypass-hook-trust")
    );
    assert!(
        codex
            .status()
            .expect("the manual Codex run must finish")
            .success()
    );

    receiver.stop();
    let captured = capture.lock().unwrap();
    assert!(captured.authenticated_requests > 0);
    assert!(captured.tool_started_requests > 0);
    assert!(
        captured.denied_requests > direct_denials,
        "the real Codex run must exercise strict PreToolUse denial"
    );
    assert!(
        !cargo_sentinel.exists(),
        "the denied cargo command must not execute the disposable fake cargo"
    );
    drop(captured);

    let existing_hook_output = fs::read_to_string(&marker).unwrap();
    assert!(!existing_hook_output.is_empty());
    assert!(
        existing_hook_output
            .lines()
            .all(|line| line == "pre-existing-hook-ran")
    );
    assert_eq!(fs::read(&base_config).unwrap(), base_before);
    assert!(!credentials.exists());

    profile.cleanup().unwrap();
    remove_metadata(&metadata_path).unwrap();
    assert!(!profile_path.exists());
    assert!(!metadata_path.exists());

    fs::remove_file(&existing_hook).unwrap();
    fs::remove_file(&marker).unwrap();
    fs::remove_file(&fake_cargo).unwrap();
    fs::remove_dir(&fake_bin).unwrap();
    fs::remove_file(&base_config).unwrap();
    fs::remove_dir_all(temporary.path()).unwrap();
    assert!(!temporary.path().exists());
}

fn write_existing_hook(path: &Path, marker: &Path) {
    fs::write(
        path,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf 'pre-existing-hook-ran\\n' >> {}\n",
            shell_quote(marker)
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).unwrap();
}

fn write_fake_cargo(path: &Path, sentinel: &Path) {
    fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf 'fake-cargo-executed\\n' > {}\nexit 0\n",
            shell_quote(sentinel)
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).unwrap();
}

fn render_base_config(existing_hook: &Path, marker: &Path) -> String {
    format!(
        "[features]\nhooks = true\n\n[[hooks.SessionStart]]\n\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = \"{} {}\"\n",
        toml_quote(existing_hook),
        toml_quote(marker)
    )
}

fn toml_quote(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn invoke_hook(binary: &Path, metadata_path: &Path, payload: &[u8]) -> Output {
    let mut child = Command::new(binary)
        .arg("hook")
        .env("CODEGOTCHI_SESSION_FILE", metadata_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(payload).unwrap();
    child.wait_with_output().unwrap()
}

#[derive(Default)]
struct Capture {
    authenticated_requests: usize,
    tool_started_requests: usize,
    denied_requests: usize,
}

struct Receiver {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Receiver {
    fn start(listener: TcpListener, token: String, capture: Arc<Mutex<Capture>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => handle_request(stream, &token, &capture),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop();
    }
}

fn handle_request(mut stream: TcpStream, token: &str, capture: &Arc<Mutex<Capture>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let Some((headers, body)) = read_request(&mut stream) else {
        return;
    };
    let authorization = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then_some(value.trim())
    });
    let expected_authorization = format!("Bearer {token}");
    if authorization != Some(expected_authorization.as_str()) {
        write_response(&mut stream, 401, br#"{}"#);
        return;
    }

    let should_deny = should_deny_request(&body);
    {
        let mut capture = capture.lock().unwrap();
        capture.authenticated_requests += 1;
        if should_deny {
            capture.tool_started_requests += 1;
        }
        if should_deny {
            capture.denied_requests += 1;
        }
    }

    let body = if should_deny {
        format!(
            "{{\"accepted\":true,\"evaluated\":true,\"strict\":true,\"blocked\":true,\"reason\":\"{MANUAL_DENIAL_REASON}\"}}"
        )
    } else {
        String::from(r#"{"accepted":true,"evaluated":true,"strict":false}"#)
    };
    write_response(&mut stream, 200, body.as_bytes());
}

fn read_request(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let (header_end, content_length) = loop {
        let count = stream.read(&mut buffer).ok()?;
        if count == 0 || bytes.len() > 128 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..count]);
        let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = std::str::from_utf8(&bytes[..position]).ok()?;
        let content_length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })?;
        if bytes.len() >= position + 4 + content_length {
            break (position, content_length);
        }
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).ok()?;
    let body_start = header_end + 4;
    let body = bytes[body_start..body_start + content_length].to_vec();
    Some((headers, body))
}

fn write_response(stream: &mut TcpStream, status: u16, body: &[u8]) {
    let reason = if status == 200 { "OK" } else { "Unauthorized" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(body);
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new(prefix: &str) -> Self {
        let path = env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

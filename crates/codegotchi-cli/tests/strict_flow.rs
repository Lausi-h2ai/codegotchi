use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Duration, Utc};
use codegotchi_cli::runtime_metadata::write_metadata;
use codegotchi_cli::{AuthoritativeRuntime, RunningServer, RuntimeMetadataV1, SqliteStore};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, EnforcementMode, EventMetadata, EventSource, Pet,
    PetSpecies,
};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use uuid::Uuid;

struct TestDatabase {
    path: PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after the epoch")
            .as_nanos();
        Self {
            path: std::env::temp_dir().join(format!("codegotchi-task-4-strict-{suffix}.sqlite")),
        }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.path.with_extension("sqlite-shm"));
        let _ = fs::remove_file(self.path.with_extension("sqlite-wal"));
    }
}

async fn invoke_hook(metadata_path: &Path, payload: &[u8]) -> Output {
    let metadata_path = metadata_path.to_path_buf();
    let payload = payload.to_vec();
    tokio::task::spawn_blocking(move || invoke_hook_blocking(&metadata_path, &payload))
        .await
        .expect("blocking hook helper joins")
}

fn invoke_hook_blocking(metadata_path: &Path, payload: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codegotchi"))
        .arg("hook")
        .env("CODEGOTCHI_SESSION_FILE", metadata_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("production hook process starts");
    child
        .stdin
        .take()
        .expect("hook stdin is available")
        .write_all(payload)
        .expect("payload reaches hook stdin");
    child.wait_with_output().expect("hook process exits")
}

async fn invoke_cli(args: &[&str], metadata_path: &Path, debug: bool) -> Output {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let metadata_path = metadata_path.to_path_buf();
    tokio::task::spawn_blocking(move || invoke_cli_blocking(&args, &metadata_path, debug))
        .await
        .expect("blocking CLI helper joins")
}

fn invoke_cli_blocking(args: &[String], metadata_path: &Path, debug: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_codegotchi"));
    command
        .args(args)
        .env("CODEGOTCHI_SESSION_FILE", metadata_path);
    if debug {
        command.env("CODEGOTCHI_ENABLE_DEBUG", "1");
    } else {
        command.env_remove("CODEGOTCHI_ENABLE_DEBUG");
    }
    command.output().expect("production CLI process starts")
}

fn parse_stdout(output: &Output) -> Value {
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    serde_json::from_slice(&output.stdout).expect("hook stdout is valid JSON")
}

fn assert_allow(output: &Output) {
    assert_eq!(parse_stdout(output), serde_json::json!({}));
}

fn safe_fixture(tool_use_id: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "session_id":"00000000-0000-0000-0000-000000000001",
        "turn_id":"strict-turn",
        "hook_event_name":"PreToolUse",
        "tool_name":"Bash",
        "tool_use_id":tool_use_id,
        "tool_input":{"command":"cargo test -p secret-project"},
        "prompt":"never-persist-this-prompt",
        "source":"secret-source-content"
    }))
    .expect("safe fixture serializes")
}

fn shell_fixture(tool_use_id: &str, command: &str, tool_name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "session_id":"00000000-0000-0000-0000-000000000001",
        "turn_id":"strict-matrix-turn",
        "hook_event_name":"PreToolUse",
        "tool_name":tool_name,
        "tool_use_id":tool_use_id,
        "tool_input":{"command":command}
    }))
    .expect("matrix fixture serializes")
}

fn incomplete_apply_patch_fixture(tool_use_id: &str, tool_input: Option<Value>) -> Vec<u8> {
    let mut payload = serde_json::Map::from_iter([
        (
            String::from("session_id"),
            Value::String(String::from("00000000-0000-0000-0000-000000000001")),
        ),
        (
            String::from("turn_id"),
            Value::String(String::from("incomplete-apply-patch-turn")),
        ),
        (
            String::from("hook_event_name"),
            Value::String(String::from("PreToolUse")),
        ),
        (
            String::from("tool_name"),
            Value::String(String::from("apply_patch")),
        ),
        (
            String::from("tool_use_id"),
            Value::String(tool_use_id.to_owned()),
        ),
    ]);
    if let Some(tool_input) = tool_input {
        payload.insert(String::from("tool_input"), tool_input);
    }
    serde_json::to_vec(&Value::Object(payload)).expect("incomplete fixture serializes")
}

async fn next_snapshot(
    snapshots: &mut tokio::sync::broadcast::Receiver<codegotchi_domain::SimulationSnapshot>,
) -> codegotchi_domain::SimulationSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(2), snapshots.recv())
        .await
        .expect("runtime mutation broadcasts promptly")
        .expect("runtime snapshot channel remains open")
}

async fn request(
    server: &RunningServer,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> (u16, Value) {
    let mut stream = TcpStream::connect(server.local_addr())
        .await
        .expect("server accepts loopback request");
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request headers write");
    stream.write_all(body).await.expect("request body write");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("response read");
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP response has headers");
    let status = String::from_utf8_lossy(&bytes[..separator])
        .lines()
        .next()
        .expect("HTTP status line")
        .split_whitespace()
        .nth(1)
        .expect("HTTP status")
        .parse()
        .expect("numeric HTTP status");
    let body = serde_json::from_slice(&bytes[separator + 4..]).unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn strict_denial_is_verified_fail_open_and_recoverable_through_normal_care() {
    let database = TestDatabase::new();
    let runtime = AuthoritativeRuntime::new(
        SqliteStore::open(&database.path).expect("SQLite opens"),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, Utc::now()),
    )
    .expect("runtime starts");
    let (_, tick_receiver) = mpsc::unbounded_channel();
    let server = RunningServer::start_with_maintenance_trigger(
        runtime.clone(),
        "task-4-strict-token",
        tick_receiver,
    )
    .await
    .expect("loopback server starts");
    assert_eq!(
        request(&server, "GET", "/api/v1/health", None, b"").await.0,
        200
    );
    let (initial, mut snapshots) = runtime.subscribe().expect("runtime subscribes");
    assert_eq!(initial.enforcement_mode, EnforcementMode::Decorative);

    let metadata_path = database.path.with_extension("runtime.json");
    let metadata = RuntimeMetadataV1::new(
        Uuid::from_u128(99),
        "/workspace/codegatchi",
        server.base_url(),
        "task-4-strict-token",
        std::process::id(),
    );
    write_metadata(&metadata_path, &metadata).expect("metadata is written");

    let missing_metadata_path = database.path.with_extension("missing-runtime.json");
    let missing_mode = invoke_cli(&["mode", "strict"], &missing_metadata_path, false).await;
    assert!(!missing_mode.status.success());
    assert!(String::from_utf8_lossy(&missing_mode.stderr).contains("metadata"));

    let stale_metadata_path = database.path.with_extension("stale-runtime.json");
    write_metadata(
        &stale_metadata_path,
        &RuntimeMetadataV1::new(
            Uuid::from_u128(98),
            "/workspace/codegatchi",
            server.base_url(),
            "task-4-strict-token",
            u32::MAX,
        ),
    )
    .expect("stale metadata is written");
    let stale_neglect = invoke_cli(&["debug", "neglect"], &stale_metadata_path, true).await;
    assert!(!stale_neglect.status.success());
    let stale_generate = invoke_cli(&["debug", "generate-poop"], &stale_metadata_path, true).await;
    assert!(!stale_generate.status.success());

    let unauthorized_metadata_path = database.path.with_extension("unauthorized-runtime.json");
    write_metadata(
        &unauthorized_metadata_path,
        &RuntimeMetadataV1::new(
            Uuid::from_u128(97),
            "/workspace/codegatchi",
            server.base_url(),
            "wrong-token",
            std::process::id(),
        ),
    )
    .expect("unauthorized metadata is written");
    let unauthorized_mode =
        invoke_cli(&["mode", "strict"], &unauthorized_metadata_path, false).await;
    assert!(!unauthorized_mode.status.success());
    let unauthorized_neglect =
        invoke_cli(&["debug", "neglect"], &unauthorized_metadata_path, true).await;
    assert!(!unauthorized_neglect.status.success());
    let unauthorized_generate = invoke_cli(
        &["debug", "generate-poop"],
        &unauthorized_metadata_path,
        true,
    )
    .await;
    assert!(!unauthorized_generate.status.success());

    let denied_without_guard = invoke_cli(&["debug", "neglect"], &metadata_path, false).await;
    assert!(!denied_without_guard.status.success());
    assert!(
        String::from_utf8_lossy(&denied_without_guard.stderr).contains("CODEGOTCHI_ENABLE_DEBUG=1")
    );
    let generate_without_guard =
        invoke_cli(&["debug", "generate-poop"], &metadata_path, false).await;
    assert!(!generate_without_guard.status.success());
    let arbitrary_debug =
        invoke_cli(&["debug", "neglect", "arbitrary"], &metadata_path, true).await;
    assert!(!arbitrary_debug.status.success());
    assert!(String::from_utf8_lossy(&arbitrary_debug.stderr).contains("arbitrary"));

    let mode = invoke_cli(&["mode", "strict"], &metadata_path, false).await;
    assert!(mode.status.success(), "mode stderr: {:?}", mode.stderr);
    assert!(String::from_utf8_lossy(&mode.stdout).contains("persisted and broadcast"));
    let mode_snapshot = next_snapshot(&mut snapshots).await;
    assert_eq!(mode_snapshot.enforcement_mode, EnforcementMode::Strict);
    assert!(mode_snapshot.last_updated_at > initial.last_updated_at);

    let extra_mode = request(
        &server,
        "POST",
        "/api/v1/mode",
        Some("task-4-strict-token"),
        br#"{"mode":"strict","arbitrary":999}"#,
    )
    .await;
    assert_eq!(extra_mode.0, 400);
    assert_eq!(extra_mode.1["error"]["code"], "invalid_json");

    let extra_debug = request(
        &server,
        "POST",
        "/api/v1/debug/neglect",
        Some("task-4-strict-token"),
        br#"{"hours":999999}"#,
    )
    .await;
    assert_eq!(extra_debug.0, 400);
    assert_eq!(extra_debug.1["error"]["code"], "invalid_json");

    let neglect = invoke_cli(&["debug", "neglect"], &metadata_path, true).await;
    assert!(
        neglect.status.success(),
        "neglect stderr: {:?}",
        neglect.stderr
    );
    assert!(String::from_utf8_lossy(&neglect.stdout).contains("persisted and broadcast"));
    let neglected = next_snapshot(&mut snapshots).await;
    assert!(neglected.needs.hunger() >= 90.0);
    assert!(neglected.last_updated_at > mode_snapshot.last_updated_at);

    let first_denial = invoke_hook(&metadata_path, &safe_fixture("strict-tool-1")).await;
    let first_value = parse_stdout(&first_denial);
    assert_eq!(
        first_value,
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "The pet refuses this action because its hunger is critical. Feed the pet in the CodeGotchi UI, then retry the Codex request afterward."
            }
        })
    );
    let reason = first_value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("denial reason is text");
    assert!(reason.contains("refuses"));
    assert!(reason.contains("hunger"));
    assert!(reason.contains("feed") || reason.contains("Feed"));
    assert!(reason.contains("CodeGotchi UI"));
    assert!(reason.contains("retry"));
    let after_first_denial = next_snapshot(&mut snapshots).await;

    let duplicate_denial = invoke_hook(&metadata_path, &safe_fixture("strict-tool-1")).await;
    assert_eq!(parse_stdout(&duplicate_denial), first_value);
    let duplicate_snapshot = runtime.snapshot();
    assert_eq!(duplicate_snapshot, after_first_denial);

    let decorative = invoke_cli(&["mode", "decorative"], &metadata_path, false).await;
    assert!(
        decorative.status.success(),
        "mode stderr: {:?}",
        decorative.stderr
    );
    let decorative_snapshot = next_snapshot(&mut snapshots).await;
    assert_eq!(
        decorative_snapshot.enforcement_mode,
        EnforcementMode::Decorative
    );
    let decorative_hook = invoke_hook(&metadata_path, &safe_fixture("decorative-tool")).await;
    assert_allow(&decorative_hook);
    let _ = next_snapshot(&mut snapshots).await;

    let strict_again = invoke_cli(&["mode", "strict"], &metadata_path, false).await;
    assert!(
        strict_again.status.success(),
        "mode stderr: {:?}",
        strict_again.stderr
    );
    let _ = next_snapshot(&mut snapshots).await;
    let neglect_again = invoke_cli(&["debug", "neglect"], &metadata_path, true).await;
    assert!(
        neglect_again.status.success(),
        "neglect stderr: {:?}",
        neglect_again.stderr
    );
    let _ = next_snapshot(&mut snapshots).await;

    let incomplete_apply_patch_cases = [
        ("missing", None),
        ("array", Some(serde_json::json!([]))),
        ("string", Some(Value::String(String::from("patch text")))),
        (
            "non-string-command",
            Some(serde_json::json!({"command": 42})),
        ),
        ("empty-command", Some(serde_json::json!({"command": ""}))),
        (
            "whitespace-command",
            Some(serde_json::json!({"command": "   "})),
        ),
    ];
    for (identity, tool_input) in incomplete_apply_patch_cases {
        let output = invoke_hook(
            &metadata_path,
            &incomplete_apply_patch_fixture(identity, tool_input),
        )
        .await;
        // The pet is severely neglected here, so even uncertain tool calls
        // are refused; unverified apply_patch input must never be trusted as
        // safe development and then escape the severe refusal.
        let value = parse_stdout(&output);
        assert_eq!(
            value["hookSpecificOutput"]["permissionDecision"], "deny",
            "{identity} must be refused at severe neglect"
        );
        let _ = next_snapshot(&mut snapshots).await;
    }

    // The pet was neglected twice, so needs are severe: every purpose except
    // CodeGotchi control is refused in strict mode.
    let severe_scope_cases = [
        (
            "unknown",
            shell_fixture("unknown", "totally-unknown-operation", "Bash"),
            false,
        ),
        (
            "compound",
            shell_fixture("compound", "cargo test && cargo build", "Bash"),
            false,
        ),
        (
            "codegotchi",
            shell_fixture("codegotchi", "codegotchi mode strict", "Bash"),
            true,
        ),
        (
            "git",
            shell_fixture("git", "git status --short", "Bash"),
            false,
        ),
        (
            "termination",
            shell_fixture("termination", "exit 0", "Bash"),
            false,
        ),
        (
            "recovery",
            shell_fixture("recovery", "pkill cargo", "Bash"),
            false,
        ),
        (
            "diagnostic",
            shell_fixture("diagnostic", "cargo --version", "Bash"),
            false,
        ),
        (
            "unknown-tool",
            shell_fixture("unknown-tool", "cargo test", "FutureTool"),
            false,
        ),
    ];
    for (name, payload, allowed) in severe_scope_cases {
        let output = invoke_hook(&metadata_path, &payload).await;
        if allowed {
            assert_allow(&output);
        } else {
            let value = parse_stdout(&output);
            assert_eq!(
                value["hookSpecificOutput"]["permissionDecision"], "deny",
                "{name} must be refused at severe neglect"
            );
        }
        let _ = next_snapshot(&mut snapshots).await;
        assert!(!name.is_empty(), "case is named for diagnostics");
    }

    let malformed = invoke_hook(&metadata_path, b"{not-json").await;
    assert_allow(&malformed);

    let invalid_metadata_path = database.path.with_extension("invalid-runtime.json");
    fs::write(&invalid_metadata_path, b"{not-json").expect("invalid metadata is written");
    let invalid_metadata =
        invoke_hook(&invalid_metadata_path, &safe_fixture("invalid-metadata")).await;
    assert_allow(&invalid_metadata);

    let transport_listener = TcpListener::bind("127.0.0.1:0").expect("transport mock binds");
    let transport_address = transport_listener
        .local_addr()
        .expect("transport mock address is available");
    let transport_metadata_path = database.path.with_extension("transport-runtime.json");
    let transport_metadata = RuntimeMetadataV1::new(
        Uuid::from_u128(101),
        "/workspace/codegatchi",
        format!("http://{transport_address}"),
        "task-4-strict-token",
        std::process::id(),
    );
    write_metadata(&transport_metadata_path, &transport_metadata).expect("transport metadata");
    let transport_failure =
        invoke_hook(&transport_metadata_path, &safe_fixture("transport-failure")).await;
    assert_allow(&transport_failure);
    drop(transport_listener);

    let malformed_listener = TcpListener::bind("127.0.0.1:0").expect("mock binds");
    let malformed_address = malformed_listener
        .local_addr()
        .expect("mock address is available");
    let malformed_thread = std::thread::spawn(move || {
        let (mut stream, _) = malformed_listener.accept().expect("hook connects to mock");
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let body = b"not-json";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).expect("mock headers");
        stream.write_all(body).expect("mock body");
    });
    let malformed_metadata_path = database.path.with_extension("malformed-runtime.json");
    let malformed_metadata = RuntimeMetadataV1::new(
        Uuid::from_u128(100),
        "/workspace/codegatchi",
        format!("http://{malformed_address}"),
        "task-4-strict-token",
        std::process::id(),
    );
    write_metadata(&malformed_metadata_path, &malformed_metadata).expect("mock metadata");
    let malformed_response = invoke_hook(
        &malformed_metadata_path,
        &safe_fixture("malformed-response"),
    )
    .await;
    assert_allow(&malformed_response);
    malformed_thread.join().expect("mock exits");

    // Severe neglect leaves both hunger and energy critical, so recovery now
    // needs two kibble to clear the mild hunger boundary plus a hammock nap
    // to restore energy. Wall-clock idle time no longer recovers energy.
    for action_id in 710_u128..712_u128 {
        let recovery = request(
            &server,
            "POST",
            "/api/v1/care/feed",
            Some("task-4-strict-token"),
            &serde_json::to_vec(&serde_json::json!({
                "actionId": Uuid::from_u128(action_id),
                "foodId": "kibble"
            }))
            .expect("feed serializes"),
        )
        .await;
        assert_eq!(recovery.0, 200);
    }
    let _ = next_snapshot(&mut snapshots).await;
    let fed = next_snapshot(&mut snapshots).await;
    assert!(
        fed.needs.hunger() < 70.0,
        "two kibble clear the mild hunger boundary"
    );
    assert_eq!(fed.enforcement_mode, EnforcementMode::Strict);

    // End the active tool before the normal care recovery path.
    runtime
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(900),
            Uuid::from_u128(1),
            "/workspace/codegatchi",
            EventSource::Codex,
            AgentEventKind::ToolCompleted,
            Some(ActivityKind::UnknownWork),
            Utc::now(),
            EventMetadata::default(),
        ))
        .expect("rest event applies");
    let _ = next_snapshot(&mut snapshots).await;

    let napped = request(
        &server,
        "POST",
        "/api/v1/care/nap",
        Some("task-4-strict-token"),
        &serde_json::to_vec(&serde_json::json!({
            "actionId": Uuid::from_u128(712),
        }))
        .expect("nap serializes"),
    )
    .await;
    assert_eq!(napped.0, 200);
    assert!(napped.1["nappingUntil"].is_string());
    let napping = next_snapshot(&mut snapshots).await;
    assert_eq!(napping.needs.energy(), 0.0);
    runtime
        .maintenance_tick_at(napping.last_updated_at + Duration::seconds(5))
        .expect("nap completion advances exactly five seconds");
    let rested = next_snapshot(&mut snapshots).await;
    assert_eq!(rested.needs.energy(), 100.0);
    assert!(rested.napping_until.is_none());
    assert!(rested.needs.hunger() < 70.0);
    let recovered_hook = invoke_hook(&metadata_path, &safe_fixture("strict-tool-2")).await;
    assert_allow(&recovered_hook);
    let _ = next_snapshot(&mut snapshots).await;

    let generate = invoke_cli(&["debug", "generate-poop"], &metadata_path, true).await;
    assert!(
        generate.status.success(),
        "generate stderr: {:?}",
        generate.stderr
    );
    let generated = next_snapshot(&mut snapshots).await;
    let poop = generated
        .pending_poops
        .first()
        .expect("debug leaves real poop");
    let clean = request(
        &server,
        "POST",
        "/api/v1/care/clean",
        Some("task-4-strict-token"),
        &serde_json::to_vec(&serde_json::json!({
            "actionId": Uuid::from_u128(701),
            "poopId": poop.id()
        }))
        .expect("clean serializes"),
    )
    .await;
    assert_eq!(clean.0, 200);
    let cleaned = next_snapshot(&mut snapshots).await;
    assert!(
        !cleaned
            .pending_poops
            .iter()
            .any(|candidate| candidate.id() == poop.id())
    );

    let persisted = SqliteStore::open(&database.path)
        .expect("SQLite reopens")
        .load()
        .expect("snapshot loads")
        .expect("snapshot exists");
    assert_eq!(persisted, runtime.snapshot());
    assert_eq!(persisted.enforcement_mode, EnforcementMode::Strict);

    fs::remove_file(&metadata_path).expect("metadata cleanup");
    fs::remove_file(&stale_metadata_path).expect("stale metadata cleanup");
    fs::remove_file(&unauthorized_metadata_path).expect("unauthorized metadata cleanup");
    fs::remove_file(&invalid_metadata_path).expect("invalid metadata cleanup");
    fs::remove_file(&transport_metadata_path).expect("transport metadata cleanup");
    fs::remove_file(&malformed_metadata_path).expect("mock metadata cleanup");
    server.shutdown().await.expect("server shuts down");
}

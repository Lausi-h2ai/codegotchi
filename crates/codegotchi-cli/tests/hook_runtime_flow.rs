use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use codegotchi_cli::runtime_metadata::write_metadata;
use codegotchi_cli::{AuthoritativeRuntime, RunningServer, RuntimeMetadataV1, SqliteStore};
use codegotchi_domain::{AgentActivityState, AgentOutcome, Pet, PetSpecies};
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
            path: std::env::temp_dir().join(format!("codegotchi-task-4-hook-{suffix}.sqlite")),
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

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hooks")
            .join(name),
    )
    .expect("hook fixture exists")
}

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0)
        .single()
        .expect("fixture time is valid")
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
        .expect("fixture reaches hook stdin");
    child.wait_with_output().expect("hook process exits")
}

fn assert_allow(output: &Output) {
    assert!(output.status.success(), "hook stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"{}\n");
    let value: Value = serde_json::from_slice(&output.stdout).expect("hook stdout is JSON");
    assert_eq!(value, serde_json::json!({}));
}

async fn next_snapshot(
    snapshots: &mut tokio::sync::broadcast::Receiver<codegotchi_domain::SimulationSnapshot>,
) -> codegotchi_domain::SimulationSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(2), snapshots.recv())
        .await
        .expect("runtime mutation broadcasts promptly")
        .expect("runtime snapshot channel remains open")
}

async fn wait_for_server(server: &RunningServer) {
    for _ in 0..50 {
        if let Ok(mut stream) = TcpStream::connect(server.local_addr()).await {
            let request =
                b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
            if stream.write_all(request).await.is_ok() {
                let mut response = Vec::new();
                if stream.read_to_end(&mut response).await.is_ok()
                    && response.starts_with(b"HTTP/1.1 200")
                {
                    return;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("loopback server did not become ready");
}

#[tokio::test]
async fn installed_hook_fixtures_drive_the_complete_authoritative_runtime_flow() {
    let database = TestDatabase::new();
    let runtime = AuthoritativeRuntime::new(
        SqliteStore::open(&database.path).expect("SQLite opens"),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .expect("runtime starts");
    let (_, tick_receiver) = mpsc::unbounded_channel();
    let server = RunningServer::start_with_maintenance_trigger(
        runtime.clone(),
        "task-4-hook-token",
        tick_receiver,
    )
    .await
    .expect("loopback server starts");
    wait_for_server(&server).await;
    let (initial, mut snapshots) = runtime.subscribe().expect("runtime subscribes");
    assert_eq!(initial.activity, AgentActivityState::Idle);

    let metadata_path = database.path.with_extension("runtime.json");
    let metadata = RuntimeMetadataV1::new(
        Uuid::from_u128(99),
        "/workspace/codegatchi",
        server.base_url(),
        "task-4-hook-token",
        std::process::id(),
    );
    write_metadata(&metadata_path, &metadata).expect("metadata is written");

    let session_start = invoke_hook(&metadata_path, &fixture("session_start.json")).await;
    assert_allow(&session_start);
    assert_eq!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::Idle
    );

    let prompt = invoke_hook(&metadata_path, &fixture("user_prompt_submit.json")).await;
    assert_allow(&prompt);
    assert!(matches!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::Active(codegotchi_domain::ActivityKind::Thinking)
    ));

    let searching_payload = br#"{
        "session_id":"00000000-0000-0000-0000-000000000001",
        "turn_id":"turn-searching",
        "hook_event_name":"PreToolUse",
        "tool_name":"Bash",
        "tool_use_id":"call-searching",
        "tool_input":{"command":"rg --hidden secret-name src"},
        "future_search_field":{"source":"do-not-persist-search-source"}
    }"#;
    let searching = invoke_hook(&metadata_path, searching_payload).await;
    assert_allow(&searching);
    assert!(matches!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::Active(codegotchi_domain::ActivityKind::Searching)
    ));

    let testing = invoke_hook(&metadata_path, &fixture("bash_pre.json")).await;
    assert_allow(&testing);
    assert!(matches!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::Active(codegotchi_domain::ActivityKind::Testing)
    ));

    let generic = invoke_hook(&metadata_path, &fixture("unknown_tool.json")).await;
    assert_allow(&generic);
    assert!(matches!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::Active(codegotchi_domain::ActivityKind::UnknownWork)
    ));

    let editing = invoke_hook(&metadata_path, &fixture("apply_patch_pre.json")).await;
    assert_allow(&editing);
    assert!(matches!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::Active(codegotchi_domain::ActivityKind::Editing)
    ));

    let success = invoke_hook(&metadata_path, &fixture("bash_post_success.json")).await;
    assert_allow(&success);
    let success_snapshot = next_snapshot(&mut snapshots).await;
    assert_eq!(success_snapshot.activity, AgentActivityState::Idle);
    assert_eq!(success_snapshot.recent_outcome, AgentOutcome::Success);

    let failure = invoke_hook(&metadata_path, &fixture("bash_post_failure.json")).await;
    assert_allow(&failure);
    let failure_snapshot = next_snapshot(&mut snapshots).await;
    assert_eq!(failure_snapshot.activity, AgentActivityState::Idle);
    assert_eq!(failure_snapshot.recent_outcome, AgentOutcome::Failure);

    let stop = invoke_hook(&metadata_path, &fixture("stop.json")).await;
    assert_allow(&stop);
    assert_eq!(
        next_snapshot(&mut snapshots).await.activity,
        AgentActivityState::WaitingForUser
    );

    let session_end = invoke_hook(&metadata_path, &fixture("session_end.json")).await;
    assert_allow(&session_end);
    let ended = next_snapshot(&mut snapshots).await;
    assert_eq!(ended.activity, AgentActivityState::Idle);
    assert!(ended.session_activities.is_empty());

    let future = invoke_hook(
        &metadata_path,
        &fixture("user_prompt_submit_future_fields.json"),
    )
    .await;
    assert_allow(&future);

    let persisted = SqliteStore::open(&database.path)
        .expect("SQLite reopens")
        .load()
        .expect("snapshot loads")
        .expect("snapshot exists");
    let persisted_json = serde_json::to_string(&persisted).expect("snapshot serializes");
    for secret in [
        "never-persist-this-prompt",
        "secret-source-content",
        "sensitive-tool-output",
        "cargo test -p secret-project",
        "do-not-persist-search-source",
    ] {
        assert!(!persisted_json.contains(secret), "privacy leak: {secret}");
    }
    assert!(persisted.processed_event_ids.len() >= 10);

    fs::remove_file(&metadata_path).expect("metadata cleanup");
    server.shutdown().await.expect("server shuts down");
}

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use codegotchi_cli::runtime_metadata::write_metadata;
use codegotchi_cli::{
    AuthoritativeRuntime, EventIngestRequest, RunningServer, RuntimeMetadataV1, SqliteStore,
};
use codegotchi_domain::{AgentEvent, AgentEventKind, EventMetadata, EventSource, Pet, PetSpecies};
use rusqlite::Connection;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast::error::TryRecvError, mpsc};
use uuid::Uuid;

const TOKEN: &str = "task-2-test-token";

struct TestDatabase {
    path: PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self {
            path: std::env::temp_dir().join(format!("codegotchi-task-2-{suffix}.sqlite")),
        }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(self.path.with_extension("sqlite-wal"));
    }
}

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).unwrap()
}

fn event(id: u128) -> AgentEvent {
    AgentEvent::new(
        Uuid::from_u128(id),
        Uuid::from_u128(7),
        "repo",
        EventSource::Codex,
        AgentEventKind::SessionStarted,
        None,
        start(),
        EventMetadata::default(),
    )
}

fn work_event(id: u128) -> AgentEvent {
    AgentEvent::new(
        Uuid::from_u128(id),
        Uuid::from_u128(7),
        "repo",
        EventSource::Codex,
        AgentEventKind::CommandStarted,
        Some(codegotchi_domain::ActivityKind::Testing),
        start(),
        EventMetadata::default(),
    )
}

fn runtime(db: &TestDatabase) -> std::sync::Arc<AuthoritativeRuntime> {
    let store = SqliteStore::open(&db.path).unwrap();
    AuthoritativeRuntime::new(
        store,
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .unwrap()
}

struct HttpResponse {
    status: u16,
    body: Value,
}

async fn request(
    server: &RunningServer,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> HttpResponse {
    let mut stream = TcpStream::connect(server.local_addr()).await.unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8_lossy(&bytes[..separator]);
    let status = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let body = serde_json::from_slice(&bytes[separator + 4..]).unwrap_or(Value::Null);
    HttpResponse { status, body }
}

#[tokio::test]
async fn production_hook_reaches_authoritative_state_and_parses_the_server_response() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();
    let metadata_path = db.path.with_extension("runtime.json");
    let metadata = RuntimeMetadataV1::new(
        Uuid::from_u128(99),
        "/tmp/codegotchi-correction-repository",
        server.base_url(),
        TOKEN,
        std::process::id(),
    );
    write_metadata(&metadata_path, &metadata).unwrap();
    let payload = br#"{
        "session_id":"00000000-0000-0000-0000-000000000002",
        "turn_id":"correction-turn",
        "hook_event_name":"UserPromptSubmit"
    }"#;
    let event = codegotchi_cli::translate_hook_json(payload, &metadata).unwrap();

    let hook_metadata_path = metadata_path.clone();
    let hook_payload = payload.to_vec();
    let output =
        tokio::task::spawn_blocking(move || invoke_hook(&hook_metadata_path, &hook_payload))
            .await
            .unwrap();
    assert!(output.status.success(), "hook stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"{}\n");
    assert!(runtime.snapshot().processed_event_ids.contains(&event.id));
    assert_eq!(
        SqliteStore::open(&db.path).unwrap().load().unwrap(),
        Some(runtime.snapshot())
    );

    let request = EventIngestRequest::new(event);
    let response_metadata = metadata.clone();
    let response = tokio::task::spawn_blocking(move || {
        codegotchi_cli::send_event_to_runtime(&response_metadata, &request)
    })
    .await
    .unwrap()
    .unwrap();
    assert!(response.accepted);
    assert!(response.duplicate);

    std::fs::remove_file(&metadata_path).unwrap();
    server.shutdown().await.unwrap();
}

fn invoke_hook(metadata_path: &std::path::Path, payload: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codegotchi"))
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

#[tokio::test]
async fn authenticated_loopback_http_is_authoritative_and_replay_safe() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();
    assert!(server.local_addr().ip().is_loopback());

    let health = request(&server, "GET", "/api/v1/health", None, b"").await;
    assert_eq!(health.status, 200);
    assert_eq!(health.body["status"], "ok");

    assert_eq!(
        request(&server, "GET", "/api/v1/state", None, b"")
            .await
            .status,
        401
    );
    assert_eq!(
        request(&server, "GET", "/api/v1/state", Some("wrong"), b"")
            .await
            .status,
        401
    );

    let state = request(&server, "GET", "/api/v1/state", Some(TOKEN), b"").await;
    assert_eq!(state.status, 200);
    assert_eq!(state.body["petId"], Uuid::from_u128(1).to_string());
    assert_eq!(state.body["inventory"]["kibble"], 50);
    assert_eq!(state.body["inventory"]["treat"], 25);
    assert_eq!(state.body["inventory"]["fruit"], 25);

    let oversized = vec![b'x'; 100 * 1024];
    let oversized_response =
        request(&server, "POST", "/api/v1/events", Some(TOKEN), &oversized).await;
    assert_eq!(oversized_response.status, 413);
    assert_eq!(oversized_response.body["error"]["code"], "body_too_large");

    let event_body = serde_json::to_vec(&EventIngestRequest::new(event(10))).unwrap();
    let first = request(&server, "POST", "/api/v1/events", Some(TOKEN), &event_body).await;
    assert_eq!(first.status, 200);
    assert_eq!(first.body["accepted"], true);
    assert_eq!(first.body["duplicate"], false);

    let duplicate = request(&server, "POST", "/api/v1/events", Some(TOKEN), &event_body).await;
    assert_eq!(duplicate.status, 200);
    assert_eq!(duplicate.body["accepted"], true);
    assert_eq!(duplicate.body["duplicate"], true);

    let invalid_feed = serde_json::json!({
        "actionId": Uuid::from_u128(20),
        "foodId": "poison"
    });
    let invalid_feed = request(
        &server,
        "POST",
        "/api/v1/care/feed",
        Some(TOKEN),
        &serde_json::to_vec(&invalid_feed).unwrap(),
    )
    .await;
    assert_eq!(invalid_feed.status, 422);
    assert_eq!(invalid_feed.body["error"]["code"], "unknown_food");

    let feed = serde_json::json!({
        "actionId": Uuid::from_u128(21),
        "foodId": "kibble"
    });
    let feed_body = serde_json::to_vec(&feed).unwrap();
    let fed = request(
        &server,
        "POST",
        "/api/v1/care/feed",
        Some(TOKEN),
        &feed_body,
    )
    .await;
    assert_eq!(fed.status, 200);
    assert_eq!(fed.body["duplicate"], false);
    let fed_again = request(
        &server,
        "POST",
        "/api/v1/care/feed",
        Some(TOKEN),
        &feed_body,
    )
    .await;
    assert_eq!(fed_again.status, 200);
    assert_eq!(fed_again.body["duplicate"], true);

    for id in 11..=20 {
        let response = request(
            &server,
            "POST",
            "/api/v1/events",
            Some(TOKEN),
            &serde_json::to_vec(&EventIngestRequest::new(work_event(id))).unwrap(),
        )
        .await;
        assert_eq!(response.status, 200);
    }
    for (action_id, food_id) in [(31_u128, "kibble"), (32, "kibble"), (33, "kibble")] {
        let response = request(
            &server,
            "POST",
            "/api/v1/care/feed",
            Some(TOKEN),
            &serde_json::to_vec(&serde_json::json!({
                "actionId": Uuid::from_u128(action_id),
                "foodId": food_id,
            }))
            .unwrap(),
        )
        .await;
        assert_eq!(response.status, 200);
        assert_eq!(response.body["duplicate"], false);
    }

    let missing_clean = serde_json::json!({
        "actionId": Uuid::from_u128(22),
        "poopId": Uuid::from_u128(999)
    });
    let missing_clean = request(
        &server,
        "POST",
        "/api/v1/care/clean",
        Some(TOKEN),
        &serde_json::to_vec(&missing_clean).unwrap(),
    )
    .await;
    assert_eq!(missing_clean.status, 422);
    assert_eq!(missing_clean.body["error"]["code"], "missing_poop");

    let poop_id = Uuid::new_v5(&Uuid::from_u128(1), b"poop:0");
    let clean = serde_json::json!({
        "actionId": Uuid::from_u128(23),
        "poopId": poop_id,
    });
    let clean_body = serde_json::to_vec(&clean).unwrap();
    let cleaned = request(
        &server,
        "POST",
        "/api/v1/care/clean",
        Some(TOKEN),
        &clean_body,
    )
    .await;
    assert_eq!(cleaned.status, 200);
    assert_eq!(cleaned.body["duplicate"], false);
    let cleaned_again = request(
        &server,
        "POST",
        "/api/v1/care/clean",
        Some(TOKEN),
        &clean_body,
    )
    .await;
    assert_eq!(cleaned_again.status, 200);
    assert_eq!(cleaned_again.body["duplicate"], true);

    assert_eq!(
        request(
            &server,
            "POST",
            "/api/v1/command",
            Some(TOKEN),
            b"{\"command\":\"touch /tmp/nope\"}",
        )
        .await
        .status,
        404
    );

    let wrong_method_state = request(&server, "POST", "/api/v1/state", Some(TOKEN), b"{}").await;
    assert_eq!(wrong_method_state.status, 405);
    assert_eq!(
        wrong_method_state.body["error"]["code"],
        "method_not_allowed"
    );
    let wrong_method_events = request(&server, "GET", "/api/v1/events", Some(TOKEN), b"").await;
    assert_eq!(wrong_method_events.status, 405);
    assert_eq!(
        wrong_method_events.body["error"]["code"],
        "method_not_allowed"
    );

    server.shutdown().await.unwrap();
}

#[test]
fn sqlite_reports_corruption_and_preserves_the_previous_row_when_a_save_fails() {
    let db = TestDatabase::new();
    let store = SqliteStore::open(&db.path).unwrap();
    let initial = codegotchi_cli::AuthoritativeRuntime::new(
        store.clone(),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .unwrap()
    .snapshot();

    let mut unsupported = initial.clone();
    unsupported.schema_version = 2;
    assert!(matches!(
        store.save(&unsupported),
        Err(codegotchi_cli::PersistenceError::UnsupportedSchemaVersion(
            2
        ))
    ));
    assert_eq!(store.load().unwrap().unwrap(), initial);

    let connection = Connection::open(&db.path).unwrap();
    connection
        .execute(
            "CREATE TRIGGER fail_snapshot_update BEFORE UPDATE ON simulation_snapshots
             BEGIN SELECT RAISE(ABORT, 'test rollback'); END;",
            [],
        )
        .unwrap();
    let mut changed = initial.clone();
    changed.name = "Changed".to_owned();
    assert!(matches!(
        store.save(&changed),
        Err(codegotchi_cli::PersistenceError::Sqlite(_))
    ));
    drop(connection);
    assert_eq!(store.load().unwrap().unwrap(), initial);

    let connection = Connection::open(&db.path).unwrap();
    connection
        .execute("DROP TRIGGER fail_snapshot_update", [])
        .unwrap();
    connection
        .execute(
            "UPDATE simulation_snapshots SET snapshot_json = 'not-json' WHERE repository_id = 'default'",
            [],
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        store.load(),
        Err(codegotchi_cli::PersistenceError::CorruptSnapshot(_))
    ));

    let connection = Connection::open(&db.path).unwrap();
    connection
        .execute(
            "UPDATE simulation_snapshots SET schema_version = 2, snapshot_json = '{}'
             WHERE repository_id = 'default'",
            [],
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        store.load(),
        Err(codegotchi_cli::PersistenceError::UnsupportedSchemaVersion(
            2
        ))
    ));
}

#[tokio::test]
async fn sqlite_reload_keeps_inventory_enforcement_and_replay_ids_without_reseeding() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    runtime
        .set_enforcement_mode(codegotchi_domain::EnforcementMode::Strict)
        .unwrap();
    runtime.apply_event(&event(100)).unwrap();
    runtime.feed(Uuid::from_u128(101), "kibble").unwrap();
    let before = runtime.snapshot();
    drop(runtime);

    let store = SqliteStore::open(&db.path).unwrap();
    let restored = AuthoritativeRuntime::new(
        store,
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .unwrap();
    assert_eq!(restored.snapshot(), before);
    assert_eq!(restored.snapshot().inventory.total(), 99);
    assert_eq!(
        restored.snapshot().enforcement_mode,
        codegotchi_domain::EnforcementMode::Strict
    );
    assert!(restored.apply_event(&event(100)).unwrap().duplicate);
    assert!(
        restored
            .feed(Uuid::from_u128(101), "kibble")
            .unwrap()
            .duplicate
    );
}

#[tokio::test]
async fn scheduled_maintenance_is_persisted_broadcast_and_shutdown_completes() {
    let db = TestDatabase::new();
    let initial_time = Utc.with_ymd_and_hms(2020, 1, 1, 12, 0, 0).unwrap();
    let runtime = AuthoritativeRuntime::new(
        SqliteStore::open(&db.path).unwrap(),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, initial_time),
    )
    .unwrap();
    let (initial, mut snapshots) = runtime.subscribe().unwrap();
    let (tick_sender, tick_receiver) = mpsc::unbounded_channel();

    assert_eq!(initial.last_updated_at, initial_time);
    assert!(matches!(snapshots.try_recv(), Err(TryRecvError::Empty)));

    let server = RunningServer::start_with_maintenance_trigger(runtime, TOKEN, tick_receiver)
        .await
        .unwrap();
    tick_sender.send(()).unwrap();
    let broadcast = tokio::time::timeout(std::time::Duration::from_secs(1), snapshots.recv())
        .await
        .expect("scheduled maintenance should broadcast within the bound")
        .unwrap();
    assert!(broadcast.last_updated_at > initial.last_updated_at);
    assert_eq!(
        SqliteStore::open(&db.path).unwrap().load().unwrap(),
        Some(broadcast.clone())
    );
    assert!(matches!(snapshots.try_recv(), Err(TryRecvError::Empty)));

    let address = server.local_addr();
    tokio::time::timeout(std::time::Duration::from_secs(1), server.shutdown())
        .await
        .expect("server shutdown should complete")
        .unwrap();
    assert!(
        tick_sender.send(()).is_err(),
        "shutdown must stop and drop the maintenance task"
    );
    assert!(TcpStream::connect(address).await.is_err());
}

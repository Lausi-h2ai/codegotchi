use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use codegotchi_cli::{AuthoritativeRuntime, EventIngestRequest, RunningServer, SqliteStore};
use codegotchi_domain::{AgentEvent, AgentEventKind, EventMetadata, EventSource, Pet, PetSpecies};
use rusqlite::Connection;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
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
    let body = serde_json::from_slice(&bytes[separator + 4..]).unwrap();
    HttpResponse { status, body }
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

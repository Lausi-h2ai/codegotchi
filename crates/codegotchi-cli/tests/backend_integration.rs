use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
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
        let path = std::env::temp_dir().join(format!("codegotchi-task-2-{suffix}.sqlite"));
        #[cfg(target_os = "macos")]
        eprintln!("backend integration test database path: {}", path.display());
        Self { path }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(self.path.with_extension("sqlite-wal"));
    }
}

#[test]
fn test_database_names_are_unique_when_created_concurrently() {
    const ATTEMPTS: usize = 512;
    let paths = Arc::new(Mutex::new(Vec::with_capacity(ATTEMPTS)));

    std::thread::scope(|scope| {
        for _ in 0..ATTEMPTS {
            let paths = Arc::clone(&paths);
            scope.spawn(move || {
                let database = TestDatabase::new();
                paths
                    .lock()
                    .expect("database path lock")
                    .push(database.path.clone());
            });
        }
    });

    let paths = paths.lock().expect("database path lock");
    let unique_paths: HashSet<_> = paths.iter().collect();
    assert_eq!(
        unique_paths.len(),
        paths.len(),
        "concurrent test databases must never share a path"
    );
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

struct RawHttpResponse {
    status: u16,
    content_length: Option<usize>,
    body_bytes: Vec<u8>,
}

async fn request(
    server: &RunningServer,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> HttpResponse {
    let response = raw_request(server, method, path, token, body).await;
    let body = serde_json::from_slice(&response.body_bytes).unwrap_or(Value::Null);
    HttpResponse {
        status: response.status,
        body,
    }
}

async fn raw_request(
    server: &RunningServer,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> RawHttpResponse {
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
    if !body.is_empty() {
        stream.write_all(body).await.unwrap();
    }
    let mut headers = Vec::new();
    let mut chunk = [0_u8; 1024];
    let separator = loop {
        let read = stream
            .read(&mut chunk)
            .await
            .unwrap_or_else(|error| panic!("read {method} {path} response headers: {error}"));
        assert!(read > 0, "connection closed before {method} {path} headers");
        headers.extend_from_slice(&chunk[..read]);
        if let Some(separator) = headers.windows(4).position(|window| window == b"\r\n\r\n") {
            break separator;
        }
    };
    let header_text = String::from_utf8_lossy(&headers[..separator]);
    let status = header_text
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let content_length = header_text
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        })
        .map(|(_, value)| value.trim().parse().unwrap());
    let mut body_bytes = headers[separator + 4..].to_vec();
    if let Some(content_length) = content_length {
        while body_bytes.len() < content_length {
            let read = stream.read(&mut chunk).await.unwrap_or_else(|error| {
                panic!(
                    "read {method} {path} response body after {} bytes: {error}",
                    body_bytes.len()
                )
            });
            assert!(
                read > 0,
                "connection closed with {}/{} {method} {path} body bytes",
                body_bytes.len(),
                content_length
            );
            body_bytes.extend_from_slice(&chunk[..read]);
        }
        body_bytes.truncate(content_length);
    } else {
        stream
            .read_to_end(&mut body_bytes)
            .await
            .unwrap_or_else(|error| {
                panic!("read {method} {path} EOF-delimited response body: {error}")
            });
    }
    RawHttpResponse {
        status,
        content_length,
        body_bytes,
    }
}

#[tokio::test]
async fn oversized_raw_http_response_is_completely_read_before_close() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime, TOKEN).await.unwrap();
    let oversized = vec![b'x'; 100 * 1024];
    let response = raw_request(&server, "POST", "/api/v1/events", Some(TOKEN), &oversized).await;
    assert_eq!(response.status, 413);
    assert!(response.content_length.is_some());
    server.shutdown().await.unwrap();
}

async fn debug_request(server: &RunningServer, token: &str, path: &str) -> HttpResponse {
    let mut stream = TcpStream::connect(server.local_addr()).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Authorization: Bearer {token}\r\n\
         x-codegotchi-debug: 1\r\n\
         Content-Type: application/json\r\n\
         Content-Length: 2\r\n\
         Connection: close\r\n\r\n{{}}"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
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

#[tokio::test]
async fn authenticated_name_route_updates_and_persists_the_pet_name() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();

    assert_eq!(
        request(&server, "POST", "/api/v1/name", None, br#"{"name":"Luna"}"#)
            .await
            .status,
        401
    );

    let response = request(
        &server,
        "POST",
        "/api/v1/name",
        Some(TOKEN),
        br#"{"name":"  Luna  "}"#,
    )
    .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body["name"], "Luna");
    assert!(!response.body["duplicate"].as_bool().unwrap());
    assert_eq!(runtime.snapshot().name, "Luna");
    assert_eq!(
        SqliteStore::open(&db.path)
            .unwrap()
            .load()
            .unwrap()
            .unwrap()
            .name,
        "Luna"
    );

    let invalid = request(
        &server,
        "POST",
        "/api/v1/name",
        Some(TOKEN),
        br#"{"name":"name\nwith newline"}"#,
    )
    .await;
    assert_eq!(invalid.status, 422);
    assert_eq!(invalid.body["error"]["code"], "invalid_name");

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
    assert_eq!(state.body["inventory"]["kibble"], u32::MAX);
    assert_eq!(state.body["inventory"]["treat"], u32::MAX);
    assert_eq!(state.body["inventory"]["fruit"], u32::MAX);
    assert_eq!(state.body["inventory"]["energy_drink"], u32::MAX);

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

    let energy_drink = serde_json::json!({
        "actionId": Uuid::from_u128(24),
        "foodId": "energy_drink"
    });
    let energy_drink_body = serde_json::to_vec(&energy_drink).unwrap();
    let drank = request(
        &server,
        "POST",
        "/api/v1/care/feed",
        Some(TOKEN),
        &energy_drink_body,
    )
    .await;
    assert_eq!(drank.status, 200);
    assert_eq!(drank.body["duplicate"], false);
    assert_eq!(drank.body["inventory"]["energy_drink"], u32::MAX);

    let nap = serde_json::json!({
        "actionId": Uuid::from_u128(25),
    });
    let nap_body = serde_json::to_vec(&nap).unwrap();
    let napped = request(&server, "POST", "/api/v1/care/nap", Some(TOKEN), &nap_body).await;
    assert_eq!(napped.status, 200);
    assert_eq!(napped.body["duplicate"], false);
    assert!(
        napped.body["nappingUntil"].is_string(),
        "nap must expose an authoritative nappingUntil deadline"
    );
    assert_eq!(napped.body["behavior"], "Sleeping");

    let napped_again = request(&server, "POST", "/api/v1/care/nap", Some(TOKEN), &nap_body).await;
    assert_eq!(napped_again.status, 200);
    assert_eq!(napped_again.body["duplicate"], true);

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

#[tokio::test]
async fn debug_neglect_drains_energy_and_a_nap_recovers_it_without_freezing_the_clock() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();

    let neglect = debug_request(&server, TOKEN, "/api/v1/debug/neglect").await;
    assert_eq!(neglect.status, 200);
    assert_eq!(neglect.body["needs"]["hunger"], 100.0);
    assert_eq!(neglect.body["needs"]["energy"], 0.0);
    assert_eq!(neglect.body["behavior"], "CriticalNeed");

    // The demo control must not jump the logical clock far into the future:
    // that froze every later maintenance tick until the wall clock caught up.
    let clock_jump = chrono::Utc::now()
        .signed_duration_since(runtime.snapshot().last_updated_at)
        .num_seconds()
        .abs();
    assert!(
        clock_jump < 60,
        "debug neglect advanced the simulation clock by {clock_jump} seconds"
    );

    let nap = serde_json::json!({
        "actionId": Uuid::from_u128(70),
    });
    let nap_body = serde_json::to_vec(&nap).unwrap();
    let napped = request(&server, "POST", "/api/v1/care/nap", Some(TOKEN), &nap_body).await;
    assert_eq!(napped.status, 200);
    assert!(
        napped.body["needs"]["energy"].as_f64().unwrap() < 1.0,
        "the nap must start from the drained meter"
    );

    // The authoritative maintenance loop ticks every second; the five-second
    // nap must refill the drained meter in real time.
    let started = std::time::Instant::now();
    let mut recovered = None;
    while started.elapsed() < std::time::Duration::from_secs(8) {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let state = request(&server, "GET", "/api/v1/state", Some(TOKEN), b"").await;
        // Wall-clock maintenance can land just after the five-second deadline:
        // the nap has completed when the authoritative deadline clears, while
        // the normal -50/hour energy decay may already have begun.
        if state.body["nappingUntil"].is_null() {
            recovered = Some(state);
            break;
        }
    }
    let recovered = recovered.expect("the nap completion deadline must clear within 8 seconds");
    assert_eq!(recovered.body["nappingUntil"], serde_json::Value::Null);
    assert!(
        recovered.body["needs"]["energy"].as_f64().unwrap() >= 99.0,
        "the completed nap must leave the energy meter near full: {:?}",
        recovered.body["needs"]["energy"]
    );

    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn debug_restock_restores_the_unlimited_inventory_and_persists() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();

    // Consume a few items through the normal care path first.
    for action in [30_u128, 31, 32, 33] {
        let feed = serde_json::json!({
            "actionId": Uuid::from_u128(action),
            "foodId": "kibble",
        });
        let fed = request(
            &server,
            "POST",
            "/api/v1/care/feed",
            Some(TOKEN),
            &serde_json::to_vec(&feed).unwrap(),
        )
        .await;
        assert_eq!(fed.status, 200);
        assert_eq!(fed.body["duplicate"], false);
    }
    let before = request(&server, "GET", "/api/v1/state", Some(TOKEN), b"").await;
    assert_eq!(before.status, 200);
    assert_eq!(before.body["inventory"]["kibble"], u32::MAX);

    let restock = debug_request(&server, TOKEN, "/api/v1/debug/restock").await;
    assert_eq!(restock.status, 200);
    assert_eq!(restock.body["inventory"]["kibble"], u32::MAX);
    assert_eq!(restock.body["inventory"]["treat"], u32::MAX);
    assert_eq!(restock.body["inventory"]["fruit"], u32::MAX);
    assert_eq!(restock.body["inventory"]["energy_drink"], u32::MAX);

    // The mutation persists through the store, not just in memory.
    let persisted = SqliteStore::open(&db.path)
        .unwrap()
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted
            .inventory
            .count(codegotchi_domain::FoodKind::Kibble),
        u32::MAX
    );
    assert_eq!(
        persisted
            .inventory
            .count(codegotchi_domain::FoodKind::EnergyDrink),
        u32::MAX
    );

    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn debug_restock_without_the_guard_header_is_forbidden() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);
    let server = RunningServer::start(runtime, TOKEN).await.unwrap();

    let response = request(&server, "POST", "/api/v1/debug/restock", Some(TOKEN), b"{}").await;
    assert_eq!(response.status, 403);
    assert_eq!(response.body["error"]["code"], "debug_disabled");

    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn debug_status_reflects_how_the_runtime_was_launched() {
    let db = TestDatabase::new();
    let runtime = runtime(&db);

    let plain = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();
    let plain_status = request(&plain, "GET", "/api/v1/debug/status", Some(TOKEN), b"").await;
    assert_eq!(plain_status.status, 200);
    assert_eq!(plain_status.body["debugEnabled"], false);
    plain.shutdown().await.unwrap();

    let enabled = RunningServer::start_with_debug(runtime, TOKEN)
        .await
        .unwrap();
    let enabled_status = request(&enabled, "GET", "/api/v1/debug/status", Some(TOKEN), b"").await;
    assert_eq!(enabled_status.status, 200);
    assert_eq!(enabled_status.body["debugEnabled"], true);

    enabled.shutdown().await.unwrap();
}

#[test]
fn legacy_snapshots_without_the_energy_care_fields_gain_ten_energy_drinks() {
    let db = TestDatabase::new();
    let store = SqliteStore::open(&db.path).unwrap();
    let fresh = codegotchi_cli::AuthoritativeRuntime::new(
        store.clone(),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .unwrap()
    .snapshot();

    // Simulate a snapshot persisted by the pre-energy binary: it has no
    // nappingUntil field and its inventory never received energy drinks.
    let mut legacy_json = serde_json::to_value(&fresh).unwrap();
    legacy_json.as_object_mut().unwrap().remove("nappingUntil");
    legacy_json["inventory"]
        .as_object_mut()
        .unwrap()
        .remove("energy_drink");
    let legacy_json = serde_json::to_string(&legacy_json).unwrap();

    let connection = Connection::open(&db.path).unwrap();
    connection
        .execute(
            "UPDATE simulation_snapshots SET schema_version = 1, snapshot_json = ?1
             WHERE repository_id = 'default'",
            [&legacy_json],
        )
        .unwrap();

    let loaded = store.load().unwrap().expect("legacy snapshot restores");
    assert_eq!(
        loaded
            .inventory
            .count(codegotchi_domain::FoodKind::EnergyDrink),
        10
    );
    assert_eq!(loaded.napping_until, None);
    assert_eq!(loaded.needs, fresh.needs);

    // A newer-format snapshot that consumed every drink is not refilled: the
    // inventory is authoritative and the migration only touches legacy rows.
    let mut spent_json = serde_json::to_value(&fresh).unwrap();
    spent_json["inventory"]
        .as_object_mut()
        .unwrap()
        .remove("energy_drink");
    let spent_json = serde_json::to_string(&spent_json).unwrap();
    connection
        .execute(
            "UPDATE simulation_snapshots SET snapshot_json = ?1",
            [&spent_json],
        )
        .unwrap();

    let spent = store.load().unwrap().expect("spent snapshot restores");
    assert_eq!(
        spent
            .inventory
            .count(codegotchi_domain::FoodKind::EnergyDrink),
        0
    );
}

#[test]
fn startup_rejects_invalid_persisted_snapshot_without_rewriting_inventory() {
    let db = TestDatabase::new();
    let store = SqliteStore::open(&db.path).unwrap();
    let initial = AuthoritativeRuntime::new(
        store,
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .unwrap()
    .snapshot();

    let mut invalid = serde_json::to_value(&initial).unwrap();
    invalid["name"] = serde_json::Value::String("🐱".repeat(33));
    invalid["inventory"] = serde_json::json!({
        "kibble": 3,
        "treat": 2,
        "fruit": 1,
        "energy_drink": 4,
    });
    let invalid_json = serde_json::to_string(&invalid).unwrap();

    let connection = Connection::open(&db.path).unwrap();
    connection
        .execute(
            "UPDATE simulation_snapshots SET snapshot_json = ?1
             WHERE repository_id = 'default'",
            [&invalid_json],
        )
        .unwrap();
    drop(connection);

    let result = AuthoritativeRuntime::new(
        SqliteStore::open(&db.path).unwrap(),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    );
    assert!(matches!(
        result,
        Err(codegotchi_cli::RuntimeError::Persistence(
            codegotchi_cli::PersistenceError::InvalidSnapshot(_)
        ))
    ));

    let connection = Connection::open(&db.path).unwrap();
    let stored_json: String = connection
        .query_row(
            "SELECT snapshot_json FROM simulation_snapshots
             WHERE repository_id = 'default'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_json, invalid_json);
    let stored: Value = serde_json::from_str(&stored_json).unwrap();
    assert_eq!(stored["inventory"]["kibble"], 3);
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
    assert_eq!(restored.snapshot().inventory.total(), u32::MAX);
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

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use codegotchi_cli::{AuthoritativeRuntime, RunningServer, SqliteStore};
use codegotchi_domain::{AgentEvent, AgentEventKind, EventMetadata, EventSource, Pet, PetSpecies};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use uuid::Uuid;

const TOKEN: &str = "task-2-ws-token";

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
            path: std::env::temp_dir().join(format!("codegotchi-task-2-ws-{suffix}.sqlite")),
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
        AgentEventKind::TurnStarted,
        None,
        start(),
        EventMetadata::default(),
    )
}

fn ws_url(server: &RunningServer) -> String {
    format!("ws://{}{}", server.local_addr(), "/api/v1/stream")
}

#[tokio::test]
async fn websocket_is_authenticated_and_reconnects_to_authoritative_snapshots() {
    let db = TestDatabase::new();
    let runtime = AuthoritativeRuntime::new(
        SqliteStore::open(&db.path).unwrap(),
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start()),
    )
    .unwrap();
    let server = RunningServer::start(runtime.clone(), TOKEN).await.unwrap();

    let mut unauthenticated = ws_url(&server).into_client_request().unwrap();
    unauthenticated
        .headers_mut()
        .insert("authorization", "Bearer wrong".parse().unwrap());
    assert!(connect_async(unauthenticated).await.is_err());

    let mut request = ws_url(&server).into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let initial = socket.next().await.unwrap().unwrap().into_text().unwrap();
    let initial: serde_json::Value = serde_json::from_str(&initial).unwrap();
    assert_eq!(initial["petId"], Uuid::from_u128(1).to_string());

    runtime.apply_event(&event(200)).unwrap();
    let mutation = socket.next().await.unwrap().unwrap().into_text().unwrap();
    let mutation: serde_json::Value = serde_json::from_str(&mutation).unwrap();
    assert!(
        mutation["processedEventIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == &Uuid::from_u128(200).to_string())
    );

    socket.send(Message::Close(None)).await.unwrap();
    let _ = socket.next().await;
    drop(socket);

    runtime.apply_event(&event(201)).unwrap();
    let mut reconnect = ws_url(&server).into_client_request().unwrap();
    reconnect
        .headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    let (mut socket, _) = connect_async(reconnect).await.unwrap();
    let authoritative = socket.next().await.unwrap().unwrap().into_text().unwrap();
    let authoritative: serde_json::Value = serde_json::from_str(&authoritative).unwrap();
    let ids = authoritative["processedEventIds"].as_array().unwrap();
    assert!(ids.iter().any(|id| id == &Uuid::from_u128(200).to_string()));
    assert!(ids.iter().any(|id| id == &Uuid::from_u128(201).to_string()));
    socket.close(None).await.unwrap();

    let mut browser_request = ws_url(&server).into_client_request().unwrap();
    browser_request
        .headers_mut()
        .insert("sec-websocket-protocol", TOKEN.parse().unwrap());
    let (mut browser_socket, response) = connect_async(browser_request).await.unwrap();
    assert_eq!(
        response.headers().get("sec-websocket-protocol").unwrap(),
        TOKEN
    );
    let browser_snapshot = browser_socket
        .next()
        .await
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap();
    let browser_snapshot: serde_json::Value = serde_json::from_str(&browser_snapshot).unwrap();
    assert_eq!(browser_snapshot["petId"], Uuid::from_u128(1).to_string());
    browser_socket.close(None).await.unwrap();

    server.shutdown().await.unwrap();
}

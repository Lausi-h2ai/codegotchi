use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use codegotchi_cli::runtime_metadata::{read_metadata, write_metadata};
use codegotchi_cli::{EventIngestRequest, RuntimeMetadataV1, translate_hook_json};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const EXPECTED_STRICT_REASON: &str = "The pet refuses this action because its hunger is critical. Feed the pet in the CodeGotchi UI, then retry the Codex request afterward.";

struct TestEnvironment {
    root: PathBuf,
    home: PathBuf,
    codex_home: PathBuf,
    state_home: PathBuf,
    runtime_home: PathBuf,
    worktree: PathBuf,
    log: PathBuf,
}

impl TestEnvironment {
    fn new(label: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codegotchi-task-6-{label}-{suffix}"));
        let home = root.join("home");
        let codex_home = root.join("codex-home");
        let state_home = root.join("state");
        let runtime_home = root.join("runtime");
        let worktree = root.join("worktree");
        let log = root.join("fake-codex.log");
        for path in [&home, &codex_home, &state_home, &runtime_home, &worktree] {
            fs::create_dir_all(path).expect("test directory creates");
        }
        Self {
            root,
            home,
            codex_home,
            state_home,
            runtime_home,
            worktree,
            log,
        }
    }

    fn state_database(&self) -> PathBuf {
        self.state_home.join("codegotchi/state.sqlite")
    }

    fn runtime_directory(&self) -> PathBuf {
        self.runtime_home.join("codegotchi")
    }
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct RunningLauncher {
    child: Option<Child>,
    stdout: Option<JoinHandle<String>>,
    release_path: PathBuf,
    metadata_path: PathBuf,
    metadata: RuntimeMetadataV1,
    profile_path: PathBuf,
}

impl RunningLauncher {
    fn stop(mut self) {
        fs::remove_file(&self.release_path).expect("fake Codex release file removes");
        let mut child = self.child.take().expect("launcher child is present");
        let status = child.wait().expect("launcher exits");
        assert!(status.success(), "launcher failed with {status}");
        self.stdout
            .take()
            .expect("launcher stdout drain is present")
            .join()
            .expect("launcher stdout drain joins");
    }
}

impl Drop for RunningLauncher {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = fs::remove_file(&self.release_path);
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(stdout) = self.stdout.take() {
            let _ = stdout.join();
        }
    }
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegotchi"))
}

fn fake_codex() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex.sh")
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hooks")
            .join(name),
    )
    .expect("hook fixture exists")
}

fn launch(environment: &TestEnvironment, label: &str) -> RunningLauncher {
    let control_directory = environment.root.join("controls");
    fs::create_dir_all(&control_directory).expect("control directory creates");
    let ready_path = control_directory.join(format!("{label}-ready"));
    let release_path = control_directory.join(format!("{label}-release"));
    let stdin_path = control_directory.join(format!("{label}-stdin"));
    let _ = fs::remove_file(&ready_path);
    fs::write(&release_path, b"hold").expect("fake Codex release file writes");

    let mut command = Command::new(binary());
    command
        .env_clear()
        .env("HOME", &environment.home)
        .env("CODEX_HOME", &environment.codex_home)
        .env("XDG_STATE_HOME", &environment.state_home)
        .env("XDG_RUNTIME_DIR", &environment.runtime_home)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("CODEGOTCHI_REAL_CODEX", fake_codex())
        .env("CODEGOTCHI_BROWSER", "none")
        .env("FAKE_CODEX_LOG", &environment.log)
        .env("FAKE_STDIN_FILE", &stdin_path)
        .env("FAKE_READY_FILE", &ready_path)
        .env("FAKE_RELEASE_FILE", &release_path)
        .current_dir(&environment.worktree)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .args(["run", "--", "codex"]);
    let mut child = command.spawn().expect("compiled launcher starts");
    drop(child.stdin.take());

    let stdout = child.stdout.take().expect("launcher stdout is piped");
    let mut reader = BufReader::new(stdout);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .expect("launcher prints its UI URL");
    let ui_url = first_line
        .strip_prefix("CodeGotchi UI: ")
        .expect("launcher output has the documented UI prefix")
        .trim()
        .to_owned();
    let stdout = thread::spawn(move || {
        let mut remainder = String::new();
        reader
            .read_to_string(&mut remainder)
            .expect("launcher stdout drains");
        remainder
    });

    wait_until("fake Codex ready", || ready_path.exists());
    let runtime_directory = environment.runtime_directory();
    let metadata_path = wait_for_metadata(&runtime_directory);
    let metadata = read_metadata(&metadata_path).expect("runtime metadata reads");
    assert_eq!(metadata.owning_pid, child.id());
    assert_eq!(
        ui_url,
        format!(
            "{}/#token={}",
            metadata.loopback_base_url, metadata.bearer_token
        )
    );
    assert!(ui_url.starts_with("http://127.0.0.1:"));

    let profile_path = wait_for_profile(&environment.codex_home);
    assert!(
        profile_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("codegotchi-") && name.ends_with(".config.toml"))
    );

    RunningLauncher {
        child: Some(child),
        stdout: Some(stdout),
        release_path,
        metadata_path,
        metadata,
        profile_path,
    }
}

fn wait_until(label: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_metadata(directory: &Path) -> PathBuf {
    let mut result = None;
    wait_until("runtime metadata", || {
        result = fs::read_dir(directory).ok().and_then(|entries| {
            entries.flatten().map(|entry| entry.path()).find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("session-") && name.ends_with(".json"))
            })
        });
        result.is_some()
    });
    result.expect("metadata path is discovered")
}

fn wait_for_profile(directory: &Path) -> PathBuf {
    let mut result = None;
    wait_until("temporary Codex profile", || {
        result = fs::read_dir(directory).ok().and_then(|entries| {
            entries.flatten().map(|entry| entry.path()).find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("codegotchi-") && name.ends_with(".config.toml")
                    })
            })
        });
        result.is_some()
    });
    result.expect("profile path is discovered")
}

fn owned_session_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("session-") && name.ends_with(".json"))
        })
        .collect()
}

fn owned_profile_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("codegotchi-") && name.ends_with(".config.toml")
                })
        })
        .collect()
}

fn invoke_hook(metadata_path: &Path, payload: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .env_clear()
        .env("CODEGOTCHI_SESSION_FILE", metadata_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("hook")
        .spawn()
        .expect("hook subprocess starts");
    child
        .stdin
        .take()
        .expect("hook stdin is piped")
        .write_all(payload)
        .expect("hook payload writes");
    child.wait_with_output().expect("hook subprocess exits")
}

fn invoke_cli(metadata_path: &Path, arguments: &[&str], debug_enabled: bool) -> Output {
    let mut command = Command::new(binary());
    command
        .env_clear()
        .env("CODEGOTCHI_SESSION_FILE", metadata_path)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if debug_enabled {
        command.env("CODEGOTCHI_ENABLE_DEBUG", "1");
    }
    command.output().expect("runtime CLI command exits")
}

fn socket_address(base_url: &str) -> SocketAddr {
    base_url
        .strip_prefix("http://")
        .expect("loopback URL uses HTTP")
        .parse()
        .expect("loopback URL has an address")
}

fn websocket_url(base_url: &str) -> String {
    format!("ws://{}/api/v1/stream", socket_address(base_url))
}

struct HttpResponse {
    status: u16,
    body: Value,
}

async fn request(
    base_url: &str,
    method: &str,
    path: &str,
    token: &str,
    body: &[u8],
) -> HttpResponse {
    let mut stream = TcpStream::connect(socket_address(base_url))
        .await
        .expect("authenticated HTTP connection opens");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("HTTP request headers write");
    stream
        .write_all(body)
        .await
        .expect("HTTP request body writes");
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .await
        .expect("HTTP response reads");
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP response has headers");
    let headers = String::from_utf8_lossy(&bytes[..separator]);
    let status = headers
        .lines()
        .next()
        .expect("HTTP status line exists")
        .split_whitespace()
        .nth(1)
        .expect("HTTP status exists")
        .parse()
        .expect("HTTP status parses");
    let body = serde_json::from_slice(&bytes[separator + 4..]).expect("JSON response body");
    HttpResponse { status, body }
}

async fn websocket_snapshot<S>(socket: &mut WebSocketStream<S>) -> Value
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let message = timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("WebSocket snapshot arrives before timeout")
        .expect("WebSocket remains open")
        .expect("WebSocket frame is valid");
    match message {
        Message::Text(text) => serde_json::from_str(&text).expect("WebSocket snapshot is JSON"),
        other => panic!("expected a text snapshot, got {other:?}"),
    }
}

async fn connect_websocket(
    metadata: &RuntimeMetadataV1,
) -> (
    WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    Value,
) {
    let mut request = websocket_url(&metadata.loopback_base_url)
        .into_client_request()
        .expect("WebSocket request builds");
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", metadata.bearer_token)
            .parse()
            .expect("authorization header is valid"),
    );
    let (mut socket, response) = connect_async(request)
        .await
        .expect("authenticated WebSocket connects");
    assert_eq!(response.status(), 101);
    let snapshot = websocket_snapshot(&mut socket).await;
    (socket, snapshot)
}

fn complete_snapshot(snapshot: &Value) {
    for key in [
        "schemaVersion",
        "petId",
        "name",
        "species",
        "needs",
        "behavior",
        "activity",
        "recentOutcome",
        "workPoints",
        "digestionPoints",
        "lastUpdatedAt",
        "pendingPoops",
        "inventory",
        "processedCareIds",
        "poopSequence",
        "sessionActivities",
        "processedEventIds",
        "lastActivityAt",
        "lastOutcomeAt",
        "consecutiveFailures",
        "enforcementMode",
    ] {
        assert!(snapshot.get(key).is_some(), "snapshot is missing {key}");
    }
}

fn snapshot_from_mutation_response(response: &Value) -> Value {
    let mut snapshot = response.clone();
    let duplicate = snapshot
        .as_object_mut()
        .expect("mutation response is an object")
        .remove("duplicate")
        .expect("mutation response has duplicate flag");
    assert!(duplicate.is_boolean(), "duplicate flag is boolean");
    complete_snapshot(&snapshot);
    snapshot
}

fn persisted_projection(snapshot: &Value) -> Value {
    json!({
        "petId": snapshot["petId"],
        "name": snapshot["name"],
        "needs": snapshot["needs"],
        "inventory": snapshot["inventory"],
        "pendingPoops": snapshot["pendingPoops"],
        "poopSequence": snapshot["poopSequence"],
        "workPoints": snapshot["workPoints"],
        "digestionPoints": snapshot["digestionPoints"],
        "enforcementMode": snapshot["enforcementMode"],
        "processedEventIds": snapshot["processedEventIds"],
        "processedCareIds": snapshot["processedCareIds"],
    })
}

fn contains_id(snapshot: &Value, key: &str, id: &str) -> bool {
    snapshot[key]
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value == id))
}

fn read_persisted_snapshot(database: &Path) -> String {
    let connection = rusqlite::Connection::open(database).expect("SQLite database opens");
    connection
        .query_row(
            "SELECT snapshot_json FROM simulation_snapshots LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("persisted snapshot exists")
}

fn assert_privacy_safe(serialized: &str, location: &str) {
    for (label, forbidden) in [
        ("prompt", "never-persist-this-prompt"),
        ("source", "secret-source-content"),
        ("command", "cargo test -p secret-project"),
        ("complete output", "sensitive-tool-output"),
    ] {
        assert!(
            !serialized.contains(forbidden),
            "{location} contains forbidden {label}"
        );
    }
}

fn assert_snapshot_privacy(state: &Value, database: &Path) {
    let serialized_state = serde_json::to_string(state).expect("state serializes");
    assert_privacy_safe(&serialized_state, "HTTP state");
    let persisted_state = read_persisted_snapshot(database);
    assert_privacy_safe(&persisted_state, "SQLite state");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn launcher_vertical_flow_persists_and_replays_across_restart() {
    let environment = TestEnvironment::new("restart");
    let first = launch(&environment, "first");
    assert!(first.profile_path.exists());
    assert!(first.metadata_path.exists());

    let initial_state = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(initial_state.status, 200);
    complete_snapshot(&initial_state.body);
    assert_eq!(initial_state.body["enforcementMode"], "decorative");

    let (mut socket, initial_stream_snapshot) = connect_websocket(&first.metadata).await;
    complete_snapshot(&initial_stream_snapshot);
    assert_eq!(
        initial_stream_snapshot["petId"],
        initial_state.body["petId"]
    );
    assert!(
        initial_stream_snapshot["processedEventIds"]
            .as_array()
            .expect("event replay set is an array")
            .is_empty()
    );

    let session_payload = fixture("session_start.json");
    let session_event = translate_hook_json(&session_payload, &first.metadata)
        .expect("installed SessionStart fixture translates");
    let session_event_id = session_event.id.to_string();
    let first_hook = invoke_hook(&first.metadata_path, &session_payload);
    assert!(first_hook.status.success());
    assert_eq!(first_hook.stdout, b"{}\n");
    let changed_stream_snapshot = websocket_snapshot(&mut socket).await;
    complete_snapshot(&changed_stream_snapshot);
    assert_ne!(changed_stream_snapshot, initial_stream_snapshot);
    assert!(contains_id(
        &changed_stream_snapshot,
        "processedEventIds",
        &session_event_id
    ));

    for fixture_name in [
        "user_prompt_submit.json",
        "bash_pre.json",
        "apply_patch_pre.json",
        "bash_post_success.json",
        "bash_post_failure.json",
        "apply_patch_post.json",
    ] {
        let payload = fixture(fixture_name);
        let event = translate_hook_json(&payload, &first.metadata)
            .expect("sensitive installed fixture translates");
        let output = invoke_hook(&first.metadata_path, &payload);
        assert!(
            output.status.success(),
            "installed fixture hook process failed: {fixture_name}"
        );
        assert_eq!(
            output.stdout, b"{}\n",
            "installed fixture was not allowed: {fixture_name}"
        );
        let snapshot = websocket_snapshot(&mut socket).await;
        complete_snapshot(&snapshot);
        assert!(contains_id(
            &snapshot,
            "processedEventIds",
            &event.id.to_string()
        ));
    }

    let sensitive_state = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(sensitive_state.status, 200);
    complete_snapshot(&sensitive_state.body);
    assert_snapshot_privacy(&sensitive_state.body, &environment.state_database());

    let after_event = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    let duplicate_hook = invoke_hook(&first.metadata_path, &session_payload);
    assert!(duplicate_hook.status.success());
    assert_eq!(duplicate_hook.stdout, b"{}\n");
    let after_duplicate_event = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(after_duplicate_event.body, after_event.body);

    let direct_duplicate = request(
        &first.metadata.loopback_base_url,
        "POST",
        "/api/v1/events",
        &first.metadata.bearer_token,
        &serde_json::to_vec(&EventIngestRequest::new(session_event)).expect("event serializes"),
    )
    .await;
    assert_eq!(direct_duplicate.status, 200);
    assert_eq!(direct_duplicate.body["duplicate"], true);

    let invalid_feed = request(
        &first.metadata.loopback_base_url,
        "POST",
        "/api/v1/care/feed",
        &first.metadata.bearer_token,
        &serde_json::to_vec(&json!({
            "actionId": "00000000-0000-0000-0000-000000000010",
            "foodId": "not-food"
        }))
        .expect("invalid care serializes"),
    )
    .await;
    assert_eq!(invalid_feed.status, 422);
    assert_eq!(invalid_feed.body["error"]["code"], "unknown_food");

    let feed_body = serde_json::to_vec(&json!({
        "actionId": "00000000-0000-0000-0000-000000000011",
        "foodId": "kibble"
    }))
    .expect("feed serializes");
    let fed = request(
        &first.metadata.loopback_base_url,
        "POST",
        "/api/v1/care/feed",
        &first.metadata.bearer_token,
        &feed_body,
    )
    .await;
    assert_eq!(fed.status, 200);
    assert_eq!(fed.body["duplicate"], false);
    let fed_snapshot = snapshot_from_mutation_response(&fed.body);
    let fed_duplicate = request(
        &first.metadata.loopback_base_url,
        "POST",
        "/api/v1/care/feed",
        &first.metadata.bearer_token,
        &feed_body,
    )
    .await;
    assert_eq!(fed_duplicate.status, 200);
    assert_eq!(fed_duplicate.body["duplicate"], true);
    assert_eq!(
        snapshot_from_mutation_response(&fed_duplicate.body),
        fed_snapshot
    );

    let debug_without_guard = invoke_cli(&first.metadata_path, &["debug", "generate-poop"], false);
    assert!(!debug_without_guard.status.success());
    assert!(String::from_utf8_lossy(&debug_without_guard.stderr).contains("disabled"));

    let debug_poop = invoke_cli(&first.metadata_path, &["debug", "generate-poop"], true);
    assert!(debug_poop.status.success());
    let after_poop = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    let poop_id = after_poop.body["pendingPoops"]
        .as_array()
        .and_then(|poops| poops.first())
        .and_then(|poop| poop.get("id"))
        .and_then(Value::as_str)
        .expect("guarded debug generation creates a poop")
        .to_owned();

    let restock_without_guard = invoke_cli(&first.metadata_path, &["debug", "restock"], false);
    assert!(!restock_without_guard.status.success());
    assert!(String::from_utf8_lossy(&restock_without_guard.stderr).contains("disabled"));

    let debug_restock = invoke_cli(&first.metadata_path, &["debug", "restock"], true);
    assert!(debug_restock.status.success());
    let after_restock = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(after_restock.body["inventory"]["kibble"], 50);
    assert_eq!(after_restock.body["inventory"]["treat"], 25);
    assert_eq!(after_restock.body["inventory"]["fruit"], 25);
    assert_eq!(after_restock.body["inventory"]["energy_drink"], 10);

    let clean_body = serde_json::to_vec(&json!({
        "actionId": "00000000-0000-0000-0000-000000000012",
        "poopId": poop_id
    }))
    .expect("clean serializes");
    let cleaned = request(
        &first.metadata.loopback_base_url,
        "POST",
        "/api/v1/care/clean",
        &first.metadata.bearer_token,
        &clean_body,
    )
    .await;
    assert_eq!(cleaned.status, 200);
    assert_eq!(cleaned.body["duplicate"], false);
    let cleaned_snapshot = snapshot_from_mutation_response(&cleaned.body);
    let cleaned_duplicate = request(
        &first.metadata.loopback_base_url,
        "POST",
        "/api/v1/care/clean",
        &first.metadata.bearer_token,
        &clean_body,
    )
    .await;
    assert_eq!(cleaned_duplicate.status, 200);
    assert_eq!(cleaned_duplicate.body["duplicate"], true);
    assert_eq!(
        snapshot_from_mutation_response(&cleaned_duplicate.body),
        cleaned_snapshot
    );

    let final_state = request(
        &first.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &first.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(final_state.status, 200);
    assert_eq!(final_state.body["pendingPoops"], json!([]));
    assert!(
        final_state.body["poopSequence"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(contains_id(
        &final_state.body,
        "processedEventIds",
        &session_event_id
    ));
    assert!(contains_id(
        &final_state.body,
        "processedCareIds",
        "00000000-0000-0000-0000-000000000011"
    ));
    assert!(contains_id(
        &final_state.body,
        "processedCareIds",
        "00000000-0000-0000-0000-000000000012"
    ));

    assert_snapshot_privacy(&final_state.body, &environment.state_database());
    let expected_projection = persisted_projection(&final_state.body);
    socket.close(None).await.expect("WebSocket closes");
    let first_profile_path = first.profile_path.clone();
    first.stop();
    assert!(!first_profile_path.exists());
    assert!(
        !owned_session_files(&environment.runtime_directory())
            .iter()
            .any(|path| path.exists())
    );
    assert!(owned_profile_files(&environment.codex_home).is_empty());
    assert!(environment.state_database().exists());

    let second = launch(&environment, "second");
    let restarted_state = request(
        &second.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &second.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(restarted_state.status, 200);
    complete_snapshot(&restarted_state.body);
    assert_eq!(
        persisted_projection(&restarted_state.body),
        expected_projection
    );
    assert_eq!(restarted_state.body["petId"], final_state.body["petId"]);
    assert_eq!(restarted_state.body["needs"], final_state.body["needs"]);
    assert_eq!(
        restarted_state.body["inventory"],
        final_state.body["inventory"]
    );
    assert_eq!(
        restarted_state.body["pendingPoops"],
        final_state.body["pendingPoops"]
    );
    assert_eq!(
        restarted_state.body["enforcementMode"],
        final_state.body["enforcementMode"]
    );
    assert_eq!(
        restarted_state.body["processedEventIds"],
        final_state.body["processedEventIds"]
    );
    assert_eq!(
        restarted_state.body["processedCareIds"],
        final_state.body["processedCareIds"]
    );
    second.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_flow_denies_cares_retries_and_fails_open_when_server_stops() {
    let environment = TestEnvironment::new("strict");
    let launcher = launch(&environment, "strict");

    let mode = invoke_cli(&launcher.metadata_path, &["mode", "strict"], false);
    assert!(mode.status.success());
    let neglect = invoke_cli(&launcher.metadata_path, &["debug", "neglect"], true);
    assert!(neglect.status.success());
    let neglected = request(
        &launcher.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &launcher.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(neglected.body["enforcementMode"], "strict");
    assert_eq!(neglected.body["needs"]["hunger"], 100.0);
    assert_eq!(neglected.body["needs"]["energy"], 0.0);

    let denial_payload = fixture("bash_pre.json");
    let denial_event = translate_hook_json(&denial_payload, &launcher.metadata)
        .expect("safe PreToolUse fixture translates");
    let denial = invoke_hook(&launcher.metadata_path, &denial_payload);
    assert!(denial.status.success());
    let expected_denial = format!(
        "{{\"hookSpecificOutput\":{{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"{EXPECTED_STRICT_REASON}\"}}}}\n"
    );
    assert_eq!(denial.stdout, expected_denial.as_bytes());
    assert!(String::from_utf8_lossy(&denial.stdout).contains("Feed the pet in the CodeGotchi UI"));
    assert!(String::from_utf8_lossy(&denial.stdout).contains("retry the Codex request afterward"));

    // Two kibble bring hunger from 100 down to 50, below the mild neglect
    // boundary, but debug neglect also drained energy, so the pet still
    // refuses until it gets time to rest.
    for (index, action_id) in [
        "00000000-0000-0000-0000-000000000021",
        "00000000-0000-0000-0000-000000000022",
    ]
    .into_iter()
    .enumerate()
    {
        let feed_body = serde_json::to_vec(&json!({
            "actionId": action_id,
            "foodId": "kibble"
        }))
        .expect("strict care serializes");
        let care = request(
            &launcher.metadata.loopback_base_url,
            "POST",
            "/api/v1/care/feed",
            &launcher.metadata.bearer_token,
            &feed_body,
        )
        .await;
        assert_eq!(care.status, 200);
        assert_eq!(care.body["duplicate"], false);
        if index == 1 {
            assert!(
                care.body["needs"]["hunger"].as_f64().unwrap_or(100.0) < 70.0,
                "two kibble must bring hunger below the mild boundary"
            );
        }
    }

    let exhausted_payload = String::from_utf8(denial_payload.clone())
        .expect("hook fixture is UTF-8")
        .replace("call-pre-bash", "call-pre-bash-exhausted")
        .into_bytes();
    let exhausted = invoke_hook(&launcher.metadata_path, &exhausted_payload);
    assert!(exhausted.status.success());
    assert!(
        String::from_utf8_lossy(&exhausted.stdout).contains("too exhausted to keep working"),
        "drained energy must keep strict mode refusing: {:?}",
        String::from_utf8_lossy(&exhausted.stdout)
    );

    // A hammock nap restores the drained meter in real time, then safe
    // development work is allowed again.
    let nap_body = serde_json::to_vec(&json!({
        "actionId": "00000000-0000-0000-0000-000000000030",
    }))
    .expect("nap care serializes");
    let napped = request(
        &launcher.metadata.loopback_base_url,
        "POST",
        "/api/v1/care/nap",
        &launcher.metadata.bearer_token,
        &nap_body,
    )
    .await;
    assert_eq!(napped.status, 200);
    let nap_deadline = Instant::now();
    let mut rested = napped;
    while rested.body["needs"]["energy"].as_f64().unwrap_or(0.0) < 100.0
        && nap_deadline.elapsed() < Duration::from_secs(8)
    {
        thread::sleep(Duration::from_millis(250));
        rested = request(
            &launcher.metadata.loopback_base_url,
            "GET",
            "/api/v1/state",
            &launcher.metadata.bearer_token,
            b"",
        )
        .await;
    }
    assert!(
        rested.body["needs"]["energy"].as_f64().unwrap_or(0.0) >= 100.0,
        "the hammock nap must restore the drained energy meter"
    );

    let retry_payload = String::from_utf8(denial_payload)
        .expect("hook fixture is UTF-8")
        .replace("call-pre-bash", "call-pre-bash-rested")
        .into_bytes();
    let retry_event = translate_hook_json(&retry_payload, &launcher.metadata)
        .expect("fresh tool-use fixture translates");
    assert_ne!(retry_event.id, denial_event.id);
    let denial_id = denial_event.id.to_string();
    let retry_id = retry_event.id.to_string();
    let before_retry = request(
        &launcher.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &launcher.metadata.bearer_token,
        b"",
    )
    .await;
    assert_eq!(before_retry.status, 200);
    complete_snapshot(&before_retry.body);
    assert!(contains_id(
        &before_retry.body,
        "processedEventIds",
        &denial_id
    ));
    assert!(!contains_id(
        &before_retry.body,
        "processedEventIds",
        &retry_id
    ));
    let retry = invoke_hook(&launcher.metadata_path, &retry_payload);
    assert!(retry.status.success());
    assert_eq!(retry.stdout, b"{}\n");
    let recovered = request(
        &launcher.metadata.loopback_base_url,
        "GET",
        "/api/v1/state",
        &launcher.metadata.bearer_token,
        b"",
    )
    .await;
    assert!(contains_id(
        &recovered.body,
        "processedEventIds",
        &denial_id
    ));
    assert!(contains_id(&recovered.body, "processedEventIds", &retry_id));
    assert_ne!(
        recovered.body["processedEventIds"],
        before_retry.body["processedEventIds"]
    );

    let stopped_metadata = launcher.metadata.clone();
    let stopped_metadata_path = environment.root.join("stopped-runtime.json");
    launcher.stop();
    assert!(!stopped_metadata_path.exists());
    let stopped_metadata = RuntimeMetadataV1::new(
        stopped_metadata.runtime_id,
        stopped_metadata.repository_root,
        stopped_metadata.loopback_base_url,
        stopped_metadata.bearer_token,
        std::process::id(),
    );
    write_metadata(&stopped_metadata_path, &stopped_metadata).expect("stopped metadata writes");
    let fail_open = invoke_hook(&stopped_metadata_path, &retry_payload);
    assert!(fail_open.status.success());
    assert_eq!(fail_open.stdout, b"{}\n");
}

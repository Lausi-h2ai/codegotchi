use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use codegotchi_cli::{AuthoritativeRuntime, RunningServer, SqliteStore};
use codegotchi_domain::{Pet, PetSpecies};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("codegotchi-task-5-assets-{label}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct HttpResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

async fn get(server: &RunningServer, path: &str, authorization: Option<&str>) -> HttpResponse {
    let mut stream = TcpStream::connect(server.local_addr()).await.unwrap();
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some(token) = authorization {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
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
    let content_type = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-type: ")
                .or_else(|| line.strip_prefix("Content-Type: "))
        })
        .unwrap_or_default()
        .to_owned();
    HttpResponse {
        status,
        content_type,
        body: bytes[separator + 4..].to_vec(),
    }
}

fn asset_path_from_reference(reference: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("web-dist")
        .join(reference.trim_start_matches('/'))
}

#[tokio::test]
async fn production_server_serves_embedded_bundle_spa_and_typed_api_errors() {
    let temp = TempDir::new("server");
    let runtime = AuthoritativeRuntime::new(
        SqliteStore::open(temp.path.join("state.sqlite")).unwrap(),
        Pet::new(
            Uuid::new_v4(),
            "Asset Test",
            PetSpecies::Cat,
            chrono::Utc::now(),
        ),
    )
    .unwrap();
    let server = RunningServer::start(runtime, "asset-test-token")
        .await
        .unwrap();

    let index_bytes = fs::read(asset_path_from_reference("/index.html")).unwrap();
    let index = get(&server, "/", None).await;
    assert_eq!(index.status, 200);
    assert_eq!(index.content_type, "text/html; charset=utf-8");
    assert_eq!(index.body, index_bytes);
    let index_text = String::from_utf8(index.body).unwrap();
    for reference in index_text
        .split(['"', '\''])
        .filter(|part| part.starts_with("/assets/"))
    {
        let response = get(&server, reference, None).await;
        assert_eq!(response.status, 200, "{reference}");
        let expected = fs::read(asset_path_from_reference(reference)).unwrap();
        assert_eq!(response.body, expected, "{reference}");
        if reference.ends_with(".js") {
            assert_eq!(
                response.content_type,
                "application/javascript; charset=utf-8"
            );
        } else if reference.ends_with(".css") {
            assert_eq!(response.content_type, "text/css; charset=utf-8");
        }
    }
    let spa = get(&server, "/room/does-not-exist", None).await;
    assert_eq!(spa.status, 200);
    assert_eq!(spa.body, index_bytes);

    let unknown_api = get(&server, "/api/v1/not-a-route", None).await;
    assert_eq!(unknown_api.status, 404);
    assert_eq!(unknown_api.content_type, "application/json");
    let error: serde_json::Value = serde_json::from_slice(&unknown_api.body).unwrap();
    assert_eq!(error["error"]["code"], "not_found");

    let unauthorized = get(&server, "/api/v1/state", None).await;
    assert_eq!(unauthorized.status, 401);
    let unauthorized_json: serde_json::Value = serde_json::from_slice(&unauthorized.body).unwrap();
    assert_eq!(unauthorized_json["error"]["code"], "unauthorized");

    let bundle = fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("web-dist/assets"))
        .unwrap()
        .flat_map(|entry| fs::read(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert!(!bundle.windows(14).any(|bytes| bytes == b"localhost:5173"));
    assert!(!bundle.windows(8).any(|bytes| bytes == b"/@vite/client"));
    server.shutdown().await.unwrap();
}

#[test]
fn installed_binary_serves_assets_without_repository_runtime_dependencies() {
    let temp = TempDir::new("install");
    let root = temp.path.join("install-root");
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let status = Command::new("cargo")
        .args([
            "install",
            "--path",
            "crates/codegotchi-cli",
            "--root",
            root.to_str().unwrap(),
            "--locked",
            "--offline",
        ])
        .current_dir(repository_root)
        .status()
        .expect("cargo install starts");
    assert!(status.success());
    let installed = root.join("bin/codegotchi");
    assert!(installed.is_file());
    assert!(!root.join("web-dist").exists());
    assert!(!root.join("pnpm").exists());

    let cwd = temp.path.join("outside-repository");
    fs::create_dir_all(&cwd).unwrap();
    let log = temp.path.join("installed.log");
    let input = temp.path.join("installed.stdin");
    let home = temp.path.join("home");
    let codex_home = temp.path.join("codex-home");
    let state = temp.path.join("state");
    let runtime = temp.path.join("runtime");
    for path in [&home, &codex_home, &state, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    let output = Command::new(&installed)
        .args(["run", "--", "codex"])
        .current_dir(&cwd)
        .env_clear()
        .env("HOME", home)
        .env("CODEX_HOME", codex_home)
        .env("XDG_STATE_HOME", state)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env(
            "CODEGOTCHI_REAL_CODEX",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex.sh"),
        )
        .env("CODEGOTCHI_BROWSER", "none")
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .output()
        .expect("installed launcher starts outside repository");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("CodeGotchi UI: http://127.0.0.1:"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("fake codex stdout"));
    assert!(log.is_file());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Vite"));
}

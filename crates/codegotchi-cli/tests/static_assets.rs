use std::fs;
use std::io::{BufRead, BufReader, Read as StdRead, Write as StdWrite};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use codegotchi_cli::{AuthoritativeRuntime, RunningServer, SqliteStore};
use codegotchi_domain::{Pet, PetSpecies};
use tokio::io::{AsyncReadExt as TokioReadExt, AsyncWriteExt as TokioWriteExt};
use tokio::net::TcpStream as TokioTcpStream;
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
    let mut stream = TokioTcpStream::connect(server.local_addr()).await.unwrap();
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

fn wait_for_file(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn installed_get(address: SocketAddr, path: &str) -> HttpResponse {
    let mut stream = TcpStream::connect(address).expect("installed server accepts HTTP");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
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

fn installed_ui_address(line: &str) -> SocketAddr {
    let url = line
        .trim()
        .strip_prefix("CodeGotchi UI: ")
        .expect("launcher prints a UI line");
    assert!(url.starts_with("http://127.0.0.1:"));
    assert!(
        url.contains("/#token="),
        "token must be in the URL fragment"
    );
    url.strip_prefix("http://")
        .unwrap()
        .split("/#")
        .next()
        .unwrap()
        .parse()
        .unwrap()
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

#[cfg(target_os = "linux")]
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
    let ready = temp.path.join("installed.ready");
    let release = temp.path.join("installed.release");
    let home = temp.path.join("home");
    let codex_home = temp.path.join("codex-home");
    let state = temp.path.join("state");
    let runtime = temp.path.join("runtime");
    for path in [&home, &codex_home, &state, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    fs::write(&release, b"").unwrap();
    let mut child = Command::new(&installed)
        .args(["run", "--", "codex"])
        .current_dir(&cwd)
        .env_clear()
        .env("HOME", home)
        .env("CODEX_HOME", &codex_home)
        .env("XDG_STATE_HOME", state)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env(
            "CODEGOTCHI_REAL_CODEX",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex.sh"),
        )
        .env("CODEGOTCHI_BROWSER", "none")
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("FAKE_READY_FILE", &ready)
        .env("FAKE_RELEASE_FILE", &release)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("installed launcher starts outside repository");
    let stdout = child.stdout.take().expect("installed stdout is piped");
    let mut stdout = BufReader::new(stdout);
    let mut ui_line = String::new();
    stdout.read_line(&mut ui_line).unwrap();
    let address = installed_ui_address(&ui_line);
    wait_for_file(&ready);
    let fake_pid: u32 = fs::read_to_string(&log)
        .expect("fake Codex log is readable")
        .lines()
        .find_map(|line| line.strip_prefix("PID\t"))
        .expect("fake Codex logs its PID")
        .parse()
        .unwrap();
    let parent_pid = String::from_utf8_lossy(
        &Command::new("ps")
            .args(["-o", "ppid=", "-p", &fake_pid.to_string()])
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .parse::<u32>()
    .unwrap();
    assert_eq!(
        fs::canonicalize(format!("/proc/{parent_pid}/exe")).unwrap(),
        installed.canonicalize().unwrap(),
        "the installed launcher owns the Codex child and embedded server"
    );

    let index_bytes = fs::read(asset_path_from_reference("/index.html")).unwrap();
    let index = installed_get(address, "/");
    assert_eq!(index.status, 200);
    assert_eq!(index.content_type, "text/html; charset=utf-8");
    assert_eq!(index.body, index_bytes);
    let index_text = String::from_utf8(index.body).unwrap();
    let reference = index_text
        .split(['"', '\''])
        .find(|part| part.starts_with("/assets/"))
        .expect("installed index references a hashed asset");
    let asset = installed_get(address, reference);
    assert_eq!(asset.status, 200);
    assert_eq!(
        asset.body,
        fs::read(asset_path_from_reference(reference)).unwrap()
    );
    if reference.ends_with(".js") {
        assert_eq!(asset.content_type, "application/javascript; charset=utf-8");
    } else if reference.ends_with(".css") {
        assert_eq!(asset.content_type, "text/css; charset=utf-8");
    } else {
        panic!("expected a hashed JS/CSS asset, got {reference}");
    }

    fs::remove_file(&release).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "installed launcher status: {status}");
    let mut remaining = String::new();
    stdout.read_to_string(&mut remaining).unwrap();
    assert!(remaining.contains("fake codex stdout"));
    assert!(log.is_file());
    assert!(!remaining.contains("Vite"));
    assert!(
        !runtime.join("codegotchi").read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("session-")
        })
    );
    assert!(codex_home.read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".config.toml")
    }));
}

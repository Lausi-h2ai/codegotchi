use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use codegotchi_cli::protocol::RuntimeMetadataV1;
use codegotchi_cli::runtime_metadata::{read_metadata, write_metadata};
use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("codegotchi-task-5-{label}-{suffix}"));
        fs::create_dir_all(&path).expect("temporary directory creates");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegotchi"))
}

fn fake_codex() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex.sh")
}

fn setup_command(temp: &TempDir, cwd: &Path) -> Command {
    let home = temp.join("home");
    let codex_home = temp.join("codex-home");
    for path in [&home, &codex_home] {
        fs::create_dir_all(path).expect("test directory creates");
    }
    let state_home = temp.join("state");
    let runtime_home = temp.join("runtime");

    let mut command = Command::new(binary());
    command
        .env_clear()
        .env("HOME", home)
        .env("CODEX_HOME", codex_home)
        .env("XDG_STATE_HOME", state_home)
        .env("XDG_RUNTIME_DIR", runtime_home)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("CODEGOTCHI_REAL_CODEX", fake_codex())
        .env("CODEGOTCHI_BROWSER", "none")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn log_fields(path: &Path, field: &str) -> Vec<String> {
    fs::read_to_string(path)
        .expect("fake Codex log exists")
        .lines()
        .filter_map(|line| line.strip_prefix(&format!("{field}\t")))
        .map(ToOwned::to_owned)
        .collect()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_text(path: &Path, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read_to_string(path)
            .ok()
            .is_some_and(|contents| contents.contains(expected))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} in {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn checksum(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).expect("checksum input exists")).to_vec()
}

fn state_database(temp: &TempDir) -> PathBuf {
    temp.join("state/codegotchi/state.sqlite")
}

fn read_snapshot(database: &Path) -> Value {
    let connection = Connection::open(database).expect("snapshot database opens");
    let snapshot: String = connection
        .query_row(
            "SELECT snapshot_json FROM simulation_snapshots LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("snapshot exists");
    serde_json::from_str(&snapshot).expect("snapshot JSON is valid")
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("fixture writes");
    let mut permissions = fs::metadata(path).expect("fixture metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).expect("fixture is executable");
}

fn run_validation_case(arguments: &[&str], codex: Option<&Path>) -> (TempDir, Output) {
    let temp = TempDir::new("validation");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).expect("validation cwd creates");
    let mut command = setup_command(&temp, &cwd);
    if let Some(codex) = codex {
        command.env("CODEGOTCHI_REAL_CODEX", codex);
    }
    command.args(arguments);
    let output = command.output().expect("validation launcher starts");
    (temp, output)
}

#[test]
fn exact_wrapper_is_transparent_and_persists_profile_but_cleans_metadata() {
    let temp = TempDir::new("transparent");
    let cwd = temp.join("worktree");
    fs::create_dir_all(&cwd).expect("working directory creates");
    let log = temp.join("codex.log");
    let input = temp.join("stdin.bin");
    let profile_copy = temp.join("profile-copy.toml");
    let metadata_copy = temp.join("metadata-copy.json");
    let mut command = setup_command(&temp, &cwd);
    command
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("FAKE_PROFILE_COPY", &profile_copy)
        .env("FAKE_METADATA_COPY", &metadata_copy)
        .env("FAKE_EXIT", "23")
        .args([
            "run",
            "--",
            "codex",
            "--color",
            "always",
            "value with spaces",
        ]);
    let mut child = command.spawn().expect("launcher starts");
    child
        .stdin
        .take()
        .expect("stdin is inherited by fake Codex")
        .write_all(b"stdin\x00bytes\n")
        .expect("stdin writes");
    let output = child.wait_with_output().expect("launcher exits");

    assert_eq!(output.status.code(), Some(23));
    assert!(output.stdout.windows(4).any(|bytes| bytes == b"\x1b[32"));
    assert!(output.stderr.windows(4).any(|bytes| bytes == b"\x1b[31"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("CodeGotchi UI: http://127.0.0.1:"));
    assert_eq!(
        fs::read(input).expect("fake stdin capture"),
        b"stdin\x00bytes\n"
    );

    let fields = log_fields(&log, "ARG");
    assert_eq!(fields.first().map(String::as_str), Some("--profile"));
    assert!(
        fields
            .get(1)
            .is_some_and(|name| name.starts_with("codegotchi-"))
    );
    assert_eq!(
        fields.get(2..),
        Some(
            [
                "--color".to_owned(),
                "always".to_owned(),
                "value with spaces".to_owned()
            ]
            .as_slice()
        )
    );
    assert_eq!(
        log_fields(&log, "CWD"),
        vec![cwd.to_string_lossy().into_owned()]
    );

    let metadata: RuntimeMetadataV1 =
        serde_json::from_slice(&fs::read(&metadata_copy).expect("metadata capture")).unwrap();
    assert!(metadata.bearer_token.len() >= 65);
    assert!(metadata.bearer_token.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            )
    }));
    assert!(!String::from_utf8_lossy(&fs::read(&metadata_copy).unwrap()).contains("prompt"));
    assert!(String::from_utf8_lossy(&fs::read(&profile_copy).unwrap()).contains(" hook"));
    let executable = binary().canonicalize().unwrap();
    assert!(
        String::from_utf8_lossy(&fs::read(&profile_copy).unwrap())
            .contains(executable.to_string_lossy().as_ref())
    );
    assert!(!fs::read_dir(temp.join("runtime")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("session-")
    }));
    assert!(
        temp.join("codex-home")
            .join(format!("{}.config.toml", fields[1]).as_str())
            .exists()
    );
}

#[test]
fn validation_rejects_malformed_shape_self_and_all_profile_conflicts_before_mutation() {
    let cases = [
        (vec!["run", "codex"], "separator `--`"),
        (vec!["run", "--", "claude"], "unsupported agent"),
        (vec!["run", "--", "codex", "-p"], "profile"),
        (vec!["run", "--", "codex", "--profile"], "profile"),
        (vec!["run", "--", "codex", "--profile=other"], "profile"),
        (vec!["run", "--", "codex", "-pother"], "profile"),
    ];
    for (arguments, expected) in cases {
        let (temp, output) = run_validation_case(&arguments, Some(&fake_codex()));
        assert!(
            !output.status.success(),
            "{arguments:?} unexpectedly succeeds"
        );
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.to_ascii_lowercase().contains(expected), "{error}");
        assert!(
            !temp.join("state").exists(),
            "validation mutated state: {arguments:?}"
        );
        assert!(
            !temp.join("runtime").exists(),
            "validation mutated runtime: {arguments:?}"
        );
        assert!(!temp.join("codex-home").read_dir().unwrap().next().is_some());
    }

    let (temp, output) = run_validation_case(
        &["run", "--", "codex"],
        Some(&binary().canonicalize().expect("launcher canonicalizes")),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("CodeGotchi executable"));
    assert!(!temp.join("state").exists());
}

#[test]
fn missing_and_non_executable_codex_are_rejected_without_runtime_files() {
    let missing = std::env::temp_dir().join(format!("codegotchi-missing-{}", Uuid::new_v4()));
    let (missing_temp, missing_output) =
        run_validation_case(&["run", "--", "codex"], Some(&missing));
    assert!(!missing_output.status.success());
    assert!(String::from_utf8_lossy(&missing_output.stderr).contains("not found"));
    assert!(!missing_temp.join("state").exists());

    let temp = TempDir::new("non-executable-codex");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(temp.join("home")).unwrap();
    fs::create_dir_all(temp.join("codex-home")).unwrap();
    fs::create_dir_all(temp.join("state")).unwrap();
    fs::create_dir_all(temp.join("runtime")).unwrap();
    let non_executable = temp.join("not-executable");
    fs::write(&non_executable, "#!/bin/sh\nexit 0\n").unwrap();
    let mut command = setup_command(&temp, &cwd);
    command.env("CODEGOTCHI_REAL_CODEX", &non_executable);
    let output = command.args(["run", "--", "codex"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("executable"));
}

#[test]
fn signal_handlers_are_installed_before_setup_can_publish_owned_files() {
    let temp = TempDir::new("setup-signal");
    let cwd = temp.join("cwd");
    let bin = temp.join("bin");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let git = bin.join("git");
    write_executable(
        &git,
        "#!/bin/sh
set -eu
touch \"$FAKE_GIT_READY\"
while [ -e \"$FAKE_GIT_RELEASE\" ]; do sleep 0.05; done
exit 1
",
    );
    let ready = temp.join("git-ready");
    let release = temp.join("git-release");
    fs::write(&release, b"").unwrap();
    let mut command = setup_command(&temp, &cwd);
    command
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("FAKE_GIT_READY", &ready)
        .env("FAKE_GIT_RELEASE", &release)
        .args(["run", "--", "codex"]);
    let mut child = command.spawn().unwrap();
    drop(child.stdin.take());
    wait_for(&ready);
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    fs::remove_file(&release).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(143), "{output:?}");
    assert!(!temp.join("state").exists());
    assert!(!temp.join("runtime").exists());
    assert!(!temp.join("codex-home").read_dir().unwrap().next().is_some());
}

#[test]
fn recursive_symlink_codex_and_browser_failure_are_nonfatal() {
    let temp = TempDir::new("symlink-browser");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let link_one = temp.join("codex-one");
    let link_two = temp.join("codex-two");
    symlink(fake_codex(), &link_one).unwrap();
    symlink(&link_one, &link_two).unwrap();
    symlink(&link_two, cwd.join("codex-relative")).unwrap();
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let mut command = setup_command(&temp, &cwd);
    command
        .env("CODEGOTCHI_REAL_CODEX", "./codex-relative")
        .env("CODEGOTCHI_BROWSER", temp.join("missing-browser"))
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .args(["run", "--", "codex"]);
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("browser"));
    assert!(log.exists());
}

#[test]
fn browser_helper_nonzero_exit_warns_without_delaying_codex() {
    let temp = TempDir::new("browser-nonzero");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let mut command = setup_command(&temp, &cwd);
    command
        .env("CODEGOTCHI_BROWSER", "/bin/false")
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .args(["run", "--", "codex"]);
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("browser"));
    assert!(log.exists());
}

#[test]
fn explicit_ui_modes_route_the_production_launcher_once() {
    let cases = [
        ("browser", true, true),
        ("terminal", false, false),
        ("both", true, false),
        ("auto", true, true),
    ];

    for (mode, expect_browser, expect_codex) in cases {
        let temp = TempDir::new(&format!("ui-{mode}"));
        let cwd = temp.join("cwd");
        fs::create_dir_all(&cwd).expect("working directory creates");
        let browser = temp.join("browser-helper");
        let browser_url = temp.join("browser-url");
        write_executable(
            &browser,
            "#!/bin/sh\nset -eu\nprintf '%s' \"$1\" >\"$FAKE_BROWSER_URL\"\n",
        );
        let log = temp.join("codex.log");
        let input = temp.join("stdin");
        let mut command = setup_command(&temp, &cwd);
        command
            .env("CODEGOTCHI_BROWSER", &browser)
            .env("FAKE_BROWSER_URL", &browser_url)
            .env("FAKE_CODEX_LOG", &log)
            .env("FAKE_STDIN_FILE", &input)
            .env("FAKE_EXIT", "17")
            .args(["run", "--ui", mode, "--", "codex"]);
        let output = command.output().expect("launcher starts");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if expect_codex {
            assert_eq!(output.status.code(), Some(17), "{mode}: {output:?}");
        } else {
            assert!(!output.status.success(), "{mode}: {stderr}");
        }
        assert_eq!(
            browser_url.exists(),
            expect_browser,
            "{mode}: browser route"
        );
        assert_eq!(log.exists(), expect_codex, "{mode}: Codex route");
        if expect_browser {
            assert!(
                stdout.contains("CodeGotchi UI: http://127.0.0.1:"),
                "{mode}: {stdout}"
            );
            let browser_url = fs::read_to_string(&browser_url).expect("browser URL capture");
            assert!(
                browser_url.starts_with("http://127.0.0.1:"),
                "{mode}: {browser_url}"
            );
        } else {
            assert!(!stdout.contains("CodeGotchi UI:"), "{mode}: {stdout}");
            assert!(
                !stdout.contains("#token="),
                "{mode}: bearer token leaked to stdout"
            );
            assert!(
                !stderr.contains("#token="),
                "{mode}: bearer token leaked to stderr"
            );
        }
        if expect_codex {
            assert_eq!(
                log_fields(&log, "PID").len(),
                1,
                "{mode}: exact one Codex spawn"
            );
        }

        let runtime = temp.join("runtime/codegotchi");
        assert!(
            !runtime.exists()
                || !runtime
                    .read_dir()
                    .expect("runtime directory reads")
                    .any(|entry| {
                        entry
                            .expect("runtime entry reads")
                            .file_name()
                            .to_string_lossy()
                            .starts_with("session-")
                    }),
            "{mode}: owned metadata survives cleanup"
        );
    }
}

#[test]
fn spawn_failure_cleans_metadata_but_persists_profile_and_keeps_database() {
    let temp = TempDir::new("spawn-failure");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let broken = temp.join("broken-codex");
    write_executable(
        &broken,
        "#!/definitely/not/a-real-codex-interpreter\nexit 1\n",
    );
    let mut command = setup_command(&temp, &cwd);
    command
        .env("CODEGOTCHI_REAL_CODEX", broken)
        .args(["run", "--", "codex"]);
    let output = command.output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not spawn"));
    assert!(state_database(&temp).exists());
    assert!(!temp.join("runtime").join("session").exists());
    assert!(temp.join("codex-home").read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".config.toml")
    }));
}

#[test]
fn base_config_and_credentials_are_byte_identical_after_run() {
    let temp = TempDir::new("config-preservation");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let codex_home = temp.join("codex-home");
    fs::create_dir_all(&codex_home).unwrap();
    let config = codex_home.join("config.toml");
    let credentials = codex_home.join("auth.json");
    fs::write(&config, b"model = \"preserve\"\n[hooks]\n").unwrap();
    fs::write(&credentials, b"{\"credential\":\"preserve\"}\n").unwrap();
    let config_before = checksum(&config);
    let credentials_before = checksum(&credentials);
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let mut command = setup_command(&temp, &cwd);
    command
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .args(["run", "--", "codex"]);
    command.output().unwrap();
    assert_eq!(checksum(&config), config_before);
    assert_eq!(checksum(&credentials), credentials_before);
    assert_eq!(
        codex_home
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() == "config.toml")
            .count(),
        1,
        "the user's base config remains the only config.toml after cleanup"
    );
}

#[test]
fn metadata_directory_modes_stale_cleanup_and_unrelated_files_are_safe() {
    let temp = TempDir::new("metadata");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let runtime = temp.join("runtime/codegotchi");
    fs::create_dir_all(&runtime).unwrap();
    let stale_id = Uuid::new_v4();
    let stale = runtime.join(format!("session-{stale_id}.json"));
    write_metadata(
        &stale,
        &RuntimeMetadataV1::new(
            stale_id,
            &cwd,
            "http://127.0.0.1:1",
            "stale-token",
            u32::MAX,
        ),
    )
    .unwrap();
    let unrelated_filename_id = Uuid::new_v4();
    let unrelated_runtime_id = Uuid::new_v4();
    let unrelated = runtime.join(format!("session-{unrelated_filename_id}.json"));
    write_metadata(
        &unrelated,
        &RuntimeMetadataV1::new(
            unrelated_runtime_id,
            &cwd,
            "http://127.0.0.1:1",
            "unrelated-token",
            u32::MAX,
        ),
    )
    .unwrap();
    let malformed = runtime.join("session-malformed.json");
    fs::write(&malformed, b"user-owned file").unwrap();
    let active_id = Uuid::new_v4();
    let active = runtime.join(format!("session-{active_id}.json"));
    write_metadata(
        &active,
        &RuntimeMetadataV1::new(
            active_id,
            &cwd,
            "http://127.0.0.1:1",
            "active-token",
            std::process::id(),
        ),
    )
    .unwrap();

    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let ready = temp.join("ready");
    let release = temp.join("release");
    fs::write(&release, b"").unwrap();
    let mut command = setup_command(&temp, &cwd);
    command
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("FAKE_READY_FILE", &ready)
        .env("FAKE_RELEASE_FILE", &release)
        .args(["run", "--", "codex"]);
    let mut child = command.spawn().unwrap();
    drop(child.stdin.take());
    wait_for(&ready);
    let owned = fs::read_dir(&runtime)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path != &stale && path != &unrelated && path != &malformed && path != &active)
        .expect("launcher session metadata exists");
    assert_eq!(
        fs::metadata(&runtime).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&owned).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let metadata = read_metadata(&owned).unwrap();
    let filename_id = owned
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("session-"))
        .and_then(|name| name.strip_suffix(".json"))
        .and_then(|name| Uuid::parse_str(name).ok())
        .expect("owned metadata filename contains a UUID");
    assert_eq!(metadata.runtime_id, filename_id);
    let token_parts: Vec<&str> = metadata.bearer_token.split('-').collect();
    assert_eq!(token_parts.len(), 2);
    assert!(
        token_parts
            .iter()
            .all(|part| part.len() == 32 && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
    );
    fs::remove_file(&release).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(!stale.exists());
    assert!(unrelated.exists());
    assert!(malformed.exists());
    assert!(active.exists());
    assert!(!owned.exists());
}

#[test]
fn signal_forwarding_preserves_conventional_status_and_cleans_up() {
    for (signal, expected_status, label) in [("INT", 130, "sigint"), ("TERM", 143, "sigterm")] {
        let temp = TempDir::new(label);
        let cwd = temp.join("cwd");
        fs::create_dir_all(&cwd).unwrap();
        let log = temp.join("codex.log");
        let input = temp.join("stdin");
        let ready = temp.join("signal-ready");
        let signal_log = temp.join("signals");
        let mut command = setup_command(&temp, &cwd);
        command
            .env("FAKE_CODEX_LOG", &log)
            .env("FAKE_STDIN_FILE", &input)
            .env("FAKE_SIGNAL_FILE", &ready)
            .env("FAKE_SIGNAL_LOG", &signal_log)
            .args(["run", "--", "codex"]);
        let mut child = command.spawn().unwrap();
        drop(child.stdin.take());
        wait_for(&ready);
        let status = Command::new("kill")
            .args([format!("-{signal}"), child.id().to_string()])
            .status()
            .expect("kill helper starts");
        assert!(status.success());
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(expected_status), "{output:?}");
        assert!(fs::read_to_string(&signal_log).unwrap().contains(signal));
        assert!(
            !temp
                .join("runtime/codegotchi")
                .read_dir()
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("session-"))
        );
        assert!(temp.join("codex-home").read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".config.toml")
        }));
    }
}

#[test]
fn sigwinch_is_forwarded_while_child_keeps_inherited_stdio() {
    let temp = TempDir::new("sigwinch");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let ready = temp.join("signal-ready");
    let signal_log = temp.join("signals");
    let mut command = setup_command(&temp, &cwd);
    command
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("FAKE_SIGNAL_FILE", &ready)
        .env("FAKE_SIGNAL_LOG", &signal_log)
        .args(["run", "--", "codex"]);
    let mut child = command.spawn().unwrap();
    drop(child.stdin.take());
    wait_for(&ready);
    assert!(
        Command::new("kill")
            .args(["-WINCH", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    wait_for_text(&signal_log, "SIGWINCH");
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(130));
    assert!(fs::read_to_string(signal_log).unwrap().contains("SIGWINCH"));
}

#[cfg(unix)]
fn process_group_id(pid: u32) -> i32 {
    process_group_info(pid).0
}

#[cfg(unix)]
fn process_group_info(pid: u32) -> (i32, i32) {
    let output = Command::new("ps")
        .args(["-o", "pgid=,tpgid=", "-p", &pid.to_string()])
        .output()
        .expect("ps starts");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut fields = stdout.split_whitespace();
    (
        fields
            .next()
            .expect("process group id exists")
            .parse()
            .expect("process group id parses"),
        fields
            .next()
            .expect("terminal process group id exists")
            .parse()
            .expect("terminal process group id parses"),
    )
}

#[cfg(unix)]
fn parent_process_id(pid: u32) -> u32 {
    let output = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .expect("ps starts");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("parent process id parses")
}

#[cfg(unix)]
fn signal_seen_before_deadline(path: &Path, expected: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if fs::read_to_string(path)
            .ok()
            .is_some_and(|contents| contents.lines().any(|line| line == expected))
        {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

#[cfg(unix)]
fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn run_outer_pty_route(
    temp: &TempDir,
    cwd: &Path,
    mode: &str,
    codex: &Path,
    browser: &Path,
    spawn_ledger: &Path,
    browser_log: &Path,
) -> Output {
    let home = temp.join("home");
    let codex_home = temp.join("codex-home");
    let state = temp.join("state");
    let runtime = temp.join("runtime");
    for path in [&home, &codex_home, &state, &runtime] {
        fs::create_dir_all(path).expect("PTY route test directory creates");
    }

    let command_line = format!(
        "CODEGOTCHI_OUTER_TTY=$(tty); export CODEGOTCHI_OUTER_TTY; stty rows 24 cols 80; exec {} run --ui {mode} -- codex",
        shell_quote_path(&binary())
    );
    Command::new("setsid")
        .args(["script", "-q", "-e", "-f", "-c", &command_line, "/dev/null"])
        .current_dir(cwd)
        .env_clear()
        .env("HOME", home)
        .env("CODEX_HOME", codex_home)
        .env("XDG_STATE_HOME", state)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("CODEGOTCHI_REAL_CODEX", codex)
        .env("CODEGOTCHI_BROWSER", browser)
        .env("FAKE_LAUNCHER_SPAWN_LEDGER", spawn_ledger)
        .env("FAKE_BROWSER_URL", browser_log)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("script creates a real outer PTY")
}

#[cfg(unix)]
fn assert_no_owned_metadata(temp: &TempDir, mode: &str) {
    let runtime = temp.join("runtime/codegotchi");
    assert!(
        !runtime.exists()
            || !runtime
                .read_dir()
                .expect("runtime directory reads")
                .any(|entry| {
                    entry
                        .expect("runtime entry reads")
                        .file_name()
                        .to_string_lossy()
                        .starts_with("session-")
                }),
        "{mode}: owned metadata survives cleanup"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires a real outer PTY; run with script and --ignored --test-threads=1"]
fn production_binary_successful_terminal_routes_use_one_pty_child_without_fallback() {
    let codex =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex-launcher-pty.sh");

    for mode in ["terminal", "both", "auto"] {
        let temp = TempDir::new(&format!("outer-pty-{mode}"));
        let cwd = temp.join("cwd");
        fs::create_dir_all(&cwd).expect("PTY route working directory creates");
        let browser_fixture = temp.join("browser-helper");
        write_executable(
            &browser_fixture,
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$1\" >>\"$FAKE_BROWSER_URL\"\n",
        );
        let spawn_ledger = temp.join("codex-spawn-ledger.log");
        let browser_log = temp.join("browser.log");
        let output = run_outer_pty_route(
            &temp,
            &cwd,
            mode,
            &codex,
            &browser_fixture,
            &spawn_ledger,
            &browser_log,
        );
        let transcript = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{mode}: production launcher failed: {transcript}"
        );
        let spawn_ledger_contents = fs::read_to_string(&spawn_ledger)
            .unwrap_or_else(|error| panic!("{mode}: spawn ledger missing: {error}; {transcript}"));
        let spawn_records: Vec<&str> = spawn_ledger_contents.lines().collect();
        assert_eq!(
            spawn_records.len(),
            1,
            "{mode}: expected one Codex spawn ledger record: {spawn_ledger_contents}"
        );
        let spawn_record = spawn_records[0];
        let field = |name: &str| {
            spawn_record
                .split('|')
                .find_map(|entry| entry.strip_prefix(name))
                .and_then(|value| value.strip_prefix('='))
                .unwrap_or_else(|| panic!("{mode}: spawn record missing {name}: {spawn_record}"))
        };
        assert!(
            field("pid").parse::<u32>().is_ok_and(|pid| pid > 0),
            "{mode}: spawn record PID is not positive: {spawn_record}"
        );
        assert_eq!(
            field("size"),
            "24 80",
            "{mode}: Codex PTY size was not the expected usable geometry: {spawn_record}"
        );
        let inner_tty = field("tty");
        let outer_tty = field("outer_tty");
        assert!(
            inner_tty.starts_with("/dev/"),
            "{mode}: Codex fixture did not report a concrete inner TTY: {spawn_record}"
        );
        assert!(
            outer_tty.starts_with("/dev/"),
            "{mode}: PTY harness did not export a concrete outer TTY: {spawn_record}"
        );
        assert_ne!(
            inner_tty, outer_tty,
            "{mode}: Codex fixture inherited the outer PTY instead of receiving a hosted PTY"
        );
        assert!(
            transcript.contains("FAKE_LAUNCHER_PTY_READY"),
            "{mode}: terminal host did not render the PTY child marker: {transcript}"
        );
        assert_eq!(
            fs::read_to_string(&browser_log)
                .ok()
                .map(|contents| contents.lines().count()),
            (mode == "both").then_some(1),
            "{mode}: browser launch count"
        );
        if mode != "both" {
            assert!(
                !transcript.contains("#token="),
                "{mode}: bearer token leaked into terminal-only output: {transcript}"
            );
        }
        assert_no_owned_metadata(&temp, mode);
    }
}

#[cfg(unix)]
#[test]
fn foreground_terminal_group_signal_is_not_forwarded_a_second_time() {
    let temp = TempDir::new("pty-signal");
    let cwd = temp.join("cwd");
    let home = temp.join("home");
    let codex_home = temp.join("codex-home");
    let state = temp.join("state");
    let runtime = temp.join("runtime");
    for path in [&cwd, &home, &codex_home, &state, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let ready = temp.join("signal-ready");
    let release = temp.join("signal-release");
    let signal_log = temp.join("signals");
    fs::write(&release, b"").unwrap();
    let command_line = format!("exec {} run -- codex", shell_quote_path(&binary()));
    let mut command = Command::new("setsid");
    command
        .args(["script", "-q", "-e", "-f", "-c", &command_line, "/dev/null"])
        .current_dir(&cwd)
        .env_clear()
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .env("XDG_STATE_HOME", &state)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("CODEGOTCHI_REAL_CODEX", fake_codex())
        .env("CODEGOTCHI_BROWSER", "none")
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("FAKE_SIGNAL_FILE", &ready)
        .env("FAKE_SIGNAL_LOG", &signal_log)
        .env("FAKE_SIGNAL_EXIT_ON_SIGNAL", "0")
        .env("FAKE_SIGNAL_RELEASE_FILE", &release)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("script creates a real PTY");
    wait_for(&ready);
    let launcher_pid = log_fields(&log, "PID")
        .first()
        .expect("fake Codex logs its PID")
        .parse()
        .expect("launcher PID parses");
    let process_group = process_group_id(launcher_pid);
    assert!(process_group > 0, "PTY process group must be positive");
    assert!(
        Command::new("kill")
            .args(["-INT", "--", &format!("-{process_group}")])
            .status()
            .unwrap()
            .success()
    );
    wait_for_text(&signal_log, "SIGINT");
    thread::sleep(Duration::from_millis(200));
    let count = fs::read_to_string(&signal_log)
        .unwrap()
        .lines()
        .filter(|line| *line == "SIGINT")
        .count();
    assert_eq!(count, 1, "the terminal group delivered SIGINT twice");
    fs::remove_file(&release).unwrap();
    let status = child.wait().unwrap();
    assert!(
        status.success() || status.code() == Some(130),
        "PTY launcher status: {status}"
    );
}

#[cfg(unix)]
#[test]
fn direct_wrapper_terminal_signals_are_forwarded_once_to_the_child() {
    let temp = TempDir::new("pty-direct-signal");
    let cwd = temp.join("cwd");
    let home = temp.join("home");
    let codex_home = temp.join("codex-home");
    let state = temp.join("state");
    let runtime = temp.join("runtime");
    for path in [&cwd, &home, &codex_home, &state, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let ready = temp.join("signal-ready");
    let release = temp.join("signal-release");
    let signal_log = temp.join("signals");
    fs::write(&release, b"").unwrap();
    let command_line = format!("exec {} run -- codex", shell_quote_path(&binary()));
    let mut command = Command::new("setsid");
    command
        .args(["script", "-q", "-e", "-f", "-c", &command_line, "/dev/null"])
        .current_dir(&cwd)
        .env_clear()
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .env("XDG_STATE_HOME", &state)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("CODEGOTCHI_REAL_CODEX", fake_codex())
        .env("CODEGOTCHI_BROWSER", "none")
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("FAKE_SIGNAL_FILE", &ready)
        .env("FAKE_SIGNAL_LOG", &signal_log)
        .env("FAKE_SIGNAL_RELEASE_FILE", &release)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("script creates a real PTY");
    wait_for(&ready);

    let fake_pid = log_fields(&log, "PID")
        .first()
        .expect("fake Codex logs its PID")
        .parse()
        .expect("fake Codex PID parses");
    let wrapper_pid = parent_process_id(fake_pid);
    let (wrapper_group, wrapper_foreground) = process_group_info(wrapper_pid);
    let (fake_group, fake_foreground) = process_group_info(fake_pid);
    assert_ne!(
        wrapper_group, fake_group,
        "Codex must run in a distinct process group"
    );
    assert_eq!(
        fake_group, wrapper_foreground,
        "the wrapper must observe Codex as the PTY foreground group"
    );
    assert_eq!(
        fake_group, fake_foreground,
        "Codex process group must own the PTY while it is interactive"
    );

    assert!(
        Command::new("kill")
            .args(["-WINCH", &wrapper_pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let saw_window_change = signal_seen_before_deadline(&signal_log, "SIGWINCH");

    assert!(
        Command::new("kill")
            .args(["-INT", &wrapper_pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let saw_interrupt = signal_seen_before_deadline(&signal_log, "SIGINT");
    let _ = fs::remove_file(&release);
    let status = child.wait().unwrap();
    let signals = fs::read_to_string(&signal_log).unwrap_or_default();
    assert!(
        saw_window_change,
        "direct SIGWINCH did not reach Codex: {signals}"
    );
    assert!(
        saw_interrupt,
        "direct SIGINT did not reach Codex: {signals}"
    );
    assert_eq!(
        signals.lines().filter(|line| *line == "SIGWINCH").count(),
        1,
        "direct SIGWINCH was delivered more than once: {signals}"
    );
    assert_eq!(
        signals.lines().filter(|line| *line == "SIGINT").count(),
        1,
        "direct SIGINT was delivered more than once: {signals}"
    );
    assert_eq!(status.code(), Some(130), "PTY launcher status: {status}");
}

#[cfg(unix)]
#[test]
fn terminal_foreground_is_restored_after_codex_exits() {
    let temp = TempDir::new("pty-restore");
    let cwd = temp.join("cwd");
    let home = temp.join("home");
    let codex_home = temp.join("codex-home");
    let state = temp.join("state");
    let runtime = temp.join("runtime");
    for path in [&cwd, &home, &codex_home, &state, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    let log = temp.join("codex.log");
    let input = temp.join("stdin");
    let restored = temp.join("restored-group");
    let command_line = format!(
        "{} run -- codex; status=$?; ps -o pgid=,tpgid= -p $$ > \"$RESTORED_GROUP\"; exit $status",
        shell_quote_path(&binary())
    );
    let output = Command::new("setsid")
        .args(["script", "-q", "-e", "-f", "-c", &command_line, "/dev/null"])
        .current_dir(&cwd)
        .env_clear()
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .env("XDG_STATE_HOME", &state)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("CODEGOTCHI_REAL_CODEX", fake_codex())
        .env("CODEGOTCHI_BROWSER", "none")
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .env("RESTORED_GROUP", &restored)
        .stdin(Stdio::null())
        .output()
        .expect("script creates a real PTY");
    assert!(
        output.status.success(),
        "PTY launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let values = fs::read_to_string(&restored).expect("shell recorded restored terminal group");
    let mut fields = values.split_whitespace();
    let shell_group: i32 = fields.next().unwrap().parse().unwrap();
    let terminal_group: i32 = fields.next().unwrap().parse().unwrap();
    assert!(shell_group > 0);
    assert_eq!(
        shell_group, terminal_group,
        "the caller's process group must own the PTY after CodeGotchi exits"
    );
}

#[test]
fn sqlite_state_is_preserved_and_reloaded_for_the_same_repository() {
    let temp = TempDir::new("persistence");
    let cwd = temp.join("repository");
    fs::create_dir_all(&cwd).unwrap();
    let first_log = temp.join("first.log");
    let first_input = temp.join("first.stdin");
    let mut first_command = setup_command(&temp, &cwd);
    first_command
        .env("FAKE_CODEX_LOG", &first_log)
        .env("FAKE_STDIN_FILE", &first_input)
        .env("CODEGOTCHI_ENABLE_DEBUG", "1")
        .env("CODEGOTCHI_BIN", binary())
        .env("FAKE_DEBUG_NEGLECT", "1")
        .args(["run", "--", "codex"]);
    assert!(first_command.output().unwrap().status.success());
    let first_snapshot = read_snapshot(&state_database(&temp));
    assert!(first_snapshot["needs"]["hunger"].as_f64().unwrap() > 0.0);

    let second_log = temp.join("second.log");
    let second_input = temp.join("second.stdin");
    let mut second_command = setup_command(&temp, &cwd);
    second_command
        .env("FAKE_CODEX_LOG", &second_log)
        .env("FAKE_STDIN_FILE", &second_input)
        .env("FAKE_DEBUG_NEGLECT", "0")
        .args(["run", "--", "codex"]);
    assert!(second_command.output().unwrap().status.success());
    let second_snapshot = read_snapshot(&state_database(&temp));
    assert_eq!(first_snapshot["petId"], second_snapshot["petId"]);
    assert_eq!(first_snapshot["needs"], second_snapshot["needs"]);
    assert_eq!(first_snapshot["inventory"], second_snapshot["inventory"]);
    assert_eq!(
        first_snapshot["processedEventIds"],
        second_snapshot["processedEventIds"]
    );

    let other = temp.join("other-repository");
    fs::create_dir_all(&other).unwrap();
    let log = temp.join("other.log");
    let input = temp.join("other.stdin");
    let mut command = setup_command(&temp, &other);
    command
        .env("FAKE_CODEX_LOG", &log)
        .env("FAKE_STDIN_FILE", &input)
        .args(["run", "--", "codex"]);
    assert!(command.output().unwrap().status.success());
    let database = Connection::open(state_database(&temp)).unwrap();
    let count: i64 = database
        .query_row("SELECT COUNT(*) FROM simulation_snapshots", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn failed_validation_does_not_even_create_an_additive_profile() {
    let temp = TempDir::new("profile-red");
    let cwd = temp.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let codex_home = temp.join("codex-home");
    fs::create_dir_all(&codex_home).unwrap();
    fs::write(codex_home.join("config.toml"), b"base").unwrap();
    let mut command = setup_command(&temp, &cwd);
    command.args(["run", "--", "codex", "--profile=bad"]);
    let output = command.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_dir(codex_home).unwrap().count(), 1);
}

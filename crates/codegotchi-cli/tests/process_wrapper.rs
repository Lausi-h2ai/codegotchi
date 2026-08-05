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
fn exact_wrapper_is_transparent_and_cleans_profile_and_metadata() {
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
        !temp
            .join("codex-home")
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
fn spawn_failure_cleans_owned_files_but_keeps_database() {
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
    assert!(!temp.join("codex-home").read_dir().unwrap().any(|entry| {
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
    let stale = runtime.join("session-stale.json");
    write_metadata(
        &stale,
        &RuntimeMetadataV1::new(
            Uuid::new_v4(),
            &cwd,
            "http://127.0.0.1:1",
            "stale-token",
            u32::MAX,
        ),
    )
    .unwrap();
    let unrelated = runtime.join("session-unrelated.json");
    fs::write(&unrelated, b"user-owned file").unwrap();
    let active = runtime.join("session-active.json");
    write_metadata(
        &active,
        &RuntimeMetadataV1::new(
            Uuid::new_v4(),
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
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("session-")
                        && name != "session-active.json"
                        && name != "session-stale.json"
                        && name != "session-unrelated.json"
                })
        })
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
        assert!(!temp.join("codex-home").read_dir().unwrap().any(|entry| {
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

#[test]
fn sqlite_state_is_preserved_and_reloaded_for_the_same_repository() {
    let temp = TempDir::new("persistence");
    let cwd = temp.join("repository");
    fs::create_dir_all(&cwd).unwrap();
    for first in [true, false] {
        let log = temp.join(if first { "first.log" } else { "second.log" });
        let input = temp.join(if first { "first.stdin" } else { "second.stdin" });
        let mut command = setup_command(&temp, &cwd);
        command
            .env("FAKE_CODEX_LOG", &log)
            .env("FAKE_STDIN_FILE", &input)
            .env("CODEGOTCHI_ENABLE_DEBUG", "1")
            .env("CODEGOTCHI_BIN", binary())
            .env("FAKE_DEBUG_NEGLECT", if first { "1" } else { "0" })
            .args(["run", "--", "codex"]);
        assert!(command.output().unwrap().status.success());
    }
    let database = Connection::open(state_database(&temp)).unwrap();
    let snapshots: Vec<Value> = database
        .prepare("SELECT snapshot_json FROM simulation_snapshots ORDER BY repository_id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str(&row.unwrap()).unwrap())
        .collect();
    assert_eq!(snapshots.len(), 1);
    assert!(snapshots[0]["needs"]["hunger"].as_f64().unwrap() > 0.0);
    let first_pet = snapshots[0]["petId"].clone();

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
    let count: i64 = database
        .query_row("SELECT COUNT(*) FROM simulation_snapshots", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 2);
    assert!(first_pet.is_string());
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

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use codegotchi_cli::{CodexInvocation, PtyCodexChild};

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codegotchi-terminal-pty-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create temporary directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn read_until<R: Read>(reader: &mut R, marker: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut byte = [0_u8; 1];
    while !output.windows(marker.len()).any(|window| window == marker) {
        reader.read_exact(&mut byte).expect("read PTY output");
        output.push(byte[0]);
    }
    output
}

#[test]
fn managed_pty_preserves_direct_invocation_input_resize_ansi_and_exit_code() {
    let temporary = TemporaryDirectory::new();
    let codex_home = temporary.path().join("codex home");
    let session_file = temporary.path().join("session metadata.json");
    fs::create_dir(&codex_home).expect("create CODEX_HOME");

    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex-pty.sh"),
        arguments: vec![
            "--literal".into(),
            "argument with spaces".into(),
            "semi;colon".into(),
        ],
        environment: vec![
            ("CODEX_HOME".into(), codex_home.as_os_str().into()),
            (
                "CODEGOTCHI_SESSION_FILE".into(),
                session_file.as_os_str().into(),
            ),
        ],
    };

    let mut child = PtyCodexChild::spawn(&invocation, 24, 80).expect("spawn fake Codex in PTY");
    let mut reader = child.reader().expect("clone PTY reader");
    let mut output = read_until(&mut reader, b"FAKE_CODEX_READY");
    child.resize(31, 120).expect("resize PTY");

    let mut writer = child.writer().expect("take PTY writer");
    writer
        .write_all(b"input delivered through pty\r\n")
        .expect("write PTY input");
    writer.flush().expect("flush PTY input");
    drop(writer);

    reader
        .read_to_end(&mut output)
        .expect("read PTY output to EOF");
    let status = child.wait().expect("wait for fake Codex");

    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("FAKE_CODEX_READY"));
    assert!(
        output
            .as_bytes()
            .windows(5)
            .any(|window| window == b"\x1b[31m")
    );
    assert!(
        output
            .as_bytes()
            .windows(3)
            .any(|window| window == b"\x1b[H")
    );
    assert!(output.contains("FAKE_CODEX_ARG[1]=<--literal>"));
    assert!(output.contains("FAKE_CODEX_ARG[2]=<argument with spaces>"));
    assert!(output.contains("FAKE_CODEX_ARG[3]=<semi;colon>"));
    assert!(output.contains(&format!("FAKE_CODEX_CODEX_HOME=<{}>", codex_home.display())));
    assert!(output.contains(&format!(
        "FAKE_CODEX_SESSION_FILE=<{}>",
        session_file.display()
    )));
    assert!(output.contains("FAKE_CODEX_INPUT=<input delivered through pty>"));
    assert!(output.contains("FAKE_CODEX_SIZE=<31 120>"));
    assert_eq!(status.exit_code(), 23);
}

#[cfg(unix)]
#[test]
fn managed_pty_preserves_sigint_and_sigterm_exit_statuses() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex-signal-pty.sh");

    let invocation = CodexInvocation {
        program: fixture.clone(),
        arguments: vec!["--interrupt".into()],
        environment: Vec::new(),
    };
    let mut interrupt_child =
        PtyCodexChild::spawn(&invocation, 24, 80).expect("spawn SIGINT fixture in PTY");
    let mut interrupt_reader = interrupt_child.reader().expect("clone SIGINT reader");
    read_until(&mut interrupt_reader, b"FAKE_SIGNAL_READY");
    interrupt_child
        .interrupt()
        .expect("deliver SIGINT to the PTY process group");
    let interrupt_status = interrupt_child.wait().expect("reap SIGINT fixture");
    assert_eq!(interrupt_status.exit_code(), 130);

    let invocation = CodexInvocation {
        program: fixture,
        arguments: vec!["--terminate".into()],
        environment: Vec::new(),
    };
    let mut terminate_child =
        PtyCodexChild::spawn(&invocation, 24, 80).expect("spawn SIGTERM fixture in PTY");
    let mut terminate_reader = terminate_child.reader().expect("clone SIGTERM reader");
    read_until(&mut terminate_reader, b"FAKE_SIGNAL_READY");
    terminate_child
        .terminate()
        .expect("deliver SIGTERM to the PTY process group");
    let terminate_status = terminate_child.wait().expect("reap SIGTERM fixture");
    assert_eq!(terminate_status.exit_code(), 143);
}

#[cfg(unix)]
#[test]
fn managed_pty_escalates_from_interrupt_to_terminate() {
    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-codex-signal-pty.sh"),
        arguments: vec!["--ignore-interrupt".into()],
        environment: Vec::new(),
    };
    let mut child = PtyCodexChild::spawn(&invocation, 24, 80).expect("spawn escalation fixture");
    let mut reader = child.reader().expect("clone escalation reader");
    read_until(&mut reader, b"FAKE_SIGNAL_READY");
    child.interrupt().expect("deliver first SIGINT");
    std::thread::sleep(std::time::Duration::from_millis(30));
    child.terminate().expect("deliver escalating SIGTERM");
    let status = child.wait().expect("reap escalation fixture");
    assert_eq!(status.exit_code(), 143);
}

#[cfg(unix)]
#[test]
fn dropping_managed_pty_kills_descendant_and_unblocks_reader() {
    let temporary = TemporaryDirectory::new();
    let descendant_file = temporary.path().join("descendant.pid");
    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-codex-descendant-pty.sh"),
        arguments: vec![descendant_file.as_os_str().into()],
        environment: Vec::new(),
    };
    let child = PtyCodexChild::spawn(&invocation, 24, 80).expect("spawn descendant fixture");
    let mut reader = child.reader().expect("clone descendant reader");
    let mut output = Vec::new();
    read_until(&mut reader, b"FAKE_DESCENDANT_READY");
    let reader_thread = std::thread::spawn(move || reader.read_to_end(&mut output));
    let descendant_pid = wait_for_pid(&descendant_file);
    drop(child);

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !reader_thread.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "blocked PTY reader did not complete after process-group teardown"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    reader_thread
        .join()
        .expect("reader join should not panic")
        .expect("reader should reach EOF after process-group cleanup");
    assert_eventually(Duration::from_secs(2), || !process_exists(descendant_pid));
}

#[cfg(unix)]
fn wait_for_pid(path: &Path) -> u32 {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(value) = fs::read_to_string(path)
            && let Ok(pid) = value.trim().parse()
        {
            return pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fixture did not publish PID"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(unix)]
fn assert_eventually(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + timeout;
    while !predicate() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(predicate(), "condition did not become true before timeout");
}

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

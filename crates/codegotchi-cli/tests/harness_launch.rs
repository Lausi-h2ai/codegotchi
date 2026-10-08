#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use codegotchi_cli::harness::Harness;
use serde_json::{Value, json};
use uuid::Uuid;

struct TemporaryDirectory(PathBuf);
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn all_new_harnesses_use_real_launch_and_hook_paths_in_browser_and_auto_fallback() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-harness.mjs");
    fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755)).unwrap();
    for harness in [
        Harness::Pi,
        Harness::Claude,
        Harness::OhMyPi,
        Harness::Hermes,
    ] {
        for ui in ["browser", "auto"] {
            let root = TemporaryDirectory(
                std::env::temp_dir().join(format!("codegotchi-harness-launch-{}", Uuid::new_v4())),
            );
            fs::create_dir(&root.0).unwrap();
            let report = root.0.join("report.json");
            // Invalid CODEX_HOME must have no effect on these harnesses.
            fs::write(root.0.join("codex-is-a-file"), b"untouched").unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_codegotchi"))
                .args([
                    "run",
                    "--ui",
                    ui,
                    "--",
                    harness.command(),
                    "-p",
                    "prompt with spaces",
                ])
                .env(harness.executable_override(), &fixture)
                .env("HOME", &root.0)
                .env("CODEX_HOME", root.0.join("codex-is-a-file"))
                .env("XDG_STATE_HOME", root.0.join("state"))
                .env("XDG_RUNTIME_DIR", root.0.join("runtime"))
                .env("CODEGOTCHI_BROWSER", "none")
                .env("CODEGOTCHI_ENABLE_DEBUG", "1")
                .env("CODEGOTCHI_ADAPTER_REPORT", &report)
                .env("CODEGOTCHI_ADAPTER_EXIT", "23")
                .current_dir(&root.0)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(23),
                "{} {ui}: {}",
                harness.command(),
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
            assert_eq!(report["arguments"], json!(["-p", "prompt with spaces"]));
            assert_eq!(report["strictBlocked"], true);
            assert_eq!(report["duringWork"], json!({"Active":"testing"}));
            assert_eq!(report["duringError"], "Failure");
            assert_eq!(
                fs::read(root.0.join("codex-is-a-file")).unwrap(),
                b"untouched"
            );
            assert!(!root.0.join(".codex").exists());
            assert_eq!(
                fs::read_dir(root.0.join("runtime/codegotchi"))
                    .unwrap()
                    .count(),
                0
            );
        }
    }
}

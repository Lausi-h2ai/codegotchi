use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use codegotchi_cli::TemporaryCodexProfile;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn temp_home() -> PathBuf {
    let path = std::env::temp_dir().join(format!("codegotchi-task-1-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn checksum(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn installed_binary_target_is_codegotchi_and_runs_the_generated_hook_command() {
    let binary = std::env::var_os("CARGO_BIN_EXE_codegotchi")
        .expect("the crate must install an explicit codegotchi binary");
    assert_eq!(
        Path::new(&binary)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("codegotchi")
    );

    let mut child = Command::new(&binary)
        .args(["hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the installed binary target must launch");
    child.stdin.as_mut().unwrap().write_all(b"{}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
}

#[test]
fn profile_is_additive_private_and_cleans_up_exactly_what_it_created() {
    let home = temp_home();
    let base = home.join("config.toml");
    let session_file = home.join("runtime-metadata.json");
    fs::write(
        &base,
        "model = \"preserve-me\"\n\n[hooks]\n[[hooks.SessionStart]]\ncommand = \"user-hook\"\n",
    )
    .unwrap();
    let before = checksum(&base);

    let profile =
        TemporaryCodexProfile::create(&home, "codegotchi-task-1", &session_file, "codegotchi hook")
            .unwrap();
    let profile_path = profile.config_path().to_path_buf();
    let content = fs::read_to_string(&profile_path).unwrap();

    assert_eq!(checksum(&base), before);
    assert_eq!(
        fs::metadata(&profile_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for event in [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "Stop",
    ] {
        assert_eq!(
            content.matches(&format!("[[hooks.{event}]]")).count(),
            1,
            "{event}"
        );
    }
    assert!(content.contains("command = \"codegotchi hook\""));

    let command = profile.codex_command("codex");
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "CODEGOTCHI_SESSION_FILE"
                && value == Some(session_file.as_os_str()))
    );
    assert!(command.get_args().any(|arg| arg == "--profile"));
    assert!(command.get_args().any(|arg| arg == profile.profile_name()));

    drop(profile);
    assert!(!profile_path.exists());
    assert_eq!(checksum(&base), before);
    assert!(session_file.parent().unwrap().exists());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn conflicting_profile_name_refuses_to_overwrite_an_existing_file() {
    let home = temp_home();
    let session_file = home.join("runtime-metadata.json");
    let existing = home.join("conflict.config.toml");
    fs::write(&existing, "user-owned = true\n").unwrap();
    let before = fs::read(&existing).unwrap();

    let error = TemporaryCodexProfile::create(&home, "conflict", &session_file, "codegotchi hook")
        .expect_err("conflicting profile must refuse creation");
    assert!(error.to_string().contains("already exists"));
    assert_eq!(fs::read(&existing).unwrap(), before);
    fs::remove_dir_all(home).unwrap();
}

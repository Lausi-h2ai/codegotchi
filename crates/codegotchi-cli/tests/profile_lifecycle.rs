use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

use codegotchi_cli::PersistentCodexProfile;
use codegotchi_cli::codex_profile::CodexProfileError;
use nix::fcntl::{Flock, FlockArg};
use nix::unistd::Uid;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn temp_home() -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("codegotchi-persistent-profile-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn checksum(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).unwrap()).to_vec()
}

fn temporary_profile_artifacts(home: &Path) -> Vec<PathBuf> {
    fs::read_dir(home)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name();
            name.to_string_lossy()
                .starts_with(".codegotchi-")
                .then_some(entry.path())
        })
        .collect()
}

fn ensure(home: &Path, hook_command: &str) -> PersistentCodexProfile {
    PersistentCodexProfile::ensure(home, home.join("runtime-metadata.json"), hook_command).unwrap()
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
fn profile_name_is_stable_and_exact_reuse_does_not_rewrite() {
    let home = temp_home();
    let first = ensure(&home, "codegotchi hook");
    let path = first.config_path().to_path_buf();
    let bytes = fs::read(&path).unwrap();
    let inode = fs::metadata(&path).unwrap().ino();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    drop(first);

    let second = ensure(&home, "codegotchi hook");
    assert_eq!(second.config_path(), path);
    assert_eq!(
        second.profile_name(),
        format!(
            "codegotchi-{}",
            Uuid::new_v5(&Uuid::NAMESPACE_URL, &bytes).simple()
        )
    );
    assert_eq!(
        second.profile_name(),
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_suffix(".config.toml")
            .unwrap()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn profile_is_additive_and_preserves_base_config_and_unrelated_hooks() {
    let home = temp_home();
    let base = home.join("config.toml");
    fs::write(
        &base,
        "model = \"preserve-me\"\n\n[hooks]\n[[hooks.SessionStart]]\ncommand = \"user-hook\"\n",
    )
    .unwrap();
    let before = checksum(&base);

    let profile = ensure(&home, "codegotchi hook");
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
    assert!(content.contains("[[hooks.SessionStart.hooks]]"));
    drop(profile);
    assert!(
        profile_path.exists(),
        "persistent profiles survive owner drop"
    );
    assert_eq!(checksum(&base), before);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn concurrent_ensures_converge_on_one_verified_profile() {
    let home = temp_home();
    let home = Arc::new(home);
    let barrier = Arc::new(Barrier::new(8));
    thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            let home = Arc::clone(&home);
            let barrier = Arc::clone(&barrier);
            workers.push(scope.spawn(move || {
                barrier.wait();
                let profile = PersistentCodexProfile::ensure(
                    home.as_ref(),
                    home.join("runtime-metadata.json"),
                    "codegotchi hook",
                )
                .unwrap();
                (
                    profile.profile_name().to_owned(),
                    profile.config_path().to_path_buf(),
                )
            }));
        }
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(fs::read_dir(home.as_ref()).unwrap().count(), 1);
    });
    fs::remove_dir_all(home.as_ref()).unwrap();
}

#[test]
fn profile_guard_serializes_creation_until_command_spawn_boundary() {
    let home = temp_home();
    let profile = ensure(&home, "codegotchi hook");
    let worker_home = home.clone();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (name_tx, name_rx) = mpsc::channel();
    let profile_guard = profile.acquire_spawn_guard().unwrap();
    let worker = thread::spawn(move || {
        let directory = File::open(&worker_home).unwrap();
        let directory = match Flock::lock(directory, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => {
                attempt_tx.send(false).unwrap();
                drop(lock);
                File::open(&worker_home).unwrap()
            }
            Err((directory, error)) => {
                assert!(
                    error == nix::errno::Errno::EAGAIN || error == nix::errno::Errno::EWOULDBLOCK,
                    "unexpected nonblocking profile lock error: {error}"
                );
                attempt_tx.send(true).unwrap();
                directory
            }
        };
        let directory_lock = Flock::lock(directory, FlockArg::LockExclusive).unwrap();
        drop(directory_lock);
        let profile = PersistentCodexProfile::ensure(
            &worker_home,
            worker_home.join("runtime-metadata.json"),
            "codegotchi changed hook",
        )
        .unwrap();
        name_tx.send(profile.profile_name().to_owned()).unwrap();
    });

    assert!(
        attempt_rx.recv().unwrap(),
        "the competing writer must observe the spawn guard before it calls ensure"
    );
    let mut command = profile_guard.codex_command("true");
    profile_guard.verify_before_spawn().unwrap();
    let mut child = profile_guard
        .spawn(&mut command)
        .expect("the guarded command actually spawns");
    assert!(
        name_rx.try_recv().is_err(),
        "a cooperating CodeGotchi writer must remain blocked until spawn returns and the guard drops"
    );
    assert!(child.wait().unwrap().success());
    drop(profile_guard);
    drop(profile);

    let changed_name = name_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("writer proceeds once the spawn-boundary guard is released");
    worker.join().unwrap();
    assert!(changed_name.starts_with("codegotchi-"));
    assert_eq!(fs::read_dir(&home).unwrap().count(), 2);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn delayed_creator_holds_directory_lock_until_permissions_and_bytes_are_complete() {
    let home = temp_home();
    let initial = ensure(&home, "codegotchi hook");
    let path = initial.config_path().to_path_buf();
    let expected = fs::read(&path).unwrap();
    drop(initial);
    fs::remove_file(&path).unwrap();

    let creator_home = home.clone();
    let creator_path = path.clone();
    let creator_expected = expected.clone();
    let (partial_tx, partial_rx) = mpsc::sync_channel(0);
    let (complete_tx, complete_rx) = mpsc::sync_channel(0);
    let creator = thread::spawn(move || {
        let directory = File::open(&creator_home).unwrap();
        let directory_lock = Flock::lock(directory, FlockArg::LockExclusive).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&creator_path)
            .unwrap();
        let split = creator_expected.len() / 2;
        file.write_all(&creator_expected[..split]).unwrap();
        partial_tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(650));
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions).unwrap();
        file.write_all(&creator_expected[split..]).unwrap();
        file.sync_all().unwrap();
        drop(directory_lock);
        complete_tx.send(()).unwrap();
    });

    partial_rx.recv().unwrap();
    let result = PersistentCodexProfile::ensure(
        &home,
        home.join("runtime-metadata.json"),
        "codegotchi hook",
    );
    assert!(
        result.is_ok(),
        "concurrent delayed creator must converge: {result:?}"
    );
    complete_rx.recv().unwrap();
    creator.join().unwrap();
    assert_eq!(fs::read(&path).unwrap(), expected);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn altered_profile_content_is_rejected_without_overwrite() {
    let home = temp_home();
    let profile = ensure(&home, "codegotchi hook");
    let path = profile.config_path().to_path_buf();
    drop(profile);
    fs::write(&path, b"altered = true\n").unwrap();
    let error = PersistentCodexProfile::ensure(
        &home,
        home.join("runtime-metadata.json"),
        "codegotchi hook",
    )
    .expect_err("altered profile must not be reused");
    assert!(error.to_string().contains("contents"));
    assert_eq!(fs::read(&path).unwrap(), b"altered = true\n");
    assert!(temporary_profile_artifacts(&home).is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn unsafe_permissions_are_rejected_without_overwrite() {
    let home = temp_home();
    let profile = ensure(&home, "codegotchi hook");
    let path = profile.config_path().to_path_buf();
    drop(profile);
    let original = fs::read(&path).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&path, permissions).unwrap();

    let error = PersistentCodexProfile::ensure(
        &home,
        home.join("runtime-metadata.json"),
        "codegotchi hook",
    )
    .expect_err("unsafe profile permissions must be rejected");
    assert!(error.to_string().contains("private") || error.to_string().contains("permission"));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(temporary_profile_artifacts(&home).is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn unreadable_existing_profile_is_rejected_without_overwrite_when_not_root() {
    if Uid::effective().is_root() {
        return;
    }
    let home = temp_home();
    let profile = ensure(&home, "codegotchi hook");
    let path = profile.config_path().to_path_buf();
    let original = fs::read(&path).unwrap();
    drop(profile);

    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&path, permissions).unwrap();
    let error = PersistentCodexProfile::ensure(
        &home,
        home.join("runtime-metadata.json"),
        "codegotchi hook",
    )
    .expect_err("unreadable profile path must be rejected");
    let mut restored_permissions = fs::metadata(&path).unwrap().permissions();
    restored_permissions.set_mode(0o600);
    fs::set_permissions(&path, restored_permissions).unwrap();

    assert!(matches!(error, CodexProfileError::Read { .. }), "{error}");
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn directory_and_symlink_collisions_are_rejected_without_mutation() {
    let home = temp_home();
    let expected = ensure(&home, "codegotchi hook");
    let path = expected.config_path().to_path_buf();
    let name = expected.profile_name().to_owned();
    drop(expected);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let directory_error = PersistentCodexProfile::ensure(
        &home,
        home.join("runtime-metadata.json"),
        "codegotchi hook",
    )
    .expect_err("directory collision must be rejected");
    assert!(
        directory_error.to_string().contains("regular")
            || directory_error.to_string().contains("directory")
    );
    fs::remove_dir(&path).unwrap();

    let target = home.join("user-owned.toml");
    fs::write(&target, b"user-owned = true\n").unwrap();
    let symlink_path = home.join(format!("{name}.config.toml"));
    symlink(&target, &symlink_path).unwrap();
    let symlink_error = PersistentCodexProfile::ensure(
        &home,
        home.join("runtime-metadata.json"),
        "codegotchi hook",
    )
    .expect_err("symlink collision must be rejected");
    assert!(
        symlink_error.to_string().contains("symlink")
            || symlink_error.to_string().contains("regular")
    );
    assert_eq!(fs::read(&target).unwrap(), b"user-owned = true\n");
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn changed_rendered_hook_bytes_get_a_new_identity_and_leave_old_profile_untouched() {
    let home = temp_home();
    let first = ensure(&home, "codegotchi hook");
    let first_path = first.config_path().to_path_buf();
    let first_bytes = fs::read(&first_path).unwrap();
    let second = ensure(&home, "codegotchi hook --changed");
    assert_ne!(first.profile_name(), second.profile_name());
    assert_ne!(first.config_path(), second.config_path());
    assert_eq!(fs::read(&first_path).unwrap(), first_bytes);
    assert!(second.config_path().exists());
    assert_eq!(fs::read_dir(&home).unwrap().count(), 2);
    fs::remove_dir_all(home).unwrap();
}

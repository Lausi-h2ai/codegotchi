use std::ffi::{OsStr, OsString};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use nix::errno::Errno;
use nix::fcntl::{AtFlags, Flock, FlockArg, OFlag, openat};
use nix::sys::stat::Mode;
use nix::unistd::{UnlinkatFlags, linkat, unlinkat};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const HOOK_EVENTS: [&str; 6] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
];
const PROFILE_PREFIX: &str = "codegotchi-";
const MANAGED_TRUST_EVENTS: [(&str, &str); 6] = [
    ("PreToolUse", "pre_tool_use"),
    ("PostToolUse", "post_tool_use"),
    ("SessionStart", "session_start"),
    ("SessionEnd", "session_end"),
    ("UserPromptSubmit", "user_prompt_submit"),
    ("Stop", "stop"),
];
const MANAGED_PROFILE_PREFIX: &[u8] = b"approvals_reviewer = \"auto_review\"\n";

#[derive(Debug, Error)]
pub enum CodexProfileError {
    #[error("Codex home is not a directory: {0}")]
    InvalidHome(PathBuf),
    #[error("Codex hook command is empty")]
    EmptyHookCommand,
    #[error("could not create Codex profile: {0}")]
    Create(#[source] std::io::Error),
    #[error("could not read Codex profile {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write Codex profile: {0}")]
    Write(#[source] std::io::Error),
    #[error("Codex profile path is a symlink: {0}")]
    Symlink(PathBuf),
    #[error("Codex profile path is not a regular file: {0}")]
    NotRegularFile(PathBuf),
    #[error("Codex profile path changed while it was being verified: {0}")]
    PathChanged(PathBuf),
    #[error("existing Codex profile is not private mode 0600: {path} (mode {mode:04o})")]
    UnsafePermissions { path: PathBuf, mode: u32 },
    #[error("existing Codex profile contents differ from the rendered hooks: {0}")]
    ContentMismatch(PathBuf),
}

/// A persistent, content-addressed additive Codex profile file.
///
/// The profile is shared by launches that render the same hook configuration.
/// It is never removed when this value is dropped; runtime metadata remains the
/// per-launch state that is cleaned up by the launcher.
#[derive(Debug)]
pub struct PersistentCodexProfile {
    codex_home: PathBuf,
    profile_name: String,
    config_path: PathBuf,
    session_file: PathBuf,
    hook_command: String,
    content: Vec<u8>,
}

/// A cooperative directory lock held from final profile validation through
/// Codex child spawn. It prevents other CodeGotchi writers from replacing a
/// profile during that handoff; non-cooperating or privileged writers remain
/// outside this guarantee because Codex opens by profile name rather than fd.
#[derive(Debug)]
pub struct PersistentCodexProfileGuard<'a> {
    profile: &'a PersistentCodexProfile,
    directory_lock: Flock<File>,
}

impl PersistentCodexProfile {
    /// Renders the complete hook configuration, derives a stable profile name
    /// from those exact bytes, and creates or safely reuses that profile.
    pub fn ensure(
        codex_home: impl AsRef<Path>,
        session_file: impl AsRef<Path>,
        hook_command: &str,
    ) -> Result<Self, CodexProfileError> {
        let codex_home = codex_home.as_ref().to_path_buf();
        if !codex_home.is_dir() {
            return Err(CodexProfileError::InvalidHome(codex_home));
        }
        if hook_command.trim().is_empty() {
            return Err(CodexProfileError::EmptyHookCommand);
        }

        let content = render_profile(hook_command);
        let profile_name = profile_name_for(&content);
        let config_path = codex_home.join(format!("{profile_name}.config.toml"));
        ensure_profile_file(&codex_home, &config_path, &content, hook_command)?;

        Ok(Self {
            codex_home,
            profile_name,
            config_path,
            session_file: session_file.as_ref().to_path_buf(),
            hook_command: hook_command.to_owned(),
            content,
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn session_file(&self) -> &Path {
        &self.session_file
    }

    /// Acquires the cooperative directory guard and verifies this exact
    /// profile. Keep the returned guard alive through command construction and
    /// child spawn.
    pub fn acquire_spawn_guard(
        &self,
    ) -> Result<PersistentCodexProfileGuard<'_>, CodexProfileError> {
        let directory_lock = lock_profile_directory(&self.codex_home)?;
        let file_name = self.config_path.file_name().ok_or_else(|| {
            CodexProfileError::Create(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "profile path has no file name",
            ))
        })?;
        verify_existing_file(
            &directory_lock,
            file_name,
            &self.config_path,
            &self.content,
            &self.hook_command,
        )?;
        Ok(PersistentCodexProfileGuard {
            profile: self,
            directory_lock,
        })
    }

    pub fn codex_command(&self, codex_program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(codex_program);
        command
            .arg("--profile")
            .arg(&self.profile_name)
            .env("CODEX_HOME", &self.codex_home)
            .env("CODEGOTCHI_SESSION_FILE", &self.session_file);
        command
    }
}

impl PersistentCodexProfileGuard<'_> {
    /// Revalidates the profile immediately before spawning Codex while this
    /// guard still excludes cooperating CodeGotchi writers.
    pub fn verify_before_spawn(&self) -> Result<(), CodexProfileError> {
        let file_name = self.profile.config_path.file_name().ok_or_else(|| {
            CodexProfileError::Create(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "profile path has no file name",
            ))
        })?;
        verify_existing_file(
            &self.directory_lock,
            file_name,
            &self.profile.config_path,
            &self.profile.content,
            &self.profile.hook_command,
        )
    }

    pub fn codex_command(&self, codex_program: impl AsRef<OsStr>) -> Command {
        self.profile.codex_command(codex_program)
    }

    /// Spawns a prepared Codex command while this cooperative directory guard
    /// is still held. The caller may drop the guard immediately after this
    /// method returns.
    pub fn spawn(&self, command: &mut Command) -> std::io::Result<Child> {
        command.spawn()
    }
}

fn profile_name_for(content: &[u8]) -> String {
    format!(
        "{PROFILE_PREFIX}{}",
        Uuid::new_v5(&Uuid::NAMESPACE_URL, content).simple()
    )
}

fn ensure_profile_file(
    codex_home: &Path,
    path: &Path,
    content: &[u8],
    hook_command: &str,
) -> Result<(), CodexProfileError> {
    ensure_profile_file_with_observer(codex_home, path, content, hook_command, |_| {})
}

fn ensure_profile_file_with_observer<F>(
    codex_home: &Path,
    path: &Path,
    content: &[u8],
    hook_command: &str,
    after_temporary_file_is_ready: F,
) -> Result<(), CodexProfileError>
where
    F: FnOnce(&OsStr),
{
    let directory_lock = lock_profile_directory(codex_home)?;
    let file_name = path.file_name().ok_or_else(|| {
        CodexProfileError::Create(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "profile path has no file name",
        ))
    })?;

    let mut temporary_file = create_temporary_file(&directory_lock, file_name)?;
    if let Err(error) = protect_created_file(&temporary_file.file) {
        return Err(CodexProfileError::Create(error));
    }
    if let Err(error) = temporary_file.file.write_all(content) {
        return Err(CodexProfileError::Write(error));
    }
    if let Err(error) = temporary_file.file.sync_all() {
        return Err(CodexProfileError::Write(error));
    }

    after_temporary_file_is_ready(&temporary_file.name);
    match linkat(
        &*directory_lock,
        temporary_file.name.as_os_str(),
        &*directory_lock,
        file_name,
        AtFlags::empty(),
    ) {
        Ok(()) => {
            temporary_file.remove_now();
            if let Err(error) = sync_profile_directory(&directory_lock) {
                return Err(CodexProfileError::Create(error));
            }
            let result =
                verify_existing_file(&directory_lock, file_name, path, content, hook_command);
            drop(temporary_file);
            result
        }
        Err(Errno::EEXIST) => {
            drop(temporary_file);
            verify_existing_file(&directory_lock, file_name, path, content, hook_command)
        }
        Err(error) => {
            drop(temporary_file);
            Err(CodexProfileError::Create(error.into()))
        }
    }
}

fn create_temporary_file<'a>(
    directory_lock: &'a Flock<File>,
    final_name: &OsStr,
) -> Result<TemporaryProfileFile<'a>, CodexProfileError> {
    for _ in 0..8 {
        let temporary_name = OsString::from(format!(
            ".{}.tmp-{}",
            final_name.to_string_lossy(),
            Uuid::new_v4().simple()
        ));
        match openat(
            &**directory_lock,
            temporary_name.as_os_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
            Mode::from_bits_truncate(0o600),
        ) {
            Ok(file_descriptor) => {
                return Ok(TemporaryProfileFile {
                    directory_lock,
                    name: temporary_name,
                    file: File::from(file_descriptor),
                    cleanup: true,
                });
            }
            Err(Errno::EEXIST) => {}
            Err(Errno::ELOOP) => {
                return Err(CodexProfileError::Symlink(PathBuf::from(temporary_name)));
            }
            Err(error) => return Err(CodexProfileError::Create(error.into())),
        }
    }
    Err(CodexProfileError::Create(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary Codex profile path",
    )))
}

struct TemporaryProfileFile<'a> {
    directory_lock: &'a Flock<File>,
    name: OsString,
    file: File,
    cleanup: bool,
}

impl TemporaryProfileFile<'_> {
    fn remove_now(&mut self) {
        if remove_temporary_file(self.directory_lock, &self.name, &self.file) {
            self.cleanup = false;
        }
    }
}

impl Drop for TemporaryProfileFile<'_> {
    fn drop(&mut self) {
        if self.cleanup {
            self.remove_now();
        }
    }
}

fn remove_temporary_file(
    directory_lock: &Flock<File>,
    temporary_name: &OsStr,
    file: &File,
) -> bool {
    let Ok(path_descriptor) = openat(
        &**directory_lock,
        temporary_name,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) else {
        return false;
    };
    let path_file = File::from(path_descriptor);
    let Ok(path_metadata) = path_file.metadata() else {
        return false;
    };
    let Ok(file_metadata) = file.metadata() else {
        return false;
    };
    if !path_metadata.file_type().is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return false;
    }
    unlinkat(
        &**directory_lock,
        temporary_name,
        UnlinkatFlags::NoRemoveDir,
    )
    .is_ok()
}

fn sync_profile_directory(directory_lock: &Flock<File>) -> std::io::Result<()> {
    directory_lock.sync_all()
}

fn lock_profile_directory(codex_home: &Path) -> Result<Flock<File>, CodexProfileError> {
    let directory = File::open(codex_home).map_err(CodexProfileError::Create)?;
    Flock::lock(directory, FlockArg::LockExclusive)
        .map_err(|(_, error)| CodexProfileError::Create(error.into()))
}

fn protect_created_file(file: &File) -> std::io::Result<()> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)
}

fn verify_existing_file(
    directory_lock: &Flock<File>,
    file_name: &OsStr,
    path: &Path,
    expected: &[u8],
    hook_command: &str,
) -> Result<(), CodexProfileError> {
    let file_descriptor = openat(
        &**directory_lock,
        file_name,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| {
        if error == Errno::ELOOP {
            CodexProfileError::Symlink(path.to_path_buf())
        } else {
            CodexProfileError::Read {
                path: path.to_path_buf(),
                source: error.into(),
            }
        }
    })?;
    let mut file = File::from(file_descriptor);
    let metadata = file.metadata().map_err(|source| CodexProfileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(CodexProfileError::NotRegularFile(path.to_path_buf()));
    }
    let mode = metadata.permissions().mode() & 0o7777;
    if mode != 0o600 {
        return Err(CodexProfileError::UnsafePermissions {
            path: path.to_path_buf(),
            mode,
        });
    }
    let mut actual = Vec::new();
    file.read_to_end(&mut actual)
        .map_err(|source| CodexProfileError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if actual != expected && !codex_managed_profile_matches(&actual, expected, path, hook_command) {
        return Err(CodexProfileError::ContentMismatch(path.to_path_buf()));
    }
    verify_profile_path_identity(directory_lock, file_name, path, &metadata)?;
    Ok(())
}

fn verify_profile_path_identity(
    directory_lock: &Flock<File>,
    file_name: &OsStr,
    path: &Path,
    expected_metadata: &Metadata,
) -> Result<(), CodexProfileError> {
    let path_descriptor = openat(
        &**directory_lock,
        file_name,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| {
        if error == Errno::ELOOP {
            CodexProfileError::Symlink(path.to_path_buf())
        } else {
            CodexProfileError::Read {
                path: path.to_path_buf(),
                source: error.into(),
            }
        }
    })?;
    let path_file = File::from(path_descriptor);
    let path_metadata = path_file
        .metadata()
        .map_err(|source| CodexProfileError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if !path_metadata.is_file() {
        return Err(CodexProfileError::NotRegularFile(path.to_path_buf()));
    }
    if path_metadata.dev() != expected_metadata.dev()
        || path_metadata.ino() != expected_metadata.ino()
    {
        return Err(CodexProfileError::PathChanged(path.to_path_buf()));
    }
    let mode = path_metadata.permissions().mode() & 0o7777;
    if mode != 0o600 {
        return Err(CodexProfileError::UnsafePermissions {
            path: path.to_path_buf(),
            mode,
        });
    }
    Ok(())
}

/// Codex 0.147 persists its normal hook approval by mutating a user profile:
/// it prepends the selected approvals reviewer and appends one trusted hash
/// state entry per discovered CodeGotchi hook. The profile name remains based
/// on the pristine rendered bytes; this validator accepts only that exact
/// Codex-managed extension and never rewrites it.
fn codex_managed_profile_matches(
    actual: &[u8],
    pristine: &[u8],
    path: &Path,
    hook_command: &str,
) -> bool {
    let Some(mut remaining) = actual.strip_prefix(MANAGED_PROFILE_PREFIX) else {
        return false;
    };
    if !remaining.starts_with(pristine) {
        return false;
    }
    remaining = &remaining[pristine.len()..];

    let Ok(state) = std::str::from_utf8(remaining) else {
        return false;
    };
    let mut lines = state.split('\n');
    if lines.next() != Some("[hooks.state]") || lines.next() != Some("") {
        return false;
    }

    let escaped_path = escape_toml(&path.to_string_lossy());
    let mut found = [false; MANAGED_TRUST_EVENTS.len()];
    loop {
        let Some(header) = lines.next() else {
            return false;
        };
        if header.is_empty() {
            return found.iter().all(|present| *present) && lines.next().is_none();
        }

        let Some((event_index, (_, event_key))) =
            MANAGED_TRUST_EVENTS
                .iter()
                .enumerate()
                .find(|(_, (_, event_key))| {
                    header == format!("[hooks.state.\"{}:{}:0:0\"]", escaped_path, event_key)
                })
        else {
            return false;
        };
        if found[event_index] {
            return false;
        }
        found[event_index] = true;

        let Some(trusted_hash) = lines.next() else {
            return false;
        };
        let expected_hash = codex_hook_trusted_hash(hook_command, event_key);
        if trusted_hash != format!("trusted_hash = \"{expected_hash}\"") {
            return false;
        }
        if lines.next() != Some("") {
            return false;
        }
    }
}

/// Reproduces Codex 0.147's normalized command-hook identity. The fields are
/// intentionally limited to the stable command, timeout, and async settings
/// emitted by `render_profile`; optional absent TOML fields are omitted before
/// Codex fingerprints the canonical JSON representation.
fn codex_hook_trusted_hash(hook_command: &str, event_key: &str) -> String {
    let timeout = if event_key == "session_end" { 1 } else { 600 };
    let identity = serde_json::json!({
        "event_name": event_key,
        "hooks": [{
            "type": "command",
            "command": hook_command,
            "timeout": timeout,
            "async": false,
        }],
    });
    let canonical = canonical_json(&identity);
    let serialized = serde_json::to_vec(&canonical).expect("hook identity is serializable");
    let hash = Sha256::digest(serialized);
    format!(
        "sha256:{}",
        hash.iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                if let Some(value) = map.get(&key) {
                    sorted.insert(key, canonical_json(value));
                }
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonical_json).collect())
        }
        other => other.clone(),
    }
}

fn render_profile(hook_command: &str) -> Vec<u8> {
    let mut content = String::from(
        "# CodeGotchi additive hook layer.\n# The base Codex config is intentionally not copied or modified.\n\n[features]\nhooks = true\n\n",
    );
    for event in HOOK_EVENTS {
        content.push_str(&format!(
            "[[hooks.{event}]]\n\n[[hooks.{event}.hooks]]\ntype = \"command\"\ncommand = \"{}\"\n\n",
            escape_toml(hook_command)
        ));
    }
    content.into_bytes()
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn abandoned_partial_temp_cannot_poison_atomic_profile_publication() {
        let home =
            std::env::temp_dir().join(format!("codegotchi-profile-publication-{}", Uuid::new_v4()));
        fs::create_dir_all(&home).unwrap();
        let content = render_profile("codegotchi hook");
        let profile_name = profile_name_for(&content);
        let final_name = format!("{profile_name}.config.toml");
        let final_path = home.join(&final_name);
        let abandoned_name = format!(".{final_name}.tmp-abandoned");
        let abandoned_path = home.join(&abandoned_name);
        let partial = content[..content.len() / 2].to_vec();
        fs::write(&abandoned_path, &partial).unwrap();
        let mut abandoned_permissions = fs::metadata(&abandoned_path).unwrap().permissions();
        abandoned_permissions.set_mode(0o600);
        fs::set_permissions(&abandoned_path, abandoned_permissions).unwrap();

        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let worker_home = home.clone();
        let worker_final_path = final_path.clone();
        let worker_content = content.clone();
        let worker = thread::spawn(move || {
            ensure_profile_file_with_observer(
                &worker_home,
                &worker_final_path,
                &worker_content,
                "codegotchi hook",
                |temporary_name| {
                    assert!(!worker_final_path.exists());
                    let temporary_path = worker_home.join(temporary_name);
                    assert_eq!(
                        fs::metadata(temporary_path).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                    ready_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                },
            )
            .unwrap();
        });

        ready_rx.recv().unwrap();
        assert!(!final_path.exists());
        assert_eq!(fs::read(&abandoned_path).unwrap(), partial);
        release_tx.send(()).unwrap();
        worker.join().unwrap();

        assert_eq!(fs::read(&final_path).unwrap(), content);
        assert_eq!(fs::read(&abandoned_path).unwrap(), partial);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn codex_managed_trust_metadata_is_reused_without_rewriting() {
        let home = std::env::temp_dir().join(format!(
            "codegotchi-profile-managed-trust-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&home).unwrap();
        let session_file = home.join("runtime-metadata.json");
        let hook_command = "codegotchi hook";
        let profile = PersistentCodexProfile::ensure(&home, &session_file, hook_command).unwrap();
        let path = profile.config_path().to_path_buf();
        let pristine = fs::read(&path).unwrap();
        let approved = approved_profile_fixture(&path, &pristine, hook_command);
        fs::write(&path, approved).unwrap();
        let inode = fs::metadata(&path).unwrap().ino();

        let reused = PersistentCodexProfile::ensure(&home, &session_file, hook_command)
            .expect("Codex 0.147 managed trust metadata should be accepted");
        assert_eq!(reused.config_path(), path);
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        assert_eq!(
            fs::read(&path).unwrap(),
            approved_profile_fixture(&path, &pristine, hook_command)
        );

        let guard = reused.acquire_spawn_guard().unwrap();
        guard
            .verify_before_spawn()
            .expect("guard revalidation uses the same managed trust grammar");
        drop(guard);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn codex_managed_trust_metadata_rejects_hook_and_metadata_mutations() {
        let mutations = [
            "altered hook command",
            "extra unrelated config",
            "foreign trust entry",
            "malformed trust hash",
        ];

        for label in mutations {
            let home = std::env::temp_dir().join(format!(
                "codegotchi-profile-managed-trust-mutation-{}",
                Uuid::new_v4()
            ));
            fs::create_dir_all(&home).unwrap();
            let session_file = home.join("runtime-metadata.json");
            let hook_command = "codegotchi hook";
            let profile =
                PersistentCodexProfile::ensure(&home, &session_file, hook_command).unwrap();
            let path = profile.config_path().to_path_buf();
            let pristine = fs::read(&path).unwrap();
            let mut approved = approved_profile_fixture(&path, &pristine, hook_command);
            match label {
                "altered hook command" => {
                    let needle = b"command = \"codegotchi hook\"";
                    let replacement = b"command = \"attacker hook\"";
                    let start = approved
                        .windows(needle.len())
                        .position(|window| window == needle)
                        .unwrap();
                    approved.splice(start..start + needle.len(), replacement.iter().copied());
                }
                "extra unrelated config" => {
                    let insert_at = approved
                        .windows(b"[features]".len())
                        .position(|window| window == b"[features]")
                        .unwrap();
                    approved.splice(
                        insert_at..insert_at,
                        b"model = \"foreign\"\n\n".iter().copied(),
                    );
                }
                "foreign trust entry" => {
                    approved.extend_from_slice(
                    b"[hooks.state.\"/foreign/profile:pre_tool_use:0:0\"]\ntrusted_hash = \"sha256:0000000000000000000000000000000000000000000000000000000000000000\"\n\n",
                );
                }
                "malformed trust hash" => {
                    let needle = b"trusted_hash = \"sha256:";
                    let start = approved
                        .windows(needle.len())
                        .position(|window| window == needle)
                        .unwrap()
                        + needle.len();
                    approved[start] = b'Z';
                }
                _ => unreachable!(),
            }
            fs::write(&path, &approved).unwrap();
            drop(profile);

            let error = PersistentCodexProfile::ensure(&home, &session_file, hook_command)
                .expect_err(label);
            assert!(
                matches!(error, CodexProfileError::ContentMismatch(_)),
                "{label}: {error}"
            );
            assert_eq!(
                fs::read(&path).unwrap(),
                approved,
                "{label} must not overwrite"
            );
            fs::remove_dir_all(home).unwrap();
        }
    }

    fn approved_profile_fixture(path: &Path, pristine: &[u8], hook_command: &str) -> Vec<u8> {
        let mut approved = b"approvals_reviewer = \"auto_review\"\n".to_vec();
        approved.extend_from_slice(pristine);
        approved.extend_from_slice(b"[hooks.state]\n\n");
        for (_event, event_key) in MANAGED_TRUST_EVENTS {
            approved.extend_from_slice(
                format!(
                    "[hooks.state.\"{}:{}:0:0\"]\ntrusted_hash = \"{}\"\n\n",
                    escape_toml(&path.to_string_lossy()),
                    event_key,
                    test_codex_hook_trusted_hash(hook_command, event_key),
                )
                .as_bytes(),
            );
        }
        approved
    }

    fn test_codex_hook_trusted_hash(hook_command: &str, event_key: &str) -> String {
        use sha2::{Digest, Sha256};

        let timeout = if event_key == "session_end" { 1 } else { 600 };
        let identity = serde_json::json!({
            "event_name": event_key,
            "hooks": [{
                "type": "command",
                "command": hook_command,
                "timeout": timeout,
                "async": false,
            }],
        });
        let serialized = serde_json::to_vec(&canonical_json(&identity)).unwrap();
        let hash = Sha256::digest(serialized);
        format!(
            "sha256:{}",
            hash.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }
}

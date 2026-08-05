use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

const HOOK_EVENTS: [&str; 6] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
];

#[derive(Debug, Error)]
pub enum CodexProfileError {
    #[error("Codex home is not a directory: {0}")]
    InvalidHome(PathBuf),
    #[error("Codex profile name is empty")]
    EmptyName,
    #[error("Codex profile name contains a path separator: {0}")]
    InvalidName(String),
    #[error("Codex profile already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("could not create Codex profile: {0}")]
    Create(#[source] std::io::Error),
    #[error("could not write Codex profile: {0}")]
    Write(#[source] std::io::Error),
    #[error("could not remove Codex profile: {0}")]
    Remove(#[source] std::io::Error),
}

/// A uniquely-owned additive Codex profile file.
#[derive(Debug)]
pub struct TemporaryCodexProfile {
    codex_home: PathBuf,
    profile_name: String,
    config_path: PathBuf,
    session_file: PathBuf,
    owned: bool,
}

impl TemporaryCodexProfile {
    pub fn create(
        codex_home: impl AsRef<Path>,
        profile_name: impl Into<String>,
        session_file: impl AsRef<Path>,
        hook_command: &str,
    ) -> Result<Self, CodexProfileError> {
        let codex_home = codex_home.as_ref().to_path_buf();
        if !codex_home.is_dir() {
            return Err(CodexProfileError::InvalidHome(codex_home));
        }
        let profile_name = profile_name.into();
        validate_name(&profile_name)?;
        if hook_command.trim().is_empty() {
            return Err(CodexProfileError::Write(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "hook command is empty",
            )));
        }
        let config_path = codex_home.join(format!("{profile_name}.config.toml"));
        if config_path.exists() {
            return Err(CodexProfileError::AlreadyExists(config_path));
        }

        let content = render_profile(hook_command);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&config_path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    CodexProfileError::AlreadyExists(config_path.clone())
                } else {
                    CodexProfileError::Create(error)
                }
            })?;
        if let Err(error) = file.write_all(content.as_bytes()) {
            let _ = fs::remove_file(&config_path);
            return Err(CodexProfileError::Write(error));
        }
        if let Err(error) = file.sync_all() {
            let _ = fs::remove_file(&config_path);
            return Err(CodexProfileError::Write(error));
        }

        Ok(Self {
            codex_home,
            profile_name,
            config_path,
            session_file: session_file.as_ref().to_path_buf(),
            owned: true,
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

    pub fn codex_command(&self, codex_program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(codex_program);
        command
            .arg("--profile")
            .arg(&self.profile_name)
            .env("CODEX_HOME", &self.codex_home)
            .env("CODEGOTCHI_SESSION_FILE", &self.session_file);
        command
    }

    pub fn cleanup(&mut self) -> Result<(), CodexProfileError> {
        if !self.owned {
            return Ok(());
        }
        match fs::remove_file(&self.config_path) {
            Ok(()) => {
                self.owned = false;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.owned = false;
                Ok(())
            }
            Err(error) => Err(CodexProfileError::Remove(error)),
        }
    }
}

impl Drop for TemporaryCodexProfile {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn validate_name(name: &str) -> Result<(), CodexProfileError> {
    if name.is_empty() {
        return Err(CodexProfileError::EmptyName);
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(CodexProfileError::InvalidName(name.to_owned()));
    }
    Ok(())
}

fn render_profile(hook_command: &str) -> String {
    let mut content = String::from(
        "# CodeGotchi Task 1 additive hook layer.\n# The base Codex config is intentionally not copied or modified.\n\n[features]\nhooks = true\n\n",
    );
    for event in HOOK_EVENTS {
        content.push_str(&format!(
            "[[hooks.{event}]]\n\n[[hooks.{event}.hooks]]\ntype = \"command\"\ncommand = \"{}\"\n\n",
            escape_toml(hook_command)
        ));
    }
    content
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

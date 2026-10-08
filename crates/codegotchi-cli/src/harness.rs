//! Harness selection and session-local integration files. Terminal hosting is shared.
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use codegotchi_domain::EventSource;
use serde_json::json;
use uuid::Uuid;

use crate::AgentInvocation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harness {
    Codex,
    Pi,
    Claude,
    OhMyPi,
    Hermes,
}

impl Harness {
    pub fn parse(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "codex" => Some(Self::Codex),
            "pi" => Some(Self::Pi),
            "claude" | "claude-code" => Some(Self::Claude),
            "omp" | "oh-my-pi" => Some(Self::OhMyPi),
            "hermes" => Some(Self::Hermes),
            _ => None,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::Claude => "claude",
            Self::OhMyPi => "omp",
            Self::Hermes => "hermes",
        }
    }

    pub fn executable_override(self) -> &'static str {
        match self {
            Self::Codex => "CODEGOTCHI_REAL_CODEX",
            Self::Pi => "CODEGOTCHI_REAL_PI",
            Self::Claude => "CODEGOTCHI_REAL_CLAUDE",
            Self::OhMyPi => "CODEGOTCHI_REAL_OMP",
            Self::Hermes => "CODEGOTCHI_REAL_HERMES",
        }
    }

    pub fn source(self) -> EventSource {
        match self {
            Self::Codex => EventSource::Codex,
            Self::Pi => EventSource::Pi,
            Self::Claude => EventSource::ClaudeCode,
            Self::OhMyPi => EventSource::OhMyPi,
            Self::Hermes => EventSource::Hermes,
        }
    }
}

/// Only the directory created by this launch is reclaimed on every exit path.
pub struct SessionIntegration {
    directory: PathBuf,
    pub invocation: AgentInvocation,
}

impl SessionIntegration {
    pub fn prepare(
        harness: Harness,
        program: &Path,
        executable: &Path,
        arguments: &[OsString],
        session_file: &Path,
        runtime_directory: &Path,
    ) -> io::Result<Self> {
        let directory = runtime_directory.join(format!("integration-{}", Uuid::new_v4()));
        fs::create_dir(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let mut integration = Self {
            directory,
            invocation: AgentInvocation {
                program: program.to_path_buf(),
                arguments: Vec::new(),
                environment: vec![
                    (
                        "CODEGOTCHI_SESSION_FILE".into(),
                        session_file.as_os_str().into(),
                    ),
                    (
                        "CODEGOTCHI_EXECUTABLE".into(),
                        executable.as_os_str().into(),
                    ),
                    ("CODEGOTCHI_HARNESS".into(), harness.command().into()),
                ],
            },
        };
        let mut trailing = arguments.to_vec();
        match harness {
            Harness::Pi | Harness::OhMyPi => {
                let extension =
                    integration.write("codegotchi.mjs", include_bytes!("integrations/pi.mjs"))?;
                integration
                    .invocation
                    .arguments
                    .extend(["--extension".into(), extension.into_os_string()]);
            }
            Harness::Claude => {
                let (mut user_settings, remaining) = claude_settings(arguments)?;
                trailing = remaining;
                let command = format!("{} hook claude", shell_quote(executable));
                let hooks = [
                    "SessionStart",
                    "SessionEnd",
                    "UserPromptSubmit",
                    "Stop",
                    "PreToolUse",
                    "PostToolUse",
                    "PostToolUseFailure",
                ]
                .into_iter()
                .map(|event| {
                    (
                        event.to_owned(),
                        json!([{"hooks": [{"type": "command", "command": command, "timeout": 5}]}]),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
                let user_hooks = user_settings
                    .entry("hooks")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or_else(|| io::Error::other("Claude settings hooks must be an object"))?;
                for (event, additions) in hooks {
                    let entries = user_hooks
                        .entry(event)
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .ok_or_else(|| {
                            io::Error::other("Claude settings hook entries must be arrays")
                        })?;
                    entries.extend(
                        additions
                            .as_array()
                            .expect("generated hook entries")
                            .iter()
                            .cloned(),
                    );
                }
                let settings = integration
                    .write("claude-settings.json", &serde_json::to_vec(&user_settings)?)?;
                integration
                    .invocation
                    .arguments
                    .extend(["--settings".into(), settings.into_os_string()]);
            }
            Harness::Hermes => {}
            Harness::Codex => {
                return Err(io::Error::other(
                    "Codex uses its persistent profile adapter",
                ));
            }
        }
        integration
            .invocation
            .arguments
            .extend_from_slice(&trailing);
        Ok(integration)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        let path = self.directory.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(bytes)?;
        Ok(path)
    }
}

/// Consolidate Claude's single CLI settings layer while retaining its existing hooks.
fn claude_settings(
    arguments: &[OsString],
) -> io::Result<(serde_json::Map<String, serde_json::Value>, Vec<OsString>)> {
    let mut settings = serde_json::Map::new();
    let mut remaining = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--" {
            remaining.extend_from_slice(&arguments[index..]);
            break;
        }
        let value = if argument == "--settings" {
            index += 1;
            Some(
                arguments
                    .get(index)
                    .ok_or_else(|| io::Error::other("--settings requires a file or JSON object"))?
                    .clone(),
            )
        } else {
            argument
                .to_str()
                .and_then(|value| value.strip_prefix("--settings="))
                .map(OsString::from)
        };
        if let Some(value) = value {
            let text = value
                .to_str()
                .ok_or_else(|| io::Error::other("Claude settings path must be UTF-8"))?;
            let bytes = if text.trim_start().starts_with('{') {
                text.as_bytes().to_vec()
            } else {
                fs::read(Path::new(&value))?
            };
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            let object = value
                .as_object()
                .ok_or_else(|| io::Error::other("Claude settings must be a JSON object"))?;
            settings.extend(object.clone());
        } else {
            remaining.push(argument.clone());
        }
        index += 1;
    }
    Ok((settings, remaining))
}

impl Drop for SessionIntegration {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Hermes currently exposes config-file shell hooks rather than a CLI overlay.
pub fn hermes_hook_configuration(executable: &Path) -> String {
    let command = format!("{} hook hermes", shell_quote(executable));
    let mut result = String::from("hooks:\n");
    for event in [
        "on_session_start",
        "on_session_end",
        "pre_tool_call",
        "post_tool_call",
    ] {
        result.push_str(&format!(
            "  {event}:\n    - command: {}\n      timeout: 5\n",
            serde_json::to_string(&command).expect("string JSON")
        ));
    }
    result
}

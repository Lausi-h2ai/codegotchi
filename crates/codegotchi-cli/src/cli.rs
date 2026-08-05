use std::error::Error;
use std::io::Write;
use std::path::Path;

use codegotchi_domain::EnforcementMode;

use crate::codex_hook::{
    CODEGOTCHI_SESSION_FILE, HookTransportError, run_hook_from_environment,
    runtime_metadata_is_active, send_debug_generate_poop_to_runtime, send_debug_neglect_to_runtime,
    send_mode_to_runtime,
};
use crate::protocol::HookOutput;
use crate::runtime_metadata::read_metadata;

pub const CODEGOTCHI_ENABLE_DEBUG: &str = "CODEGOTCHI_ENABLE_DEBUG";

#[derive(Debug)]
pub struct CliError(String);

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

pub fn run(mut arguments: impl Iterator<Item = String>) -> Result<(), CliError> {
    match arguments.next().as_deref() {
        Some("hook") if arguments.next().is_none() => {
            let output = run_hook_from_environment();
            print_hook_output(&output).map_err(|error| CliError(error.to_string()))
        }
        Some("hook") => Err(CliError(String::from(
            "the hook command takes no arguments",
        ))),
        Some("mode") => run_mode(arguments),
        Some("debug") => run_debug(arguments),
        Some(command) => Err(CliError(format!(
            "unsupported command `{command}`; use `codegotchi hook`, `codegotchi mode decorative|strict`, or a guarded `codegotchi debug` command"
        ))),
        None => Err(CliError(String::from(
            "a command is required; use `codegotchi hook`, `codegotchi mode decorative|strict`, or `codegotchi debug neglect|generate-poop`",
        ))),
    }
}

fn run_mode(mut arguments: impl Iterator<Item = String>) -> Result<(), CliError> {
    let mode = match arguments.next().as_deref() {
        Some("decorative") => EnforcementMode::Decorative,
        Some("strict") => EnforcementMode::Strict,
        Some(value) => {
            return Err(CliError(format!(
                "unsupported mode `{value}`; choose `decorative` or `strict`"
            )));
        }
        None => {
            return Err(CliError(String::from(
                "mode requires exactly one value: `decorative` or `strict`",
            )));
        }
    };
    if arguments.next().is_some() {
        return Err(CliError(String::from(
            "mode accepts exactly one value: `decorative` or `strict`",
        )));
    }

    let metadata = active_metadata()?;
    let receipt = send_mode_to_runtime(&metadata, mode).map_err(runtime_command_error)?;
    println!(
        "mode {}: {}",
        mode_name(mode),
        if receipt.duplicate {
            "already active"
        } else {
            "persisted and broadcast"
        }
    );
    Ok(())
}

fn run_debug(mut arguments: impl Iterator<Item = String>) -> Result<(), CliError> {
    let command = arguments.next();
    if arguments.next().is_some() {
        return Err(CliError(String::from(
            "debug accepts exactly `neglect` or `generate-poop`; arbitrary values are not supported",
        )));
    }
    if std::env::var(CODEGOTCHI_ENABLE_DEBUG).ok().as_deref() != Some("1") {
        return Err(CliError(String::from(
            "debug commands are disabled; set CODEGOTCHI_ENABLE_DEBUG=1 for this demo control",
        )));
    }

    let metadata = active_metadata()?;
    match command.as_deref() {
        Some("neglect") => {
            let receipt =
                send_debug_neglect_to_runtime(&metadata).map_err(runtime_command_error)?;
            println!(
                "debug neglect: {}",
                if receipt.duplicate {
                    "already applied"
                } else {
                    "persisted and broadcast"
                }
            );
            Ok(())
        }
        Some("generate-poop") => {
            let receipt =
                send_debug_generate_poop_to_runtime(&metadata).map_err(runtime_command_error)?;
            println!(
                "debug generate-poop: persisted and broadcast ({} pending poop{})",
                receipt.snapshot.pending_poops.len(),
                if receipt.snapshot.pending_poops.len() == 1 {
                    ""
                } else {
                    "s"
                }
            );
            Ok(())
        }
        Some(value) => Err(CliError(format!(
            "unsupported debug command `{value}`; choose `neglect` or `generate-poop`"
        ))),
        None => Err(CliError(String::from(
            "debug requires exactly one command: `neglect` or `generate-poop`",
        ))),
    }
}

fn active_metadata() -> Result<crate::protocol::RuntimeMetadataV1, CliError> {
    let path = std::env::var_os(CODEGOTCHI_SESSION_FILE).ok_or_else(|| {
        CliError(format!(
            "active runtime unavailable: set {CODEGOTCHI_SESSION_FILE} to the runtime metadata file"
        ))
    })?;
    let path_ref = Path::new(&path);
    let metadata = read_metadata(path_ref).map_err(|error| {
        CliError(format!(
            "active runtime metadata is unavailable at {}: {error}",
            path_ref.display()
        ))
    })?;
    if !runtime_metadata_is_active(&metadata) {
        return Err(CliError(format!(
            "active runtime metadata at {} is stale; start CodeGotchi again",
            path_ref.display()
        )));
    }
    Ok(metadata)
}

fn runtime_command_error(error: HookTransportError) -> CliError {
    CliError(format!(
        "active runtime command failed: {error}; verify CODEGOTCHI_SESSION_FILE, the runtime owner, and its bearer token"
    ))
}

fn mode_name(mode: EnforcementMode) -> &'static str {
    match mode {
        EnforcementMode::Decorative => "decorative",
        EnforcementMode::Gentle => "gentle",
        EnforcementMode::Strict => "strict",
    }
}

fn print_hook_output(output: &HookOutput) -> Result<(), std::io::Error> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, output).map_err(std::io::Error::other)?;
    stdout.write_all(b"\n")
}

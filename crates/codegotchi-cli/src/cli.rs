use std::error::Error;
use std::io::Write;

use crate::codex_hook::run_hook_from_environment;
use crate::protocol::HookOutput;

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
        Some(command) => Err(CliError(format!(
            "unsupported command `{command}` in Task 1; only `codegotchi hook` is available"
        ))),
        None => Err(CliError(String::from(
            "a command is required; use `codegotchi hook`",
        ))),
    }
}

fn print_hook_output(output: &HookOutput) -> Result<(), std::io::Error> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, output).map_err(std::io::Error::other)?;
    stdout.write_all(b"\n")
}

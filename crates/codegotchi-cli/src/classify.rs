use std::path::Path;

use codegotchi_domain::{ActivityKind, CommandCategory, CommandClassification, CommandPurpose};

/// Classifies a Codex command without retaining the command text.
///
/// The parser is intentionally conservative. A command is blockable only when
/// it has one recognized executable, no shell composition, and a recognized
/// local development subcommand. Everything else remains structured metadata
/// with an uncertain purpose and therefore fails open in Strict mode.
pub fn classify_command(tool_name: &str, command: &str) -> CommandClassification {
    if tool_name.eq_ignore_ascii_case("apply_patch") {
        return if apply_patch_input_is_verified(command) {
            CommandClassification::new(
                CommandCategory::Development,
                CommandPurpose::SafeDevelopment,
            )
        } else {
            CommandClassification::new(CommandCategory::Unknown, CommandPurpose::Uncertain)
        };
    }
    if !tool_name.eq_ignore_ascii_case("bash") || command_is_ambiguous(command) {
        return CommandClassification::new(CommandCategory::Unknown, CommandPurpose::Uncertain);
    }

    let Some(executable) = canonical_executable(tool_name, command) else {
        return CommandClassification::new(CommandCategory::Unknown, CommandPurpose::Uncertain);
    };

    match executable {
        "codegotchi" => CommandClassification::new(
            CommandCategory::CodeGotchi,
            CommandPurpose::CodeGotchiControl,
        ),
        "cargo" => development_classification(command, cargo_subcommand_is_safe),
        "npm" | "pnpm" | "yarn" => development_classification(command, package_subcommand_is_safe),
        "go" => development_classification(command, go_subcommand_is_safe),
        "make" | "cmake" | "mvn" | "gradle" | "dotnet" | "swift" => {
            development_classification(command, build_subcommand_is_safe)
        }
        "pytest" => CommandClassification::new(
            CommandCategory::Development,
            CommandPurpose::SafeDevelopment,
        ),
        "rustc" | "gcc" | "g++" | "clang" | "javac" => CommandClassification::new(
            CommandCategory::Development,
            CommandPurpose::SafeDevelopment,
        ),
        "rustup" | "node" | "bun" | "python" | "python3" => {
            CommandClassification::new(CommandCategory::Development, CommandPurpose::Uncertain)
        }
        "git" => CommandClassification::new(CommandCategory::Git, CommandPurpose::GitRecovery),
        "docker" | "podman" | "kubectl" | "helm" => CommandClassification::new(
            CommandCategory::Infrastructure,
            CommandPurpose::InfrastructureShutdown,
        ),
        "ps" | "top" | "htop" | "kill" | "pkill" | "killall" => {
            CommandClassification::new(CommandCategory::Process, CommandPurpose::ProcessRecovery)
        }
        "chmod" | "chown" | "sudo" | "ssh-keygen" | "passwd" => CommandClassification::new(
            CommandCategory::Security,
            CommandPurpose::SecurityRemediation,
        ),
        "sh" | "bash" | "zsh" | "fish" | "ls" | "cat" | "pwd" | "rg" | "grep" | "find" | "sed"
        | "awk" | "head" | "tail" | "wc" | "sort" | "stat" | "which" | "command" | "env"
        | "echo" | "printf" | "true" | "false" | "test" | "sleep" | "curl" | "wget" | "exit"
        | "return" => {
            CommandClassification::new(CommandCategory::Shell, CommandPurpose::ShellRecovery)
        }
        _ => CommandClassification::new(CommandCategory::Unknown, CommandPurpose::Uncertain),
    }
}

pub fn command_category_name(category: CommandCategory) -> &'static str {
    match category {
        CommandCategory::CodeGotchi => "code_gotchi",
        CommandCategory::Development => "development",
        CommandCategory::Process => "process",
        CommandCategory::Shell => "shell",
        CommandCategory::Git => "git",
        CommandCategory::Infrastructure => "infrastructure",
        CommandCategory::Security => "security",
        CommandCategory::Unknown => "unknown",
    }
}

pub fn command_purpose_name(purpose: CommandPurpose) -> &'static str {
    match purpose {
        CommandPurpose::SafeDevelopment => "safe_development",
        CommandPurpose::CodeGotchiControl => "code_gotchi_control",
        CommandPurpose::ProcessRecovery => "process_recovery",
        CommandPurpose::ShellRecovery => "shell_recovery",
        CommandPurpose::GitRecovery => "git_recovery",
        CommandPurpose::InfrastructureShutdown => "infrastructure_shutdown",
        CommandPurpose::SecurityRemediation => "security_remediation",
        CommandPurpose::Uncertain => "uncertain",
    }
}

pub fn executable_name(tool_name: &str, command: &str) -> Option<String> {
    if tool_name.eq_ignore_ascii_case("apply_patch") {
        return Some(String::from("apply_patch"));
    }
    canonical_executable(tool_name, command).map(ToOwned::to_owned)
}

pub fn activity_for_command(
    tool_name: &str,
    command: &str,
    exit_status: Option<i32>,
) -> ActivityKind {
    if exit_status.is_some_and(|status| status != 0) {
        return ActivityKind::Error;
    }
    if tool_name.eq_ignore_ascii_case("apply_patch") {
        return if apply_patch_input_is_verified(command) {
            ActivityKind::Editing
        } else {
            ActivityKind::UnknownWork
        };
    }
    if !tool_name.eq_ignore_ascii_case("bash") || command_is_ambiguous(command) {
        return ActivityKind::UnknownWork;
    }
    let Some(executable) = canonical_executable(tool_name, command) else {
        return ActivityKind::UnknownWork;
    };
    let first_subcommand = first_subcommand(command).unwrap_or_default();
    match executable {
        "cargo" | "npm" | "pnpm" | "yarn" | "go" | "make" | "cmake" | "mvn" | "gradle"
        | "dotnet" | "swift" => match first_subcommand {
            "test" | "check" | "clippy" | "lint" | "verify" | "bench" | "vet" => {
                ActivityKind::Testing
            }
            "build" | "compile" | "run" | "--build" => ActivityKind::Building,
            "install" | "add" => ActivityKind::Installing,
            "fmt" | "format" | "fix" => ActivityKind::Editing,
            "" => ActivityKind::Building,
            _ => ActivityKind::Thinking,
        },
        "rustc" | "gcc" | "g++" | "clang" | "javac" => ActivityKind::Building,
        "pytest" => ActivityKind::Testing,
        "git" => ActivityKind::GitOperation,
        "docker" | "podman" | "kubectl" | "helm" => ActivityKind::DockerOperation,
        "rg" | "grep" | "find" | "which" => ActivityKind::Searching,
        "cat" | "ls" | "pwd" | "sed" | "awk" | "head" | "tail" | "wc" | "sort" | "stat" => {
            ActivityKind::Reading
        }
        "curl" | "wget" => ActivityKind::WebResearch,
        "sleep" => ActivityKind::Waiting,
        "ps" | "top" | "htop" | "kill" | "pkill" | "killall" => ActivityKind::Thinking,
        "sh" | "bash" | "zsh" | "fish" | "env" | "echo" | "printf" | "true" | "false" | "test"
        | "command" | "exit" | "return" => ActivityKind::Thinking,
        _ => ActivityKind::UnknownWork,
    }
}

fn development_classification(
    command: &str,
    safe_subcommand: fn(&str) -> bool,
) -> CommandClassification {
    CommandClassification::new(
        CommandCategory::Development,
        if safe_subcommand(command) {
            CommandPurpose::SafeDevelopment
        } else {
            CommandPurpose::Uncertain
        },
    )
}

fn apply_patch_input_is_verified(command: &str) -> bool {
    !command.trim().is_empty()
}

fn cargo_subcommand_is_safe(command: &str) -> bool {
    matches!(
        first_subcommand(command),
        Some(
            "test"
                | "check"
                | "clippy"
                | "build"
                | "run"
                | "fmt"
                | "format"
                | "bench"
                | "doc"
                | "fix"
        )
    )
}

fn package_subcommand_is_safe(command: &str) -> bool {
    matches!(
        first_subcommand(command),
        Some("test" | "run" | "build" | "lint" | "check" | "format" | "fmt")
    )
}

fn go_subcommand_is_safe(command: &str) -> bool {
    matches!(
        first_subcommand(command),
        Some("test" | "build" | "run" | "fmt" | "vet")
    )
}

fn build_subcommand_is_safe(command: &str) -> bool {
    matches!(
        first_subcommand(command),
        Some(
            "test"
                | "check"
                | "build"
                | "compile"
                | "run"
                | "verify"
                | "lint"
                | "format"
                | "fmt"
                | "--build"
        )
    ) || first_subcommand(command).is_none()
}

fn canonical_executable(tool_name: &str, command: &str) -> Option<&'static str> {
    if tool_name.eq_ignore_ascii_case("apply_patch") {
        return Some("apply_patch");
    }
    if !tool_name.eq_ignore_ascii_case("bash") {
        return None;
    }

    let token = first_command_token(command)?;
    let basename = Path::new(token).file_name()?.to_str()?;
    match basename {
        "codegotchi" => Some("codegotchi"),
        "cargo" => Some("cargo"),
        "rustc" => Some("rustc"),
        "rustup" => Some("rustup"),
        "npm" => Some("npm"),
        "pnpm" => Some("pnpm"),
        "yarn" => Some("yarn"),
        "node" => Some("node"),
        "bun" => Some("bun"),
        "python" => Some("python"),
        "python3" => Some("python3"),
        "pytest" => Some("pytest"),
        "go" => Some("go"),
        "make" => Some("make"),
        "cmake" => Some("cmake"),
        "mvn" => Some("mvn"),
        "gradle" => Some("gradle"),
        "javac" => Some("javac"),
        "swift" => Some("swift"),
        "dotnet" => Some("dotnet"),
        "gcc" => Some("gcc"),
        "g++" => Some("g++"),
        "clang" => Some("clang"),
        "git" => Some("git"),
        "docker" => Some("docker"),
        "podman" => Some("podman"),
        "kubectl" => Some("kubectl"),
        "helm" => Some("helm"),
        "ps" => Some("ps"),
        "top" => Some("top"),
        "htop" => Some("htop"),
        "kill" => Some("kill"),
        "pkill" => Some("pkill"),
        "killall" => Some("killall"),
        "chmod" => Some("chmod"),
        "chown" => Some("chown"),
        "sudo" => Some("sudo"),
        "ssh-keygen" => Some("ssh-keygen"),
        "passwd" => Some("passwd"),
        "sh" => Some("sh"),
        "bash" => Some("bash"),
        "zsh" => Some("zsh"),
        "fish" => Some("fish"),
        "ls" => Some("ls"),
        "cat" => Some("cat"),
        "pwd" => Some("pwd"),
        "rg" => Some("rg"),
        "grep" => Some("grep"),
        "find" => Some("find"),
        "sed" => Some("sed"),
        "awk" => Some("awk"),
        "head" => Some("head"),
        "tail" => Some("tail"),
        "wc" => Some("wc"),
        "sort" => Some("sort"),
        "stat" => Some("stat"),
        "which" => Some("which"),
        "command" => Some("command"),
        "env" => Some("env"),
        "echo" => Some("echo"),
        "printf" => Some("printf"),
        "true" => Some("true"),
        "false" => Some("false"),
        "test" => Some("test"),
        "sleep" => Some("sleep"),
        "curl" => Some("curl"),
        "wget" => Some("wget"),
        "exit" => Some("exit"),
        "return" => Some("return"),
        _ => None,
    }
}

fn first_command_token(command: &str) -> Option<&str> {
    command
        .split_whitespace()
        .find(|token| !is_assignment_prefix(token))
}

fn first_subcommand(command: &str) -> Option<&str> {
    let mut tokens = command
        .split_whitespace()
        .filter(|token| !is_assignment_prefix(token));
    tokens.next()?;
    tokens.next()
}

fn is_assignment_prefix(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn command_is_ambiguous(command: &str) -> bool {
    let trimmed = command.trim();
    trimmed.is_empty()
        || trimmed.chars().any(|character| {
            matches!(
                character,
                ';' | '|'
                    | '&'
                    | '>'
                    | '<'
                    | '`'
                    | '$'
                    | '\\'
                    | '\n'
                    | '\r'
                    | '"'
                    | '\''
                    | '('
                    | ')'
                    | '{'
                    | '}'
            )
        })
        || trimmed
            .split_whitespace()
            .any(|token| token.starts_with('#'))
}

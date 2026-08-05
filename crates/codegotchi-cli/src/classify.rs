use std::path::Path;

use codegotchi_domain::{ActivityKind, CommandCategory, CommandClassification, CommandPurpose};

/// Classifies a Codex command without retaining the command text.
pub fn classify_command(tool_name: &str, command: &str) -> CommandClassification {
    if tool_name.eq_ignore_ascii_case("apply_patch") {
        return CommandClassification::new(
            CommandCategory::Development,
            CommandPurpose::SafeDevelopment,
        );
    }
    if !tool_name.eq_ignore_ascii_case("bash") {
        return CommandClassification::new(CommandCategory::Unknown, CommandPurpose::Uncertain);
    }

    let Some(executable) = executable_name(tool_name, command) else {
        return CommandClassification::new(CommandCategory::Unknown, CommandPurpose::Uncertain);
    };

    match executable.as_str() {
        "codegotchi" => CommandClassification::new(
            CommandCategory::CodeGotchi,
            CommandPurpose::CodeGotchiControl,
        ),
        "cargo" | "rustc" | "rustup" | "npm" | "pnpm" | "yarn" | "node" | "bun" | "python"
        | "python3" | "pytest" | "go" | "make" | "cmake" | "mvn" | "gradle" | "javac" | "swift"
        | "dotnet" | "gcc" | "g++" | "clang" => CommandClassification::new(
            CommandCategory::Development,
            CommandPurpose::SafeDevelopment,
        ),
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
        | "echo" | "printf" | "true" | "false" | "test" | "sleep" | "curl" | "wget" => {
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
    if !tool_name.eq_ignore_ascii_case("bash") {
        return None;
    }
    command
        .split_whitespace()
        .next()
        .map(Path::new)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
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
        return ActivityKind::Editing;
    }
    if !tool_name.eq_ignore_ascii_case("bash") {
        return ActivityKind::UnknownWork;
    }
    let Some(executable) = executable_name(tool_name, command) else {
        return ActivityKind::UnknownWork;
    };
    let first_subcommand = command.split_whitespace().nth(1).unwrap_or_default();
    match executable.as_str() {
        "cargo" | "npm" | "pnpm" | "yarn" | "go" | "make" | "cmake" | "mvn" | "gradle"
        | "dotnet" => match first_subcommand {
            "test" | "check" | "clippy" | "lint" | "verify" => ActivityKind::Testing,
            "build" | "compile" | "run" => ActivityKind::Building,
            "install" | "add" => ActivityKind::Installing,
            "fmt" | "format" => ActivityKind::Editing,
            _ => ActivityKind::Building,
        },
        "rustc" | "gcc" | "g++" | "clang" | "javac" | "swift" => ActivityKind::Building,
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
        | "command" => ActivityKind::Thinking,
        _ => ActivityKind::UnknownWork,
    }
}

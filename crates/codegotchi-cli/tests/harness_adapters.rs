use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use codegotchi_cli::harness::{Harness, SessionIntegration, hermes_hook_configuration};
use codegotchi_cli::harness_hook::{
    output_value, parse_input, permission_context, translate_input,
};
use codegotchi_cli::{HookOutput, RuntimeMetadataV1, parse_launch_request};
use codegotchi_domain::{ActivityKind, AgentEventKind, CommandPurpose};
use serde_json::{Value, json};
use uuid::Uuid;

fn metadata() -> RuntimeMetadataV1 {
    RuntimeMetadataV1 {
        schema_version: 1,
        repository_root: "/tmp/codegotchi-adapter".into(),
        runtime_id: Uuid::new_v4(),
        loopback_base_url: "http://127.0.0.1:1".into(),
        owning_pid: std::process::id(),
        bearer_token: "test-only".into(),
    }
}

#[test]
fn selects_all_harnesses_and_preserves_harness_specific_flags() {
    for name in ["pi", "claude", "claude-code", "omp", "oh-my-pi", "hermes"] {
        let request =
            parse_launch_request(["--", name, "-p", "prompt with spaces", "--profile=mine"])
                .unwrap();
        assert_eq!(request.harness, Harness::parse(OsStr::new(name)).unwrap());
        assert_eq!(
            request.trailing_arguments,
            ["-p", "prompt with spaces", "--profile=mine"].map(std::ffi::OsString::from)
        );
    }
    assert!(parse_launch_request(["--", "codex", "-p", "mine"]).is_err());
}

#[test]
fn normalizes_sources_sessions_tools_and_replay_without_retaining_payloads() {
    let meta = metadata();
    for harness in [
        Harness::Pi,
        Harness::Claude,
        Harness::OhMyPi,
        Harness::Hermes,
    ] {
        let payload = if harness == Harness::Hermes {
            json!({"hook_event_name":"pre_tool_call", "session_id":"sess-not-a-uuid", "tool_name":"terminal", "tool_input":{"command":"cargo test", "secret":"never-persist"}, "extra":{"tool_call_id":"tool-1"}})
        } else {
            json!({"hook_event_name":"PreToolUse", "session_id":"sess-not-a-uuid", "tool_name":"bash", "tool_input":{"command":"cargo test", "secret":"never-persist"}, "tool_use_id":"tool-1", "prompt":"private-prompt"})
        };
        let input = parse_input(harness, &serde_json::to_vec(&payload).unwrap()).unwrap();
        let event = translate_input(harness, &input, &meta).unwrap();
        assert_eq!(event.source, harness.source());
        assert_eq!(event.activity, Some(ActivityKind::Testing));
        assert_eq!(event.kind, AgentEventKind::ToolStarted);
        assert_eq!(
            event.id,
            translate_input(harness, &input, &meta).unwrap().id
        );
        assert_eq!(
            permission_context(harness, &input)
                .unwrap()
                .classification()
                .unwrap()
                .purpose(),
            CommandPurpose::SafeDevelopment
        );
        let serialized = serde_json::to_string(&event).unwrap();
        for secret in ["cargo test", "never-persist", "private-prompt"] {
            assert!(!serialized.contains(secret));
        }
    }
}

#[test]
fn idless_claude_turns_do_not_collapse_and_harness_sessions_are_distinct() {
    let meta = metadata();
    let input = parse_input(
        Harness::Claude,
        br#"{"hook_event_name":"UserPromptSubmit","session_id":"abc"}"#,
    )
    .unwrap();
    let first = translate_input(Harness::Claude, &input, &meta).unwrap();
    let second = translate_input(Harness::Claude, &input, &meta).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.session_id, second.session_id);
    assert_ne!(
        first.session_id,
        translate_input(Harness::Pi, &input, &meta)
            .unwrap()
            .session_id
    );
}

#[test]
fn hermes_lifecycle_means_turns_and_completion_carries_structured_results() {
    let meta = metadata();
    for (name, kind) in [
        ("on_session_start", AgentEventKind::TurnStarted),
        ("on_session_end", AgentEventKind::TurnCompleted),
    ] {
        let input = parse_input(Harness::Hermes, &serde_json::to_vec(&json!({"hook_event_name":name,"tool_name":null,"tool_input":null,"session_id":"sess"})).unwrap()).unwrap();
        assert_eq!(
            translate_input(Harness::Hermes, &input, &meta)
                .unwrap()
                .kind,
            kind
        );
    }
    let input = parse_input(Harness::Hermes, &serde_json::to_vec(&json!({"hook_event_name":"post_tool_call","tool_name":"terminal","tool_input":{"command":"cargo test"},"extra":{"result":"{\"exit_code\":1}","duration_ms":17}})).unwrap()).unwrap();
    let event = translate_input(Harness::Hermes, &input, &meta).unwrap();
    assert_eq!(event.activity, Some(ActivityKind::Error));
    assert_eq!(event.metadata.exit_status, Some(1));
    assert_eq!(event.metadata.duration_ms, Some(17));
}

#[test]
fn file_edit_tools_are_development_and_failure_hooks_show_errors() {
    for (harness, name) in [
        (Harness::Claude, "Edit"),
        (Harness::Pi, "write"),
        (Harness::OhMyPi, "edit"),
        (Harness::Hermes, "patch"),
    ] {
        let wire = if harness == Harness::Hermes {
            "pre_tool_call"
        } else {
            "PreToolUse"
        };
        let input = parse_input(harness, &serde_json::to_vec(&json!({"hook_event_name":wire,"tool_name":name,"tool_input":{"content":"not-retained"}})).unwrap()).unwrap();
        assert_eq!(
            permission_context(harness, &input)
                .unwrap()
                .classification()
                .unwrap()
                .purpose(),
            CommandPurpose::SafeDevelopment
        );
        let event = translate_input(harness, &input, &metadata()).unwrap();
        assert_eq!(event.activity, Some(ActivityKind::Editing));
        assert!(
            !serde_json::to_string(&event)
                .unwrap()
                .contains("not-retained")
        );
    }
    let input = parse_input(
        Harness::Claude,
        br#"{"hook_event_name":"PostToolUseFailure","tool_name":"Write"}"#,
    )
    .unwrap();
    assert_eq!(
        translate_input(Harness::Claude, &input, &metadata())
            .unwrap()
            .activity,
        Some(ActivityKind::Error)
    );
}

#[test]
fn emits_native_denials_and_fails_open_for_malformed_or_unbound_hooks() {
    let retry = HookOutput::deny("Feed Mochi, then retry the Codex request afterward.");
    assert_eq!(
        output_value(Harness::Hermes, &retry)["message"],
        "Feed Mochi, then retry the hermes request afterward."
    );
    assert_eq!(
        output_value(Harness::Codex, &retry)["hookSpecificOutput"]["permissionDecisionReason"],
        "Feed Mochi, then retry the Codex request afterward."
    );
    let deny = HookOutput::deny("feed Mochi");
    assert_eq!(
        output_value(Harness::Pi, &deny),
        json!({"block":true,"reason":"feed Mochi"})
    );
    assert_eq!(
        output_value(Harness::OhMyPi, &deny),
        output_value(Harness::Pi, &deny)
    );
    assert_eq!(
        output_value(Harness::Hermes, &deny),
        json!({"action":"block","message":"feed Mochi"})
    );
    assert_eq!(
        output_value(Harness::Claude, &deny)["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
    for harness in [
        Harness::Pi,
        Harness::Claude,
        Harness::OhMyPi,
        Harness::Hermes,
    ] {
        assert!(parse_input(harness, b"not json").is_none());
        let mut child = Command::new(env!("CARGO_BIN_EXE_codegotchi"))
            .args(["hook", harness.command()])
            .env_remove("CODEGOTCHI_SESSION_FILE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"not json").unwrap();
        assert_eq!(child.wait_with_output().unwrap().stdout, b"{}\n");
    }
}

#[test]
fn integrations_are_private_additive_and_reclaimed_without_codex_configuration() {
    let root = std::env::temp_dir().join(format!("codegotchi-harness-files-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    for harness in [
        Harness::Pi,
        Harness::Claude,
        Harness::OhMyPi,
        Harness::Hermes,
    ] {
        let integration = SessionIntegration::prepare(
            harness,
            Path::new("/usr/bin/agent"),
            Path::new("/opt/has space/codegotchi"),
            &["--model".into(), "x".into()],
            Path::new("/tmp/session.json"),
            &root,
        )
        .unwrap();
        assert!(
            !integration
                .invocation
                .environment
                .iter()
                .any(|(key, _)| key == "CODEX_HOME")
        );
        assert_eq!(
            &integration.invocation.arguments[integration.invocation.arguments.len() - 2..],
            ["--model", "x"].map(std::ffi::OsString::from)
        );
        if harness != Harness::Hermes {
            let file = Path::new(&integration.invocation.arguments[1]);
            assert!(file.exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(file).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            if harness == Harness::Claude {
                let settings: Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
                assert_eq!(settings["hooks"].as_object().unwrap().len(), 7);
                assert!(
                    settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
                        .as_str()
                        .unwrap()
                        .contains("hook claude")
                );
            }
        }
        drop(integration);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }
    assert!(hermes_hook_configuration(Path::new("/opt/a b/codegotchi")).contains("hook hermes"));
    fs::remove_dir(root).unwrap();
}

#[test]
fn claude_cli_settings_keep_user_hooks_and_options() {
    let root = std::env::temp_dir().join(format!("codegotchi-claude-settings-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let user_file = root.join("user.json");
    let original = br#"{"model":"my-model","hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"my-existing-hook"}]}]}}"#;
    fs::write(&user_file, original).unwrap();
    let integration = SessionIntegration::prepare(
        Harness::Claude,
        Path::new("/usr/bin/claude"),
        Path::new("/usr/bin/codegotchi"),
        &[
            "--settings".into(),
            user_file.as_os_str().into(),
            "-p".into(),
            "my prompt".into(),
        ],
        Path::new("/tmp/session.json"),
        &root,
    )
    .unwrap();
    let settings: Value =
        serde_json::from_slice(&fs::read(Path::new(&integration.invocation.arguments[1])).unwrap())
            .unwrap();
    assert_eq!(settings["model"], "my-model");
    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "my-existing-hook"
    );
    assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
    assert_eq!(
        &integration.invocation.arguments[2..],
        ["-p", "my prompt"].map(std::ffi::OsString::from)
    );
    assert_eq!(fs::read(&user_file).unwrap(), original);
    drop(integration);
    fs::remove_dir_all(root).unwrap();
}

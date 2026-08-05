use std::fs;
use std::path::Path;

use codegotchi_cli::{
    EventIngestRequest, HookInput, HookOutput, RuntimeMetadataV1, classify_command, translate_hook,
    translate_hook_json,
};
use codegotchi_domain::{
    ActivityKind, AgentEventKind, CommandCategory, CommandPurpose, EventSource,
};
use uuid::Uuid;

fn metadata() -> RuntimeMetadataV1 {
    RuntimeMetadataV1::new(
        Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa),
        "/workspace/codegatchi",
        "http://127.0.0.1:39123",
        "test-bearer-token",
        4242,
    )
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hooks")
        .join(name);
    fs::read_to_string(path).expect("fixture exists")
}

#[test]
fn installed_schema_fixtures_translate_to_privacy_limited_events() {
    let cases = [
        (
            "session_start.json",
            AgentEventKind::SessionStarted,
            ActivityKind::Idle,
            None,
        ),
        (
            "session_end.json",
            AgentEventKind::SessionEnded,
            ActivityKind::Idle,
            None,
        ),
        (
            "user_prompt_submit.json",
            AgentEventKind::TurnStarted,
            ActivityKind::Thinking,
            None,
        ),
        (
            "bash_pre.json",
            AgentEventKind::ToolStarted,
            ActivityKind::Testing,
            None,
        ),
        (
            "bash_post_success.json",
            AgentEventKind::ToolCompleted,
            ActivityKind::Testing,
            Some(0),
        ),
        (
            "bash_post_failure.json",
            AgentEventKind::ToolCompleted,
            ActivityKind::Error,
            Some(1),
        ),
        (
            "apply_patch_pre.json",
            AgentEventKind::ToolStarted,
            ActivityKind::Editing,
            None,
        ),
        (
            "apply_patch_post.json",
            AgentEventKind::ToolCompleted,
            ActivityKind::Editing,
            Some(0),
        ),
        (
            "stop.json",
            AgentEventKind::TurnCompleted,
            ActivityKind::Waiting,
            None,
        ),
        (
            "unknown_tool.json",
            AgentEventKind::ToolStarted,
            ActivityKind::UnknownWork,
            None,
        ),
    ];

    for (name, expected_kind, expected_activity, expected_exit) in cases {
        let raw = fixture(name);
        let input = HookInput::from_json(raw.as_bytes()).expect("sanitized fixture parses");
        let event = translate_hook(&input, &metadata()).expect("supported hook translates");

        assert_eq!(event.source, EventSource::Codex, "{name}");
        assert_eq!(event.kind, expected_kind, "{name}");
        assert_eq!(event.activity, Some(expected_activity), "{name}");
        assert_eq!(event.metadata.exit_status, expected_exit, "{name}");
        assert_eq!(event.repository_id, "/workspace/codegatchi", "{name}");
        assert_eq!(event.session_id, Uuid::from_u128(1), "{name}");

        let event_json = serde_json::to_string(&event).expect("event serializes");
        assert!(!event_json.contains("never-persist-this-prompt"), "{name}");
        assert!(!event_json.contains("secret-source-content"), "{name}");
        assert!(!event_json.contains("sensitive-tool-output"), "{name}");
        assert!(
            !event_json.contains("cargo test -p secret-project"),
            "{name}"
        );
    }
}

#[test]
fn event_ids_are_deterministic_and_ignore_discarded_or_future_fields() {
    let metadata = metadata();
    let baseline = HookInput::from_json(fixture("user_prompt_submit.json").as_bytes()).unwrap();
    let future =
        HookInput::from_json(fixture("user_prompt_submit_future_fields.json").as_bytes()).unwrap();

    let first = translate_hook(&baseline, &metadata).unwrap();
    let second = translate_hook(&baseline, &metadata).unwrap();
    let future_event = translate_hook(&future, &metadata).unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(first.id, future_event.id);
}

#[test]
fn official_event_identity_deduplicates_replay_but_distinguishes_repeats() {
    let metadata = metadata();
    let prompt = translate_hook(
        &HookInput::from_json(fixture("user_prompt_submit.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let prompt_replay = translate_hook(
        &HookInput::from_json(fixture("user_prompt_submit.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let repeated_prompt = translate_hook(
        &HookInput::from_json(fixture("user_prompt_submit_repeat.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let bash = translate_hook(
        &HookInput::from_json(fixture("bash_pre.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let repeated_bash = translate_hook(
        &HookInput::from_json(fixture("bash_pre_repeat.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let bash_post = translate_hook(
        &HookInput::from_json(fixture("bash_post_success.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();

    assert_eq!(prompt.id, prompt_replay.id);
    assert_ne!(prompt.id, repeated_prompt.id);
    assert_ne!(bash.id, repeated_bash.id);
    assert_ne!(bash.id, bash_post.id);
}

#[test]
fn lifecycle_identity_uses_official_sources_and_preserves_exact_replay() {
    let metadata = metadata();
    let startup = translate_hook(
        &HookInput::from_json(fixture("session_start.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let startup_replay = translate_hook(
        &HookInput::from_json(fixture("session_start.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let resume = translate_hook(
        &HookInput::from_json(fixture("session_start_resume.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let clear = translate_hook(
        &HookInput::from_json(fixture("session_start_clear.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let compact = translate_hook(
        &HookInput::from_json(fixture("session_start_compact.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();

    assert_eq!(startup.id, startup_replay.id);
    assert_ne!(startup.id, resume.id);
    assert_ne!(resume.id, clear.id);
    assert_ne!(clear.id, compact.id);
    assert_ne!(compact.id, startup.id);
}

#[test]
fn session_end_replay_and_repeat_are_honestly_boundary_limited() {
    let metadata = metadata();
    let end = translate_hook(
        &HookInput::from_json(fixture("session_end.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let replay = translate_hook(
        &HookInput::from_json(fixture("session_end.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let repeated_delivery = translate_hook(
        &HookInput::from_json(fixture("session_end_repeat.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    assert_eq!(end.id, replay.id);
    assert_eq!(end.id, repeated_delivery.id);
}

#[test]
fn command_classification_is_structured_and_fail_open_for_unknown_shapes() {
    let test = classify_command("Bash", "cargo test -p codegotchi-cli");
    assert_eq!(test.category(), CommandCategory::Development);
    assert_eq!(test.purpose(), CommandPurpose::SafeDevelopment);

    let git = classify_command("Bash", "git status --short");
    assert_eq!(git.category(), CommandCategory::Git);
    assert_eq!(git.purpose(), CommandPurpose::GitRecovery);

    let patch = classify_command("apply_patch", "*** Begin Patch\nsecret-source-content");
    assert_eq!(patch.category(), CommandCategory::Development);
    assert_eq!(patch.purpose(), CommandPurpose::SafeDevelopment);

    let unknown = classify_command("Bash", "a command shape that is not recognized");
    assert_eq!(unknown.category(), CommandCategory::Unknown);
    assert_eq!(unknown.purpose(), CommandPurpose::Uncertain);
}

#[test]
fn executable_metadata_is_bounded_without_changing_fail_open_classification() {
    let metadata = metadata();
    let cases = [
        ("one-token-secret", "unknown", ActivityKind::UnknownWork),
        (
            "/private/project/secrets/credentials.txt",
            "unknown",
            ActivityKind::UnknownWork,
        ),
        (
            "CODEGOTCHI_SECRET=do-not-persist",
            "unknown",
            ActivityKind::UnknownWork,
        ),
    ];

    for (command, category, activity) in cases {
        let payload = serde_json::json!({
            "session_id": "00000000-0000-0000-0000-000000000001",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": command },
            "turn_id": "turn-privacy"
        });
        let input = HookInput::from_json(&serde_json::to_vec(&payload).unwrap()).unwrap();
        let event = translate_hook(&input, &metadata).unwrap();

        assert_eq!(event.metadata.executable_name, None, "{command}");
        assert_eq!(event.metadata.command_category.as_deref(), Some(category));
        assert_eq!(event.activity, Some(activity));
        let event_json = serde_json::to_string(&event).unwrap();
        assert!(!event_json.contains(command), "{command}");
    }

    let assignment_prefixed = serde_json::json!({
        "session_id": "00000000-0000-0000-0000-000000000001",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "CODEGOTCHI_SECRET=do-not-persist cargo test" },
        "turn_id": "turn-privacy-known"
    });
    let input = HookInput::from_json(&serde_json::to_vec(&assignment_prefixed).unwrap()).unwrap();
    let event = translate_hook(&input, &metadata).unwrap();
    assert_eq!(event.metadata.executable_name.as_deref(), Some("cargo"));
    assert_eq!(
        event.metadata.command_category.as_deref(),
        Some("development")
    );
    assert_eq!(event.activity, Some(ActivityKind::Testing));
    let event_json = serde_json::to_string(&event).unwrap();
    assert!(!event_json.contains("CODEGOTCHI_SECRET"));
    assert!(!event_json.contains("do-not-persist"));
}

#[test]
fn malformed_input_and_unknown_tool_are_fail_open_and_unknown_work() {
    let metadata = metadata();
    let malformed = b"{ this is not JSON";
    assert!(translate_hook_json(malformed, &metadata).is_none());
    assert_eq!(serde_json::to_string(&HookOutput::allow()).unwrap(), "{}");

    let input = HookInput::from_json(fixture("unknown_tool.json").as_bytes()).unwrap();
    let event = translate_hook(&input, &metadata).unwrap();
    assert_eq!(event.activity, Some(ActivityKind::UnknownWork));
    assert_eq!(event.metadata.command_category.as_deref(), Some("unknown"));
}

#[test]
fn strict_denial_serializes_only_the_codex_documented_shape() {
    let output = HookOutput::deny("Feed or clean the pet before safe development work.");
    let json = serde_json::to_string(&output).unwrap();
    assert_eq!(
        json,
        r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Feed or clean the pet before safe development work."}}"#
    );

    let request = EventIngestRequest::new(
        translate_hook(
            &HookInput::from_json(fixture("bash_pre.json").as_bytes()).unwrap(),
            &metadata(),
        )
        .unwrap(),
    );
    let value = serde_json::to_value(request).unwrap();
    assert!(value.get("event").is_some());
    assert!(value.get("prompt").is_none());
    assert!(value.get("source").is_none());
    assert!(value.get("output").is_none());
}

#[test]
fn runtime_metadata_uses_the_v1_camel_case_wire_schema() {
    let value = serde_json::to_value(metadata()).unwrap();
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["runtimeId"], "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
    assert_eq!(value["repositoryRoot"], "/workspace/codegatchi");
    assert_eq!(value["loopbackBaseUrl"], "http://127.0.0.1:39123");
    assert_eq!(value["bearerToken"], "test-bearer-token");
    assert_eq!(value["owningPid"], 4242);
    assert_eq!(value.as_object().unwrap().len(), 6);
}

#[test]
fn post_success_and_failure_keep_command_metadata_but_discard_raw_values() {
    let metadata = metadata();
    let success = translate_hook(
        &HookInput::from_json(fixture("bash_post_success.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();
    let failure = translate_hook(
        &HookInput::from_json(fixture("bash_post_failure.json").as_bytes()).unwrap(),
        &metadata,
    )
    .unwrap();

    assert_eq!(success.metadata.executable_name.as_deref(), Some("cargo"));
    assert_eq!(
        success.metadata.command_category.as_deref(),
        Some("development")
    );
    assert_eq!(success.metadata.duration_ms, Some(37));
    assert_eq!(failure.metadata.executable_name.as_deref(), Some("cargo"));
    assert_eq!(failure.metadata.exit_status, Some(1));
    assert_eq!(
        HookInput::from_json(fixture("bash_post_success.json").as_bytes())
            .unwrap()
            .tool_response
            .as_ref()
            .and_then(|response| response.get("exit_code"))
            .and_then(serde_json::Value::as_i64),
        Some(0)
    );
    assert!(
        !serde_json::to_string(&success)
            .unwrap()
            .contains("sensitive-tool-output")
    );
}

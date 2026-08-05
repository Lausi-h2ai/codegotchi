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
    assert!(
        !serde_json::to_string(&success)
            .unwrap()
            .contains("sensitive-tool-output")
    );
}

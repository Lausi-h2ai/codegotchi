//! Normalizes external harness contracts at the same sanitized domain boundary.
use codegotchi_domain::{ActivityKind, CommandCategory, CommandClassification, CommandPurpose};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::harness::Harness;
use crate::protocol::{HookInput, HookOutput, PermissionContext, RuntimeMetadataV1};

pub fn parse_input(harness: Harness, bytes: &[u8]) -> Option<HookInput> {
    if harness == Harness::Codex {
        return HookInput::from_json(bytes).ok();
    }
    let mut raw: Value = serde_json::from_slice(bytes).ok()?;
    let raw = raw.as_object_mut()?;
    if harness == Harness::Hermes {
        let name = match raw.get("hook_event_name")?.as_str()? {
            // Hermes's "session" hooks are emitted per conversation turn.
            "on_session_start" => "UserPromptSubmit",
            "on_session_end" => "Stop",
            "pre_tool_call" => "PreToolUse",
            "post_tool_call" => "PostToolUse",
            _ => return None,
        };
        raw.insert("hook_event_name".into(), json!(name));
        let extra = raw.get("extra").cloned().unwrap_or(json!({}));
        if let Some(id) = extra.get("tool_call_id").and_then(Value::as_str) {
            raw.insert("tool_use_id".into(), json!(id));
        }
        if name == "PostToolUse" {
            let result = extra
                .get("result")
                .and_then(Value::as_str)
                .and_then(|text| serde_json::from_str::<Value>(text).ok());
            let exit_code = result
                .as_ref()
                .and_then(|value| value.get("exit_code").or_else(|| value.get("exitCode")))
                .cloned()
                .or_else(|| {
                    result
                        .as_ref()
                        .and_then(|value| value.get("error"))
                        .filter(|value| !value.is_null() && **value != json!(false))
                        .map(|_| json!(1))
                });
            raw.insert(
                "tool_response".into(),
                json!({"exit_code": exit_code, "duration_ms": extra.get("duration_ms")}),
            );
        }
    }
    if raw.get("hook_event_name").and_then(Value::as_str) == Some("PostToolUseFailure") {
        raw.insert("hook_event_name".into(), json!("PostToolUse"));
        raw.insert("tool_response".into(), json!({"exit_code": 1}));
    }
    let input: HookInput = serde_json::from_value(Value::Object(raw.clone())).ok()?;
    Some(input)
}

pub fn translate_input(
    harness: Harness,
    input: &HookInput,
    metadata: &RuntimeMetadataV1,
) -> Option<codegotchi_domain::AgentEvent> {
    if harness == Harness::Codex {
        return crate::codex_hook::translate_hook(input, metadata);
    }
    let mut normalized = input.clone();
    // Arbitrary session strings are scoped to both harness and wrapped runtime.
    let session = format!(
        "{}|{}|{}",
        harness.command(),
        metadata.runtime_id,
        input.session_id.as_deref().unwrap_or_default()
    );
    normalized.session_id =
        Some(Uuid::new_v5(&Uuid::NAMESPACE_URL, session.as_bytes()).to_string());
    if input.tool_name.as_deref() == Some("terminal") {
        normalized.tool_name = Some("Bash".into());
    }
    let mut event = crate::codex_hook::translate_hook(&normalized, metadata)?;
    event.source = harness.source();
    // Preserve actual tool call IDs for replay; fresh occurrence IDs prevent
    // ID-less prompts/lifecycle hooks collapsing into a single turn forever.
    let occurrence = input
        .tool_use_id
        .as_deref()
        .or(input.turn_id.as_deref())
        .or_else(|| input.future_fields.get("event_id").and_then(Value::as_str));
    event.id = occurrence.map_or_else(Uuid::new_v4, |id| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "{}|{}|{}|{}|{id}",
                harness.command(),
                metadata.runtime_id,
                event.session_id,
                input.hook_event_name
            )
            .as_bytes(),
        )
    });
    if let Some(activity) = file_activity(input.tool_name.as_deref().unwrap_or_default()) {
        event.activity = Some(if input.exit_status().is_some_and(|status| status != 0) {
            ActivityKind::Error
        } else {
            activity
        });
        event.metadata.command_category = Some(
            if activity == ActivityKind::Editing {
                "development"
            } else {
                "shell"
            }
            .into(),
        );
        event.metadata.executable_name = None;
    }
    Some(event)
}

pub fn permission_context(harness: Harness, input: &HookInput) -> Option<PermissionContext> {
    if input.hook_event_name != "PreToolUse" {
        return None;
    }
    if harness != Harness::Codex
        && file_activity(input.tool_name.as_deref().unwrap_or_default())
            == Some(ActivityKind::Editing)
    {
        return Some(PermissionContext::from_classification(
            CommandClassification::new(
                CommandCategory::Development,
                CommandPurpose::SafeDevelopment,
            ),
        ));
    }
    let mut normalized = input.clone();
    if harness == Harness::Hermes && normalized.tool_name.as_deref() == Some("terminal") {
        normalized.tool_name = Some("Bash".into());
    }
    crate::codex_hook::permission_context_for_hook(&normalized)
}

fn file_activity(tool: &str) -> Option<ActivityKind> {
    match tool {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "edit" | "write" | "write_file"
        | "patch" => Some(ActivityKind::Editing),
        "Read" | "read" | "read_file" => Some(ActivityKind::Reading),
        "Grep" | "Glob" | "grep" | "glob" | "search_files" => Some(ActivityKind::Searching),
        _ => None,
    }
}

pub fn output_value(harness: Harness, output: &HookOutput) -> Value {
    let output = match output {
        HookOutput::Deny { reason } if harness != Harness::Codex => HookOutput::deny(
            reason.replace("Codex request", &format!("{} request", harness.command())),
        ),
        output => output.clone(),
    };
    match (harness, &output) {
        (Harness::Pi | Harness::OhMyPi, HookOutput::Deny { reason }) => {
            json!({"block": true, "reason": reason})
        }
        (Harness::Hermes, HookOutput::Deny { reason }) => {
            json!({"action": "block", "message": reason})
        }
        (_, HookOutput::Allow) => json!({}),
        (_, _) => serde_json::to_value(&output).expect("hook output serializes"),
    }
}

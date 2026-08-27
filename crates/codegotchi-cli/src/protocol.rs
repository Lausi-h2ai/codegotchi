use std::path::PathBuf;

use codegotchi_domain::{
    AgentEvent, CommandCategory, CommandClassification, CommandPurpose, EnforcementMode,
    SimulationSnapshot,
};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

pub const RUNTIME_METADATA_SCHEMA_VERSION: u16 = 1;

/// The bounded, tolerant subset of a Codex hook payload used by CodeGotchi.
///
/// Codex can add fields to hook payloads. The adapter deliberately models only
/// the stable fields it needs and leaves all other fields unread. Raw values
/// remain in this type only for the duration of one hook process.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct HookInput {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default, alias = "tool_call_id")]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub tool_input: Option<Value>,
    #[serde(default, alias = "tool_output")]
    pub tool_response: Option<Value>,
    #[serde(default)]
    pub prompt: Option<Value>,
    #[serde(default)]
    pub source: Option<Value>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub stop_hook_active: Option<bool>,
    #[serde(default)]
    pub last_assistant_message: Option<Value>,
    #[serde(flatten)]
    pub future_fields: Map<String, Value>,
}

impl HookInput {
    pub fn from_json(bytes: &[u8]) -> Result<Self, HookInputError> {
        serde_json::from_slice(bytes).map_err(HookInputError::Json)
    }

    pub fn command(&self) -> Option<&str> {
        self.tool_input
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|input| input.get("command"))
            .and_then(Value::as_str)
    }

    pub fn exit_status(&self) -> Option<i32> {
        let response = self.tool_response.as_ref()?;
        if let Some(response) = response.as_object() {
            return ["exit_code", "exitCode", "status"]
                .iter()
                .find_map(|key| response.get(*key).and_then(value_as_i32))
                .or_else(|| {
                    response
                        .get("success")
                        .and_then(Value::as_bool)
                        .map(|success| if success { 0 } else { 1 })
                });
        }
        response.as_str().and_then(infer_exit_status_from_text)
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.tool_response
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|response| {
                ["duration_ms", "durationMs"]
                    .iter()
                    .find_map(|key| response.get(*key).and_then(Value::as_u64))
            })
    }

    /// Returns the stable identity Codex assigns to this invocation boundary.
    /// Turn-scoped hooks use `turn_id`; tool hooks use the more specific
    /// `tool_use_id` so two tools in one turn cannot collapse into one event.
    pub fn stable_event_identity(&self) -> Option<&str> {
        match self.hook_event_name.as_str() {
            "SessionStart" | "SessionEnd" => self.lifecycle_identity(),
            "PreToolUse" | "PostToolUse" => self
                .tool_use_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .or_else(|| self.turn_id.as_deref().filter(|id| !id.is_empty())),
            _ => self.turn_id.as_deref().filter(|id| !id.is_empty()),
        }
    }

    /// Returns the lifecycle discriminator Codex provides when one exists.
    ///
    /// `SessionStart.source` distinguishes the official startup/resume/clear/
    /// compact boundaries. `SessionEnd.reason` is retained for the same
    /// purpose when a release provides more than one reason. An otherwise
    /// identical lifecycle payload has no occurrence ID in the Codex schema,
    /// so it must remain an idempotent replay of the same event.
    pub fn lifecycle_identity(&self) -> Option<&str> {
        let identity = match self.hook_event_name.as_str() {
            "SessionStart" => self.source.as_ref().and_then(Value::as_str),
            "SessionEnd" => self.reason.as_deref(),
            _ => None,
        }?;
        (!identity.is_empty()).then_some(identity)
    }

    pub fn parsed_session_id(&self) -> Option<Uuid> {
        self.session_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
    }
}

fn value_as_i32(value: &Value) -> Option<i32> {
    value.as_i64().and_then(|number| i32::try_from(number).ok())
}

fn infer_exit_status_from_text(response: &str) -> Option<i32> {
    let response = response.to_ascii_lowercase();
    if response
        .lines()
        .any(|line| line.trim_start().starts_with("error:"))
        || response.contains("test result: failed")
        || response.contains("tests failed")
        || response.contains("build failed")
    {
        return Some(1);
    }
    if response.contains("test result: ok")
        || response.contains("tests, 0 benchmarks")
        || response.contains("passed in ")
        || (response.contains("test suites:") && response.contains("passed"))
        || response.contains("finished `")
    {
        return Some(0);
    }
    None
}

#[derive(Debug, Error)]
pub enum HookInputError {
    #[error("invalid Codex hook JSON: {0}")]
    Json(#[source] serde_json::Error),
}

/// The event envelope accepted by the authenticated loopback backend.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EventIngestRequest {
    pub event: AgentEvent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionContext>,
}

impl EventIngestRequest {
    pub fn new(event: AgentEvent) -> Self {
        Self {
            event,
            permission: None,
        }
    }

    pub fn with_permission(event: AgentEvent, classification: CommandClassification) -> Self {
        Self {
            event,
            permission: Some(PermissionContext::from_classification(classification)),
        }
    }

    pub fn new_with_permission(event: AgentEvent, classification: CommandClassification) -> Self {
        Self::with_permission(event, classification)
    }
}

/// The only optional context that may accompany a canonical PreToolUse event.
/// It is deliberately limited to policy inputs and cannot carry source text,
/// a raw command, tool output, or a transcript fragment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PermissionContext {
    pub category: String,
    pub purpose: String,
}

impl PermissionContext {
    pub fn from_classification(classification: CommandClassification) -> Self {
        Self {
            category: command_category_id(classification.category()).to_owned(),
            purpose: command_purpose_id(classification.purpose()).to_owned(),
        }
    }

    pub fn classification(&self) -> Option<CommandClassification> {
        Some(CommandClassification::new(
            parse_command_category(&self.category)?,
            parse_command_purpose(&self.purpose)?,
        ))
    }
}

fn command_category_id(category: CommandCategory) -> &'static str {
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

fn command_purpose_id(purpose: CommandPurpose) -> &'static str {
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

fn parse_command_category(value: &str) -> Option<CommandCategory> {
    Some(match value {
        "code_gotchi" => CommandCategory::CodeGotchi,
        "development" => CommandCategory::Development,
        "process" => CommandCategory::Process,
        "shell" => CommandCategory::Shell,
        "git" => CommandCategory::Git,
        "infrastructure" => CommandCategory::Infrastructure,
        "security" => CommandCategory::Security,
        "unknown" => CommandCategory::Unknown,
        _ => return None,
    })
}

fn parse_command_purpose(value: &str) -> Option<CommandPurpose> {
    Some(match value {
        "safe_development" => CommandPurpose::SafeDevelopment,
        "code_gotchi_control" => CommandPurpose::CodeGotchiControl,
        "process_recovery" => CommandPurpose::ProcessRecovery,
        "shell_recovery" => CommandPurpose::ShellRecovery,
        "git_recovery" => CommandPurpose::GitRecovery,
        "infrastructure_shutdown" => CommandPurpose::InfrastructureShutdown,
        "security_remediation" => CommandPurpose::SecurityRemediation,
        "uncertain" => CommandPurpose::Uncertain,
        _ => return None,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModeRequest {
    pub mode: EnforcementMode,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DebugRequest {}

/// Tells the browser whether the owning runtime was launched with the guarded
/// debug demo controls enabled, so the room can hide debug-only affordances.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatusResponse {
    pub debug_enabled: bool,
}

/// A deliberately tolerant response envelope shared by the hook and backend.
///
/// `decision` is a JSON value because the backend is not part of Task 1 and
/// can evolve its structured decision representation without making the hook
/// reject an otherwise valid response.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EventIngestResponse {
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub evaluated: bool,
    #[serde(default, alias = "enforcement_mode")]
    pub enforcement_mode: Option<String>,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub decision: Option<Value>,
    #[serde(default, alias = "permission_decision_reason")]
    pub reason: Option<String>,
    #[serde(default)]
    pub duplicate: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRequest {
    pub action_id: Uuid,
    pub food_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanRequest {
    pub action_id: Uuid,
    pub poop_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NapRequest {
    pub action_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PetRequest {
    pub action_id: Uuid,
    pub interaction_ms: u64,
    pub pointer_distance: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NameRequest {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMutationResponse {
    #[serde(flatten)]
    pub snapshot: SimulationSnapshot,
    pub duplicate: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

impl ErrorEnvelope {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error: ErrorBody {
                code,
                message: message.into(),
            },
        }
    }
}

impl EventIngestResponse {
    pub fn is_strict_denial(&self) -> bool {
        if !self.accepted || !self.evaluated || self.denial_reason().is_none() {
            return false;
        }
        if !(self.strict || self.enforcement_mode.as_deref() == Some("strict")) {
            return false;
        }
        self.blocked || self.decision.as_ref().is_some_and(decision_is_blocked)
    }

    pub fn denial_reason(&self) -> Option<&str> {
        self.reason.as_deref().filter(|reason| !reason.is_empty())
    }
}

fn decision_is_blocked(value: &Value) -> bool {
    match value {
        Value::String(decision) => {
            matches!(decision.as_str(), "deny" | "denied" | "block" | "blocked")
        }
        Value::Object(object) => ["decision", "permissionDecision", "kind"]
            .iter()
            .filter_map(|key| object.get(*key).and_then(Value::as_str))
            .any(|decision| matches!(decision, "deny" | "denied" | "block" | "blocked")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{NameRequest, PetRequest};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn pet_request_uses_bounded_camel_case_gesture_fields() {
        let request: PetRequest = serde_json::from_value(json!({
            "actionId": "00000000-0000-0000-0000-000000000001",
            "interactionMs": 1_500,
            "pointerDistance": 120.0,
        }))
        .expect("pet request should deserialize");

        assert_eq!(request.action_id, Uuid::from_u128(1));
        assert_eq!(request.interaction_ms, 1_500);
        assert_eq!(request.pointer_distance, 120.0);
        assert_eq!(
            serde_json::to_value(&request).expect("pet request should serialize"),
            json!({
                "actionId": "00000000-0000-0000-0000-000000000001",
                "interactionMs": 1_500,
                "pointerDistance": 120.0,
            })
        );
    }

    #[test]
    fn name_request_serializes_only_the_name_field() {
        let request: NameRequest = serde_json::from_value(json!({
            "name": "Luna",
        }))
        .expect("name request should deserialize");

        assert_eq!(request.name, "Luna");
        assert_eq!(
            serde_json::to_value(&request).expect("name request should serialize"),
            json!({"name": "Luna"})
        );
        assert!(
            serde_json::from_value::<NameRequest>(json!({
                "name": "Luna",
                "extra": true,
            }))
            .is_err()
        );
    }
}

/// Codex requires an empty JSON object for an allow/no-op hook result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HookOutput {
    Allow,
    Deny { reason: String },
}

impl HookOutput {
    pub fn allow() -> Self {
        Self::Allow
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }
}

impl Serialize for HookOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Allow => serializer.serialize_map(Some(0)).and_then(|map| map.end()),
            Self::Deny { reason } => {
                let mut outer = serializer.serialize_map(Some(1))?;
                let specific = HookSpecificOutput {
                    hook_event_name: "PreToolUse",
                    permission_decision: "deny",
                    permission_decision_reason: reason,
                };
                outer.serialize_entry("hookSpecificOutput", &specific)?;
                outer.end()
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HookSpecificOutput<'a> {
    hook_event_name: &'static str,
    permission_decision: &'static str,
    permission_decision_reason: &'a str,
}

/// Runtime discovery data written by the owning process for short-lived hooks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetadataV1 {
    pub schema_version: u16,
    pub runtime_id: Uuid,
    pub repository_root: PathBuf,
    pub loopback_base_url: String,
    pub bearer_token: String,
    pub owning_pid: u32,
}

impl RuntimeMetadataV1 {
    pub fn new(
        runtime_id: Uuid,
        repository_root: impl Into<PathBuf>,
        loopback_base_url: impl Into<String>,
        bearer_token: impl Into<String>,
        owning_pid: u32,
    ) -> Self {
        Self {
            schema_version: RUNTIME_METADATA_SCHEMA_VERSION,
            runtime_id,
            repository_root: repository_root.into(),
            loopback_base_url: loopback_base_url.into(),
            bearer_token: bearer_token.into(),
            owning_pid,
        }
    }
}

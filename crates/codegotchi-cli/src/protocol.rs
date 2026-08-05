use std::path::PathBuf;

use codegotchi_domain::AgentEvent;
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
        let response = self.tool_response.as_ref()?.as_object()?;
        ["exit_code", "exitCode", "status"]
            .iter()
            .find_map(|key| response.get(*key).and_then(value_as_i32))
            .or_else(|| {
                response
                    .get("success")
                    .and_then(Value::as_bool)
                    .map(|success| if success { 0 } else { 1 })
            })
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
        self.tool_use_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .or_else(|| self.turn_id.as_deref().filter(|id| !id.is_empty()))
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

#[derive(Debug, Error)]
pub enum HookInputError {
    #[error("invalid Codex hook JSON: {0}")]
    Json(#[source] serde_json::Error),
}

/// The event envelope accepted by the future loopback backend.
#[derive(Clone, Debug, Serialize)]
pub struct EventIngestRequest {
    pub event: AgentEvent,
}

impl EventIngestRequest {
    pub fn new(event: AgentEvent) -> Self {
        Self { event }
    }
}

/// A deliberately tolerant response envelope for the future loopback backend.
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
}

impl EventIngestResponse {
    pub fn is_strict_denial(&self) -> bool {
        if !self.accepted || !self.evaluated {
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

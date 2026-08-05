use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// The only schema version currently accepted by the domain event boundary.
pub const AGENT_EVENT_SCHEMA_VERSION: u16 = 1;

/// The producer that observed an agent event.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Codex,
    ClaudeCode,
    #[default]
    Generic,
}

/// Structured work categories. These values intentionally carry no source text.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    #[default]
    Idle,
    Thinking,
    Reading,
    Searching,
    Editing,
    Testing,
    Building,
    Installing,
    GitOperation,
    DockerOperation,
    WebResearch,
    Waiting,
    Celebrating,
    Error,
    Blocked,
    UnknownWork,
}

/// The event vocabulary shared by agent adapters and the pure domain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventKind {
    SessionStarted,
    SessionEnded,
    TurnStarted,
    TurnCompleted,
    WaitingForUser,
    OutputActivity,
    ToolStarted,
    ToolCompleted,
    CommandStarted,
    CommandCompleted,
    Interrupted,
    IntegrationError,
}

/// Structured metadata permitted on an agent event.
///
/// In particular, this type has no field for a prompt, command, output, or
/// source content. Adapters must classify those values before constructing an
/// event.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventMetadata {
    pub executable_name: Option<String>,
    pub command_category: Option<String>,
    pub exit_status: Option<i32>,
    pub duration_ms: Option<u64>,
    pub blocked: bool,
}

impl EventMetadata {
    pub fn new(
        executable_name: Option<String>,
        command_category: Option<String>,
        exit_status: Option<i32>,
        duration_ms: Option<u64>,
        blocked: bool,
    ) -> Self {
        Self {
            executable_name,
            command_category,
            exit_status,
            duration_ms,
            blocked,
        }
    }
}

/// A versioned, replay-safe event at the domain boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentEvent {
    pub id: Uuid,
    pub schema_version: u16,
    pub session_id: Uuid,
    pub repository_id: String,
    pub source: EventSource,
    pub kind: AgentEventKind,
    pub activity: Option<ActivityKind>,
    pub timestamp: DateTime<Utc>,
    pub metadata: EventMetadata,
}

impl AgentEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        session_id: Uuid,
        repository_id: impl Into<String>,
        source: EventSource,
        kind: AgentEventKind,
        activity: Option<ActivityKind>,
        timestamp: DateTime<Utc>,
        metadata: EventMetadata,
    ) -> Self {
        Self {
            id,
            schema_version: AGENT_EVENT_SCHEMA_VERSION,
            session_id,
            repository_id: repository_id.into(),
            source,
            kind,
            activity,
            timestamp,
            metadata,
        }
    }

    pub fn validate_schema_version(&self) -> Result<(), AgentEventError> {
        if self.schema_version == AGENT_EVENT_SCHEMA_VERSION {
            Ok(())
        } else {
            Err(AgentEventError::UnsupportedSchemaVersion(
                self.schema_version,
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AgentEventError {
    #[error("unsupported agent event schema version {0}; expected {AGENT_EVENT_SCHEMA_VERSION}")]
    UnsupportedSchemaVersion(u16),
}

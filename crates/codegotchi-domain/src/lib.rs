pub mod attention;
mod behavior;
pub mod care;
pub mod clock;
pub mod event;
pub mod permission;
pub mod pet;
pub mod poop;
pub mod progression;
pub mod random;

pub use attention::{
    AttentionIncidentKind, MAX_CATCH_UP_INCIDENTS, MAX_INCIDENT_DELAY_MS, MIN_INCIDENT_DELAY_MS,
    PetDemand, PetDemandKind, incident_delay_ms, incident_id, incident_kind,
};
pub use care::{CareCommand, CareError, CareResult, NAP_DURATION};
pub use clock::{Clock, FakeClock, SystemClock};
pub use event::{
    AGENT_EVENT_SCHEMA_VERSION, ActivityKind, AgentEvent, AgentEventError, AgentEventKind,
    EventMetadata, EventSource,
};
pub use permission::{
    CommandCategory, CommandClassification, CommandPurpose, EnforcementMode, PetSettings,
    RequiredAction, WorkDecision, WorkPermissionPolicy, WorkPermissionStrategy, WorkReasonCode,
};
pub use pet::{
    AgentActivityState, AgentOutcome, FoodInventory, FoodKind, Pet, PetBehavior, PetNeeds,
    PetSpecies, Poop,
};
pub use poop::{
    DefaultPoopGenerationStrategy, PoopGenerationStrategy, PoopGenerationThreshold,
    PoopThresholdError,
};
pub use progression::{
    DefaultNeedProgressionStrategy, NeedProgressionStrategy, PetSimulation,
    SIMULATION_SNAPSHOT_SCHEMA_VERSION, SessionActivity, SimulationSnapshot, SnapshotRestoreError,
};
pub use random::{RandomSource, SeededRandomSource};

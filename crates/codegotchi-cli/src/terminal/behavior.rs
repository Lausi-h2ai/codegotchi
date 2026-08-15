use codegotchi_domain::{ActivityKind, AgentActivityState, PetBehavior, SimulationSnapshot};

/// Broad, durable presentation moods derived exclusively from authoritative
/// structured state. The terminal renderer never classifies visible Codex
/// text; this mapping is the only activity projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PresentationActivity {
    #[default]
    Calm,
    Thinking,
    Working,
    Success,
    Failure,
    WaitingOrBlocked,
}

/// Maps one authoritative snapshot to its broad presentation activity.
///
/// The mapping is exact and exhaustive per the hardening spec: current
/// aggregate activity wins over stale recent outcomes, blocked/waiting states
/// always win, and `Idle` alone may fall back to a recent outcome.
#[must_use]
pub fn presentation_activity(snapshot: &SimulationSnapshot) -> PresentationActivity {
    match snapshot.activity {
        AgentActivityState::Blocked | AgentActivityState::WaitingForUser => {
            PresentationActivity::WaitingOrBlocked
        }
        AgentActivityState::Active(ActivityKind::Idle) => PresentationActivity::Calm,
        AgentActivityState::Active(ActivityKind::Thinking) => PresentationActivity::Thinking,
        AgentActivityState::Active(ActivityKind::Waiting | ActivityKind::Blocked) => {
            PresentationActivity::WaitingOrBlocked
        }
        AgentActivityState::Active(ActivityKind::Celebrating) => PresentationActivity::Success,
        AgentActivityState::Active(ActivityKind::Error) => PresentationActivity::Failure,
        AgentActivityState::Active(
            ActivityKind::Reading
            | ActivityKind::Searching
            | ActivityKind::Editing
            | ActivityKind::Testing
            | ActivityKind::Building
            | ActivityKind::Installing
            | ActivityKind::GitOperation
            | ActivityKind::DockerOperation
            | ActivityKind::WebResearch
            | ActivityKind::UnknownWork,
        ) => PresentationActivity::Working,
        AgentActivityState::Idle => match snapshot.behavior {
            PetBehavior::RecentSuccess => PresentationActivity::Success,
            PetBehavior::RecentFailure => PresentationActivity::Failure,
            _ => PresentationActivity::Calm,
        },
    }
}

/// Whether the snapshot currently represents an authoritative recovery nap.
///
/// `PetBehavior::Sleeping` alone is NOT authoritative sleep. Only an active
/// future `napping_until` is. A `Sleeping` snapshot without one is ordinary
/// idle behavior and must never use the recovery bed.
#[must_use]
pub fn has_authoritative_nap(snapshot: &SimulationSnapshot) -> bool {
    snapshot
        .napping_until
        .is_some_and(|until| snapshot.last_updated_at < until)
}

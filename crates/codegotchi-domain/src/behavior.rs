use chrono::{DateTime, Duration, Utc};

use crate::pet::{AgentActivityState, AgentOutcome, Pet, PetBehavior};

const RECENT_OUTCOME_WINDOW: Duration = Duration::minutes(5);
const SLEEP_AFTER: Duration = Duration::minutes(30);

/// Centralizes the ordered behavior policy so it cannot drift between callers.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BehaviorCoordinator;

impl BehaviorCoordinator {
    pub(crate) fn derive(
        pet: &Pet,
        now: DateTime<Utc>,
        last_activity_at: Option<DateTime<Utc>>,
        last_outcome_at: Option<DateTime<Utc>>,
    ) -> PetBehavior {
        if pet.needs().hunger() >= 90.0 {
            return PetBehavior::CriticalNeed;
        }
        if pet.needs().cleanliness() <= 10.0 {
            return PetBehavior::CriticalNeed;
        }
        if matches!(pet.activity(), AgentActivityState::Blocked) {
            return PetBehavior::Blocked;
        }
        if matches!(pet.activity(), AgentActivityState::Active(_)) {
            return PetBehavior::Working;
        }

        if is_recent(now, last_outcome_at) {
            return match pet.recent_outcome() {
                AgentOutcome::Success => PetBehavior::RecentSuccess,
                AgentOutcome::Failure => PetBehavior::RecentFailure,
                AgentOutcome::None => PetBehavior::Wandering,
            };
        }

        let last_activity = last_activity_at.unwrap_or_else(|| pet.last_updated_at());
        if is_elapsed_at_least(now, last_activity, SLEEP_AFTER) {
            return PetBehavior::Sleeping;
        }

        PetBehavior::Wandering
    }
}

fn is_recent(now: DateTime<Utc>, timestamp: Option<DateTime<Utc>>) -> bool {
    timestamp.is_some_and(|timestamp| {
        now >= timestamp && now.signed_duration_since(timestamp) <= RECENT_OUTCOME_WINDOW
    })
}

fn is_elapsed_at_least(now: DateTime<Utc>, timestamp: DateTime<Utc>, threshold: Duration) -> bool {
    now >= timestamp && now.signed_duration_since(timestamp) >= threshold
}

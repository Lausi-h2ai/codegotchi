use thiserror::Error;
use uuid::Uuid;

/// A hammock nap is fixed length: fast enough to feel like a quick power nap
/// in the UI, long enough for the per-second recovery animation to be visible.
pub const NAP_DURATION: chrono::Duration = chrono::Duration::seconds(5);

/// Minimum cumulative duration required for either a final pet or a
/// continuous petting stroke to be authoritative.
pub(crate) const PET_MIN_INTERACTION_MS: u64 = 1_500;
/// Minimum cumulative pointer distance required for either a final pet or a
/// continuous petting stroke to be authoritative.
pub(crate) const PET_MIN_POINTER_DISTANCE: f64 = 120.0;

/// A replay-safe request for one care interaction.
#[derive(Clone, Debug, PartialEq)]
pub enum CareCommand {
    Feed {
        action_id: Uuid,
        food_id: String,
    },
    CleanPoop {
        action_id: Uuid,
        poop_id: Uuid,
    },
    Pet {
        action_id: Uuid,
        interaction_ms: u64,
        pointer_distance: f32,
    },
    /// One increment of an in-progress petting gesture. A stroke must carry
    /// cumulative gesture evidence so the domain can independently validate
    /// the same duration/distance contract as [`Self::Pet`]. A validated
    /// stroke raises happiness continuously while the user pets (until the
    /// meter fills) without consuming an affection demand; the pointer-up
    /// `Pet` remains the discrete completion that resolves the oldest demand.
    ///
    /// If a qualified gesture is interrupted before pointer-up, happiness
    /// already earned by validated strokes is intentionally retained, but no
    /// affection demand is resolved until a final `Pet` is applied.
    PetStroke {
        action_id: Uuid,
        duration_ms: u64,
        distance: f64,
    },
    /// Settles the pet into the hammock for a fixed 5-second nap. Energy
    /// recovers quickly while `napping_until` is in the future.
    Nap {
        action_id: Uuid,
    },
}

impl CareCommand {
    pub(crate) fn action_id(&self) -> Uuid {
        match self {
            Self::Feed { action_id, .. }
            | Self::CleanPoop { action_id, .. }
            | Self::Pet { action_id, .. }
            | Self::PetStroke { action_id, .. }
            | Self::Nap { action_id } => *action_id,
        }
    }
}

/// Validates cumulative evidence for any authoritative petting mutation.
/// Keeping this check in the domain makes terminal/browser qualification only
/// an optimization: neither frontend can bypass the care contract.
pub(crate) fn validate_pet_evidence(duration_ms: u64, distance: f64) -> Result<(), CareError> {
    if duration_ms < PET_MIN_INTERACTION_MS {
        return Err(CareError::InsufficientDuration);
    }
    if !distance.is_finite() {
        return Err(CareError::NonFinitePointerDistance);
    }
    if distance < PET_MIN_POINTER_DISTANCE {
        return Err(CareError::InsufficientDistance);
    }
    Ok(())
}

/// The result of a care request. A duplicate has intentionally no transition
/// and is still successful from the replay boundary's perspective.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CareResult {
    Applied,
    Duplicate,
}

/// Typed validation failures for care requests.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CareError {
    #[error("unknown food id: {0}")]
    UnknownFood(String),
    #[error("food is out of stock: {0}")]
    OutOfStock(String),
    #[error("poop does not exist: {0}")]
    MissingPoop(Uuid),
    #[error("petting duration is below the minimum")]
    InsufficientDuration,
    #[error("pointer distance is not finite")]
    NonFinitePointerDistance,
    #[error("pointer distance is below the minimum")]
    InsufficientDistance,
    #[error("care condition is unsupported")]
    UnsupportedCondition,
}

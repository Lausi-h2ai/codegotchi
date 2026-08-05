use thiserror::Error;
use uuid::Uuid;

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
}

impl CareCommand {
    pub(crate) fn action_id(&self) -> Uuid {
        match self {
            Self::Feed { action_id, .. }
            | Self::CleanPoop { action_id, .. }
            | Self::Pet { action_id, .. } => *action_id,
        }
    }
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

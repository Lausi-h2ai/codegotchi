use crate::pet::Pet;

const MINIMUM_HUNGER_RECOVERY: f32 = 20.0;
const MINIMUM_CLEANLINESS_RECOVERY: f32 = 20.0;

/// The level at which the policy changes how it reports critical neglect.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum EnforcementMode {
    /// Record the pet state without affecting or warning about work.
    #[default]
    Decorative,
    /// Allow work while surfacing a care warning at critical neglect.
    Gentle,
    /// Block only explicitly safe development work at critical neglect.
    Strict,
}

/// A coarse structured category supplied by an adapter before policy evaluation.
/// It contains no command text or executable payload.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CommandCategory {
    CodeGotchi,
    Development,
    Process,
    Shell,
    Git,
    Infrastructure,
    Security,
    #[default]
    Unknown,
}

/// The semantic purpose used by the work-permission policy.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CommandPurpose {
    SafeDevelopment,
    CodeGotchiControl,
    ProcessRecovery,
    ShellRecovery,
    GitRecovery,
    InfrastructureShutdown,
    SecurityRemediation,
    #[default]
    Uncertain,
}

impl CommandPurpose {
    /// Only this purpose may be blocked by strict enforcement.
    pub const fn is_blockable(self) -> bool {
        matches!(self, Self::SafeDevelopment)
    }
}

/// Structured command metadata accepted by the policy boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommandClassification {
    pub category: CommandCategory,
    pub purpose: CommandPurpose,
}

impl CommandClassification {
    pub const fn new(category: CommandCategory, purpose: CommandPurpose) -> Self {
        Self { category, purpose }
    }

    pub const fn category(self) -> CommandCategory {
        self.category
    }

    pub const fn purpose(self) -> CommandPurpose {
        self.purpose
    }
}

/// Settings consumed by the policy. The default is intentionally fail-open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PetSettings {
    pub enforcement_mode: EnforcementMode,
}

impl PetSettings {
    pub const fn new(enforcement_mode: EnforcementMode) -> Self {
        Self { enforcement_mode }
    }

    pub const fn enforcement_mode(self) -> EnforcementMode {
        self.enforcement_mode
    }
}

impl Default for PetSettings {
    fn default() -> Self {
        Self::new(EnforcementMode::Decorative)
    }
}

/// Stable machine-readable explanation for a care warning or block.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkReasonCode {
    CriticalHunger,
    CriticalCleanliness,
}

impl WorkReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CriticalHunger => "critical_hunger",
            Self::CriticalCleanliness => "critical_cleanliness",
        }
    }
}

/// The structured care action associated with critical neglect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequiredAction {
    Feed { minimum_hunger_recovery: f32 },
    Clean { minimum_cleanliness_recovery: f32 },
}

impl RequiredAction {
    pub const fn minimum_recovery_points(self) -> f32 {
        match self {
            Self::Feed {
                minimum_hunger_recovery,
            } => minimum_hunger_recovery,
            Self::Clean {
                minimum_cleanliness_recovery,
            } => minimum_cleanliness_recovery,
        }
    }
}

/// The policy result. Warning and block results carry the same structured
/// reason/action pair so adapters can present or enforce them consistently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorkDecision {
    Allowed,
    Warning {
        reason_code: WorkReasonCode,
        required_action: RequiredAction,
    },
    Blocked {
        reason_code: WorkReasonCode,
        required_action: RequiredAction,
    },
}

impl WorkDecision {
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed | Self::Warning { .. })
    }

    pub const fn is_blocked(self) -> bool {
        matches!(self, Self::Blocked { .. })
    }

    pub const fn reason_code(self) -> Option<WorkReasonCode> {
        match self {
            Self::Allowed => None,
            Self::Warning { reason_code, .. } | Self::Blocked { reason_code, .. } => {
                Some(reason_code)
            }
        }
    }

    pub const fn required_action(self) -> Option<RequiredAction> {
        match self {
            Self::Allowed => None,
            Self::Warning {
                required_action, ..
            }
            | Self::Blocked {
                required_action, ..
            } => Some(required_action),
        }
    }
}

/// Strategy boundary for alternate policy implementations.
pub trait WorkPermissionStrategy {
    fn evaluate(
        &self,
        pet: &Pet,
        classification: &CommandClassification,
        settings: &PetSettings,
    ) -> WorkDecision;
}

/// The default policy implementation for this domain slice.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkPermissionPolicy;

impl WorkPermissionPolicy {
    /// Evaluates a structured command without inspecting any raw command text.
    pub fn evaluate(
        pet: &Pet,
        classification: &CommandClassification,
        settings: &PetSettings,
    ) -> WorkDecision {
        let Some((reason_code, required_action)) = critical_neglect(pet) else {
            return WorkDecision::Allowed;
        };

        match settings.enforcement_mode() {
            EnforcementMode::Decorative => WorkDecision::Allowed,
            EnforcementMode::Gentle => WorkDecision::Warning {
                reason_code,
                required_action,
            },
            EnforcementMode::Strict if classification.purpose().is_blockable() => {
                WorkDecision::Blocked {
                    reason_code,
                    required_action,
                }
            }
            EnforcementMode::Strict => WorkDecision::Allowed,
        }
    }
}

impl WorkPermissionStrategy for WorkPermissionPolicy {
    fn evaluate(
        &self,
        pet: &Pet,
        classification: &CommandClassification,
        settings: &PetSettings,
    ) -> WorkDecision {
        Self::evaluate(pet, classification, settings)
    }
}

fn critical_neglect(pet: &Pet) -> Option<(WorkReasonCode, RequiredAction)> {
    let needs = pet.needs();
    if needs.hunger() >= 90.0 {
        return Some((
            WorkReasonCode::CriticalHunger,
            RequiredAction::Feed {
                minimum_hunger_recovery: MINIMUM_HUNGER_RECOVERY,
            },
        ));
    }

    if needs.cleanliness() <= 10.0 {
        return Some((
            WorkReasonCode::CriticalCleanliness,
            RequiredAction::Clean {
                minimum_cleanliness_recovery: MINIMUM_CLEANLINESS_RECOVERY,
            },
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::{
        CommandCategory, CommandClassification, CommandPurpose, EnforcementMode, Pet, PetSettings,
        PetSpecies, RequiredAction, WorkDecision, WorkPermissionPolicy, WorkReasonCode,
    };

    fn pet_with_needs(hunger: f32, cleanliness: f32) -> Pet {
        let timestamp = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let mut pet = Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, timestamp);
        pet.needs_mut().set_hunger(hunger);
        pet.needs_mut().set_cleanliness(cleanliness);
        pet
    }

    fn classification(purpose: CommandPurpose) -> CommandClassification {
        CommandClassification::new(CommandCategory::Development, purpose)
    }

    #[test]
    fn structured_defaults_are_decorative_and_uncertain_fails_open() {
        assert_eq!(EnforcementMode::default(), EnforcementMode::Decorative);
        assert_eq!(
            PetSettings::default().enforcement_mode(),
            EnforcementMode::Decorative
        );
        assert_eq!(CommandPurpose::default(), CommandPurpose::Uncertain);

        let pet = pet_with_needs(90.0, 10.0);
        let decision = WorkPermissionPolicy::evaluate(
            &pet,
            &classification(CommandPurpose::Uncertain),
            &PetSettings::default(),
        );
        assert_eq!(decision, WorkDecision::Allowed);
    }

    #[test]
    fn critical_need_boundaries_are_inclusive_and_hunger_wins_ties() {
        let healthy = pet_with_needs(89.99, 10.01);
        let settings = PetSettings::new(EnforcementMode::Gentle);
        let safe_development = classification(CommandPurpose::SafeDevelopment);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&healthy, &safe_development, &settings),
            WorkDecision::Allowed
        );

        let hungry = pet_with_needs(90.0, 10.01);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&hungry, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalHunger,
                required_action: RequiredAction::Feed {
                    minimum_hunger_recovery: 20.0,
                },
            }
        );

        let filthy = pet_with_needs(89.99, 10.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&filthy, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalCleanliness,
                required_action: RequiredAction::Clean {
                    minimum_cleanliness_recovery: 20.0,
                },
            }
        );

        let tied = pet_with_needs(90.0, 10.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&tied, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalHunger,
                required_action: RequiredAction::Feed {
                    minimum_hunger_recovery: 20.0,
                },
            }
        );
    }

    #[test]
    fn decorative_is_always_silent_and_gentle_is_warning_only_at_critical_need() {
        let safe_development = classification(CommandPurpose::SafeDevelopment);
        let healthy = pet_with_needs(20.0, 80.0);
        let critical = pet_with_needs(90.0, 80.0);

        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &critical,
                &safe_development,
                &PetSettings::new(EnforcementMode::Decorative),
            ),
            WorkDecision::Allowed
        );
        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &healthy,
                &safe_development,
                &PetSettings::new(EnforcementMode::Gentle),
            ),
            WorkDecision::Allowed
        );
        assert!(matches!(
            WorkPermissionPolicy::evaluate(
                &critical,
                &safe_development,
                &PetSettings::new(EnforcementMode::Gentle),
            ),
            WorkDecision::Warning { .. }
        ));
    }
}

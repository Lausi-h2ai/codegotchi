use crate::pet::{Pet, PetNeeds};
use serde::{Deserialize, Serialize};

const MINIMUM_HUNGER_RECOVERY: f32 = 20.0;
const MINIMUM_ENERGY_RECOVERY: f32 = 20.0;
const MINIMUM_CLEANLINESS_RECOVERY: f32 = 20.0;
const MINIMUM_HAPPINESS_RECOVERY: f32 = 20.0;

/// Needs are considered mildly neglected at these boundaries. Strict mode
/// starts refusing safe development work from this point on.
const MILD_HUNGER: f32 = 70.0;
const MILD_ENERGY: f32 = 30.0;
const MILD_CLEANLINESS: f32 = 30.0;
const MILD_HAPPINESS: f32 = 30.0;

/// At moderate neglect, strict mode widens refusal to recovery work as well.
const MODERATE_HUNGER: f32 = 85.0;
const MODERATE_ENERGY: f32 = 15.0;
const MODERATE_CLEANLINESS: f32 = 15.0;
const MODERATE_HAPPINESS: f32 = 15.0;

/// At severe neglect, strict mode refuses every tool call except CodeGotchi
/// control, so the caretaker must go care for the pet.
const SEVERE_HUNGER: f32 = 95.0;
const SEVERE_ENERGY: f32 = 5.0;
const SEVERE_CLEANLINESS: f32 = 5.0;
const SEVERE_HAPPINESS: f32 = 5.0;

/// The level at which the policy changes how it reports critical neglect.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementMode {
    /// Record the pet state without affecting or warning about work.
    #[default]
    Decorative,
    /// Allow work while surfacing a care warning at critical neglect.
    Gentle,
    /// Escalating refusal: mild neglect blocks safe development work,
    /// moderate neglect also blocks recovery work, and severe neglect
    /// blocks every tool call except CodeGotchi control.
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
    /// The purpose blocked at the mild neglect tier. Moderate and severe
    /// tiers widen the blocked set further (see `WorkPermissionPolicy`).
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
    CriticalEnergy,
    CriticalCleanliness,
    CriticalHappiness,
}

impl WorkReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CriticalHunger => "critical_hunger",
            Self::CriticalEnergy => "critical_energy",
            Self::CriticalCleanliness => "critical_cleanliness",
            Self::CriticalHappiness => "critical_happiness",
        }
    }
}

/// The structured care action associated with critical neglect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequiredAction {
    Feed { minimum_hunger_recovery: f32 },
    Rest { minimum_energy_recovery: f32 },
    Clean { minimum_cleanliness_recovery: f32 },
    Pet { minimum_happiness_recovery: f32 },
}

impl RequiredAction {
    pub const fn minimum_recovery_points(self) -> f32 {
        match self {
            Self::Feed {
                minimum_hunger_recovery,
            } => minimum_hunger_recovery,
            Self::Rest {
                minimum_energy_recovery,
            } => minimum_energy_recovery,
            Self::Clean {
                minimum_cleanliness_recovery,
            } => minimum_cleanliness_recovery,
            Self::Pet {
                minimum_happiness_recovery,
            } => minimum_happiness_recovery,
        }
    }
}

/// How far a pet's needs have been neglected. Each tier widens the set of
/// tool calls the policy refuses in strict mode.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum NeglectLevel {
    None,
    Mild,
    Moderate,
    Severe,
}

impl NeglectLevel {
    fn from_needs(needs: PetNeeds) -> Self {
        let hunger = needs.hunger();
        let energy = needs.energy();
        let cleanliness = needs.cleanliness();
        let happiness = needs.happiness();
        if hunger >= SEVERE_HUNGER
            || energy <= SEVERE_ENERGY
            || cleanliness <= SEVERE_CLEANLINESS
            || happiness <= SEVERE_HAPPINESS
        {
            Self::Severe
        } else if hunger >= MODERATE_HUNGER
            || energy <= MODERATE_ENERGY
            || cleanliness <= MODERATE_CLEANLINESS
            || happiness <= MODERATE_HAPPINESS
        {
            Self::Moderate
        } else if hunger >= MILD_HUNGER
            || energy <= MILD_ENERGY
            || cleanliness <= MILD_CLEANLINESS
            || happiness <= MILD_HAPPINESS
        {
            Self::Mild
        } else {
            Self::None
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
        let Some((level, reason_code, required_action)) = neglect(pet) else {
            return WorkDecision::Allowed;
        };

        match settings.enforcement_mode() {
            EnforcementMode::Decorative => WorkDecision::Allowed,
            EnforcementMode::Gentle => WorkDecision::Warning {
                reason_code,
                required_action,
            },
            EnforcementMode::Strict if purpose_is_blocked(classification.purpose(), level) => {
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

/// Returns the neglect tier and the dominant reason/action when any need has
/// crossed its mild boundary.
fn neglect(pet: &Pet) -> Option<(NeglectLevel, WorkReasonCode, RequiredAction)> {
    let needs = pet.needs();
    let level = NeglectLevel::from_needs(needs);
    if level == NeglectLevel::None {
        return None;
    }
    let (reason_code, required_action) = dominant_need(needs);
    Some((level, reason_code, required_action))
}

/// Chooses the reason and required action for the most neglected need. Ties
/// are broken by hunger, then energy, cleanliness, then happiness for
/// determinism.
fn dominant_need(needs: PetNeeds) -> (WorkReasonCode, RequiredAction) {
    let hunger_score = needs.hunger() / 100.0;
    let energy_score = (100.0 - needs.energy()) / 100.0;
    let cleanliness_score = (100.0 - needs.cleanliness()) / 100.0;
    let happiness_score = (100.0 - needs.happiness()) / 100.0;

    if hunger_score >= energy_score
        && hunger_score >= cleanliness_score
        && hunger_score >= happiness_score
    {
        (
            WorkReasonCode::CriticalHunger,
            RequiredAction::Feed {
                minimum_hunger_recovery: MINIMUM_HUNGER_RECOVERY,
            },
        )
    } else if energy_score >= cleanliness_score && energy_score >= happiness_score {
        (
            WorkReasonCode::CriticalEnergy,
            RequiredAction::Rest {
                minimum_energy_recovery: MINIMUM_ENERGY_RECOVERY,
            },
        )
    } else if cleanliness_score >= happiness_score {
        (
            WorkReasonCode::CriticalCleanliness,
            RequiredAction::Clean {
                minimum_cleanliness_recovery: MINIMUM_CLEANLINESS_RECOVERY,
            },
        )
    } else {
        (
            WorkReasonCode::CriticalHappiness,
            RequiredAction::Pet {
                minimum_happiness_recovery: MINIMUM_HAPPINESS_RECOVERY,
            },
        )
    }
}

/// Whether strict mode refuses a purpose at the current neglect tier. The
/// blocked set grows as the pet gets more neglected, but CodeGotchi control
/// is always allowed so the caretaker can change enforcement mode.
fn purpose_is_blocked(purpose: CommandPurpose, level: NeglectLevel) -> bool {
    match level {
        NeglectLevel::None => false,
        NeglectLevel::Mild => matches!(purpose, CommandPurpose::SafeDevelopment),
        NeglectLevel::Moderate => !matches!(
            purpose,
            CommandPurpose::CodeGotchiControl | CommandPurpose::Uncertain
        ),
        NeglectLevel::Severe => !matches!(purpose, CommandPurpose::CodeGotchiControl),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use crate::{
        CommandCategory, CommandClassification, CommandPurpose, EnforcementMode, Pet, PetSettings,
        PetSpecies, RequiredAction, WorkDecision, WorkPermissionPolicy, WorkReasonCode,
    };

    fn pet_with_needs(hunger: f32, energy: f32, cleanliness: f32) -> Pet {
        let timestamp = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let mut pet = Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, timestamp);
        pet.needs_mut().set_hunger(hunger);
        pet.needs_mut().set_energy(energy);
        pet.needs_mut().set_cleanliness(cleanliness);
        pet
    }

    fn pet_with_happiness(happiness: f32) -> Pet {
        let mut pet = pet_with_needs(0.0, 100.0, 100.0);
        pet.needs_mut().set_happiness(happiness);
        pet
    }

    fn classification(purpose: CommandPurpose) -> CommandClassification {
        CommandClassification::new(CommandCategory::Development, purpose)
    }

    #[test]
    fn structured_defaults_are_decorative_and_uncertain_fails_open_below_severe() {
        assert_eq!(EnforcementMode::default(), EnforcementMode::Decorative);
        assert_eq!(
            PetSettings::default().enforcement_mode(),
            EnforcementMode::Decorative
        );
        assert_eq!(CommandPurpose::default(), CommandPurpose::Uncertain);

        let moderate = pet_with_needs(85.0, 15.0, 15.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &moderate,
                &classification(CommandPurpose::Uncertain),
                &PetSettings::new(EnforcementMode::Strict),
            ),
            WorkDecision::Allowed
        );

        let severe = pet_with_needs(95.0, 100.0, 100.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &severe,
                &classification(CommandPurpose::Uncertain),
                &PetSettings::new(EnforcementMode::Strict),
            ),
            WorkDecision::Blocked {
                reason_code: WorkReasonCode::CriticalHunger,
                required_action: RequiredAction::Feed {
                    minimum_hunger_recovery: 20.0,
                },
            }
        );
    }

    #[test]
    fn mild_neglect_boundaries_are_inclusive() {
        let settings = PetSettings::new(EnforcementMode::Gentle);
        let safe_development = classification(CommandPurpose::SafeDevelopment);

        let healthy = pet_with_needs(69.99, 30.01, 30.01);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&healthy, &safe_development, &settings),
            WorkDecision::Allowed
        );

        let hungry = pet_with_needs(70.0, 100.0, 100.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&hungry, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalHunger,
                required_action: RequiredAction::Feed {
                    minimum_hunger_recovery: 20.0,
                },
            }
        );

        let tired = pet_with_needs(0.0, 30.0, 100.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&tired, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalEnergy,
                required_action: RequiredAction::Rest {
                    minimum_energy_recovery: 20.0,
                },
            }
        );

        let filthy = pet_with_needs(0.0, 100.0, 30.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&filthy, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalCleanliness,
                required_action: RequiredAction::Clean {
                    minimum_cleanliness_recovery: 20.0,
                },
            }
        );
    }

    #[test]
    fn reason_priority_is_hunger_then_energy_then_cleanliness_on_ties() {
        let settings = PetSettings::new(EnforcementMode::Gentle);
        let safe_development = classification(CommandPurpose::SafeDevelopment);

        let hunger_wins = pet_with_needs(70.0, 30.0, 30.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&hunger_wins, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalHunger,
                required_action: RequiredAction::Feed {
                    minimum_hunger_recovery: 20.0,
                },
            }
        );

        let energy_wins = pet_with_needs(0.0, 30.0, 30.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&energy_wins, &safe_development, &settings),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalEnergy,
                required_action: RequiredAction::Rest {
                    minimum_energy_recovery: 20.0,
                },
            }
        );
    }

    #[test]
    fn strict_blocking_scope_escalates_with_neglect_level() {
        let strict = PetSettings::new(EnforcementMode::Strict);
        let safe = classification(CommandPurpose::SafeDevelopment);
        let shell = classification(CommandPurpose::ShellRecovery);
        let uncertain = classification(CommandPurpose::Uncertain);
        let control = classification(CommandPurpose::CodeGotchiControl);

        let mild = pet_with_needs(70.0, 100.0, 100.0);
        assert!(WorkPermissionPolicy::evaluate(&mild, &safe, &strict).is_blocked());
        assert_eq!(
            WorkPermissionPolicy::evaluate(&mild, &shell, &strict),
            WorkDecision::Allowed
        );

        let moderate = pet_with_needs(85.0, 100.0, 100.0);
        assert!(WorkPermissionPolicy::evaluate(&moderate, &safe, &strict).is_blocked());
        assert!(WorkPermissionPolicy::evaluate(&moderate, &shell, &strict).is_blocked());
        assert_eq!(
            WorkPermissionPolicy::evaluate(&moderate, &uncertain, &strict),
            WorkDecision::Allowed
        );

        let severe = pet_with_needs(95.0, 100.0, 100.0);
        assert!(WorkPermissionPolicy::evaluate(&severe, &safe, &strict).is_blocked());
        assert!(WorkPermissionPolicy::evaluate(&severe, &shell, &strict).is_blocked());
        assert!(WorkPermissionPolicy::evaluate(&severe, &uncertain, &strict).is_blocked());
        assert_eq!(
            WorkPermissionPolicy::evaluate(&severe, &control, &strict),
            WorkDecision::Allowed
        );
    }

    #[test]
    fn happiness_boundaries_escalate_strict_blocking_scope() {
        let strict = PetSettings::new(EnforcementMode::Strict);
        let safe = classification(CommandPurpose::SafeDevelopment);
        let shell = classification(CommandPurpose::ShellRecovery);
        let uncertain = classification(CommandPurpose::Uncertain);
        let control = classification(CommandPurpose::CodeGotchiControl);

        let healthy = pet_with_happiness(30.001);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&healthy, &safe, &strict),
            WorkDecision::Allowed
        );

        let mild = pet_with_happiness(30.0);
        assert_eq!(
            WorkPermissionPolicy::evaluate(&mild, &safe, &strict),
            WorkDecision::Blocked {
                reason_code: WorkReasonCode::CriticalHappiness,
                required_action: RequiredAction::Pet {
                    minimum_happiness_recovery: 20.0,
                },
            }
        );
        assert_eq!(
            WorkPermissionPolicy::evaluate(&mild, &shell, &strict),
            WorkDecision::Allowed
        );

        let moderate = pet_with_happiness(15.0);
        assert!(WorkPermissionPolicy::evaluate(&moderate, &safe, &strict).is_blocked());
        assert!(WorkPermissionPolicy::evaluate(&moderate, &shell, &strict).is_blocked());
        assert_eq!(
            WorkPermissionPolicy::evaluate(&moderate, &uncertain, &strict),
            WorkDecision::Allowed
        );

        let severe = pet_with_happiness(5.0);
        assert!(WorkPermissionPolicy::evaluate(&severe, &safe, &strict).is_blocked());
        assert!(WorkPermissionPolicy::evaluate(&severe, &shell, &strict).is_blocked());
        assert!(WorkPermissionPolicy::evaluate(&severe, &uncertain, &strict).is_blocked());
        assert_eq!(
            WorkPermissionPolicy::evaluate(&severe, &control, &strict),
            WorkDecision::Allowed
        );
    }

    #[test]
    fn happiness_dominates_more_neglected_needs_with_structured_pet_action() {
        let mut pet = pet_with_needs(20.0, 80.0, 80.0);
        pet.needs_mut().set_happiness(10.0);
        let gentle = PetSettings::new(EnforcementMode::Gentle);

        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &pet,
                &classification(CommandPurpose::SafeDevelopment),
                &gentle,
            ),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalHappiness,
                required_action: RequiredAction::Pet {
                    minimum_happiness_recovery: 20.0,
                },
            }
        );
    }

    #[test]
    fn cleanliness_wins_ties_over_happiness_in_dominant_need() {
        let mut pet = pet_with_needs(0.0, 100.0, 30.0);
        pet.needs_mut().set_happiness(30.0);

        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &pet,
                &classification(CommandPurpose::SafeDevelopment),
                &PetSettings::new(EnforcementMode::Gentle),
            ),
            WorkDecision::Warning {
                reason_code: WorkReasonCode::CriticalCleanliness,
                required_action: RequiredAction::Clean {
                    minimum_cleanliness_recovery: 20.0,
                },
            }
        );
    }

    #[test]
    fn decorative_is_silent_and_gentle_warns_from_mild_neglect() {
        let safe_development = classification(CommandPurpose::SafeDevelopment);
        let healthy = pet_with_needs(20.0, 80.0, 80.0);
        let mild = pet_with_needs(70.0, 80.0, 80.0);

        assert_eq!(
            WorkPermissionPolicy::evaluate(
                &mild,
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
                &mild,
                &safe_development,
                &PetSettings::new(EnforcementMode::Gentle),
            ),
            WorkDecision::Warning { .. }
        ));
    }
}

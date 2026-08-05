use chrono::{Duration, TimeZone, Utc};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, CareCommand, CareResult, CommandCategory,
    CommandClassification, CommandPurpose, DefaultNeedProgressionStrategy, EnforcementMode,
    EventMetadata, EventSource, FakeClock, FoodInventory, FoodKind, Pet, PetSettings,
    PetSimulation, PetSpecies, RequiredAction, WorkDecision, WorkPermissionPolicy, WorkReasonCode,
};
use uuid::Uuid;

type Simulation = PetSimulation<FakeClock, DefaultNeedProgressionStrategy>;

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
}

fn classification(purpose: CommandPurpose) -> CommandClassification {
    let category = match purpose {
        CommandPurpose::SafeDevelopment => CommandCategory::Development,
        CommandPurpose::CodeGotchiControl => CommandCategory::CodeGotchi,
        CommandPurpose::ProcessRecovery => CommandCategory::Process,
        CommandPurpose::ShellRecovery => CommandCategory::Shell,
        CommandPurpose::GitRecovery => CommandCategory::Git,
        CommandPurpose::InfrastructureShutdown => CommandCategory::Infrastructure,
        CommandPurpose::SecurityRemediation => CommandCategory::Security,
        CommandPurpose::Uncertain => CommandCategory::Unknown,
    };
    CommandClassification::new(category, purpose)
}

fn healthy_simulation() -> Simulation {
    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start());
    PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy)
}

fn hunger_critical_simulation() -> Simulation {
    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(2), "Mochi", PetSpecies::Cat, start());
    let mut simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);
    clock.advance(Duration::hours(90));
    simulation.current_state();
    assert_eq!(simulation.pet().needs().hunger(), 90.0);
    simulation
}

fn clean_critical_simulation() -> Simulation {
    let mut inventory = FoodInventory::default();
    inventory.add(FoodKind::Kibble, 3);
    let clock = FakeClock::new(start());
    let pet = Pet::with_inventory(
        Uuid::from_u128(3),
        "Mochi",
        PetSpecies::Cat,
        start(),
        inventory,
    );
    let mut simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);

    for action_id in 1..=3 {
        assert_eq!(
            simulation
                .apply_care(&CareCommand::Feed {
                    action_id: Uuid::from_u128(action_id),
                    food_id: "kibble".to_owned(),
                })
                .unwrap(),
            CareResult::Applied
        );
    }

    simulation
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(10),
            Uuid::from_u128(20),
            "repo",
            EventSource::Codex,
            AgentEventKind::SessionStarted,
            None,
            start(),
            EventMetadata::default(),
        ))
        .unwrap();
    for event_id in 11..=20 {
        simulation
            .apply_event(&AgentEvent::new(
                Uuid::from_u128(event_id),
                Uuid::from_u128(20),
                "repo",
                EventSource::Codex,
                AgentEventKind::CommandStarted,
                Some(ActivityKind::Editing),
                start(),
                EventMetadata::default(),
            ))
            .unwrap();
    }
    simulation
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(21),
            Uuid::from_u128(20),
            "repo",
            EventSource::Codex,
            AgentEventKind::TurnCompleted,
            None,
            start(),
            EventMetadata::default(),
        ))
        .unwrap();

    clock.advance(Duration::hours(45));
    simulation.current_state();
    assert_eq!(simulation.pet().needs().hunger(), 45.0);
    assert_eq!(simulation.pet().needs().cleanliness(), 10.0);
    assert_eq!(simulation.pet().pending_poops().len(), 1);
    simulation
}

fn both_needs_critical_simulation() -> Simulation {
    let mut simulation = clean_critical_simulation();
    simulation
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(22),
            Uuid::from_u128(21),
            "repo",
            EventSource::Codex,
            AgentEventKind::SessionStarted,
            None,
            start() + Duration::hours(90),
            EventMetadata::default(),
        ))
        .unwrap();
    assert_eq!(simulation.pet().needs().hunger(), 90.0);
    assert_eq!(simulation.pet().needs().cleanliness(), 0.0);
    simulation
}

#[test]
fn literal_matrix_all_modes_and_all_exempt_purposes_is_fail_open() {
    let purposes = [
        CommandPurpose::CodeGotchiControl,
        CommandPurpose::ProcessRecovery,
        CommandPurpose::ShellRecovery,
        CommandPurpose::GitRecovery,
        CommandPurpose::InfrastructureShutdown,
        CommandPurpose::SecurityRemediation,
        CommandPurpose::Uncertain,
    ];
    let modes = [
        EnforcementMode::Decorative,
        EnforcementMode::Gentle,
        EnforcementMode::Strict,
    ];

    for mode in modes {
        let settings = PetSettings::new(mode);
        for purpose in purposes {
            let healthy = healthy_simulation();
            assert_eq!(
                WorkPermissionPolicy::evaluate(healthy.pet(), &classification(purpose), &settings,),
                WorkDecision::Allowed,
                "healthy {mode:?} {purpose:?}"
            );

            let hungry = hunger_critical_simulation();
            assert_eq!(
                WorkPermissionPolicy::evaluate(hungry.pet(), &classification(purpose), &settings,),
                if mode == EnforcementMode::Gentle {
                    WorkDecision::Warning {
                        reason_code: WorkReasonCode::CriticalHunger,
                        required_action: RequiredAction::Feed {
                            minimum_hunger_recovery: 20.0,
                        },
                    }
                } else {
                    WorkDecision::Allowed
                },
                "hungry {mode:?} {purpose:?}"
            );

            let filthy = clean_critical_simulation();
            assert_eq!(
                WorkPermissionPolicy::evaluate(filthy.pet(), &classification(purpose), &settings,),
                if mode == EnforcementMode::Gentle {
                    WorkDecision::Warning {
                        reason_code: WorkReasonCode::CriticalCleanliness,
                        required_action: RequiredAction::Clean {
                            minimum_cleanliness_recovery: 20.0,
                        },
                    }
                } else {
                    WorkDecision::Allowed
                },
                "filthy {mode:?} {purpose:?}"
            );

            let both_critical = both_needs_critical_simulation();
            assert_eq!(
                WorkPermissionPolicy::evaluate(
                    both_critical.pet(),
                    &classification(purpose),
                    &settings,
                ),
                if mode == EnforcementMode::Gentle {
                    WorkDecision::Warning {
                        reason_code: WorkReasonCode::CriticalHunger,
                        required_action: RequiredAction::Feed {
                            minimum_hunger_recovery: 20.0,
                        },
                    }
                } else {
                    WorkDecision::Allowed
                },
                "both critical {mode:?} {purpose:?}"
            );
        }
    }
}

#[test]
fn strict_safe_development_matrix_blocks_only_critical_neglect() {
    let purpose = classification(CommandPurpose::SafeDevelopment);
    let strict = PetSettings::new(EnforcementMode::Strict);

    assert_eq!(
        WorkPermissionPolicy::evaluate(healthy_simulation().pet(), &purpose, &strict),
        WorkDecision::Allowed
    );
    assert_eq!(
        WorkPermissionPolicy::evaluate(hunger_critical_simulation().pet(), &purpose, &strict,),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalHunger,
            required_action: RequiredAction::Feed {
                minimum_hunger_recovery: 20.0,
            },
        }
    );
    assert_eq!(
        WorkPermissionPolicy::evaluate(clean_critical_simulation().pet(), &purpose, &strict),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalCleanliness,
            required_action: RequiredAction::Clean {
                minimum_cleanliness_recovery: 20.0,
            },
        }
    );
}

#[test]
fn strict_feed_recovery_moves_a_real_pet_from_blocked_to_allowed() {
    let mut inventory = FoodInventory::default();
    inventory.add(FoodKind::Kibble, 1);
    let clock = FakeClock::new(start());
    let pet = Pet::with_inventory(
        Uuid::from_u128(40),
        "Mochi",
        PetSpecies::Cat,
        start(),
        inventory,
    );
    let mut simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);
    clock.advance(Duration::hours(90));
    simulation.current_state();

    let command = classification(CommandPurpose::SafeDevelopment);
    let settings = PetSettings::new(EnforcementMode::Strict);
    assert!(matches!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &command, &settings),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalHunger,
            required_action: RequiredAction::Feed {
                minimum_hunger_recovery: 20.0
            }
        }
    ));

    assert_eq!(
        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(41),
                food_id: "kibble".to_owned(),
            })
            .unwrap(),
        CareResult::Applied
    );
    assert_eq!(simulation.pet().needs().hunger(), 65.0);
    assert_eq!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &command, &settings),
        WorkDecision::Allowed
    );
}

#[test]
fn strict_clean_recovery_moves_a_real_pet_from_blocked_to_allowed() {
    let mut simulation = clean_critical_simulation();
    let poop_id = simulation.pet().pending_poops()[0].id();
    let command = classification(CommandPurpose::SafeDevelopment);
    let settings = PetSettings::new(EnforcementMode::Strict);

    assert_eq!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &command, &settings),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalCleanliness,
            required_action: RequiredAction::Clean {
                minimum_cleanliness_recovery: 20.0,
            },
        }
    );

    assert_eq!(
        simulation
            .apply_care(&CareCommand::CleanPoop {
                action_id: Uuid::from_u128(30),
                poop_id,
            })
            .unwrap(),
        CareResult::Applied
    );
    assert_eq!(simulation.pet().needs().cleanliness(), 35.0);
    assert_eq!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &command, &settings),
        WorkDecision::Allowed
    );
}

#[test]
fn structured_reason_codes_are_stable() {
    assert_eq!(WorkReasonCode::CriticalHunger.as_str(), "critical_hunger");
    assert_eq!(
        WorkReasonCode::CriticalCleanliness.as_str(),
        "critical_cleanliness"
    );
}

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

/// Hunger rises at 1 point per idle hour, so advancing the clock by
/// `target_hunger` hours lands exactly on the requested need.
fn hunger_simulation(target_hunger: f32) -> Simulation {
    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(2), "Mochi", PetSpecies::Cat, start());
    let mut simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);
    clock.advance(Duration::hours(target_hunger as i64));
    simulation.current_state();
    assert_eq!(simulation.pet().needs().hunger(), target_hunger);
    simulation
}

/// Energy drains at 6 points per active hour, so an active session advanced
/// by `(100 - target_energy) / 6` hours lands on the requested need while
/// hunger stays below the mild boundary.
fn energy_simulation(target_energy: f32) -> Simulation {
    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(4), "Mochi", PetSpecies::Cat, start());
    let mut simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);
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
    simulation
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(11),
            Uuid::from_u128(20),
            "repo",
            EventSource::Codex,
            AgentEventKind::ToolStarted,
            Some(ActivityKind::Testing),
            start(),
            EventMetadata::default(),
        ))
        .unwrap();
    let active_hours = (100.0 - target_energy) / 6.0;
    clock.advance(Duration::minutes((active_hours * 60.0) as i64));
    simulation.current_state();
    let energy = simulation.pet().needs().energy();
    assert!((energy - target_energy).abs() <= 1.0, "energy was {energy}");
    assert!(simulation.pet().needs().hunger() < 70.0);
    simulation
}

/// One poop on the floor drains cleanliness at 2 points per idle hour, so
/// advancing `(100 - target_cleanliness) / 2` hours lands on the requested
/// need while hunger stays below the mild boundary.
fn cleanliness_simulation(target_cleanliness: f32) -> Simulation {
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

    let idle_hours = (100.0 - target_cleanliness) / 2.0;
    clock.advance(Duration::minutes((idle_hours * 60.0) as i64));
    simulation.current_state();
    let cleanliness = simulation.pet().needs().cleanliness();
    assert!(
        (cleanliness - target_cleanliness).abs() <= 1.0,
        "cleanliness was {cleanliness}"
    );
    assert_eq!(simulation.pet().pending_poops().len(), 1);
    assert!(simulation.pet().needs().hunger() < 70.0);
    simulation
}

/// A hungry pet that also has a poop on the floor. Cleanliness is worse than
/// hunger here (0.0 vs 90.0 on the inverted need scales).
fn both_needs_critical_simulation() -> Simulation {
    let mut simulation = cleanliness_simulation(10.0);
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
fn healthy_pet_allows_every_purpose_in_every_mode() {
    let purposes = [
        CommandPurpose::SafeDevelopment,
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
        for purpose in purposes {
            let healthy = healthy_simulation();
            assert_eq!(
                WorkPermissionPolicy::evaluate(
                    healthy.pet(),
                    &classification(purpose),
                    &PetSettings::new(mode),
                ),
                WorkDecision::Allowed,
                "healthy {mode:?} {purpose:?}"
            );
        }
    }
}

#[test]
fn mild_neglect_blocks_safe_development_only_and_warns_in_gentle() {
    let mild_cases = [
        (
            hunger_simulation(70.0),
            WorkReasonCode::CriticalHunger,
            RequiredAction::Feed {
                minimum_hunger_recovery: 20.0,
            },
        ),
        (
            energy_simulation(30.0),
            WorkReasonCode::CriticalEnergy,
            RequiredAction::Rest {
                minimum_energy_recovery: 20.0,
            },
        ),
        (
            cleanliness_simulation(30.0),
            WorkReasonCode::CriticalCleanliness,
            RequiredAction::Clean {
                minimum_cleanliness_recovery: 20.0,
            },
        ),
    ];

    for (simulation, reason_code, required_action) in mild_cases {
        let strict = PetSettings::new(EnforcementMode::Strict);
        assert_eq!(
            WorkPermissionPolicy::evaluate(
                simulation.pet(),
                &classification(CommandPurpose::SafeDevelopment),
                &strict,
            ),
            WorkDecision::Blocked {
                reason_code,
                required_action
            }
        );
        for purpose in [
            CommandPurpose::CodeGotchiControl,
            CommandPurpose::ProcessRecovery,
            CommandPurpose::ShellRecovery,
            CommandPurpose::GitRecovery,
            CommandPurpose::InfrastructureShutdown,
            CommandPurpose::SecurityRemediation,
            CommandPurpose::Uncertain,
        ] {
            assert_eq!(
                WorkPermissionPolicy::evaluate(simulation.pet(), &classification(purpose), &strict,),
                WorkDecision::Allowed,
                "mild neglect {purpose:?}"
            );
        }
        assert_eq!(
            WorkPermissionPolicy::evaluate(
                simulation.pet(),
                &classification(CommandPurpose::SafeDevelopment),
                &PetSettings::new(EnforcementMode::Gentle),
            ),
            WorkDecision::Warning {
                reason_code,
                required_action
            }
        );
    }
}

#[test]
fn moderate_neglect_blocks_safe_and_recovery_work_but_keeps_control_and_uncertain() {
    let moderate_cases = [
        (
            hunger_simulation(85.0),
            WorkReasonCode::CriticalHunger,
            RequiredAction::Feed {
                minimum_hunger_recovery: 20.0,
            },
        ),
        (
            energy_simulation(15.0),
            WorkReasonCode::CriticalEnergy,
            RequiredAction::Rest {
                minimum_energy_recovery: 20.0,
            },
        ),
        (
            cleanliness_simulation(15.0),
            WorkReasonCode::CriticalCleanliness,
            RequiredAction::Clean {
                minimum_cleanliness_recovery: 20.0,
            },
        ),
    ];

    for (simulation, reason_code, required_action) in moderate_cases {
        let strict = PetSettings::new(EnforcementMode::Strict);
        for purpose in [
            CommandPurpose::SafeDevelopment,
            CommandPurpose::ProcessRecovery,
            CommandPurpose::ShellRecovery,
            CommandPurpose::GitRecovery,
            CommandPurpose::InfrastructureShutdown,
            CommandPurpose::SecurityRemediation,
        ] {
            assert_eq!(
                WorkPermissionPolicy::evaluate(simulation.pet(), &classification(purpose), &strict,),
                WorkDecision::Blocked {
                    reason_code,
                    required_action
                },
                "moderate neglect {purpose:?}"
            );
        }
        for purpose in [CommandPurpose::CodeGotchiControl, CommandPurpose::Uncertain] {
            assert_eq!(
                WorkPermissionPolicy::evaluate(simulation.pet(), &classification(purpose), &strict,),
                WorkDecision::Allowed,
                "moderate neglect exempt {purpose:?}"
            );
        }
    }
}

#[test]
fn severe_neglect_blocks_every_tool_call_except_codegotchi_control() {
    let severe_cases = [
        (
            hunger_simulation(95.0),
            WorkReasonCode::CriticalHunger,
            RequiredAction::Feed {
                minimum_hunger_recovery: 20.0,
            },
        ),
        (
            energy_simulation(5.0),
            WorkReasonCode::CriticalEnergy,
            RequiredAction::Rest {
                minimum_energy_recovery: 20.0,
            },
        ),
        (
            cleanliness_simulation(5.0),
            WorkReasonCode::CriticalCleanliness,
            RequiredAction::Clean {
                minimum_cleanliness_recovery: 20.0,
            },
        ),
    ];

    for (simulation, reason_code, required_action) in severe_cases {
        let strict = PetSettings::new(EnforcementMode::Strict);
        for purpose in [
            CommandPurpose::SafeDevelopment,
            CommandPurpose::ProcessRecovery,
            CommandPurpose::ShellRecovery,
            CommandPurpose::GitRecovery,
            CommandPurpose::InfrastructureShutdown,
            CommandPurpose::SecurityRemediation,
            CommandPurpose::Uncertain,
        ] {
            assert_eq!(
                WorkPermissionPolicy::evaluate(simulation.pet(), &classification(purpose), &strict,),
                WorkDecision::Blocked {
                    reason_code,
                    required_action
                },
                "severe neglect {purpose:?}"
            );
        }
        assert_eq!(
            WorkPermissionPolicy::evaluate(
                simulation.pet(),
                &classification(CommandPurpose::CodeGotchiControl),
                &strict,
            ),
            WorkDecision::Allowed
        );
    }
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
    assert_eq!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &command, &settings),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalHunger,
            required_action: RequiredAction::Feed {
                minimum_hunger_recovery: 20.0
            }
        }
    );

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
    let mut simulation = cleanliness_simulation(10.0);
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
fn exhausted_pet_unblocks_after_resting() {
    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(5), "Mochi", PetSpecies::Cat, start());
    let mut simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);
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
    simulation
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(11),
            Uuid::from_u128(20),
            "repo",
            EventSource::Codex,
            AgentEventKind::ToolStarted,
            Some(ActivityKind::Testing),
            start(),
            EventMetadata::default(),
        ))
        .unwrap();
    clock.advance(Duration::minutes(850));
    simulation.current_state();
    let strict = PetSettings::new(EnforcementMode::Strict);
    let shell = classification(CommandPurpose::ShellRecovery);
    assert_eq!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &shell, &strict),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalEnergy,
            required_action: RequiredAction::Rest {
                minimum_energy_recovery: 20.0,
            },
        }
    );

    simulation
        .apply_event(&AgentEvent::new(
            Uuid::from_u128(12),
            Uuid::from_u128(20),
            "repo",
            EventSource::Codex,
            AgentEventKind::ToolCompleted,
            Some(ActivityKind::Testing),
            start(),
            EventMetadata::default(),
        ))
        .unwrap();
    clock.advance(Duration::minutes(120));
    simulation.current_state();
    assert!(simulation.pet().needs().energy() > 30.0);
    assert_eq!(
        WorkPermissionPolicy::evaluate(simulation.pet(), &shell, &strict),
        WorkDecision::Allowed
    );
}

#[test]
fn worst_need_dominates_and_cleanliness_beats_hunger_when_filthier() {
    let simulation = both_needs_critical_simulation();
    assert_eq!(
        WorkPermissionPolicy::evaluate(
            simulation.pet(),
            &classification(CommandPurpose::Uncertain),
            &PetSettings::new(EnforcementMode::Strict),
        ),
        WorkDecision::Blocked {
            reason_code: WorkReasonCode::CriticalCleanliness,
            required_action: RequiredAction::Clean {
                minimum_cleanliness_recovery: 20.0,
            },
        }
    );
}

#[test]
fn structured_reason_codes_are_stable() {
    assert_eq!(WorkReasonCode::CriticalHunger.as_str(), "critical_hunger");
    assert_eq!(WorkReasonCode::CriticalEnergy.as_str(), "critical_energy");
    assert_eq!(
        WorkReasonCode::CriticalCleanliness.as_str(),
        "critical_cleanliness"
    );
}

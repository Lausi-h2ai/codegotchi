use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Duration, TimeZone, Utc};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, CareCommand, CareError, CareResult, Clock,
    DefaultNeedProgressionStrategy, EventMetadata, EventSource, FakeClock, FoodInventory, FoodKind,
    NAP_DURATION, Pet, PetBehavior, PetSimulation, PetSpecies, PoopGenerationStrategy,
    PoopGenerationThreshold, RandomSource, SeededRandomSource,
};
use uuid::Uuid;

type Simulation = PetSimulation<FakeClock, DefaultNeedProgressionStrategy>;

fn start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
}

fn inventory(food: FoodKind, amount: u32) -> FoodInventory {
    let mut inventory = FoodInventory::default();
    inventory.add(food, amount);
    inventory
}

fn without_attention(simulation: Simulation, clock: &FakeClock) -> Simulation {
    let mut snapshot = simulation.snapshot();
    snapshot.next_incident_at = Some(start() + Duration::days(365));
    PetSimulation::from_snapshot(snapshot, clock.clone(), DefaultNeedProgressionStrategy).unwrap()
}

fn simulation_with_inventory(food: FoodKind, amount: u32) -> (FakeClock, Simulation) {
    let clock = FakeClock::new(start());
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        start(),
        inventory(food, amount),
    );
    let simulation = without_attention(
        PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy),
        &clock,
    );
    (clock, simulation)
}

fn event(id: u128, session_id: u128, kind: AgentEventKind, timestamp: DateTime<Utc>) -> AgentEvent {
    AgentEvent::new(
        Uuid::from_u128(id),
        Uuid::from_u128(session_id),
        "repo",
        EventSource::Codex,
        kind,
        Some(ActivityKind::Testing),
        timestamp,
        EventMetadata::default(),
    )
}

fn failure_event(id: u128, session_id: u128, timestamp: DateTime<Utc>) -> AgentEvent {
    let mut event = event(id, session_id, AgentEventKind::CommandCompleted, timestamp);
    event.metadata.exit_status = Some(1);
    event
}

#[test]
fn care_flow_seeds_inventory_feeds_works_cleans_and_replays_authoritatively() {
    let clock = FakeClock::new(start());
    let mut seeded = FoodInventory::default();
    seeded.add(FoodKind::Kibble, 3);
    let pet = Pet::with_inventory(
        Uuid::from_u128(42),
        "Mochi",
        PetSpecies::Cat,
        start(),
        seeded,
    );
    let mut simulation = PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy);

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
        .apply_event(&event(100, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();
    for event_id in 101..=110 {
        simulation
            .apply_event(&event(event_id, 7, AgentEventKind::CommandStarted, start()))
            .unwrap();
    }

    assert_eq!(simulation.pet().inventory().count(FoodKind::Kibble), 0);
    assert_eq!(simulation.pet().digestion_points(), 20);
    assert_eq!(simulation.pet().work_points(), 0);
    assert_eq!(simulation.pet().pending_poops().len(), 1);
    let poop = simulation.pet().pending_poops()[0];
    assert_eq!(poop.id(), Uuid::new_v5(&Uuid::from_u128(42), b"poop:0"));
    assert_eq!(poop.created_at(), start());
    assert_eq!(simulation.snapshot().poop_sequence, 1);

    assert_eq!(
        simulation
            .apply_care(&CareCommand::CleanPoop {
                action_id: Uuid::from_u128(200),
                poop_id: poop.id(),
            })
            .unwrap(),
        CareResult::Applied
    );
    assert!(simulation.pet().pending_poops().is_empty());
    assert_eq!(simulation.pet().needs().cleanliness(), 100.0);
    assert!(
        simulation
            .snapshot()
            .processed_care_ids
            .contains(&Uuid::from_u128(200))
    );

    let before_duplicates = simulation.snapshot();
    assert_eq!(
        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(1),
                food_id: "not-a-food".to_owned(),
            })
            .unwrap(),
        CareResult::Duplicate
    );
    assert_eq!(
        simulation
            .apply_care(&CareCommand::CleanPoop {
                action_id: Uuid::from_u128(200),
                poop_id: Uuid::from_u128(999),
            })
            .unwrap(),
        CareResult::Duplicate
    );
    assert_eq!(simulation.snapshot(), before_duplicates);
}

#[test]
fn unlimited_care_items_apply_effects_without_depleting() {
    let clock = FakeClock::new(start());
    let pet = Pet::with_inventory(
        Uuid::from_u128(43),
        "Mochi",
        PetSpecies::Cat,
        start(),
        FoodInventory::unlimited(),
    );
    let mut simulation = without_attention(
        PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy),
        &FakeClock::new(start()),
    );

    for action_id in 1..=100 {
        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(action_id),
                food_id: "kibble".to_owned(),
            })
            .unwrap();
    }

    assert_eq!(
        simulation.pet().inventory().count(FoodKind::Kibble),
        u32::MAX
    );
    assert_eq!(simulation.pet().needs().hunger(), 0.0);
}

#[test]
fn all_foods_apply_literal_effects_and_consume_one_authoritative_item() {
    let cases = [
        (FoodKind::Kibble, "kibble", 25.0, 40),
        (FoodKind::Treat, "treat", 10.0, 20),
        (FoodKind::Fruit, "fruit", 15.0, 25),
    ];

    for (food, food_id, hunger_reduction, digestion) in cases {
        let (clock, mut simulation) = simulation_with_inventory(food, 1);
        clock.advance(Duration::hours(50));
        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::new_v4(),
                food_id: food_id.to_owned(),
            })
            .unwrap();

        assert_eq!(simulation.pet().needs().hunger(), 100.0 - hunger_reduction);
        assert_eq!(simulation.pet().digestion_points(), digestion);
        assert_eq!(simulation.pet().inventory().count(food), 0);
    }
}

#[test]
fn fruit_restores_a_small_amount_of_energy() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Fruit, 1);
    clock.advance(Duration::hours(1));

    let before = simulation.current_state();
    assert_eq!(before.needs.energy(), 50.0);

    simulation
        .apply_care(&CareCommand::Feed {
            action_id: Uuid::from_u128(300),
            food_id: "fruit".to_owned(),
        })
        .unwrap();

    let after = simulation.snapshot();
    assert_eq!(after.needs.energy(), 55.0);
    assert_eq!(after.needs.energy() - before.needs.energy(), 5.0);
}

#[test]
fn care_need_effects_are_exact_at_unclamped_public_baselines() {
    let baseline = start() + Duration::minutes(96);

    let (clock, mut kibble) = simulation_with_inventory(FoodKind::Kibble, 1);
    kibble
        .apply_event(&event(401, 7, AgentEventKind::SessionStarted, baseline))
        .unwrap();
    clock.set(baseline);
    let before_kibble = kibble.snapshot();
    assert_eq!(before_kibble.needs.hunger(), 40.0);
    assert_eq!(
        kibble
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(402),
                food_id: "kibble".to_owned(),
            })
            .unwrap(),
        CareResult::Applied
    );
    let after_kibble = kibble.snapshot();
    assert_eq!(after_kibble.needs.hunger(), 15.0);
    assert_eq!(
        after_kibble.needs.hunger() - before_kibble.needs.hunger(),
        -25.0
    );
    assert_eq!(after_kibble.digestion_points, 40);

    let (clock, mut fruit) = simulation_with_inventory(FoodKind::Fruit, 1);
    fruit
        .apply_event(&event(403, 7, AgentEventKind::SessionStarted, baseline))
        .unwrap();
    clock.set(baseline);
    let before_fruit = fruit.snapshot();
    fruit
        .apply_care(&CareCommand::Feed {
            action_id: Uuid::from_u128(404),
            food_id: "fruit".to_owned(),
        })
        .unwrap();
    let after_fruit = fruit.snapshot();
    assert_eq!(
        after_fruit.needs.hunger() - before_fruit.needs.hunger(),
        -15.0
    );
    assert_eq!(after_fruit.digestion_points, 25);

    let (clock, mut treat) = simulation_with_inventory(FoodKind::Treat, 1);
    treat
        .apply_event(&event(405, 7, AgentEventKind::SessionStarted, baseline))
        .unwrap();
    for event_id in 406..=408 {
        treat
            .apply_event(&failure_event(event_id, 7, baseline))
            .unwrap();
    }
    clock.set(baseline);
    let before_treat = treat.snapshot();
    assert_eq!(before_treat.needs.hunger(), 40.0);
    assert_eq!(before_treat.needs.happiness(), 76.0);
    treat
        .apply_care(&CareCommand::Feed {
            action_id: Uuid::from_u128(409),
            food_id: "treat".to_owned(),
        })
        .unwrap();
    let after_treat = treat.snapshot();
    assert_eq!(
        after_treat.needs.hunger() - before_treat.needs.hunger(),
        -10.0
    );
    assert_eq!(
        after_treat.needs.happiness() - before_treat.needs.happiness(),
        5.0
    );
    assert_eq!(after_treat.digestion_points, 20);

    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(410), "Mochi", PetSpecies::Cat, start());
    let mut petting = without_attention(
        PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy),
        &clock,
    );
    petting
        .apply_event(&event(411, 7, AgentEventKind::SessionStarted, baseline))
        .unwrap();
    for event_id in 412..=414 {
        petting
            .apply_event(&failure_event(event_id, 7, baseline))
            .unwrap();
    }
    clock.set(baseline);
    let before_petting = petting.snapshot();
    assert_eq!(before_petting.needs.happiness(), 76.0);
    petting
        .apply_care(&CareCommand::Pet {
            action_id: Uuid::from_u128(415),
            interaction_ms: 1_500,
            pointer_distance: 120.0,
        })
        .unwrap();
    let after_petting = petting.snapshot();
    assert_eq!(
        after_petting.needs.happiness() - before_petting.needs.happiness(),
        10.0
    );

    let (clock, mut cleaning) = simulation_with_inventory(FoodKind::Kibble, 3);
    cleaning
        .apply_event(&event(416, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();
    for action_id in 417..=419 {
        cleaning
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(action_id),
                food_id: "kibble".to_owned(),
            })
            .unwrap();
    }
    for event_id in 420..=429 {
        cleaning
            .apply_event(&event(event_id, 7, AgentEventKind::CommandStarted, start()))
            .unwrap();
    }
    cleaning
        .apply_event(&event(430, 7, AgentEventKind::SessionEnded, start()))
        .unwrap();
    clock.advance(Duration::minutes(10));
    let before_cleaning = cleaning.current_state();
    assert_eq!(before_cleaning.needs.cleanliness(), 60.0);
    let poop_id = before_cleaning.pending_poops[0].id();
    cleaning
        .apply_care(&CareCommand::CleanPoop {
            action_id: Uuid::from_u128(431),
            poop_id,
        })
        .unwrap();
    let after_cleaning = cleaning.snapshot();
    assert_eq!(
        after_cleaning.needs.cleanliness() - before_cleaning.needs.cleanliness(),
        25.0
    );
    assert_eq!(after_cleaning.needs.cleanliness(), 85.0);
}

#[test]
fn energy_drink_restores_energy_and_gives_a_sugar_rush_without_digestion() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::EnergyDrink, 1);
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();
    simulation
        .apply_event(&event(2, 7, AgentEventKind::CommandStarted, start()))
        .unwrap();
    clock.advance(Duration::minutes(72));
    let drained = simulation.current_state();
    assert_eq!(drained.needs.energy(), 40.0);
    assert_eq!(drained.needs.happiness(), 100.0);
    simulation
        .apply_event(&failure_event(3, 7, start() + Duration::minutes(72)))
        .unwrap();
    simulation
        .apply_event(&failure_event(4, 7, start() + Duration::minutes(72)))
        .unwrap();
    let before = simulation.current_state();
    assert_eq!(before.needs.energy(), 40.0);
    assert_eq!(before.needs.happiness(), 88.0);
    assert_eq!(before.digestion_points, 0);

    simulation
        .apply_care(&CareCommand::Feed {
            action_id: Uuid::from_u128(5),
            food_id: "energy_drink".to_owned(),
        })
        .unwrap();

    let after = simulation.snapshot();
    assert_eq!(after.needs.energy(), 80.0);
    assert_eq!(after.needs.happiness(), 93.0);
    assert_eq!(after.needs.hunger(), before.needs.hunger());
    assert_eq!(after.digestion_points, 0);
    assert_eq!(simulation.pet().inventory().count(FoodKind::EnergyDrink), 0);
}

#[test]
fn hammock_nap_restores_energy_over_five_seconds_and_clears_itself() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 0);
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();
    simulation
        .apply_event(&event(2, 7, AgentEventKind::CommandStarted, start()))
        .unwrap();
    clock.advance(Duration::hours(20));
    let tired = simulation.current_state();
    assert_eq!(tired.needs.energy(), 0.0);

    assert_eq!(
        simulation
            .apply_care(&CareCommand::Nap {
                action_id: Uuid::from_u128(3),
            })
            .unwrap(),
        CareResult::Applied
    );
    let napping = simulation.snapshot();
    assert_eq!(napping.needs.energy(), 0.0);
    assert_eq!(napping.behavior, PetBehavior::Sleeping);
    assert_eq!(
        napping.napping_until,
        Some(start() + Duration::hours(20) + NAP_DURATION)
    );

    clock.advance(Duration::seconds(1));
    assert_eq!(simulation.current_state().needs.energy(), 20.0);
    assert_eq!(simulation.current_state().behavior, PetBehavior::Sleeping);

    clock.advance(Duration::seconds(3));
    assert_eq!(simulation.current_state().needs.energy(), 80.0);
    assert_eq!(simulation.current_state().behavior, PetBehavior::Sleeping);

    clock.advance(Duration::seconds(1));
    let finished = simulation.current_state();
    assert_eq!(finished.needs.energy(), 100.0);
    assert_eq!(finished.napping_until, None);
}

#[test]
fn debug_neglect_drains_needs_at_the_current_clock_without_freezing_the_timeline() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 0);
    simulation.apply_debug_neglect();

    let neglected = simulation.snapshot();
    assert_eq!(neglected.needs.hunger(), 100.0);
    assert_eq!(neglected.needs.energy(), 0.0);
    assert_eq!(neglected.behavior, PetBehavior::CriticalNeed);
    assert!(
        neglected.last_updated_at < start() + Duration::hours(1),
        "demo neglect must not jump the logical clock far into the future"
    );

    // The world keeps moving: a hammock nap immediately recovers the drained
    // meter over its normal five seconds.
    simulation
        .apply_care(&CareCommand::Nap {
            action_id: Uuid::from_u128(60),
        })
        .unwrap();
    clock.advance(Duration::seconds(5));
    let finished = simulation.current_state();
    assert_eq!(finished.needs.energy(), 100.0);
    assert_eq!(finished.napping_until, None);
}

#[test]
fn nap_is_replay_safe_and_can_restart_from_a_new_action() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 0);
    let nap = CareCommand::Nap {
        action_id: Uuid::from_u128(5),
    };
    assert_eq!(simulation.apply_care(&nap).unwrap(), CareResult::Applied);
    let first_deadline = simulation.snapshot().napping_until;
    assert!(first_deadline.is_some());

    assert_eq!(simulation.apply_care(&nap).unwrap(), CareResult::Duplicate);
    assert_eq!(simulation.snapshot().napping_until, first_deadline);

    clock.advance(Duration::seconds(2));
    assert_eq!(
        simulation
            .apply_care(&CareCommand::Nap {
                action_id: Uuid::from_u128(6),
            })
            .unwrap(),
        CareResult::Applied
    );
    assert!(simulation.snapshot().napping_until.is_some());
}

#[test]
fn unknown_food_and_out_of_stock_are_distinct_and_atomic() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 0);
    let before = simulation.snapshot();
    let unknown = CareCommand::Feed {
        action_id: Uuid::from_u128(1),
        food_id: "mystery".to_owned(),
    };
    assert!(matches!(
        simulation.apply_care(&unknown),
        Err(CareError::UnknownFood(_))
    ));
    assert_eq!(simulation.snapshot(), before);

    let out_of_stock = CareCommand::Feed {
        action_id: Uuid::from_u128(2),
        food_id: "kibble".to_owned(),
    };
    assert!(matches!(
        simulation.apply_care(&out_of_stock),
        Err(CareError::OutOfStock(_))
    ));
    assert_eq!(simulation.snapshot(), before);
    assert_eq!(clock.now(), start());
}

#[test]
fn failed_action_id_is_not_recorded_and_succeeds_after_condition_is_corrected() {
    let (_clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 1);
    let action_id = Uuid::from_u128(50);
    let invalid = CareCommand::Feed {
        action_id,
        food_id: "unknown".to_owned(),
    };
    assert!(simulation.apply_care(&invalid).is_err());
    assert!(
        !simulation
            .snapshot()
            .processed_care_ids
            .contains(&action_id)
    );

    let corrected = CareCommand::Feed {
        action_id,
        food_id: "kibble".to_owned(),
    };
    assert_eq!(
        simulation.apply_care(&corrected).unwrap(),
        CareResult::Applied
    );
    assert!(
        simulation
            .snapshot()
            .processed_care_ids
            .contains(&action_id)
    );
}

#[test]
fn petting_validation_order_is_duration_then_finite_distance_then_distance() {
    let (_clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 0);

    let action_id = Uuid::from_u128(60);
    assert!(matches!(
        simulation.apply_care(&CareCommand::Pet {
            action_id,
            interaction_ms: 1_499,
            pointer_distance: f32::NAN,
        }),
        Err(CareError::InsufficientDuration)
    ));
    assert!(matches!(
        simulation.apply_care(&CareCommand::Pet {
            action_id,
            interaction_ms: 1_500,
            pointer_distance: f32::NAN,
        }),
        Err(CareError::NonFinitePointerDistance)
    ));
    assert!(matches!(
        simulation.apply_care(&CareCommand::Pet {
            action_id,
            interaction_ms: 1_500,
            pointer_distance: 119.999,
        }),
        Err(CareError::InsufficientDistance)
    ));

    assert_eq!(
        simulation
            .apply_care(&CareCommand::Pet {
                action_id,
                interaction_ms: 1_500,
                pointer_distance: 120.0,
            })
            .unwrap(),
        CareResult::Applied
    );
    assert!(
        simulation
            .snapshot()
            .processed_care_ids
            .contains(&action_id)
    );
}

#[test]
fn missing_poop_is_typed_atomic_and_does_not_record_the_action() {
    let (_clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 0);
    let before = simulation.snapshot();
    let action_id = Uuid::from_u128(70);
    assert!(matches!(
        simulation.apply_care(&CareCommand::CleanPoop {
            action_id,
            poop_id: Uuid::from_u128(404),
        }),
        Err(CareError::MissingPoop(_))
    ));
    assert_eq!(simulation.snapshot(), before);
    assert!(
        !simulation
            .snapshot()
            .processed_care_ids
            .contains(&action_id)
    );
}

#[test]
fn duplicate_care_is_checked_before_clock_and_payload_validation() {
    struct OneThenPanicClock {
        timestamp: DateTime<Utc>,
        first: AtomicBool,
    }

    impl Clock for OneThenPanicClock {
        fn now(&self) -> DateTime<Utc> {
            if self.first.swap(false, Ordering::Relaxed) {
                self.timestamp
            } else {
                panic!("duplicate care must not read the clock");
            }
        }
    }

    let mut seeded = FoodInventory::default();
    seeded.add(FoodKind::Kibble, 1);
    let pet = Pet::with_inventory(
        Uuid::from_u128(80),
        "Mochi",
        PetSpecies::Cat,
        start(),
        seeded,
    );
    let mut simulation = PetSimulation::new(
        pet,
        OneThenPanicClock {
            timestamp: start(),
            first: AtomicBool::new(true),
        },
        DefaultNeedProgressionStrategy,
    );

    let action_id = Uuid::from_u128(81);
    simulation
        .apply_care(&CareCommand::Feed {
            action_id,
            food_id: "kibble".to_owned(),
        })
        .unwrap();
    assert_eq!(
        simulation
            .apply_care(&CareCommand::Feed {
                action_id,
                food_id: "invalid-after-duplicate".to_owned(),
            })
            .unwrap(),
        CareResult::Duplicate
    );
}

#[test]
fn poop_thresholds_emit_repeated_deterministic_v5_ids_and_consume_points() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 5);
    for action_id in 1..=5 {
        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(action_id),
                food_id: "kibble".to_owned(),
            })
            .unwrap();
    }
    simulation
        .apply_event(&event(100, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();

    for event_id in 101..=120 {
        simulation
            .apply_event(&event(event_id, 7, AgentEventKind::CommandStarted, start()))
            .unwrap();
    }

    assert_eq!(simulation.pet().pending_poops().len(), 2);
    assert_eq!(simulation.pet().digestion_points(), 0);
    assert_eq!(simulation.pet().work_points(), 0);
    assert_eq!(simulation.snapshot().poop_sequence, 2);
    assert_eq!(
        simulation.pet().pending_poops()[0].id(),
        Uuid::new_v5(&Uuid::from_u128(1), b"poop:0")
    );
    assert_eq!(
        simulation.pet().pending_poops()[1].id(),
        Uuid::new_v5(&Uuid::from_u128(1), b"poop:1")
    );
    assert_eq!(simulation.pet().pending_poops()[0].created_at(), start());
    assert_eq!(clock.now(), start());
}

#[test]
fn successful_feed_can_cross_the_pooping_threshold_at_its_logical_time() {
    let (clock, mut simulation) = simulation_with_inventory(FoodKind::Kibble, 3);
    simulation
        .apply_event(&event(300, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();
    for event_id in 301..=310 {
        simulation
            .apply_event(&event(event_id, 7, AgentEventKind::CommandStarted, start()))
            .unwrap();
    }
    clock.advance(Duration::hours(1));

    for action_id in 311..=313 {
        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(action_id),
                food_id: "kibble".to_owned(),
            })
            .unwrap();
    }

    assert_eq!(simulation.pet().pending_poops().len(), 1);
    assert_eq!(
        simulation.pet().pending_poops()[0].created_at(),
        start() + Duration::hours(1)
    );
    assert_eq!(simulation.pet().digestion_points(), 20);
    assert_eq!(simulation.pet().work_points(), 0);
}

#[test]
fn custom_poop_strategy_changes_actual_generation_through_the_public_port() {
    struct LowThresholdStrategy;

    impl PoopGenerationStrategy for LowThresholdStrategy {
        fn threshold(
            &self,
            _digestion_points: u64,
            _work_points: u64,
        ) -> Option<PoopGenerationThreshold> {
            Some(PoopGenerationThreshold::new(20, 5).unwrap())
        }
    }

    let clock = FakeClock::new(start());
    let pet = Pet::with_inventory(
        Uuid::from_u128(90),
        "Mochi",
        PetSpecies::Cat,
        start(),
        inventory(FoodKind::Kibble, 1),
    );
    let mut simulation = PetSimulation::with_poop_strategy(
        pet,
        clock,
        DefaultNeedProgressionStrategy,
        LowThresholdStrategy,
    );

    simulation
        .apply_care(&CareCommand::Feed {
            action_id: Uuid::from_u128(91),
            food_id: "kibble".to_owned(),
        })
        .unwrap();
    simulation
        .apply_event(&event(92, 7, AgentEventKind::SessionStarted, start()))
        .unwrap();
    simulation
        .apply_event(&event(93, 7, AgentEventKind::CommandStarted, start()))
        .unwrap();

    assert_eq!(simulation.pet().pending_poops().len(), 1);
    assert_eq!(simulation.pet().digestion_points(), 20);
    assert_eq!(simulation.pet().work_points(), 0);
    assert_eq!(
        simulation.pet().pending_poops()[0].id(),
        Uuid::new_v5(&Uuid::from_u128(90), b"poop:0")
    );
    assert_eq!(simulation.pet().pending_poops()[0].created_at(), start());
}

#[test]
fn poop_threshold_rejects_zero_costs_before_they_can_loop() {
    assert!(PoopGenerationThreshold::new(0, 5).is_err());
    assert!(PoopGenerationThreshold::new(20, 0).is_err());
}

#[test]
fn seeded_random_source_is_repeatable_seed_sensitive_and_bounded() {
    let mut first = SeededRandomSource::new(123);
    let mut second = SeededRandomSource::new(123);
    let first_values = (0..8).map(|_| first.next_u64()).collect::<Vec<_>>();
    let second_values = (0..8).map(|_| second.next_u64()).collect::<Vec<_>>();
    assert_eq!(first_values, second_values);

    let mut different = SeededRandomSource::new(124);
    let different_values = (0..8).map(|_| different.next_u64()).collect::<Vec<_>>();
    assert_ne!(first_values, different_values);

    let mut zero = SeededRandomSource::new(0);
    let mut normalized = SeededRandomSource::new(0);
    assert_eq!(zero.next_u64(), normalized.next_u64());

    for _ in 0..128 {
        let unit = zero.next_f32();
        assert!((0.0..1.0).contains(&unit));
        assert!(zero.next_bounded(17) < 17);
    }
    assert_eq!(zero.next_bounded(0), 0);
}

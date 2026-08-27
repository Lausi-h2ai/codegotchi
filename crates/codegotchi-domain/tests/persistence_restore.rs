use chrono::{Duration, TimeZone, Utc};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, CareCommand, DefaultNeedProgressionStrategy,
    EnforcementMode, EventMetadata, EventSource, FakeClock, FoodInventory, FoodKind, NAP_DURATION,
    Pet, PetSimulation, PetSpecies,
};
use uuid::Uuid;

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).unwrap()
}

fn simulation() -> (
    FakeClock,
    PetSimulation<FakeClock, DefaultNeedProgressionStrategy>,
) {
    let clock = FakeClock::new(start());
    let mut inventory = FoodInventory::default();
    inventory.add(FoodKind::Kibble, 50);
    inventory.add(FoodKind::Treat, 25);
    inventory.add(FoodKind::Fruit, 25);
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        start(),
        inventory,
    );
    let simulation = PetSimulation::new(pet, clock.clone(), DefaultNeedProgressionStrategy);
    let mut snapshot = simulation.snapshot();
    snapshot.next_incident_at = Some(start() + Duration::days(365));
    let simulation =
        PetSimulation::from_snapshot(snapshot, clock.clone(), DefaultNeedProgressionStrategy)
            .unwrap();
    (clock.clone(), simulation)
}

fn event(
    id: u128,
    kind: AgentEventKind,
    activity: Option<ActivityKind>,
    timestamp: chrono::DateTime<Utc>,
) -> AgentEvent {
    AgentEvent::new(
        Uuid::from_u128(id),
        Uuid::from_u128(7),
        "repo",
        EventSource::Codex,
        kind,
        activity,
        timestamp,
        EventMetadata::default(),
    )
}

fn feed(id: u128, food_id: &str) -> CareCommand {
    CareCommand::Feed {
        action_id: Uuid::from_u128(id),
        food_id: food_id.to_owned(),
    }
}

#[test]
fn snapshot_json_round_trip_restores_all_continuation_state() {
    let (clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(10, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    simulation
        .apply_event(&event(
            11,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start() + Duration::hours(1),
        ))
        .unwrap();
    simulation.apply_care(&feed(20, "kibble")).unwrap();
    simulation.apply_care(&feed(21, "treat")).unwrap();
    for id in 12..=21 {
        simulation
            .apply_event(&event(
                id,
                AgentEventKind::CommandStarted,
                Some(ActivityKind::Testing),
                start() + Duration::hours(1),
            ))
            .unwrap();
    }
    for id in 22..=24 {
        simulation.apply_care(&feed(id, "kibble")).unwrap();
    }
    clock.advance(Duration::minutes(2));
    simulation.current_state();
    simulation.set_enforcement_mode(EnforcementMode::Strict);

    let before = simulation.snapshot();
    let encoded = serde_json::to_vec(&before).unwrap();
    let decoded = serde_json::from_slice(&encoded).unwrap();
    let restored = PetSimulation::from_snapshot(
        decoded,
        FakeClock::new(before.last_updated_at),
        DefaultNeedProgressionStrategy,
    )
    .unwrap();

    assert_eq!(restored.snapshot(), before);
    assert_eq!(
        restored.pet().inventory().count(FoodKind::Kibble),
        46,
        "the domain still preserves finite fixture inventories; the product runtime normalizes them"
    );
    assert_eq!(
        restored.pet().pending_poops().len(),
        before.pending_poops.len()
    );
    assert_eq!(restored.enforcement_mode(), EnforcementMode::Strict);
}

#[test]
fn restore_rejects_unsupported_versions_and_invariant_violations() {
    let (_clock, simulation) = simulation();
    let mut unsupported = simulation.snapshot();
    unsupported.schema_version = 2;
    let error = PetSimulation::from_snapshot(
        unsupported,
        FakeClock::new(start()),
        DefaultNeedProgressionStrategy,
    )
    .err()
    .expect("unsupported version must be rejected");
    assert!(matches!(
        error,
        codegotchi_domain::SnapshotRestoreError::UnsupportedSchemaVersion(2)
    ));

    let mut invalid = simulation.snapshot();
    invalid.needs.set_hunger(50.0);
    invalid.needs.set_energy(50.0);
    invalid.needs.set_happiness(50.0);
    invalid.needs.set_cleanliness(50.0);
    invalid.pet_id = Uuid::nil();
    let error = PetSimulation::from_snapshot(
        invalid,
        FakeClock::new(start()),
        DefaultNeedProgressionStrategy,
    )
    .err()
    .expect("invalid snapshot must be rejected");
    assert!(matches!(
        error,
        codegotchi_domain::SnapshotRestoreError::InvariantViolation(_)
    ));
}

#[test]
fn replay_ids_survive_restore_and_duplicates_remain_total_noops() {
    let (_clock, mut original) = simulation();
    let first = event(
        30,
        AgentEventKind::TurnStarted,
        Some(ActivityKind::Testing),
        start(),
    );
    original.apply_event(&first).unwrap();
    original.apply_care(&feed(31, "fruit")).unwrap();
    let before = original.snapshot();

    let mut restored = PetSimulation::from_snapshot(
        before.clone(),
        FakeClock::new(start() + Duration::days(1)),
        DefaultNeedProgressionStrategy,
    )
    .unwrap();
    restored.apply_event(&first).unwrap();
    assert_eq!(
        restored.apply_care(&feed(31, "fruit")).unwrap(),
        codegotchi_domain::CareResult::Duplicate
    );
    assert_eq!(restored.snapshot(), before);
}

#[test]
fn restored_simulations_continue_deterministically() {
    let (_clock, mut first) = simulation();
    first
        .apply_event(&event(40, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    first
        .apply_event(&event(
            41,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Building),
            start() + Duration::hours(1),
        ))
        .unwrap();
    let snapshot = first.snapshot();

    let clock_a = FakeClock::new(start() + Duration::hours(2));
    let clock_b = FakeClock::new(start() + Duration::hours(2));
    let mut a = PetSimulation::from_snapshot(
        snapshot.clone(),
        clock_a.clone(),
        DefaultNeedProgressionStrategy,
    )
    .unwrap();
    let mut b =
        PetSimulation::from_snapshot(snapshot, clock_b.clone(), DefaultNeedProgressionStrategy)
            .unwrap();
    a.current_state();
    b.current_state();
    let next = event(
        42,
        AgentEventKind::WaitingForUser,
        None,
        start() + Duration::hours(2),
    );
    a.apply_event(&next).unwrap();
    b.apply_event(&next).unwrap();
    assert_eq!(a.snapshot(), b.snapshot());
}

#[test]
fn hammock_nap_deadline_survives_restore_and_keeps_recovering() {
    let (clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(50, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    simulation
        .apply_event(&event(
            51,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Building),
            start(),
        ))
        .unwrap();
    clock.advance(Duration::hours(20));
    simulation.current_state();
    simulation
        .apply_care(&CareCommand::Nap {
            action_id: Uuid::from_u128(52),
        })
        .unwrap();
    let snapshot = simulation.snapshot();
    assert_eq!(
        snapshot.napping_until,
        Some(start() + Duration::hours(20) + NAP_DURATION)
    );

    let mut restored = PetSimulation::from_snapshot(
        snapshot,
        FakeClock::new(start() + Duration::hours(20) + Duration::seconds(2)),
        DefaultNeedProgressionStrategy,
    )
    .unwrap();
    let resumed = restored.current_state();
    assert_eq!(resumed.needs.energy(), 40.0);
    assert_eq!(resumed.behavior, codegotchi_domain::PetBehavior::Sleeping);
}

#[test]
fn future_dated_snapshots_are_reanchored_to_the_wall_clock_on_restore() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_care(&CareCommand::Nap {
            action_id: Uuid::from_u128(60),
        })
        .unwrap();
    let mut snapshot = simulation.snapshot();

    // Persist a timeline that is eight days ahead of the restore clock, the
    // shape left behind by a wall-clock correction or the old demo neglect.
    let ahead = Duration::days(8);
    snapshot.last_updated_at += ahead;
    snapshot.napping_until = snapshot.napping_until.map(|until| until + ahead);
    snapshot.last_activity_at = snapshot.last_activity_at.map(|at| at + ahead);
    snapshot.last_outcome_at = snapshot.last_outcome_at.map(|at| at + ahead);

    let restore_clock = FakeClock::new(start());
    let mut restored = PetSimulation::from_snapshot(
        snapshot,
        restore_clock.clone(),
        DefaultNeedProgressionStrategy,
    )
    .unwrap();

    // The whole timeline is translated back, so the nap resumes at the wall
    // clock instead of freezing until the wall clock catches up.
    let resumed = restored.snapshot();
    assert_eq!(resumed.last_updated_at, start());
    assert_eq!(
        resumed.napping_until,
        Some(start() + codegotchi_domain::NAP_DURATION)
    );

    restore_clock.advance(Duration::seconds(2));
    let progressed = restored.current_state();
    assert_eq!(progressed.last_updated_at, start() + Duration::seconds(2));
    assert_eq!(
        progressed.napping_until,
        Some(start() + codegotchi_domain::NAP_DURATION)
    );
    assert_eq!(
        progressed.behavior,
        codegotchi_domain::PetBehavior::Sleeping
    );
}

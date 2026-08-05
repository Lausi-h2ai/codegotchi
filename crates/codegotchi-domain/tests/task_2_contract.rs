use chrono::{Duration, TimeZone, Utc};
use codegotchi_domain::{
    ActivityKind, AgentActivityState, AgentEvent, AgentEventKind, AgentOutcome,
    DefaultNeedProgressionStrategy, EventMetadata, EventSource, FakeClock, Pet, PetBehavior,
    PetSimulation, PetSpecies,
};
use uuid::Uuid;

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
}

fn simulation() -> (
    FakeClock,
    PetSimulation<FakeClock, DefaultNeedProgressionStrategy>,
) {
    let clock = FakeClock::new(start());
    let pet = Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start());
    (
        clock.clone(),
        PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy),
    )
}

fn event(
    id: u128,
    session_id: u128,
    kind: AgentEventKind,
    activity: Option<ActivityKind>,
    timestamp: chrono::DateTime<Utc>,
) -> AgentEvent {
    AgentEvent {
        id: Uuid::from_u128(id),
        schema_version: 1,
        session_id: Uuid::from_u128(session_id),
        repository_id: "repo".to_owned(),
        source: EventSource::Codex,
        kind,
        activity,
        timestamp,
        metadata: EventMetadata::default(),
    }
}

#[test]
fn active_progression_uses_the_previous_activity_state() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    simulation
        .apply_event(&event(
            2,
            7,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start(),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            3,
            7,
            AgentEventKind::WaitingForUser,
            None,
            start() + Duration::hours(1),
        ))
        .unwrap();

    assert_eq!(simulation.pet().needs().hunger(), 4.0);
    assert_eq!(simulation.pet().needs().energy(), 94.0);
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::WaitingForUser
    );
    assert_eq!(simulation.pet().work_points(), 1);
}

#[test]
fn idle_progression_restores_energy_at_event_time() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(
            1,
            7,
            AgentEventKind::SessionStarted,
            None,
            start() + Duration::hours(1),
        ))
        .unwrap();

    assert_eq!(simulation.pet().needs().hunger(), 1.0);
    assert_eq!(simulation.pet().needs().energy(), 100.0);
}

#[test]
fn completion_outcomes_only_affect_testing_and_building_with_known_status() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, None, start()))
        .unwrap();

    let mut non_work_completion = event(
        2,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Editing),
        start(),
    );
    non_work_completion.metadata.exit_status = Some(0);
    simulation.apply_event(&non_work_completion).unwrap();
    assert_eq!(simulation.pet().recent_outcome(), AgentOutcome::None);

    let mut success = event(
        3,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Testing),
        start() + Duration::seconds(1),
    );
    success.metadata.exit_status = Some(0);
    simulation.apply_event(&success).unwrap();
    assert_eq!(simulation.pet().recent_outcome(), AgentOutcome::Success);
    assert_eq!(simulation.consecutive_failures(), 0);

    let mut failure = event(
        4,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Building),
        start() + Duration::seconds(2),
    );
    failure.metadata.exit_status = Some(1);
    simulation.apply_event(&failure).unwrap();
    assert_eq!(simulation.pet().needs().happiness(), 96.0);
    assert_eq!(simulation.pet().recent_outcome(), AgentOutcome::Failure);
    assert_eq!(simulation.consecutive_failures(), 1);
}

#[test]
fn work_point_events_and_structured_activity_defaults_are_deterministic() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    simulation
        .apply_event(&event(2, 7, AgentEventKind::TurnStarted, None, start()))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Thinking)
    );
    assert_eq!(simulation.pet().work_points(), 1);

    simulation
        .apply_event(&event(3, 7, AgentEventKind::OutputActivity, None, start()))
        .unwrap();
    simulation
        .apply_event(&event(4, 7, AgentEventKind::ToolStarted, None, start()))
        .unwrap();
    simulation
        .apply_event(&event(
            5,
            7,
            AgentEventKind::CommandStarted,
            Some(ActivityKind::Testing),
            start(),
        ))
        .unwrap();
    assert_eq!(simulation.pet().work_points(), 12);
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );

    simulation
        .apply_event(&event(6, 7, AgentEventKind::TurnCompleted, None, start()))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::WaitingForUser
    );
}

#[test]
fn duplicate_event_ids_are_total_noops_before_progression() {
    let (_clock, mut simulation) = simulation();
    let first = event(
        1,
        7,
        AgentEventKind::TurnStarted,
        Some(ActivityKind::Editing),
        start(),
    );
    simulation.apply_event(&first).unwrap();
    let before = simulation.snapshot();
    simulation.apply_event(&first).unwrap();

    assert_eq!(simulation.snapshot(), before);
}

#[test]
fn unsupported_schema_versions_return_a_typed_error_without_mutation() {
    let (_clock, mut simulation) = simulation();
    let mut unsupported = event(1, 7, AgentEventKind::SessionStarted, None, start());
    unsupported.schema_version = 2;
    let before = simulation.snapshot();

    assert!(simulation.apply_event(&unsupported).is_err());
    assert_eq!(simulation.snapshot(), before);
}

#[test]
fn behavior_is_stored_and_maintenance_handles_the_sleep_boundary() {
    let (clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    assert_eq!(simulation.pet().behavior(), PetBehavior::Wandering);

    clock.advance(Duration::minutes(30));
    let current = simulation.current_state();
    assert_eq!(current.behavior, PetBehavior::Sleeping);
    assert_eq!(simulation.pet().behavior(), PetBehavior::Sleeping);
}

#[test]
fn session_lifecycle_exposes_only_registered_session_activity() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(
            1,
            101,
            AgentEventKind::SessionStarted,
            None,
            start(),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            2,
            102,
            AgentEventKind::SessionStarted,
            None,
            start(),
        ))
        .unwrap();
    assert_eq!(simulation.session_activities().len(), 2);

    simulation
        .apply_event(&event(3, 101, AgentEventKind::SessionEnded, None, start()))
        .unwrap();
    assert_eq!(simulation.session_activities().len(), 1);
    assert!(
        simulation
            .session_activities()
            .contains_key(&Uuid::from_u128(102))
    );
}

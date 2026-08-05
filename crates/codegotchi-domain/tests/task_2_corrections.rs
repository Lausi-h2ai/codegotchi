use chrono::{Duration, TimeZone, Utc};
use codegotchi_domain::{
    ActivityKind, AgentActivityState, AgentEvent, AgentEventKind, AgentOutcome, Clock,
    DefaultNeedProgressionStrategy, EventMetadata, EventSource, FakeClock, Pet, PetSimulation,
    PetSpecies,
};
use uuid::Uuid;

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
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

struct PanicClock;

impl Clock for PanicClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        panic!("apply_event must not read the injected clock");
    }
}

#[test]
fn event_time_is_the_only_timeline_for_apply_and_replay_needs_no_clock_schedule() {
    let (_clock_a, mut first) = simulation();
    let (_clock_b, mut second) = simulation();
    let events = [
        event(1, 7, AgentEventKind::SessionStarted, None, start()),
        event(
            2,
            7,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start() + Duration::hours(1),
        ),
        event(
            3,
            7,
            AgentEventKind::TurnCompleted,
            None,
            start() + Duration::hours(2),
        ),
    ];

    for current in events {
        first.apply_event(&current).unwrap();
        second.apply_event(&current).unwrap();
    }

    assert_eq!(first.snapshot(), second.snapshot());
    assert_eq!(first.pet().last_updated_at(), start() + Duration::hours(2));
    assert_eq!(first.pet().needs().hunger(), 5.0);
    assert_eq!(first.pet().needs().energy(), 94.0);
}

#[test]
fn apply_event_does_not_read_the_injected_clock() {
    let pet = Pet::new(Uuid::from_u128(2), "Mochi", PetSpecies::Cat, start());
    let mut simulation = PetSimulation::new(pet, PanicClock, DefaultNeedProgressionStrategy);

    simulation
        .apply_event(&event(
            1,
            7,
            AgentEventKind::SessionStarted,
            None,
            start() + Duration::hours(1),
        ))
        .unwrap();

    assert_eq!(
        simulation.pet().last_updated_at(),
        start() + Duration::hours(1)
    );
}

#[test]
fn future_event_time_advances_and_older_event_time_cannot_rewind_or_decay_again() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(
            1,
            7,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start() + Duration::hours(2),
        ))
        .unwrap();
    let before_older = simulation.snapshot();

    simulation
        .apply_event(&event(
            2,
            7,
            AgentEventKind::OutputActivity,
            None,
            start() + Duration::hours(1),
        ))
        .unwrap();

    assert_eq!(
        simulation.pet().last_updated_at(),
        start() + Duration::hours(2)
    );
    assert_eq!(simulation.pet().needs(), before_older.needs);
    assert_eq!(simulation.last_activity_at(), Some(start()));
}

#[test]
fn duplicate_and_unsupported_events_leave_the_complete_snapshot_unchanged() {
    let (_clock, mut simulation) = simulation();
    let first = event(
        1,
        7,
        AgentEventKind::TurnStarted,
        Some(ActivityKind::Editing),
        start() + Duration::hours(1),
    );
    simulation.apply_event(&first).unwrap();
    let before_duplicate = simulation.snapshot();
    simulation.apply_event(&first).unwrap();
    assert_eq!(simulation.snapshot(), before_duplicate);

    let mut duplicate_with_unsupported_schema = first.clone();
    duplicate_with_unsupported_schema.schema_version = 2;
    duplicate_with_unsupported_schema.timestamp = start() + Duration::days(1);
    simulation
        .apply_event(&duplicate_with_unsupported_schema)
        .unwrap();
    assert_eq!(simulation.snapshot(), before_duplicate);

    let mut unsupported = event(
        2,
        7,
        AgentEventKind::SessionEnded,
        None,
        start() + Duration::hours(2),
    );
    unsupported.schema_version = 2;
    let before_unsupported = simulation.snapshot();
    assert!(simulation.apply_event(&unsupported).is_err());
    assert_eq!(simulation.snapshot(), before_unsupported);
}

#[test]
fn registered_sessions_keep_independent_activity_and_unknown_sessions_fail_open() {
    let (_clock, mut simulation) = simulation();
    let session_a = 10;
    let session_b = 20;
    simulation
        .apply_event(&event(
            1,
            session_a,
            AgentEventKind::SessionStarted,
            None,
            start(),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            2,
            session_b,
            AgentEventKind::SessionStarted,
            None,
            start(),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            3,
            session_a,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start() + Duration::seconds(1),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            4,
            session_b,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Testing),
            start() + Duration::seconds(2),
        ))
        .unwrap();

    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );

    let mut complete_a = event(
        5,
        session_a,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Editing),
        start() + Duration::seconds(3),
    );
    complete_a.metadata.exit_status = Some(0);
    simulation.apply_event(&complete_a).unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );

    simulation
        .apply_event(&event(
            6,
            999,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Reading),
            start() + Duration::seconds(4),
        ))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );
    assert_eq!(simulation.pet().work_points(), 3);

    simulation
        .apply_event(&event(
            7,
            999,
            AgentEventKind::SessionEnded,
            None,
            start() + Duration::seconds(5),
        ))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );

    simulation
        .apply_event(&event(
            8,
            session_a,
            AgentEventKind::SessionEnded,
            None,
            start() + Duration::seconds(6),
        ))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );
}

#[test]
fn unknown_work_cannot_postpone_sleep_or_mutate_the_activity_baseline() {
    let (baseline_clock, mut baseline) = simulation();
    let (observed_clock, mut observed) = simulation();
    let unknown_work = event(
        90,
        999,
        AgentEventKind::TurnStarted,
        Some(ActivityKind::Editing),
        start() + Duration::minutes(10),
    );

    observed.apply_event(&unknown_work).unwrap();

    assert!(observed.session_activities().is_empty());
    assert_eq!(observed.session_activities(), baseline.session_activities());
    assert_eq!(observed.last_activity_at(), baseline.last_activity_at());
    assert_eq!(observed.pet().activity(), baseline.pet().activity());
    assert_eq!(observed.pet().behavior(), baseline.pet().behavior());
    assert_eq!(
        observed.pet().recent_outcome(),
        baseline.pet().recent_outcome()
    );
    assert_eq!(observed.last_outcome_at(), baseline.last_outcome_at());
    assert_eq!(
        observed.consecutive_failures(),
        baseline.consecutive_failures()
    );

    baseline_clock.advance(Duration::minutes(30));
    observed_clock.advance(Duration::minutes(30));
    let baseline_state = baseline.current_state();
    let observed_state = observed.current_state();

    assert_eq!(
        baseline_state.behavior,
        codegotchi_domain::PetBehavior::Sleeping
    );
    assert_eq!(observed_state.behavior, baseline_state.behavior);
    assert_eq!(observed.pet().behavior(), baseline_state.behavior);
    assert_eq!(observed.pet().activity(), baseline_state.activity);
    assert_eq!(
        observed.session_activities(),
        &baseline_state.session_activities
    );
    assert_eq!(observed.last_activity_at(), baseline_state.last_activity_at);
    assert_eq!(observed.last_outcome_at(), baseline_state.last_outcome_at);
    assert_eq!(
        observed.consecutive_failures(),
        baseline_state.consecutive_failures
    );
    assert_eq!(
        observed.pet().last_updated_at(),
        baseline_state.last_updated_at
    );
    assert_eq!(observed.pet().needs(), baseline_state.needs);

    assert_eq!(baseline.pet().work_points(), 0);
    assert_eq!(observed.pet().work_points(), 1);
    assert!(
        observed
            .processed_event_ids()
            .contains(&Uuid::from_u128(90))
    );
    assert!(baseline.processed_event_ids().is_empty());
}

#[test]
fn aggregate_activity_uses_blocked_active_waiting_idle_priority_and_uuid_ties() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(
            20,
            10,
            AgentEventKind::SessionStarted,
            None,
            start(),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            21,
            20,
            AgentEventKind::SessionStarted,
            None,
            start(),
        ))
        .unwrap();

    simulation
        .apply_event(&event(
            22,
            10,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start() + Duration::seconds(1),
        ))
        .unwrap();
    simulation
        .apply_event(&event(
            23,
            20,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Testing),
            start() + Duration::seconds(1),
        ))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );

    let mut blocked = event(
        24,
        10,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Testing),
        start() + Duration::seconds(2),
    );
    blocked.metadata.blocked = true;
    simulation.apply_event(&blocked).unwrap();
    assert_eq!(simulation.pet().activity(), AgentActivityState::Blocked);

    simulation
        .apply_event(&event(
            25,
            10,
            AgentEventKind::SessionEnded,
            None,
            start() + Duration::seconds(3),
        ))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Testing)
    );

    simulation
        .apply_event(&event(
            26,
            20,
            AgentEventKind::WaitingForUser,
            None,
            start() + Duration::seconds(4),
        ))
        .unwrap();
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::WaitingForUser
    );

    simulation
        .apply_event(&event(
            27,
            20,
            AgentEventKind::SessionEnded,
            None,
            start() + Duration::seconds(5),
        ))
        .unwrap();
    assert_eq!(simulation.pet().activity(), AgentActivityState::Idle);

    simulation
        .apply_event(&event(
            28,
            999,
            AgentEventKind::ToolStarted,
            Some(ActivityKind::Reading),
            start() + Duration::seconds(6),
        ))
        .unwrap();
    assert_eq!(simulation.pet().activity(), AgentActivityState::Idle);
    assert!(simulation.session_activities().is_empty());
    assert_eq!(simulation.pet().work_points(), 7);
}

#[test]
fn session_lifecycle_and_error_events_do_not_reset_the_work_baseline() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(
            30,
            7,
            AgentEventKind::SessionStarted,
            None,
            start() + Duration::hours(1),
        ))
        .unwrap();
    assert_eq!(simulation.last_activity_at(), Some(start()));

    simulation
        .apply_event(&event(
            31,
            7,
            AgentEventKind::TurnStarted,
            Some(ActivityKind::Editing),
            start() + Duration::hours(2),
        ))
        .unwrap();
    assert_eq!(
        simulation.last_activity_at(),
        Some(start() + Duration::hours(2))
    );

    simulation
        .apply_event(&event(
            32,
            7,
            AgentEventKind::IntegrationError,
            None,
            start() + Duration::hours(3),
        ))
        .unwrap();
    assert_eq!(
        simulation.last_activity_at(),
        Some(start() + Duration::hours(2))
    );

    simulation
        .apply_event(&event(
            33,
            7,
            AgentEventKind::SessionEnded,
            None,
            start() + Duration::hours(4),
        ))
        .unwrap();
    assert_eq!(
        simulation.last_activity_at(),
        Some(start() + Duration::hours(2))
    );
}

#[test]
fn completion_events_do_not_add_work_points_and_unknown_status_is_non_penalizing() {
    let (_clock, mut simulation) = simulation();
    simulation
        .apply_event(&event(1, 7, AgentEventKind::SessionStarted, None, start()))
        .unwrap();
    let mut unknown = event(
        2,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Testing),
        start() + Duration::seconds(1),
    );
    unknown.metadata.exit_status = None;
    simulation.apply_event(&unknown).unwrap();
    assert_eq!(simulation.pet().work_points(), 0);
    assert_eq!(simulation.pet().needs().happiness(), 100.0);
    assert_eq!(simulation.pet().recent_outcome(), AgentOutcome::None);
    assert_eq!(simulation.consecutive_failures(), 0);
    assert_eq!(simulation.last_outcome_at(), None);

    let mut success = event(
        3,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Testing),
        start() + Duration::seconds(2),
    );
    success.metadata.exit_status = Some(0);
    simulation.apply_event(&success).unwrap();
    assert_eq!(simulation.pet().recent_outcome(), AgentOutcome::Success);
    assert_eq!(simulation.consecutive_failures(), 0);
    assert_eq!(
        simulation.last_outcome_at(),
        Some(start() + Duration::seconds(2))
    );

    let mut failure = event(
        4,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Building),
        start() + Duration::seconds(3),
    );
    failure.metadata.exit_status = Some(1);
    simulation.apply_event(&failure).unwrap();
    assert_eq!(simulation.pet().recent_outcome(), AgentOutcome::Failure);
    assert_eq!(simulation.consecutive_failures(), 1);

    let mut blocked = event(
        5,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Testing),
        start() + Duration::seconds(4),
    );
    blocked.metadata.exit_status = Some(0);
    blocked.metadata.blocked = true;
    let before_blocked = simulation.snapshot();
    simulation.apply_event(&blocked).unwrap();
    assert_eq!(
        simulation.pet().needs().happiness(),
        before_blocked.needs.happiness()
    );
    assert_eq!(
        simulation.pet().recent_outcome(),
        before_blocked.recent_outcome
    );
    assert_eq!(
        simulation.consecutive_failures(),
        before_blocked.consecutive_failures
    );
    assert_eq!(simulation.last_outcome_at(), before_blocked.last_outcome_at);
}

#[test]
fn maintenance_is_the_only_clock_driven_operation_and_stores_behavior() {
    let (clock, mut simulation) = simulation();
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
    let before_clock_advance = simulation.snapshot();
    clock.advance(Duration::hours(1));
    assert_eq!(simulation.snapshot(), before_clock_advance);

    let current = simulation.current_state();
    assert_eq!(current.needs.hunger(), 4.0);
    assert_eq!(current.needs.energy(), 94.0);
    assert_eq!(simulation.pet().behavior(), current.behavior);
    assert_eq!(
        simulation.pet().activity(),
        AgentActivityState::Active(ActivityKind::Editing)
    );
}

#[test]
fn event_json_round_trip_preserves_the_versioned_privacy_limited_shape() {
    let mut current = event(
        1,
        7,
        AgentEventKind::CommandCompleted,
        Some(ActivityKind::Testing),
        start(),
    );
    current.metadata.executable_name = Some("cargo".to_owned());
    current.metadata.command_category = Some("testing".to_owned());
    current.metadata.exit_status = Some(0);
    current.metadata.duration_ms = Some(42);
    let json = serde_json::to_string(&current).unwrap();
    let decoded: AgentEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, current);
    assert!(json.contains("schema_version"));
    assert!(json.contains("executable_name"));
    assert!(!json.contains("prompt"));
    assert!(!json.contains("output"));
    assert!(!json.contains("command_text"));
}

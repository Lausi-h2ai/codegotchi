use chrono::{Duration, TimeZone, Utc};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, Clock, DefaultNeedProgressionStrategy, EventMetadata,
    EventSource, FakeClock, Pet, PetSimulation, PetSpecies,
};
use uuid::Uuid;

type Simulation = PetSimulation<FakeClock, DefaultNeedProgressionStrategy>;

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
}

fn event(id: u128, kind: AgentEventKind, timestamp: chrono::DateTime<Utc>) -> AgentEvent {
    AgentEvent {
        id: Uuid::from_u128(id),
        schema_version: 1,
        session_id: Uuid::from_u128(77),
        repository_id: "repo".to_owned(),
        source: EventSource::Codex,
        kind,
        activity: Some(ActivityKind::Editing),
        timestamp,
        metadata: EventMetadata::default(),
    }
}

fn new_simulation(clock_time: chrono::DateTime<Utc>) -> (FakeClock, Simulation) {
    let clock = FakeClock::new(clock_time);
    let pet = Pet::new(Uuid::from_u128(2), "Mochi", PetSpecies::Cat, start());
    (
        clock.clone(),
        PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy),
    )
}

#[test]
fn replay_uses_only_event_timestamps_and_needs_no_clock_schedule() {
    let (clock_a, mut first) = new_simulation(start());
    let (clock_b, mut second) = new_simulation(start() + Duration::days(7));
    let events = [
        event(1, AgentEventKind::SessionStarted, start()),
        event(2, AgentEventKind::TurnStarted, start()),
        event(
            3,
            AgentEventKind::OutputActivity,
            start() + Duration::minutes(30),
        ),
        event(
            4,
            AgentEventKind::TurnCompleted,
            start() + Duration::hours(1),
        ),
        event(
            5,
            AgentEventKind::SessionEnded,
            start() + Duration::hours(2),
        ),
    ];

    for event in &events {
        first.apply_event(event).unwrap();
        second.apply_event(event).unwrap();
    }

    assert_eq!(first.snapshot(), second.snapshot());
    assert_eq!(clock_a.now(), start());
    assert_eq!(clock_b.now(), start() + Duration::days(7));
}

#[test]
fn replaying_a_duplicate_does_not_change_the_full_snapshot() {
    let (clock, mut simulation) = new_simulation(start());
    let first = event(1, AgentEventKind::TurnStarted, start());
    simulation.apply_event(&first).unwrap();
    let before = simulation.snapshot();

    clock.advance(Duration::hours(3));
    simulation.apply_event(&first).unwrap();

    assert_eq!(simulation.snapshot(), before);
}

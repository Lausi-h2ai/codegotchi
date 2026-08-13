use chrono::{Duration, Utc};
use codegotchi_cli::{AuthoritativeRuntime, SqliteStore};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, CommandCategory, CommandClassification,
    CommandPurpose, DefaultNeedProgressionStrategy, EnforcementMode, EventMetadata, EventSource,
    FakeClock, FoodInventory, Pet, PetSimulation, PetSpecies,
};
use uuid::Uuid;

#[test]
fn ingest_event_catches_up_wall_clock_before_strict_permission_evaluation() {
    let initial_time = Utc::now() - Duration::hours(2);
    let pet = Pet::with_inventory(
        Uuid::from_u128(42),
        "Mochi",
        PetSpecies::Cat,
        initial_time,
        FoodInventory::starter(),
    );
    let simulation = PetSimulation::new(
        pet,
        FakeClock::new(initial_time),
        DefaultNeedProgressionStrategy,
    );
    let mut initial = simulation.snapshot();
    initial.enforcement_mode = EnforcementMode::Strict;

    let runtime = AuthoritativeRuntime::new(SqliteStore::open(":memory:").unwrap(), initial)
        .expect("runtime restores the stale persisted snapshot");
    let before = runtime.snapshot();
    assert_eq!(before.needs.energy(), 100.0);

    let event_time = Utc::now();
    let event = AgentEvent::new(
        Uuid::from_u128(9001),
        Uuid::from_u128(7001),
        "repo",
        EventSource::Codex,
        AgentEventKind::ToolStarted,
        Some(ActivityKind::Testing),
        event_time,
        EventMetadata::default(),
    );

    let receipt = runtime
        .ingest_event(
            &event,
            Some(CommandClassification::new(
                CommandCategory::Development,
                CommandPurpose::SafeDevelopment,
            )),
        )
        .expect("work event is ingested");

    assert!(
        receipt.decision.is_blocked(),
        "strict permission must use wall-clock catch-up before deciding"
    );
    assert!(receipt.snapshot.needs.energy() <= 5.0);
    assert!(receipt.snapshot.last_updated_at >= event_time);
}

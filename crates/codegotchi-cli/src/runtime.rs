use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Duration, Utc};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventError, AgentEventKind, CareCommand, CareError, CareResult,
    CommandCategory, CommandClassification, CommandPurpose, DefaultNeedProgressionStrategy,
    EnforcementMode, FoodInventory, FoodKind, Pet, PetNameError, PetSettings, PetSimulation,
    SimulationSnapshot, SnapshotRestoreError, SystemClock, WorkDecision, WorkPermissionPolicy,
};
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::persistence::{PersistenceError, SqliteStore};

type RuntimeSimulation = PetSimulation<SystemClock, DefaultNeedProgressionStrategy>;

#[derive(Clone)]
pub enum RuntimeInitial {
    Pet(Pet),
    Snapshot(SimulationSnapshot),
}

impl From<Pet> for RuntimeInitial {
    fn from(pet: Pet) -> Self {
        Self::Pet(pet)
    }
}

impl From<SimulationSnapshot> for RuntimeInitial {
    fn from(snapshot: SimulationSnapshot) -> Self {
        Self::Snapshot(snapshot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MutationReceipt {
    pub snapshot: SimulationSnapshot,
    pub duplicate: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventIngestReceipt {
    pub snapshot: SimulationSnapshot,
    pub duplicate: bool,
    pub decision: WorkDecision,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    #[error(transparent)]
    Event(#[from] AgentEventError),
    #[error(transparent)]
    Care(#[from] CareError),
    #[error(transparent)]
    PetName(#[from] PetNameError),
    #[error(transparent)]
    Restore(#[from] SnapshotRestoreError),
    #[error("authoritative runtime lock is poisoned")]
    LockPoisoned,
}

pub struct AuthoritativeRuntime {
    store: SqliteStore,
    simulation: Mutex<RuntimeSimulation>,
    snapshots: broadcast::Sender<SimulationSnapshot>,
}

impl AuthoritativeRuntime {
    pub fn new(
        store: SqliteStore,
        initial: impl Into<RuntimeInitial>,
    ) -> Result<Arc<Self>, RuntimeError> {
        let initial = initial.into();
        let initial_snapshot = match initial {
            RuntimeInitial::Pet(pet) => {
                let pet = seed_new_pet(pet);
                PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot()
            }
            RuntimeInitial::Snapshot(snapshot) => snapshot,
        };
        let loaded_snapshot = store.load_or_initialize(initial_snapshot)?;
        let mut snapshot = loaded_snapshot;
        let unlimited_inventory = FoodInventory::unlimited();
        if snapshot.inventory != unlimited_inventory {
            snapshot.inventory = unlimited_inventory;
            store.save(&snapshot)?;
        }
        let simulation =
            PetSimulation::from_snapshot(snapshot, SystemClock, DefaultNeedProgressionStrategy)?;
        let (snapshots, _) = broadcast::channel(32);
        Ok(Arc::new(Self {
            store,
            simulation: Mutex::new(simulation),
            snapshots,
        }))
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        self.simulation
            .lock()
            .map(|simulation| simulation.snapshot())
            .unwrap_or_else(|poisoned| poisoned.into_inner().snapshot())
    }

    pub fn enforcement_mode(&self) -> EnforcementMode {
        self.snapshot().enforcement_mode
    }

    pub fn set_enforcement_mode(
        &self,
        mode: EnforcementMode,
    ) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let duplicate = before.enforcement_mode == mode;
        let progress_at = before.last_updated_at + Duration::milliseconds(1);
        simulation.current_state_at(progress_at);
        simulation.set_enforcement_mode(mode);
        self.persist_and_broadcast(&mut simulation, before, duplicate)
    }

    pub fn rename(&self, name: impl Into<String>) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let changed = simulation.rename(name)?;
        if !changed {
            return Ok(MutationReceipt {
                snapshot: before,
                duplicate: true,
            });
        }
        self.persist_and_broadcast(&mut simulation, before, false)
    }

    /// Evaluates an optional structured permission context and applies the
    /// canonical event while holding the same simulation lock. A denied
    /// PreToolUse is recorded as blocked so the browser sees one authoritative
    /// state transition and the hook can make its decision from this response.
    pub fn ingest_event(
        &self,
        event: &AgentEvent,
        classification: Option<CommandClassification>,
    ) -> Result<EventIngestReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let classification = classification.unwrap_or(CommandClassification::new(
            CommandCategory::Unknown,
            CommandPurpose::Uncertain,
        ));
        let duplicate = before.processed_event_ids.contains(&event.id);
        if duplicate {
            let decision = WorkPermissionPolicy::evaluate(
                simulation.pet(),
                &classification,
                &PetSettings::new(before.enforcement_mode),
            );
            return Ok(EventIngestReceipt {
                snapshot: simulation.snapshot(),
                duplicate: true,
                decision,
            });
        }

        // Permission is a decision about whether work may start now, so first
        // apply all authoritative wall-clock neglect through the later of the
        // runtime clock and the event's observed timestamp. Duplicate events
        // return above without moving time, preserving replay semantics.
        event.validate_schema_version()?;
        simulation.current_state_at(Utc::now().max(event.timestamp));
        let decision = WorkPermissionPolicy::evaluate(
            simulation.pet(),
            &classification,
            &PetSettings::new(before.enforcement_mode),
        );
        let mut accepted_event = event.clone();
        if decision.is_blocked() {
            accepted_event.metadata.blocked = true;
            accepted_event.activity = Some(ActivityKind::Blocked);
        } else if accepted_event.kind == AgentEventKind::ToolCompleted
            && accepted_event
                .metadata
                .exit_status
                .is_some_and(|status| status != 0)
            && accepted_event.activity == Some(ActivityKind::Error)
            && accepted_event.metadata.command_category.as_deref() == Some("development")
        {
            // The installed hook contract labels a failed command as Error,
            // while the domain's completion reducer uses Testing/Building to
            // apply its failure outcome. This internal reducer hint is never
            // persisted as event content; the canonical event remains privacy
            // limited at the boundary.
            accepted_event.activity = Some(ActivityKind::Testing);
        }
        simulation.apply_event(&accepted_event)?;
        let receipt = self.persist_and_broadcast(&mut simulation, before, false)?;
        Ok(EventIngestReceipt {
            snapshot: receipt.snapshot,
            duplicate: receipt.duplicate,
            decision,
        })
    }

    pub fn apply_event(&self, event: &AgentEvent) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let duplicate = before.processed_event_ids.contains(&event.id);
        simulation.apply_event(event)?;
        if duplicate {
            return Ok(MutationReceipt {
                snapshot: simulation.snapshot(),
                duplicate: true,
            });
        }
        self.persist_and_broadcast(&mut simulation, before, false)
    }

    pub fn feed(
        &self,
        action_id: Uuid,
        food_id: impl Into<String>,
    ) -> Result<MutationReceipt, RuntimeError> {
        self.apply_care(CareCommand::Feed {
            action_id,
            food_id: food_id.into(),
        })
    }

    pub fn clean(&self, action_id: Uuid, poop_id: Uuid) -> Result<MutationReceipt, RuntimeError> {
        self.apply_care(CareCommand::CleanPoop { action_id, poop_id })
    }

    pub fn nap(&self, action_id: Uuid) -> Result<MutationReceipt, RuntimeError> {
        self.apply_care(CareCommand::Nap { action_id })
    }

    pub fn pet(
        &self,
        action_id: Uuid,
        interaction_ms: u64,
        pointer_distance: f32,
    ) -> Result<MutationReceipt, RuntimeError> {
        self.apply_care(CareCommand::Pet {
            action_id,
            interaction_ms,
            pointer_distance,
        })
    }

    /// Applies one increment of an in-progress petting gesture so happiness
    /// rises continuously while the user pets, not only on pointer release.
    /// The cumulative evidence is revalidated by the domain before any
    /// authoritative happiness mutation is persisted.
    pub fn pet_stroke(
        &self,
        action_id: Uuid,
        duration_ms: u64,
        distance: f64,
    ) -> Result<MutationReceipt, RuntimeError> {
        self.apply_care(CareCommand::PetStroke {
            action_id,
            duration_ms,
            distance,
        })
    }

    pub fn apply_care(&self, command: CareCommand) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let duplicate = before
            .processed_care_ids
            .contains(&care_action_id(&command));
        let result = simulation.apply_care(&command)?;
        debug_assert_eq!(duplicate, matches!(result, CareResult::Duplicate));
        if matches!(result, CareResult::Duplicate) {
            return Ok(MutationReceipt {
                snapshot: simulation.snapshot(),
                duplicate: true,
            });
        }
        self.persist_and_broadcast(&mut simulation, before, false)
    }

    pub fn maintenance_tick(&self) -> Result<bool, RuntimeError> {
        self.maintenance_tick_at(Utc::now())
    }

    pub fn maintenance_tick_at(&self, timestamp: DateTime<Utc>) -> Result<bool, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        simulation.current_state_at(timestamp);
        if simulation.snapshot() == before {
            return Ok(false);
        }
        self.persist_and_broadcast(&mut simulation, before, false)?;
        Ok(true)
    }

    /// Fixed demo transition: make hunger and energy critical at the current
    /// wall clock without jumping the simulation timeline into the future.
    /// No caller-supplied value is accepted.
    pub fn debug_neglect(&self) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        simulation.apply_debug_neglect();
        self.persist_and_broadcast(&mut simulation, before, false)
    }

    /// Fixed demo transition: restore the unlimited care-item pantry at the
    /// current wall clock. There is no caller-supplied value.
    pub fn debug_restock(&self) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        simulation.apply_debug_restock();
        self.persist_and_broadcast(&mut simulation, before, false)
    }

    /// Fixed demo transition: feed a bounded amount of known food and apply a
    /// bounded number of testing work events so the domain's normal threshold
    /// logic creates a real authoritative poop. There is no arbitrary input.
    pub fn debug_generate_poop(&self) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let (food, feed_count) = if simulation.pet().inventory().count(FoodKind::Kibble) >= 3 {
            (FoodKind::Kibble, 3_u128)
        } else if simulation.pet().inventory().count(FoodKind::Treat) >= 5 {
            (FoodKind::Treat, 5_u128)
        } else if simulation.pet().inventory().count(FoodKind::Fruit) >= 4 {
            (FoodKind::Fruit, 4_u128)
        } else {
            return Err(RuntimeError::Care(CareError::OutOfStock(
                "debug_generate_poop needs fixed demo food".to_owned(),
            )));
        };

        let transition = (|| {
            for index in 0..feed_count {
                let action_id = Uuid::new_v5(
                    &before.pet_id,
                    format!(
                        "codegotchi-debug-generate-poop-feed:{}:{index}",
                        before.poop_sequence
                    )
                    .as_bytes(),
                );
                simulation.apply_care(&CareCommand::Feed {
                    action_id,
                    food_id: food.id().to_owned(),
                })?;
            }
            let session_id = Uuid::new_v5(&before.pet_id, b"codegotchi-debug-session");
            for index in 0..10_u128 {
                let event_id = Uuid::new_v5(
                    &before.pet_id,
                    format!(
                        "codegotchi-debug-generate-poop-work:{}:{}:{}",
                        before.poop_sequence, before.work_points, index
                    )
                    .as_bytes(),
                );
                let event = AgentEvent::new(
                    event_id,
                    session_id,
                    "codegotchi-debug",
                    codegotchi_domain::EventSource::Generic,
                    AgentEventKind::ToolStarted,
                    Some(ActivityKind::Testing),
                    before.last_updated_at + Duration::milliseconds(index as i64 + 1),
                    Default::default(),
                );
                simulation.apply_event(&event)?;
            }
            if simulation.pet().pending_poops().len() == before.pending_poops.len() {
                return Err(RuntimeError::Care(CareError::UnsupportedCondition));
            }
            Ok::<(), RuntimeError>(())
        })();

        if let Err(error) = transition {
            *simulation =
                PetSimulation::from_snapshot(before, SystemClock, DefaultNeedProgressionStrategy)?;
            return Err(error);
        }

        self.persist_and_broadcast(&mut simulation, before, false)
    }

    pub fn subscribe(
        &self,
    ) -> Result<(SimulationSnapshot, broadcast::Receiver<SimulationSnapshot>), RuntimeError> {
        let simulation = self.lock_simulation()?;
        let snapshot = simulation.snapshot();
        let receiver = self.snapshots.subscribe();
        drop(simulation);
        Ok((snapshot, receiver))
    }

    fn lock_simulation(&self) -> Result<MutexGuard<'_, RuntimeSimulation>, RuntimeError> {
        self.simulation
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned)
    }

    fn persist_and_broadcast(
        &self,
        simulation: &mut RuntimeSimulation,
        before: SimulationSnapshot,
        duplicate: bool,
    ) -> Result<MutationReceipt, RuntimeError> {
        let next = simulation.snapshot();
        if let Err(error) = self.store.save(&next) {
            *simulation =
                PetSimulation::from_snapshot(before, SystemClock, DefaultNeedProgressionStrategy)?;
            return Err(error.into());
        }
        self.broadcast(next.clone());
        Ok(MutationReceipt {
            snapshot: next,
            duplicate,
        })
    }

    fn broadcast(&self, snapshot: SimulationSnapshot) {
        let _ = self.snapshots.send(snapshot);
    }
}

fn care_action_id(command: &CareCommand) -> Uuid {
    match command {
        CareCommand::Feed { action_id, .. }
        | CareCommand::CleanPoop { action_id, .. }
        | CareCommand::Pet { action_id, .. }
        | CareCommand::PetStroke { action_id, .. }
        | CareCommand::Nap { action_id } => *action_id,
    }
}

fn seed_new_pet(pet: Pet) -> Pet {
    if !pet.inventory().is_empty() {
        return pet;
    }
    Pet::with_inventory(
        pet.id(),
        pet.name().to_owned(),
        pet.species(),
        pet.last_updated_at(),
        FoodInventory::unlimited(),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use codegotchi_domain::{
        AttentionIncidentKind, CareError, DefaultNeedProgressionStrategy, FakeClock, FoodInventory,
        Pet, PetBehavior, PetNeeds, PetSimulation, PetSpecies, incident_delay_ms, incident_kind,
    };
    use tokio::sync::broadcast::error::TryRecvError;
    use uuid::Uuid;

    use super::{AuthoritativeRuntime, RuntimeError};
    use crate::persistence::SqliteStore;

    fn pet_id_with_first_affection_incident() -> Uuid {
        (1..10_000)
            .map(Uuid::from_u128)
            .find(|pet_id| incident_kind(*pet_id, 0) == AttentionIncidentKind::Affection)
            .expect("fixture should have an affection incident")
    }

    #[test]
    fn pet_is_a_replay_safe_authoritative_mutation() {
        let pet_id = pet_id_with_first_affection_incident();
        let start = Utc::now();
        let clock = FakeClock::new(start);
        let delay = incident_delay_ms(pet_id, 0);
        let pet = Pet::with_inventory(
            pet_id,
            "Mochi",
            PetSpecies::Cat,
            start,
            FoodInventory::starter(),
        );
        let mut simulation = PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy);
        simulation.current_state_at(start + Duration::milliseconds(delay as i64));
        let mut initial = simulation.snapshot();
        initial.needs = PetNeeds::new(0.0, 100.0, 50.0, 100.0);
        initial.behavior = PetBehavior::Wandering;
        let initial_happiness = initial.needs.happiness();

        let runtime = AuthoritativeRuntime::new(SqliteStore::open(":memory:").unwrap(), initial)
            .expect("runtime should restore the fixture");
        let action_id = Uuid::from_u128(9001);

        let first = runtime
            .pet(action_id, 1_500, 120.0)
            .expect("first pet should apply");
        assert!(!first.duplicate);
        assert_eq!(first.snapshot.pending_demands.len(), 0);
        assert!(first.snapshot.needs.happiness() > initial_happiness);
        assert!(first.snapshot.needs.happiness() <= initial_happiness + 10.0);

        let second = runtime
            .pet(action_id, 1_500, 120.0)
            .expect("replayed pet should be accepted");
        assert!(second.duplicate);
        assert_eq!(second.snapshot, first.snapshot);
    }

    #[test]
    fn restoring_a_runtime_normalizes_existing_care_items_to_unlimited() {
        let start = Utc::now();
        let mut finite = FoodInventory::default();
        finite.add(codegotchi_domain::FoodKind::Kibble, 1);
        let pet = Pet::with_inventory(
            Uuid::from_u128(9003),
            "Mochi",
            codegotchi_domain::PetSpecies::Cat,
            start,
            finite,
        );
        let initial =
            PetSimulation::new(pet, FakeClock::new(start), DefaultNeedProgressionStrategy)
                .snapshot();
        let store = SqliteStore::open(":memory:").unwrap();
        let runtime = AuthoritativeRuntime::new(store.clone(), initial).unwrap();

        runtime
            .feed(Uuid::from_u128(9004), "kibble")
            .expect("first feed should apply");
        runtime
            .feed(Uuid::from_u128(9005), "kibble")
            .expect("care items should not run out");

        assert_eq!(
            runtime
                .snapshot()
                .inventory
                .count(codegotchi_domain::FoodKind::Kibble),
            u32::MAX
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .expect("runtime snapshot persists")
                .inventory
                .count(codegotchi_domain::FoodKind::Kibble),
            u32::MAX
        );
    }

    #[test]
    fn rename_persists_broadcasts_and_reports_same_name_as_a_duplicate() {
        let start = Utc::now();
        let store = SqliteStore::open(":memory:").unwrap();
        let runtime = AuthoritativeRuntime::new(
            store.clone(),
            Pet::new(Uuid::from_u128(9006), "Mochi", PetSpecies::Cat, start),
        )
        .unwrap();
        let (_, mut snapshots) = runtime.subscribe().unwrap();

        let renamed = runtime.rename("  Luna  ").expect("valid name should apply");

        assert!(!renamed.duplicate);
        assert_eq!(renamed.snapshot.name, "Luna");
        assert_eq!(snapshots.try_recv().unwrap(), renamed.snapshot);
        assert_eq!(store.load().unwrap().unwrap().name, "Luna");

        let repeated = runtime.rename("Luna").expect("same name is a no-op");

        assert!(repeated.duplicate);
        assert_eq!(repeated.snapshot, renamed.snapshot);
        assert!(matches!(snapshots.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn pet_stroke_revalidates_cumulative_evidence_and_is_replay_safe() {
        let pet_id = pet_id_with_first_affection_incident();
        let start = Utc::now();
        let clock = FakeClock::new(start);
        let delay = incident_delay_ms(pet_id, 0);
        let pet = Pet::with_inventory(
            pet_id,
            "Mochi",
            PetSpecies::Cat,
            start,
            FoodInventory::starter(),
        );
        let mut simulation = PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy);
        simulation.current_state_at(start + Duration::milliseconds(delay as i64));
        let mut initial = simulation.snapshot();
        initial.needs = PetNeeds::new(0.0, 100.0, 50.0, 100.0);
        initial.behavior = PetBehavior::Wandering;

        let runtime = AuthoritativeRuntime::new(SqliteStore::open(":memory:").unwrap(), initial)
            .expect("runtime should restore the fixture");
        let invalid = runtime
            .pet_stroke(Uuid::from_u128(9002), 1_500, 112.0)
            .expect_err("below-threshold distance must be rejected by the runtime");
        assert!(matches!(
            invalid,
            RuntimeError::Care(CareError::InsufficientDistance)
        ));

        let before = runtime.snapshot();
        let applied = runtime
            .pet_stroke(Uuid::from_u128(9002), 1_500, 128.0)
            .expect("qualified cumulative evidence should apply");
        assert!(!applied.duplicate);
        assert!(applied.snapshot.needs.happiness() > before.needs.happiness());
        assert_eq!(
            applied.snapshot.pending_demands.len(),
            before.pending_demands.len(),
            "continuous strokes must not resolve affection"
        );

        let replay = runtime
            .pet_stroke(Uuid::from_u128(9002), 1_500, 128.0)
            .expect("replayed stroke should be accepted as a no-op");
        assert!(replay.duplicate);
        assert_eq!(replay.snapshot, applied.snapshot);
    }

    #[tokio::test]
    async fn maintenance_catch_up_persists_and_broadcasts_one_bounded_snapshot() {
        let initial_time = Utc::now() - Duration::days(2);
        let pet_id = Uuid::from_u128(42);
        let pet = Pet::with_inventory(
            pet_id,
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
        initial.next_incident_at = Some(initial_time + Duration::minutes(1));
        let store = SqliteStore::open(":memory:").unwrap();
        let runtime = AuthoritativeRuntime::new(store.clone(), initial).unwrap();
        let before = runtime.snapshot();
        let (_, mut snapshots) = runtime.subscribe().unwrap();
        let target = Utc::now();

        assert!(before.last_updated_at < target);
        assert!(before.next_incident_at.expect("scheduled incident") < target);
        assert!(runtime.maintenance_tick_at(target).unwrap());

        let broadcast = snapshots.recv().await.unwrap();
        let persisted = store.load().unwrap().expect("maintenance should persist");
        assert_eq!(broadcast, persisted);
        assert_eq!(broadcast, runtime.snapshot());
        let generated_incidents = broadcast.pending_demands.len() + broadcast.pending_poops.len();
        assert_eq!(generated_incidents, 5);
        assert!(broadcast.last_updated_at >= target);
        assert!(broadcast.next_incident_at.expect("future schedule") > target);
        assert!(
            broadcast.needs.hunger() > before.needs.hunger()
                || broadcast.needs.energy() < before.needs.energy()
                || broadcast.needs.happiness() < before.needs.happiness()
                || broadcast.needs.cleanliness() < before.needs.cleanliness()
        );

        let second_target = target + Duration::seconds(1);
        assert!(runtime.maintenance_tick_at(second_target).unwrap());
        let second = snapshots.recv().await.unwrap();
        assert_eq!(
            second.pending_demands.len() + second.pending_poops.len(),
            generated_incidents
        );
        assert!(second.next_incident_at.expect("future schedule") > second_target);
        assert!(matches!(snapshots.try_recv(), Err(TryRecvError::Empty)));
    }
}

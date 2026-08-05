use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Duration, Utc};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventError, AgentEventKind, CareCommand, CareError, CareResult,
    CommandCategory, CommandClassification, CommandPurpose, DefaultNeedProgressionStrategy,
    EnforcementMode, FoodInventory, FoodKind, Pet, PetSettings, PetSimulation, SimulationSnapshot,
    SnapshotRestoreError, SystemClock, WorkDecision, WorkPermissionPolicy,
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
        let snapshot = store.load_or_initialize(initial_snapshot)?;
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
        let decision = WorkPermissionPolicy::evaluate(
            simulation.pet(),
            &classification,
            &PetSettings::new(before.enforcement_mode),
        );
        let duplicate = before.processed_event_ids.contains(&event.id);
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
        if duplicate {
            return Ok(EventIngestReceipt {
                snapshot: simulation.snapshot(),
                duplicate: true,
                decision,
            });
        }
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

    /// Fixed demo transition: advance the authoritative simulation enough for
    /// idle hunger to become critical. No caller-supplied value is accepted.
    pub fn debug_neglect(&self) -> Result<MutationReceipt, RuntimeError> {
        let mut simulation = self.lock_simulation()?;
        let before = simulation.snapshot();
        let timestamp = before.last_updated_at + Duration::hours(100);
        simulation.current_state_at(timestamp);
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
        | CareCommand::Pet { action_id, .. } => *action_id,
    }
}

fn seed_new_pet(pet: Pet) -> Pet {
    if !pet.inventory().is_empty() {
        return pet;
    }
    let mut inventory = FoodInventory::default();
    inventory.add(codegotchi_domain::FoodKind::Kibble, 50);
    inventory.add(codegotchi_domain::FoodKind::Treat, 25);
    inventory.add(codegotchi_domain::FoodKind::Fruit, 25);
    Pet::with_inventory(
        pet.id(),
        pet.name().to_owned(),
        pet.species(),
        pet.last_updated_at(),
        inventory,
    )
}

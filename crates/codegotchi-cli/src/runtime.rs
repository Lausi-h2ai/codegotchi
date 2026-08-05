use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use codegotchi_domain::{
    AgentEvent, AgentEventError, CareCommand, CareError, CareResult,
    DefaultNeedProgressionStrategy, EnforcementMode, FoodInventory, Pet, PetSimulation,
    SimulationSnapshot, SnapshotRestoreError, SystemClock,
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
        if before.enforcement_mode == mode {
            return Ok(MutationReceipt {
                snapshot: before,
                duplicate: true,
            });
        }
        simulation.set_enforcement_mode(mode);
        self.persist_and_broadcast(&mut simulation, before, false)
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

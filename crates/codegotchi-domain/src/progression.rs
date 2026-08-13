use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    NAP_DURATION,
    attention::{
        AttentionIncidentKind, MAX_CATCH_UP_INCIDENTS, PetDemand, PetDemandKind, incident_delay_ms,
        incident_id, incident_kind,
    },
    behavior::BehaviorCoordinator,
    care::{CareCommand, CareError, CareResult},
    clock::Clock,
    event::{ActivityKind, AgentEvent, AgentEventError, AgentEventKind},
    permission::{EnforcementMode, PetSettings},
    pet::{AgentActivityState, AgentOutcome, FoodKind, Pet, PetBehavior, PetNeeds, Poop},
    poop::{DefaultPoopGenerationStrategy, PoopGenerationStrategy},
};

const HUNGER_PER_HOUR: f32 = 25.0;
const ENERGY_PER_HOUR: f32 = -50.0;
const INCIDENT_PRESSURE_PER_HOUR: f32 = 240.0;
/// The scheduler has one representable terminal deadline. It keeps the
/// internal scheduler type as `DateTime<Utc>` while making the final valid
/// attention sequence persistable without ever attempting to add another
/// incident.
const TERMINAL_INCIDENT_AT: DateTime<Utc> = DateTime::<Utc>::MAX_UTC;
/// A hammock nap is a deliberately fast power nap: 20 energy points per
/// second, so a 5-second nap restores the full meter from empty.
const NAP_ENERGY_PER_HOUR: f32 = 72_000.0;

/// Applies elapsed need changes without consulting wall-clock state.
pub trait NeedProgressionStrategy {
    fn progress(&self, pet: &mut Pet, elapsed: Duration, previous_activity: AgentActivityState);
}

/// The deterministic linear progression required by the domain plan.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultNeedProgressionStrategy;

impl NeedProgressionStrategy for DefaultNeedProgressionStrategy {
    fn progress(&self, pet: &mut Pet, elapsed: Duration, _previous_activity: AgentActivityState) {
        let elapsed_hours = elapsed_hours_precise(elapsed);
        if elapsed_hours <= 0.0 {
            return;
        }

        let affection_count = pet
            .pending_demands()
            .iter()
            .filter(|demand| demand.kind() == PetDemandKind::Affection)
            .count() as f64;
        let snack_count = pet
            .pending_demands()
            .iter()
            .filter(|demand| demand.kind() == PetDemandKind::Snack)
            .count() as f64;
        let poop_count = pet.pending_poops().len() as f64;

        // Energy recovers at the nap rate for the portion of this elapsed
        // segment that overlaps the nap window, and at the normal rate for
        // the remainder. The nap window always starts at the segment boundary
        // when a nap is active (care advances time first), so only the
        // deadline is needed here.
        let segment_start = pet.last_updated_at() - elapsed;
        let segment_end = pet.last_updated_at();
        let nap_hours = pet
            .napping_until()
            .map(|until| nap_elapsed_hours_precise(segment_start, segment_end, until))
            .unwrap_or_default();

        let needs = pet.needs_mut();
        needs.adjust_hunger_precise(
            (HUNGER_PER_HOUR as f64 + INCIDENT_PRESSURE_PER_HOUR as f64 * snack_count)
                * elapsed_hours,
        );
        needs.adjust_happiness_precise(
            -(INCIDENT_PRESSURE_PER_HOUR as f64) * affection_count * elapsed_hours,
        );
        needs.adjust_cleanliness_precise(
            -(INCIDENT_PRESSURE_PER_HOUR as f64) * poop_count * elapsed_hours,
        );
        needs.adjust_energy_precise(
            ENERGY_PER_HOUR as f64 * (elapsed_hours - nap_hours)
                + NAP_ENERGY_PER_HOUR as f64 * nap_hours,
        );
    }
}

/// The activity and logical update time of one registered agent session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionActivity {
    activity: AgentActivityState,
    updated_at: DateTime<Utc>,
}

impl SessionActivity {
    pub fn activity(&self) -> AgentActivityState {
        self.activity
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// The complete versioned state needed to continue one deterministic simulation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationSnapshot {
    pub schema_version: u16,
    pub pet_id: Uuid,
    pub name: String,
    pub species: crate::pet::PetSpecies,
    pub needs: PetNeeds,
    pub behavior: PetBehavior,
    pub activity: AgentActivityState,
    pub recent_outcome: AgentOutcome,
    pub work_points: u64,
    pub digestion_points: u64,
    pub last_updated_at: DateTime<Utc>,
    pub pending_poops: Vec<Poop>,
    #[serde(default)]
    pub pending_demands: Vec<PetDemand>,
    pub inventory: crate::pet::FoodInventory,
    pub processed_care_ids: BTreeSet<Uuid>,
    pub poop_sequence: u64,
    #[serde(default)]
    /// The next incident sequence while the scheduler is active. A terminal
    /// scheduler keeps the final issued sequence (`u64::MAX - 1`) here and
    /// stores [`TERMINAL_INCIDENT_AT`] as `next_incident_at`.
    pub attention_sequence: u64,
    #[serde(default)]
    pub next_incident_at: Option<DateTime<Utc>>,
    pub session_activities: BTreeMap<Uuid, SessionActivity>,
    pub processed_event_ids: BTreeSet<Uuid>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub last_outcome_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub enforcement_mode: EnforcementMode,
    /// Deadline of the current hammock nap, if any. Serialized with a default
    /// so snapshots persisted before naps existed still restore cleanly.
    #[serde(default)]
    pub napping_until: Option<DateTime<Utc>>,
}

pub const SIMULATION_SNAPSHOT_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SnapshotRestoreError {
    #[error(
        "unsupported simulation snapshot schema version {0}; expected {SIMULATION_SNAPSHOT_SCHEMA_VERSION}"
    )]
    UnsupportedSchemaVersion(u16),
    #[error("simulation snapshot invariant violation: {0}")]
    InvariantViolation(String),
}

/// Owns external time and the strategy that advances the clock-free aggregate.
pub struct PetSimulation<C, N, P = DefaultPoopGenerationStrategy> {
    pet: Pet,
    clock: C,
    progression: N,
    poop_strategy: P,
    session_activities: BTreeMap<Uuid, SessionActivity>,
    processed_event_ids: BTreeSet<Uuid>,
    processed_care_ids: BTreeSet<Uuid>,
    last_activity_at: Option<DateTime<Utc>>,
    last_outcome_at: Option<DateTime<Utc>>,
    consecutive_failures: u32,
    settings: PetSettings,
    attention_sequence: u64,
    next_incident_at: DateTime<Utc>,
}

impl<C, N> PetSimulation<C, N, DefaultPoopGenerationStrategy>
where
    C: Clock,
    N: NeedProgressionStrategy,
{
    pub fn new(pet: Pet, clock: C, progression: N) -> Self {
        Self::with_poop_strategy(pet, clock, progression, DefaultPoopGenerationStrategy)
    }

    pub fn from_snapshot(
        snapshot: SimulationSnapshot,
        clock: C,
        progression: N,
    ) -> Result<Self, SnapshotRestoreError> {
        Self::with_poop_strategy_from_snapshot(
            snapshot,
            clock,
            progression,
            DefaultPoopGenerationStrategy,
        )
    }
}

impl<C, N, P> PetSimulation<C, N, P>
where
    C: Clock,
    N: NeedProgressionStrategy,
    P: PoopGenerationStrategy,
{
    /// Constructs a simulation with an explicitly injected poop strategy.
    pub fn with_poop_strategy(pet: Pet, clock: C, progression: N, poop_strategy: P) -> Self {
        let initial_timestamp = pet.last_updated_at();
        let attention_sequence = 0;
        let next_incident_at = initial_timestamp
            + Duration::milliseconds(incident_delay_ms(pet.id(), attention_sequence) as i64);
        Self {
            pet,
            clock,
            progression,
            poop_strategy,
            session_activities: BTreeMap::new(),
            processed_event_ids: BTreeSet::new(),
            processed_care_ids: BTreeSet::new(),
            last_activity_at: Some(initial_timestamp),
            last_outcome_at: None,
            consecutive_failures: 0,
            settings: PetSettings::default(),
            attention_sequence,
            next_incident_at,
        }
    }

    pub fn pet(&self) -> &Pet {
        &self.pet
    }

    pub fn session_activities(&self) -> &BTreeMap<Uuid, SessionActivity> {
        &self.session_activities
    }

    pub fn processed_event_ids(&self) -> &BTreeSet<Uuid> {
        &self.processed_event_ids
    }

    pub fn processed_care_ids(&self) -> &BTreeSet<Uuid> {
        &self.processed_care_ids
    }

    pub fn last_activity_at(&self) -> Option<DateTime<Utc>> {
        self.last_activity_at
    }

    pub fn last_outcome_at(&self) -> Option<DateTime<Utc>> {
        self.last_outcome_at
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn enforcement_mode(&self) -> EnforcementMode {
        self.settings.enforcement_mode()
    }

    pub fn set_enforcement_mode(&mut self, mode: EnforcementMode) {
        self.settings = PetSettings::new(mode);
    }

    /// Guarded demo transition: make the pet visibly neglected at the current
    /// wall clock. Hunger and energy both become critical so the strict-mode
    /// refusal tiers and the energy care loop can be demonstrated, but the
    /// simulation clock stays at the wall clock. The previous implementation
    /// advanced the logical timeline 100 hours into the future, which froze
    /// every later progression (naps, hunger, maintenance) until the wall
    /// clock caught up.
    pub fn apply_debug_neglect(&mut self) {
        self.current_state_at(self.clock.now());
        let needs = self.pet.needs_mut();
        needs.set_hunger(100.0);
        needs.set_energy(0.0);
        self.refresh_behavior(self.pet.last_updated_at());
    }

    /// Guarded demo control: restore the starter pantry (50/25/25/10) at the
    /// current wall clock. Needs, timeline, and behavior are untouched, so a
    /// drained demo pet can be refed without waiting for the wall clock.
    pub fn apply_debug_restock(&mut self) {
        self.current_state_at(self.clock.now());
        self.pet.restock_inventory_to_starter();
        self.refresh_behavior(self.pet.last_updated_at());
    }

    /// Advances elapsed effects to the injected clock and stores behavior.
    pub fn advance_time(&mut self) -> Duration {
        let previous_activity = self.pet.activity();
        let elapsed = self.advance_elapsed_to(self.clock.now(), previous_activity);
        self.refresh_behavior(self.pet.last_updated_at());
        elapsed
    }

    /// Advances to the injected clock, refreshes the stored behavior, and
    /// returns the current in-memory read model.
    pub fn current_state(&mut self) -> SimulationSnapshot {
        self.current_state_at(self.clock.now())
    }

    /// Advances to an explicit logical timestamp for deterministic maintenance.
    pub fn current_state_at(&mut self, timestamp: DateTime<Utc>) -> SimulationSnapshot {
        if timestamp <= self.pet.last_updated_at() {
            return self.snapshot();
        }
        let previous_activity = self.pet.activity();
        self.advance_elapsed_to(timestamp, previous_activity);
        self.refresh_behavior(self.pet.last_updated_at());
        self.snapshot()
    }

    /// Returns the stored aggregate state without advancing time.
    pub fn snapshot(&self) -> SimulationSnapshot {
        SimulationSnapshot {
            schema_version: SIMULATION_SNAPSHOT_SCHEMA_VERSION,
            pet_id: self.pet.id(),
            name: self.pet.name().to_owned(),
            species: self.pet.species(),
            needs: self.pet.needs(),
            behavior: self.pet.behavior(),
            activity: self.pet.activity(),
            recent_outcome: self.pet.recent_outcome(),
            work_points: self.pet.work_points(),
            digestion_points: self.pet.digestion_points(),
            last_updated_at: self.pet.last_updated_at(),
            pending_poops: self.pet.pending_poops().to_vec(),
            pending_demands: self.pet.pending_demands().to_vec(),
            inventory: self.pet.inventory().clone(),
            processed_care_ids: self.processed_care_ids.clone(),
            poop_sequence: self.pet.poop_sequence(),
            attention_sequence: self.attention_sequence,
            next_incident_at: Some(self.next_incident_at),
            session_activities: self.session_activities.clone(),
            processed_event_ids: self.processed_event_ids.clone(),
            last_activity_at: self.last_activity_at,
            last_outcome_at: self.last_outcome_at,
            consecutive_failures: self.consecutive_failures,
            enforcement_mode: self.settings.enforcement_mode(),
            napping_until: self.pet.napping_until(),
        }
    }

    /// Applies one event on the monotonic event-time timeline. Duplicate IDs
    /// return before schema validation, timeline movement, or any transition.
    pub fn apply_event(&mut self, event: &AgentEvent) -> Result<(), AgentEventError> {
        if self.processed_event_ids.contains(&event.id) {
            return Ok(());
        }

        event.validate_schema_version()?;

        let previous_activity = self.pet.activity();
        let logical_time = self.pet.last_updated_at().max(event.timestamp);
        self.advance_elapsed_to(logical_time, previous_activity);
        self.processed_event_ids.insert(event.id);
        self.apply_transition(event, logical_time);
        if is_work_bearing_event(event.kind) {
            self.generate_poops(self.pet.last_updated_at());
        }
        self.refresh_activity();
        self.refresh_behavior(logical_time);
        Ok(())
    }

    /// Applies one validated, replay-safe care request against the injected
    /// clock and refreshes the stored derived behavior.
    pub fn apply_care(&mut self, command: &CareCommand) -> Result<CareResult, CareError> {
        let action_id = command.action_id();
        if self.processed_care_ids.contains(&action_id) {
            return Ok(CareResult::Duplicate);
        }

        self.validate_care(command)?;

        let previous_activity = self.pet.activity();
        let now = self.clock.now();
        self.advance_elapsed_to(now, previous_activity);
        self.apply_care_transition(command);
        if matches!(command, CareCommand::Feed { .. }) {
            self.generate_poops(self.pet.last_updated_at());
        }
        self.processed_care_ids.insert(action_id);
        self.refresh_behavior(self.pet.last_updated_at());
        Ok(CareResult::Applied)
    }

    pub fn apply_events<'a, I>(&mut self, events: I) -> Result<(), AgentEventError>
    where
        I: IntoIterator<Item = &'a AgentEvent>,
    {
        for event in events {
            self.apply_event(event)?;
        }
        Ok(())
    }

    fn apply_transition(&mut self, event: &AgentEvent, logical_time: DateTime<Utc>) {
        match event.kind {
            AgentEventKind::SessionStarted => {
                self.session_activities.insert(
                    event.session_id,
                    SessionActivity {
                        activity: AgentActivityState::Idle,
                        updated_at: logical_time,
                    },
                );
            }
            AgentEventKind::SessionEnded => {
                self.session_activities.remove(&event.session_id);
            }
            AgentEventKind::TurnStarted => {
                self.pet.add_work_points(1);
                self.apply_registered_work(event, logical_time, ActivityKind::Thinking);
            }
            AgentEventKind::OutputActivity => {
                self.pet.add_work_points(1);
                self.apply_registered_work(event, logical_time, ActivityKind::UnknownWork);
            }
            AgentEventKind::ToolStarted => {
                self.pet.add_work_points(5);
                self.apply_registered_work(event, logical_time, ActivityKind::UnknownWork);
            }
            AgentEventKind::CommandStarted => {
                self.pet.add_work_points(5);
                self.apply_registered_work(event, logical_time, ActivityKind::UnknownWork);
            }
            AgentEventKind::WaitingForUser | AgentEventKind::TurnCompleted => {
                self.apply_registered_end(event.session_id, logical_time, true);
            }
            AgentEventKind::ToolCompleted | AgentEventKind::CommandCompleted => {
                let was_active = self
                    .session_activities
                    .get(&event.session_id)
                    .is_some_and(|session| is_active(session.activity));
                if was_active {
                    self.last_activity_at = Some(logical_time);
                }
                if let Some(session) = self.session_activities.get_mut(&event.session_id) {
                    session.activity = if event.metadata.blocked
                        || event.activity == Some(ActivityKind::Blocked)
                    {
                        AgentActivityState::Blocked
                    } else {
                        AgentActivityState::Idle
                    };
                    session.updated_at = logical_time;
                }
                self.apply_outcome(event, logical_time);
            }
            AgentEventKind::Interrupted | AgentEventKind::IntegrationError => {
                if let Some(session) = self.session_activities.get_mut(&event.session_id) {
                    session.activity = AgentActivityState::Idle;
                    session.updated_at = logical_time;
                }
            }
        }
    }

    fn validate_care(&self, command: &CareCommand) -> Result<(), CareError> {
        match command {
            CareCommand::Feed { food_id, .. } => {
                let food = FoodKind::from_id(food_id)
                    .ok_or_else(|| CareError::UnknownFood(food_id.clone()))?;
                if !self.pet.inventory().contains(food) {
                    return Err(CareError::OutOfStock(food_id.clone()));
                }
            }
            CareCommand::CleanPoop { poop_id, .. } => {
                if !self
                    .pet
                    .pending_poops()
                    .iter()
                    .any(|poop| poop.id() == *poop_id)
                {
                    return Err(CareError::MissingPoop(*poop_id));
                }
            }
            CareCommand::Pet {
                interaction_ms,
                pointer_distance,
                ..
            } => {
                if *interaction_ms < 1_500 {
                    return Err(CareError::InsufficientDuration);
                }
                if !pointer_distance.is_finite() {
                    return Err(CareError::NonFinitePointerDistance);
                }
                if *pointer_distance < 120.0 {
                    return Err(CareError::InsufficientDistance);
                }
            }
            CareCommand::Nap { .. } => {}
        }

        Ok(())
    }

    fn apply_care_transition(&mut self, command: &CareCommand) {
        match command {
            CareCommand::Feed { food_id, .. } => {
                let Some(food) = FoodKind::from_id(food_id) else {
                    return;
                };
                if !self.pet.consume_food(food) {
                    return;
                }
                match food {
                    FoodKind::Kibble => {
                        self.pet.needs_mut().adjust_hunger(-25.0);
                        self.pet.add_digestion_points(40);
                    }
                    FoodKind::Treat => {
                        self.pet.needs_mut().adjust_hunger(-10.0);
                        self.pet.needs_mut().adjust_happiness(5.0);
                        self.pet.add_digestion_points(20);
                    }
                    FoodKind::Fruit => {
                        self.pet.needs_mut().adjust_hunger(-15.0);
                        self.pet.add_digestion_points(25);
                    }
                    FoodKind::EnergyDrink => {
                        self.pet.needs_mut().adjust_energy(40.0);
                        self.pet.needs_mut().adjust_happiness(5.0);
                    }
                }
                if matches!(food, FoodKind::Kibble | FoodKind::Treat | FoodKind::Fruit) {
                    self.resolve_oldest_demand(PetDemandKind::Snack);
                }
            }
            CareCommand::CleanPoop { poop_id, .. } => {
                if let Some(index) = self
                    .pet
                    .pending_poops
                    .iter()
                    .position(|poop| poop.id() == *poop_id)
                {
                    self.pet.pending_poops.remove(index);
                    self.pet.needs_mut().adjust_cleanliness(25.0);
                }
            }
            CareCommand::Pet { .. } => {
                self.pet.needs_mut().adjust_happiness(10.0);
                self.resolve_oldest_demand(PetDemandKind::Affection);
            }
            CareCommand::Nap { .. } => {
                self.pet
                    .start_nap(self.pet.last_updated_at() + NAP_DURATION);
            }
        }
    }

    fn resolve_oldest_demand(&mut self, kind: PetDemandKind) {
        if let Some(index) = self
            .pet
            .pending_demands()
            .iter()
            .position(|demand| demand.kind() == kind)
        {
            self.pet.remove_demand(index);
        }
    }

    fn generate_poops(&mut self, created_at: DateTime<Utc>) {
        while let Some(threshold) = self
            .poop_strategy
            .threshold(self.pet.digestion_points(), self.pet.work_points())
        {
            // PoopGenerationThreshold rejects zero values at its public
            // constructor. Keep this defensive guard at the mutation boundary
            // so an invalid implementation can never create a nonterminating
            // loop if the representation changes later.
            if threshold.digestion_points() == 0 || threshold.work_points() == 0 {
                break;
            }
            if self.pet.digestion_points() < threshold.digestion_points()
                || self.pet.work_points() < threshold.work_points()
            {
                break;
            }

            let sequence = self.pet.poop_sequence();
            let name = format!("poop:{sequence}");
            let id = Uuid::new_v5(&self.pet.id(), name.as_bytes());
            self.pet
                .consume_digestion_points(threshold.digestion_points());
            self.pet.consume_work_points(threshold.work_points());
            self.pet.push_poop(Poop::new(id, created_at));
            self.pet.advance_poop_sequence();
        }
    }

    fn apply_registered_work(
        &mut self,
        event: &AgentEvent,
        logical_time: DateTime<Utc>,
        default_activity: ActivityKind,
    ) {
        let activity = activity_state(event, default_activity);
        if !self.session_activities.contains_key(&event.session_id) {
            return;
        }

        self.last_activity_at = Some(logical_time);
        if let Some(session) = self.session_activities.get_mut(&event.session_id) {
            session.activity = activity;
            session.updated_at = logical_time;
        }
    }

    fn apply_registered_end(
        &mut self,
        session_id: Uuid,
        logical_time: DateTime<Utc>,
        waiting: bool,
    ) {
        let was_active = self
            .session_activities
            .get(&session_id)
            .is_some_and(|session| is_active(session.activity));
        if was_active {
            self.last_activity_at = Some(logical_time);
        }
        if let Some(session) = self.session_activities.get_mut(&session_id) {
            session.activity = if waiting {
                AgentActivityState::WaitingForUser
            } else {
                AgentActivityState::Idle
            };
            session.updated_at = logical_time;
        }
    }

    fn apply_outcome(&mut self, event: &AgentEvent, logical_time: DateTime<Utc>) {
        if event.metadata.blocked {
            return;
        }

        let Some(activity) = completion_activity(event) else {
            return;
        };
        if !matches!(activity, ActivityKind::Testing | ActivityKind::Building) {
            return;
        }

        let Some(exit_status) = event.metadata.exit_status else {
            return;
        };

        self.last_outcome_at = Some(logical_time);
        if exit_status == 0 {
            self.pet.needs_mut().adjust_happiness(8.0);
            self.pet.set_outcome(AgentOutcome::Success);
            self.consecutive_failures = 0;
        } else {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            let penalty = 4.0 * self.consecutive_failures.min(3) as f32;
            self.pet.needs_mut().adjust_happiness(-penalty);
            self.pet.set_outcome(AgentOutcome::Failure);
        }
    }

    fn progress_segment_to(
        &mut self,
        target: DateTime<Utc>,
        previous_activity: AgentActivityState,
    ) -> Duration {
        let elapsed = self.pet.advance_to(target);
        if elapsed > Duration::zero() {
            self.progression
                .progress(&mut self.pet, elapsed, previous_activity);
        }
        self.pet.clear_expired_nap(target);
        elapsed
    }

    fn advance_elapsed_to(
        &mut self,
        target: DateTime<Utc>,
        previous_activity: AgentActivityState,
    ) -> Duration {
        let start = self.pet.last_updated_at();
        if target <= start {
            return Duration::zero();
        }
        let mut created = 0usize;

        while self.next_incident_at != TERMINAL_INCIDENT_AT
            && self.next_incident_at <= target
            && created < MAX_CATCH_UP_INCIDENTS
        {
            let due = self.next_incident_at.max(self.pet.last_updated_at());
            self.progress_segment_to(due, previous_activity);
            self.create_attention_incident(due);
            created += 1;

            if self.attention_sequence == u64::MAX - 1 {
                self.next_incident_at = TERMINAL_INCIDENT_AT;
                break;
            }

            self.attention_sequence += 1;
            self.next_incident_at = due
                + Duration::milliseconds(
                    incident_delay_ms(self.pet.id(), self.attention_sequence) as i64
                );
        }

        if created == MAX_CATCH_UP_INCIDENTS
            && self.next_incident_at != TERMINAL_INCIDENT_AT
            && self.next_incident_at <= target
        {
            self.progress_segment_to(target, previous_activity);
            self.next_incident_at = target
                + Duration::milliseconds(
                    incident_delay_ms(self.pet.id(), self.attention_sequence) as i64
                );
        } else {
            self.progress_segment_to(target, previous_activity);
        }

        target.signed_duration_since(start).max(Duration::zero())
    }

    fn create_attention_incident(&mut self, created_at: DateTime<Utc>) {
        match incident_kind(self.pet.id(), self.attention_sequence) {
            AttentionIncidentKind::Affection => self.pet.push_demand(PetDemand::new(
                incident_id(
                    self.pet.id(),
                    self.attention_sequence,
                    AttentionIncidentKind::Affection,
                ),
                PetDemandKind::Affection,
                created_at,
            )),
            AttentionIncidentKind::Snack => self.pet.push_demand(PetDemand::new(
                incident_id(
                    self.pet.id(),
                    self.attention_sequence,
                    AttentionIncidentKind::Snack,
                ),
                PetDemandKind::Snack,
                created_at,
            )),
            AttentionIncidentKind::Poop => self.pet.push_poop(Poop::new_attention(
                incident_id(
                    self.pet.id(),
                    self.attention_sequence,
                    AttentionIncidentKind::Poop,
                ),
                created_at,
                self.attention_sequence,
            )),
        }
    }

    fn refresh_activity(&mut self) {
        let aggregate = self.aggregate_activity();
        self.pet.set_activity(aggregate);
    }

    fn aggregate_activity(&self) -> AgentActivityState {
        aggregate_activity(&self.session_activities)
    }

    fn refresh_behavior(&mut self, logical_time: DateTime<Utc>) {
        let behavior = BehaviorCoordinator::derive(
            &self.pet,
            logical_time,
            self.last_activity_at,
            self.last_outcome_at,
        );
        self.pet.set_behavior(behavior);
    }
}

impl<C, N, P> PetSimulation<C, N, P>
where
    C: Clock,
    N: NeedProgressionStrategy,
    P: PoopGenerationStrategy,
{
    pub fn with_poop_strategy_from_snapshot(
        snapshot: SimulationSnapshot,
        clock: C,
        progression: N,
        poop_strategy: P,
    ) -> Result<Self, SnapshotRestoreError> {
        let mut snapshot = snapshot;
        let now = clock.now();
        validate_snapshot(&snapshot)?;
        reanchor_snapshot_to_wall_clock(&mut snapshot, now);
        let next_incident_at = snapshot.next_incident_at.unwrap_or_else(|| {
            now + Duration::milliseconds(incident_delay_ms(
                snapshot.pet_id,
                snapshot.attention_sequence,
            ) as i64)
        });
        let pet = Pet::from_snapshot_with_demands(
            snapshot.pet_id,
            snapshot.name.clone(),
            snapshot.species,
            snapshot.needs,
            snapshot.behavior,
            snapshot.work_points,
            snapshot.digestion_points,
            snapshot.last_updated_at,
            snapshot.pending_poops.clone(),
            snapshot.pending_demands.clone(),
            snapshot.activity,
            snapshot.recent_outcome,
            snapshot.inventory.clone(),
            snapshot.poop_sequence,
            snapshot.napping_until,
        );
        Ok(Self {
            pet,
            clock,
            progression,
            poop_strategy,
            session_activities: snapshot.session_activities,
            processed_event_ids: snapshot.processed_event_ids,
            processed_care_ids: snapshot.processed_care_ids,
            last_activity_at: snapshot.last_activity_at,
            last_outcome_at: snapshot.last_outcome_at,
            consecutive_failures: snapshot.consecutive_failures,
            settings: PetSettings::new(snapshot.enforcement_mode),
            attention_sequence: snapshot.attention_sequence,
            next_incident_at,
        })
    }
}

/// Repairs a persisted snapshot whose logical clock is ahead of the wall
/// clock. The aggregate timeline is monotonic and refuses to move backwards,
/// so a future-dated snapshot (a clock correction, or an old demo neglect
/// that jumped 100 hours ahead) would freeze all progression — including a
/// hammock nap — until the wall clock caught up. Translating every stored
/// timestamp back by the same delta resumes the simulation at the wall clock
/// while preserving all relative timing (nap deadlines, sessions, outcomes,
/// and poop ages).
fn reanchor_snapshot_to_wall_clock(snapshot: &mut SimulationSnapshot, now: DateTime<Utc>) {
    if snapshot.last_updated_at <= now {
        return;
    }

    let shift = snapshot.last_updated_at - now;
    let shift_back = |timestamp: DateTime<Utc>| timestamp - shift;
    snapshot.last_updated_at = now;
    snapshot.napping_until = snapshot.napping_until.map(shift_back);
    snapshot.last_activity_at = snapshot.last_activity_at.map(shift_back);
    snapshot.last_outcome_at = snapshot.last_outcome_at.map(shift_back);
    for session in snapshot.session_activities.values_mut() {
        session.updated_at = shift_back(session.updated_at);
    }
    for poop in &mut snapshot.pending_poops {
        poop.shift_created_at(-shift);
    }
    for demand in &mut snapshot.pending_demands {
        demand.shift_created_at(-shift);
    }
    snapshot.next_incident_at = snapshot.next_incident_at.map(|timestamp| {
        if timestamp == TERMINAL_INCIDENT_AT {
            timestamp
        } else {
            shift_back(timestamp)
        }
    });
}

fn validate_snapshot(snapshot: &SimulationSnapshot) -> Result<(), SnapshotRestoreError> {
    if snapshot.schema_version != SIMULATION_SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotRestoreError::UnsupportedSchemaVersion(
            snapshot.schema_version,
        ));
    }
    if snapshot.pet_id.is_nil() {
        return Err(SnapshotRestoreError::InvariantViolation(
            "pet id must not be nil".to_owned(),
        ));
    }
    if snapshot.name.trim().is_empty() {
        return Err(SnapshotRestoreError::InvariantViolation(
            "pet name must not be empty".to_owned(),
        ));
    }
    if snapshot.attention_sequence == u64::MAX {
        return Err(SnapshotRestoreError::InvariantViolation(
            "attention sequence exhausted".to_owned(),
        ));
    }
    let terminal_schedule = snapshot.next_incident_at == Some(TERMINAL_INCIDENT_AT);
    if terminal_schedule && snapshot.attention_sequence != u64::MAX - 1 {
        return Err(SnapshotRestoreError::InvariantViolation(
            "terminal incident schedule requires the final sequence".to_owned(),
        ));
    }
    if !snapshot.needs.is_valid() {
        return Err(SnapshotRestoreError::InvariantViolation(
            "needs must be finite and bounded".to_owned(),
        ));
    }
    if snapshot
        .pending_poops
        .iter()
        .map(|poop| poop.id())
        .collect::<BTreeSet<_>>()
        .len()
        != snapshot.pending_poops.len()
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "pending poop ids must be unique".to_owned(),
        ));
    }
    let has_invalid_attention_poop = snapshot.pending_poops.iter().any(|poop| {
        let Some(sequence) = poop.attention_sequence() else {
            return false;
        };
        sequence > snapshot.attention_sequence
            || (!terminal_schedule && sequence == snapshot.attention_sequence)
            || incident_kind(snapshot.pet_id, sequence) != AttentionIncidentKind::Poop
            || incident_id(snapshot.pet_id, sequence, AttentionIncidentKind::Poop) != poop.id()
    });
    if has_invalid_attention_poop {
        return Err(SnapshotRestoreError::InvariantViolation(
            "attention poop provenance is invalid".to_owned(),
        ));
    }
    let has_work_poop = snapshot
        .pending_poops
        .iter()
        .any(|poop| poop.attention_sequence().is_none());
    // Snapshots written before Poop provenance was added can contain an
    // attention-generated poop without `attention_sequence`. Their persisted
    // scheduler fields identify the trusted legacy shape; retain those poops
    // as unprovenanced entries rather than attempting an unbounded ID lookup.
    let is_legacy_unprovenanced_snapshot = has_work_poop
        && snapshot.attention_sequence > 0
        && snapshot.next_incident_at.is_some()
        && snapshot.poop_sequence == 0;
    if has_work_poop && snapshot.poop_sequence == 0 && !is_legacy_unprovenanced_snapshot {
        return Err(SnapshotRestoreError::InvariantViolation(
            "pending poops require a positive poop sequence".to_owned(),
        ));
    }
    if snapshot
        .pending_demands
        .iter()
        .map(|demand| demand.id())
        .collect::<BTreeSet<_>>()
        .len()
        != snapshot.pending_demands.len()
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "pending demand ids must be unique".to_owned(),
        ));
    }
    if snapshot
        .processed_event_ids
        .iter()
        .chain(snapshot.processed_care_ids.iter())
        .any(Uuid::is_nil)
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "replay ids must not be nil".to_owned(),
        ));
    }
    if snapshot
        .inventory
        .quantities()
        .any(|(_, quantity)| quantity == 0)
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "inventory must not store zero quantities".to_owned(),
        ));
    }
    if snapshot
        .session_activities
        .values()
        .any(|session| session.updated_at() > snapshot.last_updated_at)
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "session activity cannot be newer than the aggregate".to_owned(),
        ));
    }
    if snapshot
        .last_activity_at
        .is_some_and(|timestamp| timestamp > snapshot.last_updated_at)
        || snapshot
            .last_outcome_at
            .is_some_and(|timestamp| timestamp > snapshot.last_updated_at)
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "activity and outcome timestamps cannot be newer than the aggregate".to_owned(),
        ));
    }
    if snapshot
        .napping_until
        .is_some_and(|until| until <= snapshot.last_updated_at)
    {
        return Err(SnapshotRestoreError::InvariantViolation(
            "a persisted nap deadline must still be in the future".to_owned(),
        ));
    }

    let expected_activity = aggregate_activity(&snapshot.session_activities);
    if snapshot.activity != expected_activity {
        return Err(SnapshotRestoreError::InvariantViolation(
            "stored activity does not match registered sessions".to_owned(),
        ));
    }

    let pet = Pet::from_snapshot_with_demands(
        snapshot.pet_id,
        snapshot.name.clone(),
        snapshot.species,
        snapshot.needs,
        snapshot.behavior,
        snapshot.work_points,
        snapshot.digestion_points,
        snapshot.last_updated_at,
        snapshot.pending_poops.clone(),
        snapshot.pending_demands.clone(),
        snapshot.activity,
        snapshot.recent_outcome,
        snapshot.inventory.clone(),
        snapshot.poop_sequence,
        snapshot.napping_until,
    );
    let expected_behavior = BehaviorCoordinator::derive(
        &pet,
        snapshot.last_updated_at,
        snapshot.last_activity_at,
        snapshot.last_outcome_at,
    );
    if snapshot.behavior != expected_behavior {
        return Err(SnapshotRestoreError::InvariantViolation(
            "stored behavior is not derivable from the snapshot".to_owned(),
        ));
    }
    Ok(())
}

fn completion_activity(event: &AgentEvent) -> Option<ActivityKind> {
    match event.kind {
        AgentEventKind::ToolCompleted | AgentEventKind::CommandCompleted => event.activity,
        _ => None,
    }
}

fn aggregate_activity(sessions: &BTreeMap<Uuid, SessionActivity>) -> AgentActivityState {
    if sessions
        .values()
        .any(|session| session.activity == AgentActivityState::Blocked)
    {
        return AgentActivityState::Blocked;
    }

    if let Some((_, session)) = sessions
        .iter()
        .filter(|(_, session)| is_active(session.activity))
        .max_by(|(left_id, left), (right_id, right)| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left_id.cmp(right_id))
        })
    {
        return session.activity;
    }

    if sessions
        .values()
        .any(|session| session.activity == AgentActivityState::WaitingForUser)
    {
        return AgentActivityState::WaitingForUser;
    }

    AgentActivityState::Idle
}

fn is_work_bearing_event(kind: AgentEventKind) -> bool {
    matches!(
        kind,
        AgentEventKind::TurnStarted
            | AgentEventKind::OutputActivity
            | AgentEventKind::ToolStarted
            | AgentEventKind::CommandStarted
    )
}

fn activity_state(event: &AgentEvent, default: ActivityKind) -> AgentActivityState {
    if event.metadata.blocked {
        return AgentActivityState::Blocked;
    }

    match event.activity.unwrap_or(default) {
        ActivityKind::Idle => AgentActivityState::Idle,
        ActivityKind::Waiting => AgentActivityState::WaitingForUser,
        ActivityKind::Blocked => AgentActivityState::Blocked,
        activity => AgentActivityState::Active(activity),
    }
}

fn is_active(activity: AgentActivityState) -> bool {
    matches!(activity, AgentActivityState::Active(_))
}

fn elapsed_hours_precise(elapsed: Duration) -> f64 {
    let seconds = elapsed.num_seconds();
    let remainder = elapsed - Duration::seconds(seconds);
    seconds as f64 / 3_600.0
        + remainder.num_nanoseconds().unwrap_or_default() as f64 / 3_600_000_000_000.0
}

fn nap_elapsed_hours_precise(
    segment_start: DateTime<Utc>,
    segment_end: DateTime<Utc>,
    napping_until: DateTime<Utc>,
) -> f64 {
    if napping_until <= segment_start {
        return 0.0;
    }
    elapsed_hours_precise(segment_end.min(napping_until) - segment_start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::pet::{FoodInventory, PetSpecies};
    use chrono::TimeZone;
    use serde_json::{Value, json};

    fn start() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
    }

    fn event(
        id: u128,
        session_id: u128,
        kind: AgentEventKind,
        activity: Option<ActivityKind>,
        timestamp: DateTime<Utc>,
    ) -> AgentEvent {
        AgentEvent {
            id: Uuid::from_u128(id),
            schema_version: 1,
            session_id: Uuid::from_u128(session_id),
            repository_id: "repo".to_owned(),
            source: crate::event::EventSource::Codex,
            kind,
            activity,
            timestamp,
            metadata: crate::event::EventMetadata::default(),
        }
    }

    fn simulation() -> PetSimulation<FakeClock, DefaultNeedProgressionStrategy> {
        let clock = FakeClock::new(start());
        let pet = Pet::new(
            Uuid::from_u128(9),
            "Mochi",
            crate::pet::PetSpecies::Cat,
            start(),
        );
        PetSimulation::new(pet, clock, DefaultNeedProgressionStrategy)
    }

    fn simulation_with_demands(
        demands: impl IntoIterator<Item = (u128, PetDemandKind)>,
    ) -> PetSimulation<FakeClock, DefaultNeedProgressionStrategy> {
        let mut pet = Pet::with_inventory(
            Uuid::from_u128(9),
            "Mochi",
            PetSpecies::Cat,
            start(),
            FoodInventory::starter(),
        );
        for (id, kind) in demands {
            pet.push_demand(PetDemand::new(Uuid::from_u128(id), kind, start()));
        }
        PetSimulation::new(pet, FakeClock::new(start()), DefaultNeedProgressionStrategy)
    }

    fn demand_ids(
        simulation: &PetSimulation<FakeClock, DefaultNeedProgressionStrategy>,
    ) -> Vec<Uuid> {
        simulation
            .pet()
            .pending_demands()
            .iter()
            .map(PetDemand::id)
            .collect()
    }

    #[test]
    fn petting_resolves_only_oldest_affection_demand() {
        let mut simulation = simulation_with_demands([
            (1, PetDemandKind::Affection),
            (2, PetDemandKind::Snack),
            (3, PetDemandKind::Affection),
        ]);

        simulation
            .apply_care(&CareCommand::Pet {
                action_id: Uuid::from_u128(99),
                interaction_ms: 1_500,
                pointer_distance: 120.0,
            })
            .unwrap();

        assert_eq!(
            demand_ids(&simulation),
            vec![Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }

    #[test]
    fn food_resolves_only_oldest_snack_demand_after_consumption() {
        for (food_id, action_id) in [("kibble", 100), ("treat", 101), ("fruit", 102)] {
            let mut simulation = simulation_with_demands([
                (1, PetDemandKind::Affection),
                (2, PetDemandKind::Snack),
                (3, PetDemandKind::Snack),
            ]);

            simulation
                .apply_care(&CareCommand::Feed {
                    action_id: Uuid::from_u128(action_id),
                    food_id: food_id.to_owned(),
                })
                .unwrap();

            assert_eq!(
                demand_ids(&simulation),
                vec![Uuid::from_u128(1), Uuid::from_u128(3)],
                "food={food_id}"
            );
        }
    }

    #[test]
    fn energy_drink_does_not_resolve_snack_demand() {
        let mut simulation =
            simulation_with_demands([(1, PetDemandKind::Affection), (2, PetDemandKind::Snack)]);

        simulation
            .apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(103),
                food_id: "energy_drink".to_owned(),
            })
            .unwrap();

        assert_eq!(
            demand_ids(&simulation),
            vec![Uuid::from_u128(1), Uuid::from_u128(2)]
        );
    }

    #[test]
    fn out_of_stock_food_does_not_resolve_snack_demand() {
        let mut pet = Pet::new(Uuid::from_u128(9), "Mochi", PetSpecies::Cat, start());
        pet.push_demand(PetDemand::new(
            Uuid::from_u128(1),
            PetDemandKind::Snack,
            start(),
        ));
        let mut simulation =
            PetSimulation::new(pet, FakeClock::new(start()), DefaultNeedProgressionStrategy);

        assert_eq!(
            simulation.apply_care(&CareCommand::Feed {
                action_id: Uuid::from_u128(104),
                food_id: "kibble".to_owned(),
            }),
            Err(CareError::OutOfStock("kibble".to_owned()))
        );
        assert_eq!(demand_ids(&simulation), vec![Uuid::from_u128(1)]);
    }

    fn pet_with_activity(activity: AgentActivityState) -> Pet {
        let mut pet = Pet::new(Uuid::from_u128(9), "Mochi", PetSpecies::Cat, start());
        pet.set_activity(activity);
        pet
    }

    #[test]
    fn healthy_pet_uses_wall_clock_baseline_in_every_activity_state() {
        for activity in [
            AgentActivityState::Idle,
            AgentActivityState::WaitingForUser,
            AgentActivityState::Blocked,
            AgentActivityState::Active(ActivityKind::Editing),
        ] {
            let mut pet = pet_with_activity(activity);
            DefaultNeedProgressionStrategy.progress(&mut pet, Duration::hours(1), activity);
            assert_eq!(pet.needs().hunger(), 25.0, "activity={activity:?}");
            assert_eq!(pet.needs().energy(), 50.0, "activity={activity:?}");
        }
    }

    #[test]
    fn nap_recovery_applies_only_to_the_nap_covered_slice() {
        let mut pet = pet_with_activity(AgentActivityState::Idle);
        pet.needs_mut().set_energy(50.0);
        pet.start_nap(start() + Duration::seconds(2));
        let elapsed = pet.advance_to(start() + Duration::seconds(5));

        DefaultNeedProgressionStrategy.progress(&mut pet, elapsed, AgentActivityState::Idle);

        // Three seconds decay at 50/hour, then two seconds recover at 20/second.
        let expected = 50.0 - 3.0 * (50.0 / 3_600.0) + 2.0 * 20.0;
        assert!((pet.needs().energy() - expected).abs() < 0.0001);
        assert_eq!(pet.needs().hunger(), 25.0 * (5.0 / 3_600.0));
    }

    #[test]
    fn stacked_affection_pressure_is_four_points_per_minute_in_waiting_state() {
        let at = start();
        let mut pet = pet_with_activity(AgentActivityState::WaitingForUser);
        pet.push_demand(PetDemand::new(
            Uuid::from_u128(1),
            PetDemandKind::Affection,
            at,
        ));
        pet.push_demand(PetDemand::new(
            Uuid::from_u128(2),
            PetDemandKind::Affection,
            at,
        ));
        let elapsed = pet.advance_to(at + Duration::minutes(5));

        DefaultNeedProgressionStrategy.progress(
            &mut pet,
            elapsed,
            AgentActivityState::WaitingForUser,
        );

        assert_eq!(pet.needs().happiness(), 60.0);
    }

    #[test]
    fn stacked_snack_pressure_increases_hunger_beyond_the_wall_clock_baseline() {
        let at = start();
        let mut pet = pet_with_activity(AgentActivityState::WaitingForUser);
        pet.push_demand(PetDemand::new(Uuid::from_u128(3), PetDemandKind::Snack, at));
        pet.push_demand(PetDemand::new(Uuid::from_u128(4), PetDemandKind::Snack, at));
        let elapsed = pet.advance_to(at + Duration::minutes(5));

        DefaultNeedProgressionStrategy.progress(
            &mut pet,
            elapsed,
            AgentActivityState::WaitingForUser,
        );

        let expected = (25.0 + 2.0 * 240.0) * (5.0 / 60.0);
        assert!((pet.needs().hunger() - expected).abs() < 0.0001);
    }

    #[test]
    fn stacked_poop_pressure_is_four_points_per_minute() {
        let at = start();
        let mut pet = pet_with_activity(AgentActivityState::WaitingForUser);
        pet.push_poop(Poop::new(Uuid::from_u128(5), at));
        pet.push_poop(Poop::new(Uuid::from_u128(6), at));
        let elapsed = pet.advance_to(at + Duration::minutes(5));

        DefaultNeedProgressionStrategy.progress(
            &mut pet,
            elapsed,
            AgentActivityState::WaitingForUser,
        );

        assert_eq!(pet.needs().cleanliness(), 60.0);
    }

    #[test]
    fn new_simulation_persists_an_absolute_first_incident_deadline() {
        let simulation = simulation();
        let snapshot = simulation.snapshot();

        assert!(snapshot.pending_demands.is_empty());
        assert_eq!(snapshot.attention_sequence, 0);
        assert_eq!(
            snapshot.next_incident_at,
            Some(start() + Duration::milliseconds(incident_delay_ms(Uuid::from_u128(9), 0) as i64))
        );
    }

    fn snapshot_with_deadline(deadline: DateTime<Utc>) -> SimulationSnapshot {
        let simulation = simulation();
        let mut snapshot = simulation.snapshot();
        snapshot.next_incident_at = Some(deadline);
        snapshot
    }

    fn raw_poop(id: Uuid, attention_sequence: u64) -> Value {
        json!({
            "id": id,
            "createdAt": start(),
            "attentionSequence": attention_sequence,
        })
    }

    fn legacy_raw_poop(id: Uuid, created_at: DateTime<Utc>) -> Value {
        json!({
            "id": id,
            "createdAt": created_at,
        })
    }

    fn snapshot_with_raw_poops(
        attention_sequence: u64,
        poop_sequence: u64,
        poops: Value,
    ) -> SimulationSnapshot {
        let mut encoded = serde_json::to_value(simulation().snapshot()).unwrap();
        let object = encoded.as_object_mut().expect("snapshot object");
        object.insert("attentionSequence".to_owned(), json!(attention_sequence));
        object.insert("poopSequence".to_owned(), json!(poop_sequence));
        object.insert("pendingPoops".to_owned(), poops);
        serde_json::from_value(encoded).unwrap()
    }

    fn snapshot_with_raw_poop(attention_sequence: u64, poop: Value) -> SimulationSnapshot {
        snapshot_with_raw_poops(attention_sequence, 1, json!([poop]))
    }

    fn restore_raw_poop(
        snapshot: SimulationSnapshot,
    ) -> Result<PetSimulation<FakeClock, DefaultNeedProgressionStrategy>, SnapshotRestoreError>
    {
        PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
    }

    #[test]
    fn restore_rejects_attention_poop_with_invalid_kind_or_id_provenance() {
        let pet_id = Uuid::from_u128(9);
        let kind_mismatch = snapshot_with_raw_poop(
            1,
            raw_poop(incident_id(pet_id, 0, AttentionIncidentKind::Affection), 0),
        );
        assert!(matches!(
            restore_raw_poop(kind_mismatch),
            Err(SnapshotRestoreError::InvariantViolation(_))
        ));

        let poop_sequence = (0..3)
            .find(|sequence| incident_kind(pet_id, *sequence) == AttentionIncidentKind::Poop)
            .expect("one of the first three incidents is poop");
        let id_mismatch = snapshot_with_raw_poop(
            poop_sequence + 1,
            raw_poop(Uuid::from_u128(123), poop_sequence),
        );
        assert!(matches!(
            restore_raw_poop(id_mismatch),
            Err(SnapshotRestoreError::InvariantViolation(_))
        ));
    }

    #[test]
    fn restore_rejects_extreme_attention_sequence_without_historical_scan() {
        let snapshot = snapshot_with_raw_poop(u64::MAX, raw_poop(Uuid::from_u128(123), 0));
        assert!(matches!(
            restore_raw_poop(snapshot),
            Err(SnapshotRestoreError::InvariantViolation(_))
        ));
    }

    #[test]
    fn legacy_attention_poop_restores_reanchors_and_reserializes_without_provenance() {
        let pet_id = Uuid::from_u128(9);
        let attention_sequence = (0..3)
            .find(|sequence| incident_kind(pet_id, *sequence) == AttentionIncidentKind::Poop)
            .expect("one of the first three incidents is poop");
        let ahead = Duration::days(8);
        let mut encoded = serde_json::to_value(simulation().snapshot()).unwrap();
        let object = encoded.as_object_mut().expect("snapshot object");
        object.insert("lastUpdatedAt".to_owned(), json!(start() + ahead));
        object.insert("behavior".to_owned(), json!("Sleeping"));
        object.insert(
            "attentionSequence".to_owned(),
            json!(attention_sequence + 1),
        );
        object.insert("poopSequence".to_owned(), json!(0));
        object.insert(
            "pendingPoops".to_owned(),
            json!([legacy_raw_poop(
                incident_id(pet_id, attention_sequence, AttentionIncidentKind::Poop),
                start() + ahead,
            )]),
        );

        let legacy: SimulationSnapshot = serde_json::from_value(encoded).unwrap();
        assert_eq!(legacy.pending_poops[0].attention_sequence(), None);
        let restored = PetSimulation::from_snapshot(
            legacy,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        assert_eq!(restored.pet().pending_poops()[0].created_at(), start());

        let serialized = serde_json::to_value(restored.snapshot()).unwrap();
        assert!(
            !serialized["pendingPoops"][0]
                .as_object()
                .expect("poop object")
                .contains_key("attentionSequence")
        );
        let restored_again = PetSimulation::from_snapshot(
            serde_json::from_value(serialized).unwrap(),
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        assert_eq!(
            restored_again.pet().pending_poops()[0].created_at(),
            start()
        );
    }

    #[test]
    fn mixed_legacy_and_provenanced_attention_poops_restore_without_downgrading_provenance() {
        let pet_id = Uuid::from_u128(9);
        let poop_sequences = (0..6)
            .filter(|sequence| incident_kind(pet_id, *sequence) == AttentionIncidentKind::Poop)
            .take(2)
            .collect::<Vec<_>>();
        let legacy_sequence = poop_sequences[0];
        let provenanced_sequence = poop_sequences[1];
        let attention_sequence = provenanced_sequence + 1;
        let snapshot = snapshot_with_raw_poops(
            attention_sequence,
            0,
            json!([
                legacy_raw_poop(
                    incident_id(pet_id, legacy_sequence, AttentionIncidentKind::Poop),
                    start(),
                ),
                raw_poop(
                    incident_id(pet_id, provenanced_sequence, AttentionIncidentKind::Poop),
                    provenanced_sequence,
                ),
            ]),
        );

        let restored = restore_raw_poop(snapshot).unwrap();
        assert_eq!(restored.pet().pending_poops()[0].attention_sequence(), None);
        assert_eq!(
            restored.pet().pending_poops()[1].attention_sequence(),
            Some(provenanced_sequence)
        );
    }

    #[test]
    fn incident_creation_happens_only_after_crossing_the_due_timestamp() {
        let snapshot = snapshot_with_deadline(start() + Duration::minutes(3));
        let mut simulation = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();

        let before_due =
            simulation.current_state_at(start() + Duration::minutes(3) - Duration::milliseconds(1));
        assert!(before_due.pending_demands.is_empty());
        assert!(before_due.pending_poops.is_empty());
        assert_eq!(before_due.attention_sequence, 0);

        let after_due =
            simulation.current_state_at(start() + Duration::minutes(3) + Duration::milliseconds(1));
        assert_eq!(after_due.attention_sequence, 1);
        match incident_kind(Uuid::from_u128(9), 0) {
            AttentionIncidentKind::Affection | AttentionIncidentKind::Snack => {
                assert_eq!(after_due.pending_demands.len(), 1);
                assert!(after_due.pending_poops.is_empty());
                assert_eq!(
                    after_due.pending_demands[0].created_at(),
                    start() + Duration::minutes(3)
                );
            }
            AttentionIncidentKind::Poop => {
                assert!(after_due.pending_demands.is_empty());
                assert_eq!(after_due.pending_poops.len(), 1);
                assert_eq!(
                    after_due.pending_poops[0].created_at(),
                    start() + Duration::minutes(3)
                );
            }
        }
    }

    #[test]
    fn rollback_or_equal_target_does_not_consume_an_overdue_incident_schedule() {
        let snapshot = snapshot_with_deadline(start() - Duration::minutes(1));
        let mut simulation = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        let before = simulation.snapshot();

        let equal = simulation.current_state_at(start());
        assert_eq!(equal, before);

        let rollback = simulation.current_state_at(start() - Duration::seconds(1));
        assert_eq!(rollback, before);
    }

    #[test]
    fn restored_exhausted_attention_sequence_is_rejected_explicitly() {
        let mut snapshot = simulation().snapshot();
        snapshot.attention_sequence = u64::MAX;

        let error = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .err()
        .expect("an exhausted attention sequence cannot be restored");

        assert!(matches!(
            error,
            SnapshotRestoreError::InvariantViolation(message)
                if message == "attention sequence exhausted"
        ));
    }

    #[test]
    fn final_attention_incident_enters_a_terminal_schedule_without_repeating() {
        let mut snapshot = simulation().snapshot();
        snapshot.attention_sequence = u64::MAX - 1;
        snapshot.next_incident_at = Some(start());
        let mut simulation = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();

        let final_state = simulation.current_state_at(start() + Duration::seconds(1));
        assert_eq!(final_state.attention_sequence, u64::MAX - 1);
        assert_eq!(final_state.next_incident_at, Some(DateTime::<Utc>::MAX_UTC));
        assert_eq!(
            final_state.pending_demands.len() + final_state.pending_poops.len(),
            1
        );

        let encoded = serde_json::to_string(&final_state).unwrap();
        let restored_final_state: SimulationSnapshot = serde_json::from_str(&encoded).unwrap();
        let mut resumed = PetSimulation::from_snapshot(
            restored_final_state,
            FakeClock::new(start() + Duration::seconds(1)),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        let after_resume = resumed.current_state_at(start() + Duration::seconds(2));
        assert_eq!(
            after_resume.attention_sequence,
            final_state.attention_sequence
        );
        assert_eq!(after_resume.next_incident_at, final_state.next_incident_at);
        assert_eq!(after_resume.pending_demands, final_state.pending_demands);
        assert_eq!(after_resume.pending_poops, final_state.pending_poops);
    }

    #[test]
    fn serialized_restart_keeps_partitioned_need_progression_within_float_bound() {
        let snapshot = snapshot_with_deadline(start() + Duration::minutes(3));
        let split = start() + Duration::minutes(7);
        let target = start() + Duration::minutes(14);
        let mut uninterrupted = PetSimulation::from_snapshot(
            snapshot.clone(),
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        let expected = uninterrupted.current_state_at(target);

        let mut partitioned = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        partitioned.current_state_at(split);
        let encoded = serde_json::to_string(&partitioned.snapshot()).unwrap();
        let restored_snapshot: SimulationSnapshot = serde_json::from_str(&encoded).unwrap();
        let mut resumed = PetSimulation::from_snapshot(
            restored_snapshot,
            FakeClock::new(split),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        let actual = resumed.current_state_at(target);

        assert_eq!(actual.attention_sequence, expected.attention_sequence);
        assert_eq!(actual.pending_demands, expected.pending_demands);
        assert_eq!(actual.pending_poops, expected.pending_poops);
        assert_eq!(actual.next_incident_at, expected.next_incident_at);
        let maximum_drift = [
            (actual.needs.hunger() - expected.needs.hunger()).abs(),
            (actual.needs.energy() - expected.needs.energy()).abs(),
            (actual.needs.happiness() - expected.needs.happiness()).abs(),
            (actual.needs.cleanliness() - expected.needs.cleanliness()).abs(),
        ]
        .into_iter()
        .fold(0.0_f32, f32::max);
        // PetNeeds restores its skipped fixed-point accumulator from the
        // visible f32 values. Across this representative at-most-five-incident
        // window, the resulting restart drift stays below one f32 ULP at
        // the 0..=100 need range (7.63e-6), with a small 1e-5 safety bound.
        assert!(maximum_drift <= 0.00001, "restart drift: {maximum_drift}");
    }

    #[test]
    fn equivalent_wall_clock_advancement_is_deterministic_with_at_most_five_incidents() {
        let snapshot = snapshot_with_deadline(start() + Duration::minutes(3));
        let target = start() + Duration::minutes(14);
        let mut jumped = PetSimulation::from_snapshot(
            snapshot.clone(),
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        let mut stepped = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();

        jumped.current_state_at(target);
        let mut timestamp = start();
        while timestamp < target {
            timestamp += Duration::seconds(1);
            stepped.current_state_at(timestamp.min(target));
        }

        assert!(jumped.snapshot().attention_sequence <= MAX_CATCH_UP_INCIDENTS as u64);
        assert_eq!(jumped.snapshot(), stepped.snapshot());
    }

    #[test]
    fn long_gap_creates_five_incidents_and_reanchors_after_the_target() {
        let target = start() + Duration::hours(24);
        let snapshot = snapshot_with_deadline(start() + Duration::minutes(3));
        let mut simulation = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();

        let state = simulation.current_state_at(target);

        assert_eq!(state.attention_sequence, MAX_CATCH_UP_INCIDENTS as u64);
        assert_eq!(state.pending_demands.len() + state.pending_poops.len(), 5);
        assert_eq!(state.needs.hunger(), 100.0);
        assert_eq!(state.needs.energy(), 0.0);
        assert_eq!(state.needs.happiness(), 0.0);
        assert_eq!(state.needs.cleanliness(), 0.0);
        let next = state.next_incident_at.unwrap();
        assert!(next > target);
        assert!(next <= target + Duration::minutes(5));

        let after_one_second = simulation.current_state_at(target + Duration::seconds(1));
        assert_eq!(after_one_second.attention_sequence, 5);
        assert_eq!(
            after_one_second.pending_demands.len() + after_one_second.pending_poops.len(),
            5
        );
    }

    #[test]
    fn pending_demands_round_trip_through_simulation_snapshots() {
        let mut original = simulation();
        let demand = PetDemand::new(
            Uuid::from_u128(77),
            PetDemandKind::Affection,
            start() - Duration::minutes(2),
        );
        original.pet.push_demand(demand.clone());

        let restored = PetSimulation::from_snapshot(
            original.snapshot(),
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();

        assert_eq!(restored.pet().pending_demands(), &[demand]);
    }

    #[test]
    fn future_reanchoring_shifts_pending_demand_and_incident_deadline_timestamps() {
        let mut simulation = simulation();
        simulation.pet.push_demand(PetDemand::new(
            Uuid::from_u128(78),
            PetDemandKind::Snack,
            start(),
        ));
        let mut snapshot = simulation.snapshot();
        let ahead = Duration::days(8);
        snapshot.last_updated_at += ahead;
        snapshot.behavior = PetBehavior::Sleeping;
        snapshot.next_incident_at = snapshot.next_incident_at.map(|at| at + ahead);
        snapshot.pending_demands[0] =
            PetDemand::new(Uuid::from_u128(78), PetDemandKind::Snack, start() + ahead);

        let restored = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        let resumed = restored.snapshot();

        assert_eq!(resumed.last_updated_at, start());
        assert_eq!(resumed.pending_demands[0].created_at(), start());
        assert_eq!(
            resumed.next_incident_at,
            Some(start() + Duration::milliseconds(incident_delay_ms(Uuid::from_u128(9), 0) as i64))
        );
    }

    #[test]
    fn duplicate_pending_demand_ids_are_rejected_during_restore() {
        let simulation = simulation();
        let mut snapshot = simulation.snapshot();
        let duplicate = PetDemand::new(Uuid::from_u128(79), PetDemandKind::Affection, start());
        snapshot.pending_demands = vec![duplicate.clone(), duplicate];

        let error = PetSimulation::from_snapshot(
            snapshot,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .err()
        .expect("duplicate demand IDs must be rejected");
        assert!(matches!(
            error,
            SnapshotRestoreError::InvariantViolation(message)
                if message == "pending demand ids must be unique"
        ));
    }

    #[test]
    fn every_deterministic_schedule_reaches_strict_severe_neglect_within_thirty_minutes() {
        use crate::permission::{
            CommandCategory, CommandClassification, CommandPurpose, WorkPermissionPolicy,
        };

        for pet_id in 1..=100u128 {
            let pet = Pet::new(Uuid::from_u128(pet_id), "Mochi", PetSpecies::Cat, start());
            let mut simulation =
                PetSimulation::new(pet, FakeClock::new(start()), DefaultNeedProgressionStrategy);
            simulation.set_enforcement_mode(EnforcementMode::Strict);
            simulation.current_state_at(start() + Duration::minutes(30));

            let decision = WorkPermissionPolicy::evaluate(
                simulation.pet(),
                &CommandClassification::new(
                    CommandCategory::Development,
                    CommandPurpose::Uncertain,
                ),
                &PetSettings::new(EnforcementMode::Strict),
            );
            assert!(decision.is_blocked(), "pet_id={pet_id}");
        }
    }

    #[test]
    fn legacy_snapshots_without_attention_fields_schedule_from_restore_wall_clock() {
        let snapshot = simulation().snapshot();
        let mut encoded = serde_json::to_value(snapshot).unwrap();
        let object = encoded.as_object_mut().expect("snapshot object");
        object.remove("pendingDemands");
        object.remove("attentionSequence");
        object.remove("nextIncidentAt");

        let legacy: SimulationSnapshot = serde_json::from_value(encoded).unwrap();
        assert!(legacy.pending_demands.is_empty());
        assert_eq!(legacy.attention_sequence, 0);
        assert_eq!(legacy.next_incident_at, None);

        let restored = PetSimulation::from_snapshot(
            legacy,
            FakeClock::new(start()),
            DefaultNeedProgressionStrategy,
        )
        .unwrap();
        assert_eq!(
            restored.snapshot().next_incident_at,
            Some(start() + Duration::milliseconds(incident_delay_ms(Uuid::from_u128(9), 0) as i64))
        );
    }

    #[test]
    fn debug_restock_restores_the_starter_inventory_without_touching_needs() {
        let mut pantry = FoodInventory::default();
        pantry.add(FoodKind::Kibble, 3);
        pantry.add(FoodKind::Treat, 1);
        let pet = Pet::with_inventory(
            Uuid::from_u128(9),
            "Mochi",
            PetSpecies::Cat,
            start(),
            pantry,
        );
        let mut simulation =
            PetSimulation::new(pet, FakeClock::new(start()), DefaultNeedProgressionStrategy);
        simulation.pet.needs_mut().set_hunger(42.0);

        simulation.apply_debug_restock();

        assert_eq!(simulation.pet.inventory(), &FoodInventory::starter());
        assert_eq!(simulation.pet.needs().hunger(), 42.0);
        assert_eq!(simulation.pet.behavior(), PetBehavior::Wandering);
    }

    #[test]
    fn completion_events_do_not_add_work_points() {
        let mut simulation = simulation();
        simulation
            .apply_event(&event(1, 1, AgentEventKind::SessionStarted, None, start()))
            .unwrap();
        let mut completion = event(
            2,
            1,
            AgentEventKind::CommandCompleted,
            Some(ActivityKind::Testing),
            start(),
        );
        completion.metadata.exit_status = Some(0);
        simulation.apply_event(&completion).unwrap();
        assert_eq!(simulation.pet.work_points(), 0);
    }

    #[test]
    fn completion_statuses_have_exact_outcome_effects() {
        let mut simulation = simulation();
        simulation.pet.needs_mut().set_happiness(50.0);

        let mut unknown_testing = event(
            1,
            1,
            AgentEventKind::CommandCompleted,
            Some(ActivityKind::Testing),
            start(),
        );
        unknown_testing.metadata.exit_status = None;
        simulation.apply_event(&unknown_testing).unwrap();

        let mut unknown_building = event(
            2,
            1,
            AgentEventKind::ToolCompleted,
            Some(ActivityKind::Building),
            start() + Duration::seconds(1),
        );
        unknown_building.metadata.exit_status = None;
        simulation.apply_event(&unknown_building).unwrap();
        assert_eq!(simulation.pet.needs().happiness(), 50.0);
        assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::None);
        assert_eq!(simulation.last_outcome_at, None);
        assert_eq!(simulation.consecutive_failures, 0);

        let mut success = event(
            3,
            1,
            AgentEventKind::CommandCompleted,
            Some(ActivityKind::Testing),
            start() + Duration::seconds(2),
        );
        success.metadata.exit_status = Some(0);
        simulation.apply_event(&success).unwrap();
        assert_eq!(simulation.pet.needs().happiness(), 58.0);
        assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::Success);
        assert_eq!(
            simulation.last_outcome_at,
            Some(start() + Duration::seconds(2))
        );
        assert_eq!(simulation.consecutive_failures, 0);

        let mut failure = event(
            4,
            1,
            AgentEventKind::ToolCompleted,
            Some(ActivityKind::Building),
            start() + Duration::seconds(3),
        );
        failure.metadata.exit_status = Some(7);
        simulation.apply_event(&failure).unwrap();
        assert_eq!(simulation.pet.needs().happiness(), 54.0);
        assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::Failure);
        assert_eq!(
            simulation.last_outcome_at,
            Some(start() + Duration::seconds(3))
        );
        assert_eq!(simulation.consecutive_failures, 1);

        let mut blocked = event(
            5,
            1,
            AgentEventKind::CommandCompleted,
            Some(ActivityKind::Testing),
            start() + Duration::seconds(4),
        );
        blocked.metadata.exit_status = Some(0);
        blocked.metadata.blocked = true;
        simulation.apply_event(&blocked).unwrap();
        assert_eq!(simulation.pet.needs().happiness(), 54.0);
        assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::Failure);
        assert_eq!(
            simulation.last_outcome_at,
            Some(start() + Duration::seconds(3))
        );
        assert_eq!(simulation.consecutive_failures, 1);

        let mut blocked_activity = event(
            6,
            1,
            AgentEventKind::CommandCompleted,
            Some(ActivityKind::Blocked),
            start() + Duration::seconds(5),
        );
        blocked_activity.metadata.exit_status = Some(0);
        simulation.apply_event(&blocked_activity).unwrap();
        assert_eq!(simulation.pet.needs().happiness(), 54.0);
        assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::Failure);
        assert_eq!(
            simulation.last_outcome_at,
            Some(start() + Duration::seconds(3))
        );
        assert_eq!(simulation.consecutive_failures, 1);
    }

    #[test]
    fn failure_penalties_saturate_and_success_resets_after_a_multi_failure_streak() {
        let cases = [
            (ActivityKind::Testing, [4.0, 8.0, 12.0, 12.0]),
            (ActivityKind::Building, [4.0, 8.0, 12.0, 12.0]),
        ];

        for (activity, penalties) in cases {
            let mut simulation = simulation();
            simulation.pet.needs_mut().set_happiness(60.0);

            for (index, expected_penalty) in penalties.into_iter().enumerate() {
                let mut failure = event(
                    (index + 1) as u128,
                    1,
                    AgentEventKind::CommandCompleted,
                    Some(activity),
                    start() + Duration::seconds((index + 1) as i64),
                );
                failure.metadata.exit_status = Some(1);
                let before = simulation.pet.needs().happiness();
                simulation.apply_event(&failure).unwrap();

                assert_eq!(
                    before - simulation.pet.needs().happiness(),
                    expected_penalty
                );
                assert_eq!(simulation.consecutive_failures, (index + 1) as u32);
                assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::Failure);
            }

            let mut success = event(
                5,
                1,
                AgentEventKind::CommandCompleted,
                Some(activity),
                start() + Duration::seconds(5),
            );
            success.metadata.exit_status = Some(0);
            let before_success = simulation.pet.needs().happiness();
            simulation.apply_event(&success).unwrap();

            assert_eq!(simulation.pet.needs().happiness() - before_success, 8.0);
            assert_eq!(simulation.consecutive_failures, 0);
            assert_eq!(simulation.pet.recent_outcome(), AgentOutcome::Success);
        }
    }

    #[test]
    fn behavior_boundaries_and_priority_are_stored_on_pet() {
        let mut simulation = simulation();
        simulation.pet.needs_mut().set_hunger(89.999);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Wandering);
        simulation.pet.needs_mut().set_hunger(90.0);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::CriticalNeed);

        simulation.pet.needs_mut().set_hunger(0.0);
        simulation.pet.needs_mut().set_cleanliness(10.001);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Wandering);
        simulation.pet.needs_mut().set_cleanliness(10.0);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::CriticalNeed);

        simulation.pet.needs_mut().set_cleanliness(100.0);
        simulation.pet.needs_mut().set_energy(10.001);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Wandering);
        simulation.pet.needs_mut().set_energy(10.0);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::CriticalNeed);
        simulation.pet.needs_mut().set_energy(100.0);
        simulation.pet.set_activity(AgentActivityState::Blocked);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Blocked);
        simulation
            .pet
            .set_activity(AgentActivityState::Active(ActivityKind::Editing));
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Working);
    }

    #[test]
    fn behavior_has_exact_recent_and_sleep_boundaries_and_combined_priority() {
        let mut simulation = simulation();
        simulation.pet.needs_mut().set_hunger(0.0);
        simulation.pet.needs_mut().set_cleanliness(100.0);
        simulation.pet.set_activity(AgentActivityState::Idle);
        simulation.pet.set_outcome(AgentOutcome::Success);
        simulation.last_activity_at = Some(start());
        simulation.last_outcome_at = Some(start());

        simulation.refresh_behavior(start() + Duration::minutes(5));
        assert_eq!(simulation.pet.behavior(), PetBehavior::RecentSuccess);
        simulation.refresh_behavior(start() + Duration::minutes(5) + Duration::milliseconds(1));
        assert_eq!(simulation.pet.behavior(), PetBehavior::Wandering);

        simulation.pet.set_outcome(AgentOutcome::Failure);
        simulation.last_outcome_at = Some(start());
        simulation.refresh_behavior(start() + Duration::minutes(5));
        assert_eq!(simulation.pet.behavior(), PetBehavior::RecentFailure);

        simulation.pet.set_outcome(AgentOutcome::None);
        simulation.last_outcome_at = None;
        simulation.refresh_behavior(start() + Duration::minutes(29) + Duration::seconds(59));
        assert_eq!(simulation.pet.behavior(), PetBehavior::Wandering);
        simulation.refresh_behavior(start() + Duration::minutes(30));
        assert_eq!(simulation.pet.behavior(), PetBehavior::Sleeping);

        simulation.pet.needs_mut().set_hunger(90.0);
        simulation.pet.set_activity(AgentActivityState::Blocked);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::CriticalNeed);

        simulation.pet.needs_mut().set_hunger(89.999);
        simulation.pet.set_activity(AgentActivityState::Blocked);
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Blocked);

        simulation
            .pet
            .set_activity(AgentActivityState::Active(ActivityKind::Testing));
        simulation.refresh_behavior(start());
        assert_eq!(simulation.pet.behavior(), PetBehavior::Working);
    }
}

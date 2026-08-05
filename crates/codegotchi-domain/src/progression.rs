use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    behavior::BehaviorCoordinator,
    care::{CareCommand, CareError, CareResult},
    clock::Clock,
    event::{ActivityKind, AgentEvent, AgentEventError, AgentEventKind},
    pet::{AgentActivityState, AgentOutcome, FoodKind, Pet, PetBehavior, PetNeeds, Poop},
    poop::{DefaultPoopGenerationStrategy, PoopGenerationStrategy},
};

const SECONDS_PER_HOUR: f32 = 3_600.0;
const ACTIVE_HUNGER_PER_HOUR: f32 = 4.0;
const ACTIVE_ENERGY_PER_HOUR: f32 = -6.0;
const IDLE_HUNGER_PER_HOUR: f32 = 1.0;
const IDLE_ENERGY_PER_HOUR: f32 = 8.0;
const POOP_CLEANLINESS_PER_HOUR: f32 = -2.0;

/// Applies elapsed need changes without consulting wall-clock state.
pub trait NeedProgressionStrategy {
    fn progress(&self, pet: &mut Pet, elapsed: Duration, previous_activity: AgentActivityState);
}

/// The deterministic linear progression required by the domain plan.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultNeedProgressionStrategy;

impl NeedProgressionStrategy for DefaultNeedProgressionStrategy {
    fn progress(&self, pet: &mut Pet, elapsed: Duration, previous_activity: AgentActivityState) {
        let elapsed_hours = elapsed_hours(elapsed);
        if elapsed_hours <= 0.0 {
            return;
        }

        let (hunger_rate, energy_rate) = match previous_activity {
            AgentActivityState::Active(_) => (ACTIVE_HUNGER_PER_HOUR, ACTIVE_ENERGY_PER_HOUR),
            AgentActivityState::Idle
            | AgentActivityState::WaitingForUser
            | AgentActivityState::Blocked => (IDLE_HUNGER_PER_HOUR, IDLE_ENERGY_PER_HOUR),
        };

        let pending_poop_count = pet.pending_poops().len() as f32;
        let needs = pet.needs_mut();
        needs.adjust_hunger(hunger_rate * elapsed_hours);
        needs.adjust_energy(energy_rate * elapsed_hours);
        needs.adjust_cleanliness(POOP_CLEANLINESS_PER_HOUR * elapsed_hours * pending_poop_count);
    }
}

/// The activity and logical update time of one registered agent session.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// A deterministic in-memory test/read model, not a persistence DTO.
#[derive(Clone, Debug, PartialEq)]
pub struct SimulationSnapshot {
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
    pub inventory: crate::pet::FoodInventory,
    pub processed_care_ids: BTreeSet<Uuid>,
    pub poop_sequence: u64,
    pub session_activities: BTreeMap<Uuid, SessionActivity>,
    pub processed_event_ids: BTreeSet<Uuid>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub last_outcome_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
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
}

impl<C, N> PetSimulation<C, N, DefaultPoopGenerationStrategy>
where
    C: Clock,
    N: NeedProgressionStrategy,
{
    pub fn new(pet: Pet, clock: C, progression: N) -> Self {
        Self::with_poop_strategy(pet, clock, progression, DefaultPoopGenerationStrategy)
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
        self.advance_time();
        self.snapshot()
    }

    /// Returns the stored aggregate state without advancing time.
    pub fn snapshot(&self) -> SimulationSnapshot {
        SimulationSnapshot {
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
            inventory: self.pet.inventory().clone(),
            processed_care_ids: self.processed_care_ids.clone(),
            poop_sequence: self.pet.poop_sequence(),
            session_activities: self.session_activities.clone(),
            processed_event_ids: self.processed_event_ids.clone(),
            last_activity_at: self.last_activity_at,
            last_outcome_at: self.last_outcome_at,
            consecutive_failures: self.consecutive_failures,
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
            }
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

    fn advance_elapsed_to(
        &mut self,
        target: DateTime<Utc>,
        previous_activity: AgentActivityState,
    ) -> Duration {
        let elapsed = self.pet.advance_to(target);
        if elapsed > Duration::zero() {
            self.progression
                .progress(&mut self.pet, elapsed, previous_activity);
        }
        elapsed
    }

    fn refresh_activity(&mut self) {
        let aggregate = self.aggregate_activity();
        self.pet.set_activity(aggregate);
    }

    fn aggregate_activity(&self) -> AgentActivityState {
        if self
            .session_activities
            .values()
            .any(|session| session.activity == AgentActivityState::Blocked)
        {
            return AgentActivityState::Blocked;
        }

        if let Some((_, session)) = self
            .session_activities
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

        if self
            .session_activities
            .values()
            .any(|session| session.activity == AgentActivityState::WaitingForUser)
        {
            return AgentActivityState::WaitingForUser;
        }

        AgentActivityState::Idle
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

fn completion_activity(event: &AgentEvent) -> Option<ActivityKind> {
    match event.kind {
        AgentEventKind::ToolCompleted | AgentEventKind::CommandCompleted => event.activity,
        _ => None,
    }
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

fn elapsed_hours(elapsed: Duration) -> f32 {
    let Ok(duration) = elapsed.to_std() else {
        return 0.0;
    };

    duration.as_secs_f32() / SECONDS_PER_HOUR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use chrono::TimeZone;

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

    #[test]
    fn fractional_active_progression_and_multiple_poops_are_linear() {
        let mut simulation = simulation();
        simulation
            .pet
            .pending_poops
            .push(Poop::new(Uuid::from_u128(1), start()));
        simulation
            .pet
            .pending_poops
            .push(Poop::new(Uuid::from_u128(2), start()));
        simulation
            .pet
            .set_activity(AgentActivityState::Active(ActivityKind::Editing));

        simulation.advance_elapsed_to(
            start() + Duration::minutes(30),
            AgentActivityState::Active(ActivityKind::Editing),
        );

        assert_eq!(simulation.pet.needs().hunger(), 2.0);
        assert_eq!(simulation.pet.needs().energy(), 97.0);
        assert_eq!(simulation.pet.needs().cleanliness(), 98.0);
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

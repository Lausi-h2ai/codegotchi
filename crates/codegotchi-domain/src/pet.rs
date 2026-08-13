use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::attention::PetDemand;
use crate::event::ActivityKind;

const NEED_MIN: f32 = 0.0;
const NEED_MAX: f32 = 100.0;
const NEED_SCALE: f64 = 1_000_000_000_000.0;
const NEED_SCALE_MAX: i64 = 100_000_000_000_000;

fn clamp_need(value: f32) -> f32 {
    if value.is_nan() || value == f32::NEG_INFINITY {
        NEED_MIN
    } else if value == f32::INFINITY {
        NEED_MAX
    } else {
        value.clamp(NEED_MIN, NEED_MAX)
    }
}

/// The four bounded needs owned by a pet.
///
/// Hunger is inverted relative to the other needs: zero means full and 100 means starving.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PetNeeds {
    hunger: f32,
    energy: f32,
    happiness: f32,
    cleanliness: f32,
    #[serde(skip)]
    exact: Option<[i64; 4]>,
}

impl PetNeeds {
    pub fn new(hunger: f32, energy: f32, happiness: f32, cleanliness: f32) -> Self {
        let mut needs = Self {
            hunger: clamp_need(hunger),
            energy: clamp_need(energy),
            happiness: clamp_need(happiness),
            cleanliness: clamp_need(cleanliness),
            exact: None,
        };
        needs.sync_exact_from_visible();
        needs
    }

    pub fn hunger(self) -> f32 {
        self.hunger
    }

    pub fn energy(self) -> f32 {
        self.energy
    }

    pub fn happiness(self) -> f32 {
        self.happiness
    }

    pub fn cleanliness(self) -> f32 {
        self.cleanliness
    }

    pub fn set_hunger(&mut self, value: f32) {
        self.set_exact(0, value);
    }

    pub fn set_energy(&mut self, value: f32) {
        self.set_exact(1, value);
    }

    pub fn set_happiness(&mut self, value: f32) {
        self.set_exact(2, value);
    }

    pub fn set_cleanliness(&mut self, value: f32) {
        self.set_exact(3, value);
    }

    pub fn adjust_hunger(&mut self, delta: f32) {
        self.adjust_exact(0, delta);
    }

    pub(crate) fn adjust_hunger_precise(&mut self, delta: f64) {
        self.adjust_exact_precise(0, delta);
    }

    pub fn adjust_energy(&mut self, delta: f32) {
        self.adjust_exact(1, delta);
    }

    pub(crate) fn adjust_energy_precise(&mut self, delta: f64) {
        self.adjust_exact_precise(1, delta);
    }

    pub fn adjust_happiness(&mut self, delta: f32) {
        self.adjust_exact(2, delta);
    }

    pub(crate) fn adjust_happiness_precise(&mut self, delta: f64) {
        self.adjust_exact_precise(2, delta);
    }

    pub fn adjust_cleanliness(&mut self, delta: f32) {
        self.adjust_exact(3, delta);
    }

    pub(crate) fn adjust_cleanliness_precise(&mut self, delta: f64) {
        self.adjust_exact_precise(3, delta);
    }

    fn sync_exact_from_visible(&mut self) {
        self.exact = Some([
            scale_need(self.hunger),
            scale_need(self.energy),
            scale_need(self.happiness),
            scale_need(self.cleanliness),
        ]);
    }

    fn set_exact(&mut self, index: usize, value: f32) {
        if self.exact.is_none() {
            self.sync_exact_from_visible();
        }
        let value = clamp_need(value);
        self.exact.as_mut().expect("exact needs initialized")[index] = scale_need(value);
        self.set_visible_from_exact(index);
    }

    fn adjust_exact(&mut self, index: usize, delta: f32) {
        self.adjust_exact_precise(index, delta as f64);
    }

    fn adjust_exact_precise(&mut self, index: usize, delta: f64) {
        if !delta.is_finite() {
            self.set_exact(index, self.visible(index) + delta as f32);
            return;
        }
        if self.exact.is_none() {
            self.sync_exact_from_visible();
        }
        let exact = self.exact.as_mut().expect("exact needs initialized");
        let scaled_delta = (delta * NEED_SCALE).round();
        let scaled_delta = if scaled_delta >= i64::MAX as f64 {
            i64::MAX
        } else if scaled_delta <= i64::MIN as f64 {
            i64::MIN
        } else {
            scaled_delta as i64
        };
        exact[index] = exact[index]
            .saturating_add(scaled_delta)
            .clamp(0, NEED_SCALE_MAX);
        self.set_visible_from_exact(index);
    }

    fn visible(&self, index: usize) -> f32 {
        match index {
            0 => self.hunger,
            1 => self.energy,
            2 => self.happiness,
            3 => self.cleanliness,
            _ => unreachable!("need index"),
        }
    }

    fn set_visible_from_exact(&mut self, index: usize) {
        let value =
            self.exact.as_ref().expect("exact needs initialized")[index] as f64 / NEED_SCALE;
        let value = value as f32;
        match index {
            0 => self.hunger = value,
            1 => self.energy = value,
            2 => self.happiness = value,
            3 => self.cleanliness = value,
            _ => unreachable!("need index"),
        }
    }

    pub(crate) fn is_valid(self) -> bool {
        [self.hunger, self.energy, self.happiness, self.cleanliness]
            .into_iter()
            .all(|value| value.is_finite() && (NEED_MIN..=NEED_MAX).contains(&value))
    }
}

impl PartialEq for PetNeeds {
    fn eq(&self, other: &Self) -> bool {
        self.hunger == other.hunger
            && self.energy == other.energy
            && self.happiness == other.happiness
            && self.cleanliness == other.cleanliness
    }
}

fn scale_need(value: f32) -> i64 {
    (value as f64 * NEED_SCALE).round() as i64
}

impl Default for PetNeeds {
    fn default() -> Self {
        Self::new(0.0, 100.0, 100.0, 100.0)
    }
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PetSpecies {
    #[default]
    Cat,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum PetBehavior {
    #[default]
    Wandering,
    Sleeping,
    Working,
    CriticalNeed,
    Blocked,
    RecentSuccess,
    RecentFailure,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentActivityState {
    #[default]
    Idle,
    WaitingForUser,
    Active(ActivityKind),
    Blocked,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentOutcome {
    #[default]
    None,
    Success,
    Failure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FoodKind {
    Kibble,
    Treat,
    Fruit,
    EnergyDrink,
}

impl FoodKind {
    pub fn from_id(food_id: &str) -> Option<Self> {
        match food_id {
            "kibble" => Some(Self::Kibble),
            "treat" => Some(Self::Treat),
            "fruit" => Some(Self::Fruit),
            "energy_drink" => Some(Self::EnergyDrink),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Kibble => "kibble",
            Self::Treat => "treat",
            Self::Fruit => "fruit",
            Self::EnergyDrink => "energy_drink",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FoodInventory {
    #[serde(flatten)]
    quantities: BTreeMap<FoodKind, u32>,
}

impl FoodInventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// The starter pantry every new demo pet receives: enough of each food to
    /// exercise the care loop without running dry during a session.
    pub fn starter() -> Self {
        let mut inventory = Self::default();
        inventory.add(FoodKind::Kibble, 50);
        inventory.add(FoodKind::Treat, 25);
        inventory.add(FoodKind::Fruit, 25);
        inventory.add(FoodKind::EnergyDrink, 10);
        inventory
    }

    /// Restores the exact starter quantities, so a debug restock is a fixed,
    /// deterministic transition rather than an unbounded top-up.
    pub fn restock_to_starter(&mut self) {
        *self = Self::starter();
    }

    pub fn count(&self, food: FoodKind) -> u32 {
        self.quantities.get(&food).copied().unwrap_or_default()
    }

    pub fn add(&mut self, food: FoodKind, amount: u32) {
        if amount == 0 {
            return;
        }

        let count = self.quantities.entry(food).or_default();
        *count = count.saturating_add(amount);
    }

    pub fn remove(&mut self, food: FoodKind, amount: u32) -> bool {
        let count = self.count(food);
        if count < amount {
            return false;
        }

        if amount == count {
            self.quantities.remove(&food);
        } else if let Some(existing) = self.quantities.get_mut(&food) {
            *existing -= amount;
        }

        true
    }

    pub fn contains(&self, food: FoodKind) -> bool {
        self.count(food) > 0
    }

    pub fn is_empty(&self) -> bool {
        self.quantities.is_empty()
    }

    pub fn total(&self) -> u32 {
        self.quantities
            .values()
            .copied()
            .fold(0, u32::saturating_add)
    }

    pub fn quantities(&self) -> impl Iterator<Item = (FoodKind, u32)> + '_ {
        self.quantities
            .iter()
            .map(|(food, amount)| (*food, *amount))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Poop {
    id: Uuid,
    created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attention_sequence: Option<u64>,
}

impl Poop {
    pub fn new(id: Uuid, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            created_at,
            attention_sequence: None,
        }
    }

    pub(crate) fn new_attention(
        id: Uuid,
        created_at: DateTime<Utc>,
        attention_sequence: u64,
    ) -> Self {
        Self {
            id,
            created_at,
            attention_sequence: Some(attention_sequence),
        }
    }

    pub fn id(self) -> Uuid {
        self.id
    }

    pub fn created_at(self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn attention_sequence(self) -> Option<u64> {
        self.attention_sequence
    }

    pub(crate) fn shift_created_at(&mut self, shift: Duration) {
        self.created_at += shift;
    }
}

#[derive(Clone)]
pub struct Pet {
    id: Uuid,
    name: String,
    species: PetSpecies,
    needs: PetNeeds,
    behavior: PetBehavior,
    work_points: u64,
    digestion_points: u64,
    last_updated_at: DateTime<Utc>,
    pub(crate) pending_poops: Vec<Poop>,
    pub(crate) pending_demands: Vec<PetDemand>,
    activity: AgentActivityState,
    recent_outcome: AgentOutcome,
    inventory: FoodInventory,
    poop_sequence: u64,
    napping_until: Option<DateTime<Utc>>,
}

impl Pet {
    pub fn new<N>(id: Uuid, name: N, species: PetSpecies, initial_timestamp: DateTime<Utc>) -> Self
    where
        N: Into<String>,
    {
        Self {
            id,
            name: name.into(),
            species,
            needs: PetNeeds::default(),
            behavior: PetBehavior::default(),
            work_points: 0,
            digestion_points: 0,
            last_updated_at: initial_timestamp,
            pending_poops: Vec::new(),
            pending_demands: Vec::new(),
            activity: AgentActivityState::default(),
            recent_outcome: AgentOutcome::default(),
            inventory: FoodInventory::default(),
            poop_sequence: 0,
            napping_until: None,
        }
    }

    /// Constructs a pet with an initial inventory seed. The inventory remains
    /// read-only through the aggregate after construction.
    pub fn with_inventory<N>(
        id: Uuid,
        name: N,
        species: PetSpecies,
        initial_timestamp: DateTime<Utc>,
        inventory: FoodInventory,
    ) -> Self
    where
        N: Into<String>,
    {
        let mut pet = Self::new(id, name, species, initial_timestamp);
        pet.inventory = inventory;
        pet
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub(crate) fn from_snapshot(
        id: Uuid,
        name: String,
        species: PetSpecies,
        needs: PetNeeds,
        behavior: PetBehavior,
        work_points: u64,
        digestion_points: u64,
        last_updated_at: DateTime<Utc>,
        pending_poops: Vec<Poop>,
        activity: AgentActivityState,
        recent_outcome: AgentOutcome,
        inventory: FoodInventory,
        poop_sequence: u64,
        napping_until: Option<DateTime<Utc>>,
    ) -> Self {
        Self::from_snapshot_with_demands(
            id,
            name,
            species,
            needs,
            behavior,
            work_points,
            digestion_points,
            last_updated_at,
            pending_poops,
            Vec::new(),
            activity,
            recent_outcome,
            inventory,
            poop_sequence,
            napping_until,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_snapshot_with_demands(
        id: Uuid,
        name: String,
        species: PetSpecies,
        needs: PetNeeds,
        behavior: PetBehavior,
        work_points: u64,
        digestion_points: u64,
        last_updated_at: DateTime<Utc>,
        pending_poops: Vec<Poop>,
        pending_demands: Vec<PetDemand>,
        activity: AgentActivityState,
        recent_outcome: AgentOutcome,
        inventory: FoodInventory,
        poop_sequence: u64,
        napping_until: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            name,
            species,
            needs,
            behavior,
            work_points,
            digestion_points,
            last_updated_at,
            pending_poops,
            pending_demands,
            activity,
            recent_outcome,
            inventory,
            poop_sequence,
            napping_until,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn species(&self) -> PetSpecies {
        self.species
    }

    pub fn needs(&self) -> PetNeeds {
        self.needs
    }

    pub(crate) fn needs_mut(&mut self) -> &mut PetNeeds {
        &mut self.needs
    }

    pub fn behavior(&self) -> PetBehavior {
        self.behavior
    }

    pub fn activity(&self) -> AgentActivityState {
        self.activity
    }

    pub fn recent_outcome(&self) -> AgentOutcome {
        self.recent_outcome
    }

    pub fn inventory(&self) -> &FoodInventory {
        &self.inventory
    }

    pub fn pending_poops(&self) -> &[Poop] {
        &self.pending_poops
    }

    pub fn pending_demands(&self) -> &[PetDemand] {
        &self.pending_demands
    }

    pub fn work_points(&self) -> u64 {
        self.work_points
    }

    pub(crate) fn add_work_points(&mut self, points: u64) {
        self.work_points = self.work_points.saturating_add(points);
    }

    pub fn digestion_points(&self) -> u64 {
        self.digestion_points
    }

    pub fn poop_sequence(&self) -> u64 {
        self.poop_sequence
    }

    pub fn napping_until(&self) -> Option<DateTime<Utc>> {
        self.napping_until
    }

    #[allow(dead_code)]
    pub(crate) fn add_digestion_points(&mut self, points: u64) {
        self.digestion_points = self.digestion_points.saturating_add(points);
    }

    pub(crate) fn consume_work_points(&mut self, points: u64) {
        self.work_points = self.work_points.saturating_sub(points);
    }

    pub(crate) fn consume_food(&mut self, food: FoodKind) -> bool {
        self.inventory.remove(food, 1)
    }

    pub(crate) fn restock_inventory_to_starter(&mut self) {
        self.inventory.restock_to_starter();
    }

    pub(crate) fn consume_digestion_points(&mut self, points: u64) {
        self.digestion_points = self.digestion_points.saturating_sub(points);
    }

    pub(crate) fn push_poop(&mut self, poop: Poop) {
        self.pending_poops.push(poop);
    }

    #[allow(dead_code)]
    pub(crate) fn push_demand(&mut self, demand: PetDemand) {
        self.pending_demands.push(demand);
    }

    #[allow(dead_code)]
    pub(crate) fn remove_demand(&mut self, index: usize) -> PetDemand {
        self.pending_demands.remove(index)
    }

    pub(crate) fn advance_poop_sequence(&mut self) {
        self.poop_sequence = self.poop_sequence.saturating_add(1);
    }

    pub(crate) fn start_nap(&mut self, until: DateTime<Utc>) {
        self.napping_until = Some(until);
    }

    pub(crate) fn clear_expired_nap(&mut self, now: DateTime<Utc>) {
        if self.napping_until.is_some_and(|until| now >= until) {
            self.napping_until = None;
        }
    }

    /// Whether the pet is napping at the given instant. The nap ends exactly
    /// when the clock reaches its deadline.
    pub fn is_napping(&self, now: DateTime<Utc>) -> bool {
        self.napping_until.is_some_and(|until| now < until)
    }

    pub(crate) fn set_activity(&mut self, activity: AgentActivityState) {
        self.activity = activity;
    }

    pub(crate) fn set_outcome(&mut self, outcome: AgentOutcome) {
        self.recent_outcome = outcome;
    }

    pub fn last_updated_at(&self) -> DateTime<Utc> {
        self.last_updated_at
    }

    /// Returns positive elapsed time and preserves the last timestamp on clock rollback.
    pub(crate) fn advance_to(&mut self, timestamp: DateTime<Utc>) -> Duration {
        if timestamp <= self.last_updated_at {
            return Duration::zero();
        }

        let elapsed = timestamp.signed_duration_since(self.last_updated_at);
        self.last_updated_at = timestamp;
        elapsed
    }

    pub(crate) fn set_behavior(&mut self, behavior: PetBehavior) {
        self.behavior = behavior;
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use uuid::Uuid;

    use super::{
        AgentActivityState, AgentOutcome, FoodInventory, Pet, PetBehavior, PetNeeds, PetSpecies,
        Poop,
    };
    use crate::attention::{PetDemand, PetDemandKind};

    #[test]
    fn needs_clamp_at_both_bounds() {
        let mut needs = PetNeeds::default();

        needs.adjust_hunger(150.0);
        needs.adjust_energy(-150.0);
        needs.adjust_happiness(-150.0);
        needs.adjust_cleanliness(150.0);

        assert_eq!(needs.hunger(), 100.0);
        assert_eq!(needs.energy(), 0.0);
        assert_eq!(needs.happiness(), 0.0);
        assert_eq!(needs.cleanliness(), 100.0);

        needs.set_hunger(-1.0);
        needs.set_energy(101.0);
        needs.set_happiness(101.0);
        needs.set_cleanliness(-1.0);

        assert_eq!(needs.hunger(), 0.0);
        assert_eq!(needs.energy(), 100.0);
        assert_eq!(needs.happiness(), 100.0);
        assert_eq!(needs.cleanliness(), 0.0);
    }

    #[test]
    fn pet_defaults_are_stable_and_explicit() {
        let start = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let pet = Pet::new(
            Uuid::from_u128(1),
            String::from("Mochi"),
            PetSpecies::Cat,
            start,
        );

        assert_eq!(pet.id(), Uuid::from_u128(1));
        assert_eq!(pet.name(), "Mochi");
        assert_eq!(pet.species(), PetSpecies::Cat);
        assert_eq!(pet.needs(), PetNeeds::default());
        assert_eq!(pet.behavior(), PetBehavior::Wandering);
        assert_eq!(pet.activity(), AgentActivityState::Idle);
        assert_eq!(pet.recent_outcome(), AgentOutcome::None);
        assert!(pet.pending_poops().is_empty());
        assert!(pet.pending_demands().is_empty());
        assert_eq!(pet.inventory(), &FoodInventory::default());
        assert_eq!(pet.work_points(), 0);
        assert_eq!(pet.digestion_points(), 0);
        assert_eq!(pet.last_updated_at(), start);
    }

    #[test]
    fn pending_demands_preserve_insertion_order() {
        let at = Utc.with_ymd_and_hms(2026, 8, 13, 10, 0, 0).unwrap();
        let mut pet = Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, at);
        let first = PetDemand::new(Uuid::from_u128(10), PetDemandKind::Affection, at);
        let second = PetDemand::new(Uuid::from_u128(11), PetDemandKind::Snack, at);
        pet.push_demand(first.clone());
        pet.push_demand(second.clone());
        assert_eq!(pet.pending_demands(), &[first, second]);
    }

    #[test]
    fn poop_and_inventory_have_small_domain_values() {
        let timestamp = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let poop = Poop::new(Uuid::from_u128(7), timestamp);
        assert_eq!(poop.id(), Uuid::from_u128(7));
        assert_eq!(poop.created_at(), timestamp);

        let mut inventory = FoodInventory::default();
        inventory.add(super::FoodKind::Kibble, 2);
        assert_eq!(inventory.count(super::FoodKind::Kibble), 2);
        assert!(inventory.remove(super::FoodKind::Kibble, 1));
        assert_eq!(inventory.count(super::FoodKind::Kibble), 1);
        assert!(!inventory.remove(super::FoodKind::Kibble, 2));
        assert_eq!(inventory.count(super::FoodKind::Kibble), 1);
    }

    #[test]
    fn pet_treats_backward_clock_movement_as_zero_elapsed_time() {
        let start = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let mut pet = Pet::new(
            Uuid::from_u128(2),
            String::from("Mochi"),
            PetSpecies::Cat,
            start,
        );

        assert_eq!(
            pet.advance_to(start + Duration::hours(1)),
            Duration::hours(1)
        );

        assert_eq!(pet.advance_to(start - Duration::hours(1)), Duration::zero());
        assert_eq!(pet.last_updated_at(), start + Duration::hours(1));

        assert_eq!(
            pet.advance_to(start + Duration::hours(2)),
            Duration::hours(1)
        );
    }

    #[test]
    fn point_additions_saturate_inside_the_pet_module() {
        let start = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let mut pet = Pet::new(Uuid::from_u128(3), "Mochi", PetSpecies::Cat, start);

        pet.add_work_points(3);
        pet.add_digestion_points(5);
        assert_eq!(pet.work_points(), 3);
        assert_eq!(pet.digestion_points(), 5);

        pet.add_work_points(u64::MAX);
        pet.add_digestion_points(u64::MAX);
        assert_eq!(pet.work_points(), u64::MAX);
        assert_eq!(pet.digestion_points(), u64::MAX);
    }
}

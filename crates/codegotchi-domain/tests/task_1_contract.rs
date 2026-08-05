use chrono::{TimeZone, Utc};
use codegotchi_domain::{
    AgentActivityState, AgentOutcome, FoodInventory, Pet, PetBehavior, PetNeeds, PetSpecies, Poop,
};
use uuid::Uuid;

#[test]
fn pet_constructor_initializes_the_task_one_aggregate_contract() {
    let id = Uuid::from_u128(7);
    let start = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
    let pet = Pet::new(id, String::from("Mochi"), PetSpecies::Cat, start);

    assert_eq!(pet.id(), id);
    assert_eq!(pet.name(), "Mochi");
    assert_eq!(pet.species(), PetSpecies::Cat);
    assert_eq!(pet.needs(), PetNeeds::default());
    assert_eq!(pet.behavior(), PetBehavior::Wandering);
    assert_eq!(pet.activity(), AgentActivityState::Idle);
    assert_eq!(pet.recent_outcome(), AgentOutcome::None);
    assert_eq!(pet.inventory(), &FoodInventory::default());
    assert_eq!(pet.work_points(), 0_u64);
    assert_eq!(pet.digestion_points(), 0_u64);
    assert_eq!(pet.last_updated_at(), start);
    assert!(pet.pending_poops().is_empty());
}

#[test]
fn needs_map_non_finite_values_and_clamp_every_mutation() {
    let mut needs = PetNeeds::new(f32::NAN, f32::NEG_INFINITY, f32::INFINITY, 50.0);

    assert_eq!(needs.hunger(), 0.0);
    assert_eq!(needs.energy(), 0.0);
    assert_eq!(needs.happiness(), 100.0);
    assert_eq!(needs.cleanliness(), 50.0);

    needs.set_hunger(f32::INFINITY);
    needs.set_energy(f32::NEG_INFINITY);
    needs.set_happiness(f32::NAN);
    needs.set_cleanliness(101.0);
    assert_eq!(needs.hunger(), 100.0);
    assert_eq!(needs.energy(), 0.0);
    assert_eq!(needs.happiness(), 0.0);
    assert_eq!(needs.cleanliness(), 100.0);

    needs.adjust_hunger(f32::NAN);
    needs.adjust_energy(f32::NEG_INFINITY);
    needs.adjust_happiness(f32::INFINITY);
    needs.adjust_cleanliness(-f32::INFINITY);
    assert_eq!(needs.hunger(), 0.0);
    assert_eq!(needs.energy(), 0.0);
    assert_eq!(needs.happiness(), 100.0);
    assert_eq!(needs.cleanliness(), 0.0);
}

#[test]
fn poop_owns_a_uuid_identity_and_creation_timestamp() {
    let id = Uuid::from_u128(9);
    let created_at = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
    let poop = Poop::new(id, created_at);

    assert_eq!(poop.id(), id);
    assert_eq!(poop.created_at(), created_at);
}

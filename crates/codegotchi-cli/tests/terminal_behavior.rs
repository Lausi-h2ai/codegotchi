use std::time::Duration;

use chrono::Utc;
use codegotchi_cli::terminal::{IdleIntent, PetPose, PresentationState, RoomObject};
use codegotchi_domain::{
    ActivityKind, AgentActivityState, DefaultNeedProgressionStrategy, FoodInventory, Pet,
    PetBehavior, PetSimulation, PetSpecies, SimulationSnapshot, SystemClock,
};
use ratatui::layout::{Position, Rect};
use uuid::Uuid;

const FULL_ROOM: Rect = Rect::new(0, 0, 120, 45);
const TICKS: u32 = 400;
const TICK_MS: u64 = 250;

fn base_snapshot(now: chrono::DateTime<Utc>) -> SimulationSnapshot {
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::starter(),
    );
    PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot()
}

fn frames(
    state: &mut PresentationState,
    snapshot: Option<&SimulationSnapshot>,
) -> Vec<(u64, PetPose, (i16, i16))> {
    (1..=TICKS)
        .map(|tick| {
            let now = Duration::from_millis(tick as u64 * TICK_MS);
            let frame = state.tick(now, snapshot, FULL_ROOM);
            (tick as u64 * TICK_MS, frame.pose, frame.offset)
        })
        .collect()
}

/// The same seed and inputs must produce byte-for-byte identical frames.
#[test]
fn same_seed_produces_identical_frames() {
    let mut first = PresentationState::new(42);
    let mut second = PresentationState::new(42);
    assert_eq!(frames(&mut first, None), frames(&mut second, None));
}

/// The autonomous intent enum has exactly the presentation-only variants.
/// Exhaustively matching it is a compile-time guarantee that autonomous
/// behavior cannot express Feed/Clean/Nap/Pet.
#[test]
fn autonomous_intents_contain_no_care_actions() {
    let intents = [
        IdleIntent::Wander(Position::new(1, 1)),
        IdleIntent::Sit,
        IdleIntent::Inspect(RoomObject::Bed),
        IdleIntent::LookOutWindow,
        IdleIntent::Yawn,
        IdleIntent::WatchCodex,
        IdleIntent::Celebrate,
        IdleIntent::Worry,
    ];
    for intent in intents {
        let _ = match intent {
            IdleIntent::Wander(_) => 0,
            IdleIntent::Sit => 0,
            IdleIntent::Inspect(_) => 0,
            IdleIntent::LookOutWindow => 0,
            IdleIntent::Yawn => 0,
            IdleIntent::WatchCodex => 0,
            IdleIntent::Celebrate => 0,
            IdleIntent::Worry => 0,
        };
    }
}

/// `PetBehavior::Sleeping` without an active `napping_until` is generic idle:
/// the autonomous presentation must never use the recovery-bed sleep pose.
#[test]
fn generic_sleeping_without_nap_never_uses_bed_sleep_pose() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = None;
    snapshot.activity = AgentActivityState::Idle;

    let mut state = PresentationState::new(7);
    let rendered = frames(&mut state, Some(&snapshot));
    assert!(
        rendered.iter().all(|(_, pose, _)| *pose != PetPose::Sleep),
        "generic sleeping must never present the bed-sleep pose"
    );
    assert!(
        rendered.iter().any(|(_, pose, _)| *pose == PetPose::Yawn),
        "generic sleeping should present doze/yawn idling"
    );
}

/// An active authoritative nap always presents the bed-sleep pose.
#[test]
fn authoritative_nap_uses_bed_sleep_pose() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = Some(now + chrono::Duration::minutes(10));
    snapshot.activity = AgentActivityState::Idle;

    let mut state = PresentationState::new(7);
    let frame = state.tick(Duration::from_secs(1), Some(&snapshot), FULL_ROOM);
    assert_eq!(frame.pose, PetPose::Sleep);
    for tick in 2..=20 {
        let frame = state.tick(
            Duration::from_millis(tick * 250),
            Some(&snapshot),
            FULL_ROOM,
        );
        assert_eq!(frame.pose, PetPose::Sleep);
    }
}

/// Thinking/Working are modifiers, not exclusive loops: calm life must
/// continue while the external activity stays active.
#[test]
fn thinking_and_working_modifiers_return_to_calm_life() {
    for (kind, name) in [
        (ActivityKind::Thinking, "Thinking"),
        (ActivityKind::Building, "Working"),
    ] {
        let now = Utc::now();
        let mut snapshot = base_snapshot(now);
        snapshot.activity = AgentActivityState::Active(kind);

        let mut state = PresentationState::new(9);
        let rendered = frames(&mut state, Some(&snapshot));
        assert!(
            rendered
                .iter()
                .any(|(_, pose, _)| *pose == PetPose::Curious),
            "{name} should occasionally look toward Codex"
        );
        assert!(
            rendered.iter().any(|(_, pose, _)| {
                matches!(
                    pose,
                    PetPose::Idle | PetPose::Sit | PetPose::WalkA | PetPose::WalkB
                )
            }),
            "{name} must not become an exclusive animation loop; calm life continues"
        );
    }
}

/// Success and Failure produce short reactions, then resume calm life.
#[test]
fn success_and_failure_produce_short_reactions() {
    let now = Utc::now();
    let mut success = base_snapshot(now);
    success.activity = AgentActivityState::Active(ActivityKind::Celebrating);
    let mut success_state = PresentationState::new(11);
    let success_frames = frames(&mut success_state, Some(&success));
    assert!(
        success_frames
            .iter()
            .any(|(_, pose, _)| *pose == PetPose::Happy),
        "Success should produce a short celebrate reaction"
    );
    assert!(
        success_frames
            .iter()
            .any(|(_, pose, _)| *pose != PetPose::Happy),
        "Celebrate must expire back to calm life"
    );

    let mut failure = base_snapshot(now);
    failure.activity = AgentActivityState::Active(ActivityKind::Error);
    let mut failure_state = PresentationState::new(12);
    let failure_frames = frames(&mut failure_state, Some(&failure));
    assert!(
        failure_frames
            .iter()
            .any(|(_, pose, _)| *pose == PetPose::Upset),
        "Failure should produce a short worried reaction"
    );
    assert!(
        failure_frames
            .iter()
            .any(|(_, pose, _)| *pose != PetPose::Upset),
        "Worry must expire back to calm life"
    );
}

/// A hungry pet lingers near the food anchor without ever being able to feed
/// itself (structurally enforced by the intent enum).
#[test]
fn hungry_pet_lingers_near_food_without_care() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.needs.set_hunger(20.0);
    snapshot.activity = AgentActivityState::Idle;

    let mut state = PresentationState::new(13);
    let rendered = frames(&mut state, Some(&snapshot));
    assert!(
        rendered
            .iter()
            .any(|(_, pose, _)| *pose == PetPose::Curious),
        "hungry pet should express attention near food"
    );
}

/// Wandering moves the pet within the room bounds and never leaves the lane.
#[test]
fn wander_stays_within_room_bounds_and_moves() {
    let mut state = PresentationState::new(14);
    let rendered = frames(&mut state, None);
    let max_x = 120u16.saturating_sub(12) as i16;
    assert!(
        rendered.iter().any(|(_, _, offset)| *offset != (0, 0)),
        "calm life should include wandering"
    );
    for (_, _, offset) in &rendered {
        assert!(
            offset.0.abs() <= max_x,
            "wander x offset {offset:?} escaped the room lane"
        );
    }
}

/// Small (Minimal) rooms must not panic the wander clamp when the room is
/// too short for a vertical lane.
#[test]
fn wander_survives_small_minimal_rooms_without_panicking() {
    let mut state = PresentationState::new(17);
    let minimal = Rect::new(0, 0, 80, 3);
    for tick in 1..=200 {
        let _ = state.tick(Duration::from_millis(tick as u64 * TICK_MS), None, minimal);
    }
}

/// Even with critical needs the presentation state only produces presentation
/// poses and intents; care is structurally impossible.
#[test]
fn critical_needs_never_emit_care_intents() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.needs.set_hunger(5.0);
    snapshot.needs.set_energy(5.0);
    snapshot.needs.set_happiness(5.0);
    snapshot.activity = AgentActivityState::Idle;

    let mut state = PresentationState::new(15);
    for tick in 1..=TICKS {
        let frame = state.tick(
            Duration::from_millis(tick as u64 * TICK_MS),
            Some(&snapshot),
            FULL_ROOM,
        );
        assert!(
            frame.pose != PetPose::Sleep,
            "critical needs must not imply an authoritative nap"
        );
    }
}

#[test]
fn lonely_pet_seeks_attention_without_resolving_affection() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.needs.set_happiness(10.0);
    snapshot.activity = AgentActivityState::Idle;

    let mut state = PresentationState::new(16);
    let rendered = frames(&mut state, Some(&snapshot));
    assert!(
        rendered
            .iter()
            .any(|(_, pose, _)| *pose == PetPose::Curious),
        "lonely pet should express attention-seeking presentation"
    );
}

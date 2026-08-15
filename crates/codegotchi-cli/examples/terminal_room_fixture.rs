//! Deterministic terminal-room renderer fixture.
//!
//! Renders the Full / Compact / Minimal room projections with a drained pet
//! snapshot and steps the seeded presentation clock so the pet visibly moves.
//! This is the production renderer, not a fork of it, and is intended for
//! human/vision review (VISUAL_FIDELITY_UNVERIFIED).

use std::time::Duration;

use chrono::Utc;
use codegotchi_cli::terminal::{PresentationState, render_room};
use codegotchi_domain::{
    DefaultNeedProgressionStrategy, FoodInventory, Pet, PetSimulation, PetSpecies, SystemClock,
};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
};
use uuid::Uuid;

fn main() {
    let now = Utc::now();
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::starter(),
    );
    let simulation = PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy);
    let mut snapshot = simulation.snapshot();
    // Drained pet: starving, exhausted, lonely, dirty.
    snapshot.needs.set_hunger(100.0);
    snapshot.needs.set_energy(0.0);
    snapshot.needs.set_happiness(0.0);
    snapshot.needs.set_cleanliness(0.0);

    let mut presentation = PresentationState::new(7);
    for (name, area, times) in [
        (
            "FULL",
            Rect::new(0, 0, 120, 14),
            &[
                Duration::from_secs(5),
                Duration::from_secs(20),
                Duration::from_secs(45),
            ][..],
        ),
        (
            "COMPACT",
            Rect::new(0, 0, 120, 7),
            &[Duration::from_secs(5), Duration::from_secs(25)][..],
        ),
        (
            "MINIMAL",
            Rect::new(0, 0, 120, 3),
            &[Duration::from_secs(5)][..],
        ),
    ] {
        let total = times.last().copied().unwrap_or_default();
        let mut tick_ms = 0u64;
        while Duration::from_millis(tick_ms) <= total {
            tick_ms += 250;
            let _ = presentation.tick(Duration::from_millis(tick_ms), Some(&snapshot), area);
            if !times.contains(&Duration::from_millis(tick_ms)) {
                continue;
            }
            let frame = presentation.frame();
            let mut buffer = Buffer::filled(area, Cell::new(" "));
            render_room(area, &mut buffer, &snapshot, &frame);
            println!(
                "==== {name} at t={}ms pose={:?} offset={:?} ====",
                tick_ms, frame.pose, frame.offset
            );
            for y in 0..area.height {
                let mut row = String::new();
                for x in 0..area.width {
                    if let Some(cell) = buffer.cell((x, y)) {
                        row.push_str(cell.symbol());
                    }
                }
                println!("{row}");
            }
        }
    }
}

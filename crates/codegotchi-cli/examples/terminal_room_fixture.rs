//! Deterministic terminal-room compositor fixture.
//!
//! This uses the same Ratatui compositor and room renderer as the production
//! terminal session. It is intentionally bounded and Codex-free, making it
//! safe to launch in an xterm for screenshot review.

use std::{error::Error, io, thread, time::Duration};

use chrono::Utc;
use codegotchi_cli::terminal::{
    CrosstermTerminal, PresentationFrame, RoomAmbience, RoomRenderOptions, TerminalBackend,
    TerminalGuard, TerminalRunError, TerminalThemePreset, render_room_with_options,
};
use codegotchi_domain::{
    DefaultNeedProgressionStrategy, FoodInventory, Pet, PetBehavior, PetDemand, PetDemandKind,
    PetSimulation, PetSpecies, Poop, SystemClock,
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use uuid::Uuid;

fn main() {
    if let Err(error) = run_fixture() {
        eprintln!("terminal_room_fixture: {error}");
        std::process::exit(1);
    }
}

fn run_fixture() -> Result<(), Box<dyn Error>> {
    // Screenshots are visual proof and must retain preset colors even when the
    // caller's environment sets NO_COLOR.
    crossterm::style::force_color_output(true);
    let theme = parse_theme()?;
    let ambience = match std::env::var("CG_FIXTURE_TIME_OF_DAY")
        .unwrap_or_else(|_| String::from("day"))
        .as_str()
    {
        "night" => RoomAmbience::Night,
        "day" => RoomAmbience::Day,
        value => {
            return Err(format!("CG_FIXTURE_TIME_OF_DAY must be day|night, got {value}").into());
        }
    };
    let layout = std::env::var("CG_FIXTURE_LAYOUT").unwrap_or_else(|_| String::from("full"));
    let pause_ms = std::env::var("CG_FIXTURE_PAUSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3_000);

    let result = TerminalGuard::run_with(CrosstermTerminal::new(), |guard| {
        draw_fixture(guard, &layout, theme, ambience, pause_ms)
            .map_err(|error| io::Error::other(error.to_string()))
    });

    match result {
        Ok(()) => Ok(()),
        Err(TerminalRunError::Initialization(error)) => Err(Box::new(error)),
        Err(TerminalRunError::Body { error, restoration }) => {
            if let Some(restoration) = restoration {
                Err(Box::new(io::Error::other(format!(
                    "fixture failed: {error}; restoration failed: {restoration}"
                ))))
            } else {
                Err(Box::new(error))
            }
        }
        Err(TerminalRunError::Restoration(error)) => Err(Box::new(error)),
    }
}

fn parse_theme() -> Result<TerminalThemePreset, Box<dyn Error>> {
    let value = std::env::var("CG_FIXTURE_THEME").unwrap_or_else(|_| String::from("auto"));
    value.parse::<TerminalThemePreset>().map_err(|error| {
        format!("CG_FIXTURE_THEME must be auto|mono|soft-green|amber|night: {error}").into()
    })
}

fn draw_fixture(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    layout: &str,
    theme: TerminalThemePreset,
    ambience: RoomAmbience,
    pause_ms: u64,
) -> Result<(), Box<dyn Error>> {
    let (columns, rows) = guard.backend_mut().size()?;
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
    // Keep care/status content visually representative and deterministic.
    snapshot.needs.set_hunger(100.0);
    snapshot.needs.set_energy(0.0);
    snapshot.needs.set_happiness(0.0);
    snapshot.needs.set_cleanliness(0.0);
    for index in 0..3_u128 {
        snapshot
            .pending_poops
            .push(Poop::new(Uuid::from_u128(0x7000 + index), now));
    }
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(0xaffec7),
        PetDemandKind::Affection,
        now,
    ));
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(0x5ac9),
        PetDemandKind::Snack,
        now,
    ));
    match std::env::var("CG_FIXTURE_SLEEP").as_deref() {
        Ok("bed") => {
            snapshot.behavior = PetBehavior::Sleeping;
            snapshot.napping_until = Some(now + chrono::Duration::minutes(30));
        }
        Ok("doze") => {
            snapshot.behavior = PetBehavior::Sleeping;
            snapshot.napping_until = None;
        }
        Ok("awake") | Err(_) => {}
        Ok(value) => {
            return Err(format!("CG_FIXTURE_SLEEP must be awake|bed|doze, got {value}").into());
        }
    }

    let room_height = match layout {
        "full" => 14,
        "compact" => 7,
        "minimal" => 3,
        "all" => 14,
        value => {
            return Err(
                format!("CG_FIXTURE_LAYOUT must be full|compact|minimal|all, got {value}").into(),
            );
        }
    };
    let options = RoomRenderOptions::for_theme(theme, ambience);
    let backend = CrosstermBackend::new(guard.writer_mut());
    let mut terminal = Terminal::new(backend)?;
    let layouts: &[u16] = match layout {
        "all" => &[14, 7, 3],
        _ => &[room_height],
    };
    for &height in layouts {
        terminal.clear()?;
        terminal.draw(|frame| {
            let area = Rect::new(0, 0, columns, height.min(rows));
            render_room_with_options(
                area,
                frame.buffer_mut(),
                &snapshot,
                &PresentationFrame::default(),
                options,
                None,
            );
        })?;
        if pause_ms > 0 {
            thread::sleep(Duration::from_millis(pause_ms));
        }
    }
    drop(terminal);
    Ok(())
}

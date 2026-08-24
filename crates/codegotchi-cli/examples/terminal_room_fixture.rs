//! Deterministic terminal-room compositor fixture.
//!
//! This uses the same Ratatui compositor and room renderer as the production
//! terminal session. It is intentionally bounded and Codex-free, making it
//! safe to launch in an xterm for screenshot review.

use std::{error::Error, io, thread, time::Duration};

use chrono::Utc;
use codegotchi_cli::terminal::{
    CrosstermTerminal, PetPose, PresentationFrame, RoomAmbience, RoomRenderOptions,
    TerminalBackend, TerminalGuard, TerminalRunError, TerminalThemePreset,
    render_room_with_options,
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
    let pose = std::env::var("CG_FIXTURE_POSE").unwrap_or_else(|_| String::from("idle"));
    let poses = fixture_poses(&pose)?;
    let pause_ms = std::env::var("CG_FIXTURE_PAUSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3_000);

    let result = TerminalGuard::run_with(CrosstermTerminal::new(), |guard| {
        draw_fixture(guard, &layout, theme, ambience, pause_ms, &poses)
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

fn parse_pose(value: &str) -> Result<PetPose, String> {
    match value {
        "idle" => Ok(PetPose::Idle),
        "blink" => Ok(PetPose::Blink),
        "walk-a" => Ok(PetPose::WalkA),
        "walk-b" => Ok(PetPose::WalkB),
        "sit" => Ok(PetPose::Sit),
        "doze" => Ok(PetPose::Doze),
        "sleep" => Ok(PetPose::Sleep),
        "yawn" => Ok(PetPose::Yawn),
        "curious" => Ok(PetPose::Curious),
        "happy" => Ok(PetPose::Happy),
        "upset" => Ok(PetPose::Upset),
        "eating" => Ok(PetPose::Eating),
        "petted" => Ok(PetPose::Petted),
        value => Err(format!(
            "CG_FIXTURE_POSE must be idle|blink|walk-a|walk-b|sit|doze|sleep|yawn|curious|happy|upset|eating|petted|all, got {value}"
        )),
    }
}

fn fixture_poses(value: &str) -> Result<Vec<PetPose>, String> {
    if value == "all" {
        return Ok(vec![
            PetPose::Idle,
            PetPose::Blink,
            PetPose::WalkA,
            PetPose::WalkB,
            PetPose::Sit,
            PetPose::Doze,
            PetPose::Yawn,
            PetPose::Curious,
            PetPose::Happy,
            PetPose::Upset,
            PetPose::Eating,
            PetPose::Petted,
            PetPose::Sleep,
        ]);
    }

    Ok(vec![parse_pose(value)?])
}

fn draw_fixture(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    layout: &str,
    theme: TerminalThemePreset,
    ambience: RoomAmbience,
    pause_ms: u64,
    poses: &[PetPose],
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
    let bottom = std::env::var("CG_FIXTURE_BOTTOM").ok().as_deref() == Some("1");
    let layouts: &[u16] = match layout {
        "all" => &[14, 7, 3],
        _ => &[room_height],
    };
    for &height in layouts {
        for &pose in poses {
            terminal.clear()?;
            terminal.draw(|frame| {
                let room_height = height.min(rows);
                let top = if bottom {
                    rows.saturating_sub(room_height)
                } else {
                    0
                };
                let area = Rect::new(0, top, columns, room_height);
                let presentation = PresentationFrame {
                    pose,
                    offset: (0, 0),
                };
                render_room_with_options(
                    area,
                    frame.buffer_mut(),
                    &snapshot,
                    &presentation,
                    options,
                    None,
                );
            })?;
            if pause_ms > 0 {
                thread::sleep(Duration::from_millis(pause_ms));
            }
        }
    }
    drop(terminal);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegotchi_cli::terminal::PetPose;

    #[test]
    fn parse_pose_accepts_every_fixture_spelling() {
        let cases = [
            ("idle", PetPose::Idle),
            ("blink", PetPose::Blink),
            ("walk-a", PetPose::WalkA),
            ("walk-b", PetPose::WalkB),
            ("sit", PetPose::Sit),
            ("doze", PetPose::Doze),
            ("sleep", PetPose::Sleep),
            ("yawn", PetPose::Yawn),
            ("curious", PetPose::Curious),
            ("happy", PetPose::Happy),
            ("upset", PetPose::Upset),
            ("eating", PetPose::Eating),
            ("petted", PetPose::Petted),
        ];

        for (spelling, expected) in cases {
            assert_eq!(parse_pose(spelling), Ok(expected), "spelling={spelling}");
        }
    }

    #[test]
    fn parse_pose_rejects_unknown_values() {
        let error = parse_pose("dancing").expect_err("unknown pose should be rejected");
        assert!(error.contains("dancing"));
    }

    #[test]
    fn fixture_poses_all_returns_poses_in_enum_order() {
        assert_eq!(
            fixture_poses("all").expect("all should expand to every pose"),
            vec![
                PetPose::Idle,
                PetPose::Blink,
                PetPose::WalkA,
                PetPose::WalkB,
                PetPose::Sit,
                PetPose::Doze,
                PetPose::Yawn,
                PetPose::Curious,
                PetPose::Happy,
                PetPose::Upset,
                PetPose::Eating,
                PetPose::Petted,
                PetPose::Sleep,
            ]
        );
    }
}

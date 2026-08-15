use codegotchi_domain::{PetBehavior, SimulationSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
};

use super::behavior::{PresentationActivity, has_authoritative_nap, presentation_activity};
use super::theme::{SemanticTone, auto_style};

const FULL_ROOM_HEIGHT: u16 = 14;
const COMPACT_ROOM_HEIGHT: u16 = 7;

/// Renders the authoritative CodeGotchi room projection into `area`.
///
/// The room is a pure projection of the snapshot: the pet never feeds, cleans,
/// naps, or pets itself here, and bed sleep is used only when
/// [`has_authoritative_nap`] says so. Rendering is deterministic so the same
/// snapshot always produces the same cells.
pub fn render_room(area: Rect, buffer: &mut Buffer, snapshot: &SimulationSnapshot) {
    if area.is_empty() {
        return;
    }
    reset_area(area, buffer);

    let activity = presentation_activity(snapshot);
    let napping = has_authoritative_nap(snapshot);
    let mode = room_mode(area.height);

    match mode {
        RoomMode::Full => render_full(area, buffer, snapshot, activity, napping),
        RoomMode::Compact => render_compact(area, buffer, snapshot, activity, napping),
        RoomMode::Minimal => render_minimal(area, buffer, snapshot, activity, napping),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoomMode {
    Full,
    Compact,
    Minimal,
}

fn room_mode(height: u16) -> RoomMode {
    if height >= FULL_ROOM_HEIGHT {
        RoomMode::Full
    } else if height >= COMPACT_ROOM_HEIGHT {
        RoomMode::Compact
    } else {
        RoomMode::Minimal
    }
}

fn reset_area(area: Rect, buffer: &mut Buffer) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buffer.cell_mut(Position { x, y }) {
                cell.reset();
            }
        }
    }
}

fn put(area: Rect, buffer: &mut Buffer, x: u16, y: u16, symbol: &str, style: Style) {
    if x >= area.width || y >= area.height {
        return;
    }
    let Some(cell) = buffer.cell_mut(Position {
        x: area.x + x,
        y: area.y + y,
    }) else {
        return;
    };
    cell.set_symbol(symbol).set_style(style);
}

fn put_line(area: Rect, buffer: &mut Buffer, y: u16, text: &str, style: Style) {
    for (offset, ch) in text.chars().enumerate() {
        put(
            area,
            buffer,
            u16::try_from(offset).unwrap_or(u16::MAX),
            y,
            &ch.to_string(),
            style,
        );
    }
}

fn put_text(area: Rect, buffer: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    for (offset, ch) in text.chars().enumerate() {
        put(
            area,
            buffer,
            x.saturating_add(u16::try_from(offset).unwrap_or(u16::MAX)),
            y,
            &ch.to_string(),
            style,
        );
    }
}

fn render_full(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
) {
    let needs = snapshot.needs;
    let status_lines = [
        format!("Hunger {:8}", need_bar(needs.hunger())),
        format!("Energy {:8}", need_bar(needs.energy())),
        format!("Happy  {:8}", need_bar(needs.happiness())),
        format!("Clean  {:8}", need_bar(needs.cleanliness())),
    ];
    for (index, line) in status_lines.iter().enumerate() {
        put_line(
            area,
            buffer,
            u16::try_from(index).unwrap_or(u16::MAX),
            line,
            auto_style(SemanticTone::Tone3),
        );
    }

    let activity_label = format!("{:?}", activity);
    put_text(
        area,
        buffer,
        20,
        0,
        &activity_label,
        auto_style(SemanticTone::Tone2),
    );

    let pet_x = area.width.saturating_sub(26).max(2);
    let pet_area = Rect::new(
        area.x.saturating_add(pet_x),
        area.y,
        area.width.saturating_sub(pet_x).min(14),
        area.height,
    );
    if napping {
        put_sprite(pet_area, buffer, &BED_FULL, 0, 0);
        put_line(
            pet_area,
            buffer,
            4,
            "z z z",
            auto_style(SemanticTone::Tone2),
        );
    } else if snapshot.behavior == PetBehavior::Sleeping {
        put_sprite(pet_area, buffer, &PET_FULL, 0, 0);
        put_line(pet_area, buffer, 4, "z", auto_style(SemanticTone::Tone2));
    } else {
        put_sprite(pet_area, buffer, &PET_FULL, 0, 0);
    }
}

fn render_compact(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
) {
    let needs = snapshot.needs;
    let status_line = format!(
        "H {} E {} P {} C {}",
        need_percent(needs.hunger()),
        need_percent(needs.energy()),
        need_percent(needs.happiness()),
        need_percent(needs.cleanliness()),
    );
    put_line(
        area,
        buffer,
        0,
        &status_line,
        auto_style(SemanticTone::Tone3),
    );

    let pet_x = area.width.saturating_sub(18).max(2);
    let pet_area = Rect::new(
        area.x.saturating_add(pet_x),
        area.y,
        area.width.saturating_sub(pet_x).min(10),
        area.height,
    );
    if napping {
        put_sprite(pet_area, buffer, &BED_COMPACT, 0, 0);
        put_line(pet_area, buffer, 3, "z", auto_style(SemanticTone::Tone2));
    } else if snapshot.behavior == PetBehavior::Sleeping {
        put_sprite(pet_area, buffer, &PET_COMPACT, 0, 0);
        put_line(pet_area, buffer, 3, "z", auto_style(SemanticTone::Tone2));
    } else {
        put_sprite(pet_area, buffer, &PET_COMPACT, 0, 0);
    }

    let activity_label = format!("{:?}", activity);
    put_line(
        area,
        buffer,
        1,
        &activity_label,
        auto_style(SemanticTone::Tone2),
    );
}

fn render_minimal(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    _activity: PresentationActivity,
    napping: bool,
) {
    let needs = snapshot.needs;
    let state = if napping {
        "SLEEP"
    } else if snapshot.behavior == PetBehavior::Sleeping {
        "doze"
    } else {
        "ok"
    };
    let line = format!(
        "CG {} H{} E{} P{} C{}",
        state,
        need_percent(needs.hunger()),
        need_percent(needs.energy()),
        need_percent(needs.happiness()),
        need_percent(needs.cleanliness()),
    );
    put_line(area, buffer, 0, &line, auto_style(SemanticTone::Tone3));
}

fn put_sprite(area: Rect, buffer: &mut Buffer, sprite: &[&str], x: u16, y: u16) {
    for (row, line) in sprite.iter().enumerate() {
        let row = u16::try_from(row).unwrap_or(u16::MAX);
        for (offset, ch) in line.chars().enumerate() {
            let style = match ch {
                '█' | '▀' | '▄' => auto_style(SemanticTone::Tone1),
                '┌' | '┐' | '└' | '┘' | '─' | '│' => auto_style(SemanticTone::Tone2),
                _ => auto_style(SemanticTone::Tone3),
            };
            put(
                area,
                buffer,
                x + u16::try_from(offset).unwrap_or(u16::MAX),
                y + row,
                &ch.to_string(),
                style,
            );
        }
    }
}

fn need_bar(value: f32) -> String {
    let filled = (value.clamp(0.0, 1.0) * 8.0).round() as usize;
    let mut bar = "█".repeat(filled);
    bar.push_str(&"░".repeat(8usize.saturating_sub(filled)));
    bar
}

fn need_percent(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 100.0).round() as u8
}

const PET_FULL: [&str; 5] = [
    "  ▄▄▄▄▄▄  ",
    " █  ██ ██ ",
    " █  ▄▄   █",
    " █▄▄▄▄▄▄█ ",
    "  ▀▀▀▀▀▀  ",
];

const PET_COMPACT: [&str; 3] = [" ▄▄▄▄▄ ", "█ ██ ██", " ▀▀▀▀▀ "];

const BED_FULL: [&str; 5] = [
    "┌──────────┐",
    "│▄▄▄▄▄▄▄▄▄▄│",
    "│▀▀▀▀▀▀▀▀▀▀│",
    "└──────────┘",
    "            ",
];

const BED_COMPACT: [&str; 3] = ["┌────────┐", "│▄▄▄▄▄▄▄▄│", "└────────┘"];

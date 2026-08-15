use codegotchi_domain::{FoodKind, PetBehavior, SimulationSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
};
use uuid::Uuid;

use super::behavior::{PresentationActivity, has_authoritative_nap, presentation_activity};
use super::theme::{SemanticTone, auto_style};

const FULL_ROOM_HEIGHT: u16 = 14;
const COMPACT_ROOM_HEIGHT: u16 = 7;

/// Stable interactive regions of the room. Rendering and mouse hit testing
/// share this geometry so affordances always correspond to drawn objects.
#[derive(Clone, Debug)]
pub struct RoomGeometry {
    /// The pet sprite region; the petting and food-drop target.
    pub pet: Rect,
    /// The bed region; a click submits an authoritative nap.
    pub bed: Option<Rect>,
    /// The food tray region; a press starts a food drag.
    pub food: Option<Rect>,
    /// The food id stocked in the tray (kibble by default).
    pub food_id: String,
    /// Authoritative poop objects with their click regions.
    pub poops: Vec<(Uuid, Rect)>,
    /// True when the room uses the Minimal layout.
    pub minimal: bool,
}

impl RoomGeometry {
    /// Returns the food region and its stocked id when a press starts a drag.
    #[must_use]
    pub fn food_hit(&self, point: Position) -> Option<(Rect, String)> {
        self.food
            .filter(|rect| rect.contains(point))
            .map(|rect| (rect, self.food_id.clone()))
    }

    /// Returns the authoritative poop id under the pointer, if any.
    #[must_use]
    pub fn poop_hit(&self, point: Position) -> Option<Uuid> {
        self.poops
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(id, _)| *id)
    }
}

/// Computes the interactive geometry for the current room rectangle and
/// authoritative snapshot. Deterministic: the same inputs always produce the
/// same regions.
#[must_use]
pub fn room_geometry(area: Rect, snapshot: &SimulationSnapshot) -> RoomGeometry {
    if area.is_empty() {
        return RoomGeometry {
            pet: Rect::ZERO,
            bed: None,
            food: None,
            food_id: FoodKind::Kibble.id().to_owned(),
            poops: Vec::new(),
            minimal: false,
        };
    }
    match room_mode(area.height) {
        RoomMode::Full => full_geometry(area, snapshot),
        RoomMode::Compact => compact_geometry(area, snapshot),
        RoomMode::Minimal => minimal_geometry(area, snapshot),
    }
}

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
    let geometry = room_geometry(area, snapshot);

    match room_mode(area.height) {
        RoomMode::Full => render_full(area, buffer, snapshot, activity, napping, &geometry),
        RoomMode::Compact => render_compact(area, buffer, snapshot, activity, napping, &geometry),
        RoomMode::Minimal => render_minimal(area, buffer, snapshot, napping),
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

fn full_geometry(area: Rect, snapshot: &SimulationSnapshot) -> RoomGeometry {
    let pet_x = area.width.saturating_sub(34).max(2);
    let pet = Rect::new(
        area.x.saturating_add(pet_x),
        area.y.saturating_add(4),
        (area.width.saturating_sub(pet_x)).min(14),
        5,
    );
    let bed_x = area.width.saturating_sub(16).max(4);
    let bed = Rect::new(
        area.x.saturating_add(bed_x),
        area.y.saturating_add(8),
        (area.width.saturating_sub(bed_x)).min(12),
        4,
    );
    let food = Rect::new(area.x.saturating_add(2), area.y.saturating_add(11), 12, 2);
    let poops = poop_slots(area, snapshot, 16, 12, 3);
    RoomGeometry {
        pet,
        bed: Some(bed),
        food: Some(food),
        food_id: FoodKind::Kibble.id().to_owned(),
        poops,
        minimal: false,
    }
}

fn compact_geometry(area: Rect, snapshot: &SimulationSnapshot) -> RoomGeometry {
    let pet_x = area.width.saturating_sub(26).max(2);
    let pet = Rect::new(
        area.x.saturating_add(pet_x),
        area.y.saturating_add(1),
        (area.width.saturating_sub(pet_x)).min(10),
        3,
    );
    let bed_x = area.width.saturating_sub(12).max(4);
    let bed = Rect::new(
        area.x.saturating_add(bed_x),
        area.y.saturating_add(3),
        (area.width.saturating_sub(bed_x)).min(10),
        2,
    );
    let food = Rect::new(area.x.saturating_add(2), area.y.saturating_add(5), 10, 1);
    let poops = poop_slots(area, snapshot, 14, 5, 2);
    RoomGeometry {
        pet,
        bed: Some(bed),
        food: Some(food),
        food_id: FoodKind::Kibble.id().to_owned(),
        poops,
        minimal: false,
    }
}

fn minimal_geometry(area: Rect, snapshot: &SimulationSnapshot) -> RoomGeometry {
    let pet = Rect::new(area.x, area.y, 3, 1);
    let food = Rect::new(area.x, area.y.saturating_add(1), 7, 1);
    let bed = Rect::new(area.x.saturating_add(9), area.y.saturating_add(1), 4, 1);
    let poops = snapshot
        .pending_poops
        .iter()
        .take(2)
        .enumerate()
        .map(|(index, poop)| {
            (
                poop.id(),
                Rect::new(
                    area.x.saturating_add(15 + index as u16 * 5),
                    area.y.saturating_add(1),
                    5,
                    1,
                ),
            )
        })
        .collect();
    RoomGeometry {
        pet,
        bed: Some(bed),
        food: Some(food),
        food_id: FoodKind::Kibble.id().to_owned(),
        poops,
        minimal: true,
    }
}

fn poop_slots(
    area: Rect,
    snapshot: &SimulationSnapshot,
    x: u16,
    y: u16,
    limit: usize,
) -> Vec<(Uuid, Rect)> {
    snapshot
        .pending_poops
        .iter()
        .take(limit)
        .enumerate()
        .map(|(index, poop)| {
            (
                poop.id(),
                Rect::new(
                    area.x.saturating_add(x + index as u16 * 4),
                    area.y.saturating_add(y),
                    2,
                    1,
                ),
            )
        })
        .collect()
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
    geometry: &RoomGeometry,
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
    put_text(
        area,
        buffer,
        20,
        0,
        &format!("{:?}", activity),
        auto_style(SemanticTone::Tone2),
    );

    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        put_sprite(area, buffer, &BED_FULL, bed_x, bed_y);
    }
    if napping {
        if let Some(bed) = geometry.bed {
            let bed_x = bed.x.saturating_sub(area.x);
            let bed_y = bed.y.saturating_sub(area.y);
            put_sprite(area, buffer, &PET_SLEEP_FULL, bed_x, bed_y);
            put_line(
                area,
                buffer,
                bed_y.saturating_add(4),
                "z z z",
                auto_style(SemanticTone::Tone2),
            );
        }
    } else {
        let pet_x = geometry.pet.x.saturating_sub(area.x);
        let pet_y = geometry.pet.y.saturating_sub(area.y);
        if snapshot.behavior == PetBehavior::Sleeping {
            put_sprite(area, buffer, &PET_DOZE_FULL, pet_x, pet_y);
            put_line(
                area,
                buffer,
                pet_y.saturating_add(5),
                "z",
                auto_style(SemanticTone::Tone2),
            );
        } else {
            put_sprite(area, buffer, &PET_FULL, pet_x, pet_y);
        }
    }

    if let Some(food) = geometry.food {
        let food_y = food.y.saturating_sub(area.y);
        let stocked = snapshot.inventory.count(FoodKind::Kibble);
        put_line(
            area,
            buffer,
            food_y,
            &format!("KIB x{stocked}  drag to pet"),
            auto_style(SemanticTone::Tone2),
        );
    }
    for (_, rect) in &geometry.poops {
        let x = rect.x.saturating_sub(area.x);
        let y = rect.y.saturating_sub(area.y);
        put(area, buffer, x, y, "●", auto_style(SemanticTone::Tone1));
    }
}

fn render_compact(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
    geometry: &RoomGeometry,
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
    put_line(
        area,
        buffer,
        1,
        &format!("{:?}", activity),
        auto_style(SemanticTone::Tone2),
    );

    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        put_sprite(area, buffer, &BED_COMPACT, bed_x, bed_y);
    }
    if napping {
        if let Some(bed) = geometry.bed {
            let bed_x = bed.x.saturating_sub(area.x);
            let bed_y = bed.y.saturating_sub(area.y);
            put_sprite(area, buffer, &PET_SLEEP_COMPACT, bed_x, bed_y);
            put_line(
                area,
                buffer,
                bed_y.saturating_add(2),
                "z z z",
                auto_style(SemanticTone::Tone2),
            );
        }
    } else {
        let pet_x = geometry.pet.x.saturating_sub(area.x);
        let pet_y = geometry.pet.y.saturating_sub(area.y);
        if snapshot.behavior == PetBehavior::Sleeping {
            put_sprite(area, buffer, &PET_DOZE_COMPACT, pet_x, pet_y);
            put_line(
                area,
                buffer,
                pet_y.saturating_add(3),
                "z",
                auto_style(SemanticTone::Tone2),
            );
        } else {
            put_sprite(area, buffer, &PET_COMPACT, pet_x, pet_y);
        }
    }

    if let Some(food) = geometry.food {
        let food_y = food.y.saturating_sub(area.y);
        let stocked = snapshot.inventory.count(FoodKind::Kibble);
        put_line(
            area,
            buffer,
            food_y,
            &format!("KIB x{stocked}"),
            auto_style(SemanticTone::Tone2),
        );
    }
    for (_, rect) in &geometry.poops {
        let x = rect.x.saturating_sub(area.x);
        let y = rect.y.saturating_sub(area.y);
        put(area, buffer, x, y, "●", auto_style(SemanticTone::Tone1));
    }
}

fn render_minimal(area: Rect, buffer: &mut Buffer, snapshot: &SimulationSnapshot, napping: bool) {
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

    let stocked = snapshot.inventory.count(FoodKind::Kibble);
    let mut affordances = format!("FOOD x{stocked}  BED");
    for _ in &snapshot.pending_poops {
        affordances.push_str("  POOP");
    }
    put_line(
        area,
        buffer,
        1,
        &affordances,
        auto_style(SemanticTone::Tone2),
    );
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
                x.saturating_add(u16::try_from(offset).unwrap_or(u16::MAX)),
                y.saturating_add(row),
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

const PET_DOZE_FULL: [&str; 5] = [
    "  ▄▄▄▄▄▄  ",
    " █  ██ ██ ",
    " █  ▄▄   █",
    " █▄▄▄▄▄▄█ ",
    "  ▀▀▀▀▀▀  ",
];

const PET_SLEEP_FULL: [&str; 5] = [
    "  ▄▄▄▄▄▄  ",
    " █  ██ ██ ",
    " █        █",
    " █▄▄▄▄▄▄█ ",
    "  ▀▀▀▀▀▀  ",
];

const PET_COMPACT: [&str; 3] = [" ▄▄▄▄▄ ", "█ ██ ██", " ▀▀▀▀▀ "];

const PET_DOZE_COMPACT: [&str; 3] = [" ▄▄▄▄▄ ", "█ ██ ██", " ▀▀▀▀▀ "];

const PET_SLEEP_COMPACT: [&str; 3] = [" ▄▄▄▄▄ ", "█     █", " ▀▀▀▀▀ "];

const BED_FULL: [&str; 4] = [
    "┌──────────┐",
    "│▄▄▄▄▄▄▄▄▄▄│",
    "│▀▀▀▀▀▀▀▀▀▀│",
    "└──────────┘",
];

const BED_COMPACT: [&str; 2] = ["┌────────┐", "└────────┘"];

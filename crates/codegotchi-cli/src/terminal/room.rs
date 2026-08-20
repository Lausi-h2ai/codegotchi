use codegotchi_domain::{FoodKind, PetBehavior, SimulationSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
};
use uuid::Uuid;

use super::behavior::{
    PetPose, PresentationActivity, PresentationFrame, has_authoritative_nap, presentation_activity,
};
use super::sprites::{draw_sprite_with_palette, pet_sprite, pet_sprite_compact};
use super::theme::{ResolvedPalette, SemanticTone, TerminalThemePreset};

const FULL_ROOM_HEIGHT: u16 = 14;
const COMPACT_ROOM_HEIGHT: u16 = 7;

/// Presentation-only sky state for the Full room window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoomAmbience {
    /// Bright skyline/window treatment.
    #[default]
    Day,
    /// Dark skyline/window treatment.
    Night,
}

/// Resolved presentation inputs shared by every room draw site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomRenderOptions {
    palette: ResolvedPalette,
    ambience: RoomAmbience,
}

impl Default for RoomRenderOptions {
    fn default() -> Self {
        Self::for_theme(TerminalThemePreset::Auto, RoomAmbience::Day)
    }
}

impl RoomRenderOptions {
    /// Creates options from a named terminal theme and presentation ambience.
    #[must_use]
    pub fn for_theme(theme: TerminalThemePreset, ambience: RoomAmbience) -> Self {
        Self {
            palette: theme.resolve(),
            ambience,
        }
    }

    /// Creates options from an already-resolved palette.
    #[must_use]
    pub const fn new(palette: ResolvedPalette, ambience: RoomAmbience) -> Self {
        Self { palette, ambience }
    }

    /// Returns the resolved style mapping used by the room and sprites.
    #[must_use]
    pub const fn palette(self) -> ResolvedPalette {
        self.palette
    }

    /// Returns the presentation-only Full-room ambience.
    #[must_use]
    pub const fn ambience(self) -> RoomAmbience {
        self.ambience
    }
}

/// One draggable food source rendered from authoritative inventory counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoodSource {
    pub rect: Rect,
    pub food_id: &'static str,
    pub count: u32,
}

/// Stable interactive regions of the room. Rendering and mouse hit testing
/// share this geometry so affordances always correspond to drawn objects.
#[derive(Clone, Debug)]
pub struct RoomGeometry {
    /// The pet sprite region; the petting and food-drop target.
    pub pet: Rect,
    /// The bed region; a click submits an authoritative nap.
    pub bed: Option<Rect>,
    /// Stocked draggable food sources with authoritative counts.
    pub food_sources: Vec<FoodSource>,
    /// Authoritative poop objects with their click regions.
    pub poops: Vec<(Uuid, Rect)>,
    /// True when the room uses the Minimal layout.
    pub minimal: bool,
}

impl RoomGeometry {
    /// Returns the stocked food id under the pointer, if any.
    #[must_use]
    pub fn food_hit(&self, point: Position) -> Option<&'static str> {
        self.food_sources
            .iter()
            .find(|source| source.rect.contains(point))
            .map(|source| source.food_id)
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
    room_geometry_with_frame(area, snapshot, &PresentationFrame::default())
}

/// Computes the interactive geometry for the current room rectangle,
/// authoritative snapshot, and presentation frame. The pet hitbox follows the
/// frame's wander offset so mouse care stays aligned with the visible pet.
#[must_use]
pub fn room_geometry_with_frame(
    area: Rect,
    snapshot: &SimulationSnapshot,
    frame: &PresentationFrame,
) -> RoomGeometry {
    if area.is_empty() {
        return RoomGeometry {
            pet: Rect::ZERO,
            bed: None,
            food_sources: Vec::new(),
            poops: Vec::new(),
            minimal: false,
        };
    }
    match room_mode(area.height) {
        RoomMode::Full => full_geometry(area, snapshot, frame.offset),
        RoomMode::Compact => compact_geometry(area, snapshot, frame.offset),
        RoomMode::Minimal => minimal_geometry(area, snapshot),
    }
}

/// Renders the authoritative CodeGotchi room projection into `area`.
///
/// The room is a pure projection of the snapshot: the pet never feeds, cleans,
/// naps, or pets itself here, and bed sleep is used only when
/// [`has_authoritative_nap`] says so. Rendering is deterministic so the same
/// snapshot always produces the same cells.
pub fn render_room(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    frame: &PresentationFrame,
    drag: Option<(&str, Position)>,
) {
    render_room_with_options(
        area,
        buffer,
        snapshot,
        frame,
        RoomRenderOptions::default(),
        drag,
    );
}

/// Renders the room using one resolved palette and presentation ambience.
///
/// All status, furniture, sprite, and affordance draw sites consume the same
/// options. This prevents a selected terminal theme from being partially
/// applied when one private renderer is updated later.
pub fn render_room_with_options(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    frame: &PresentationFrame,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
) {
    if area.is_empty() {
        return;
    }
    reset_area(area, buffer, options.palette());

    let activity = presentation_activity(snapshot);
    let napping = has_authoritative_nap(snapshot);
    let geometry = room_geometry_with_frame(area, snapshot, frame);

    match room_mode(area.height) {
        RoomMode::Full => render_full(
            area, buffer, snapshot, activity, napping, frame, &geometry, options, drag,
        ),
        RoomMode::Compact => render_compact(
            area, buffer, snapshot, activity, napping, frame, &geometry, options, drag,
        ),
        RoomMode::Minimal => {
            render_minimal(area, buffer, snapshot, napping, options, drag, &geometry)
        }
    }
}

/// Convenience entry point for callers that already resolved a palette.
pub fn render_room_with_palette(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    frame: &PresentationFrame,
    palette: ResolvedPalette,
    ambience: RoomAmbience,
    drag: Option<(&str, Position)>,
) {
    render_room_with_options(
        area,
        buffer,
        snapshot,
        frame,
        RoomRenderOptions::new(palette, ambience),
        drag,
    );
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

fn full_geometry(area: Rect, snapshot: &SimulationSnapshot, offset: (i16, i16)) -> RoomGeometry {
    if area.width >= 80 {
        let pet = offset_rect(
            Rect::new(area.x.saturating_add(50), area.y.saturating_add(4), 12, 6),
            offset,
            area,
        );
        let bed = Rect::new(area.x.saturating_add(97), area.y.saturating_add(5), 23, 7);
        let food_sources = wide_food_sources(area, snapshot, 32, 8, 12, 5, 13);
        let poops = wide_poop_slots(area, snapshot, 82, 8, 3, 5, 4, 5);
        return RoomGeometry {
            pet,
            bed: Some(bed),
            food_sources,
            poops,
            minimal: false,
        };
    }
    let pet_x = area.width.saturating_sub(52).max(2);
    let pet = offset_rect(
        Rect::new(
            area.x.saturating_add(pet_x),
            area.y.saturating_add(4),
            (area.width.saturating_sub(pet_x)).min(14),
            6,
        ),
        offset,
        area,
    );
    // Keep the compact fixture's legacy 40-column bed anchor while leaving
    // enough right margin in the production-width room for the whole sprite.
    let bed_x = if area.width <= 64 {
        area.width.saturating_sub(16).max(4)
    } else {
        area.width.saturating_sub(28).max(4)
    };
    let bed = Rect::new(
        area.x.saturating_add(bed_x),
        area.y.saturating_add(8),
        (area.width.saturating_sub(bed_x)).min(12),
        4,
    );
    let food_sources = food_sources(area, snapshot, 2, 11, 15, 16);
    let poops = poop_slots(area, snapshot, 16, 12, 3);
    RoomGeometry {
        pet,
        bed: Some(bed),
        food_sources,
        poops,
        minimal: false,
    }
}

fn compact_geometry(area: Rect, snapshot: &SimulationSnapshot, offset: (i16, i16)) -> RoomGeometry {
    if area.width >= 80 {
        let pet = offset_rect(
            Rect::new(area.x.saturating_add(2), area.y.saturating_add(2), 9, 4),
            offset,
            area,
        );
        let bed = Rect::new(area.x.saturating_add(91), area.y.saturating_add(2), 23, 4);
        let food_sources = wide_food_sources(area, snapshot, 34, 3, 10, 4, 11);
        let poops = wide_poop_slots(area, snapshot, 78, 3, 2, 5, 4, 6);
        return RoomGeometry {
            pet,
            bed: Some(bed),
            food_sources,
            poops,
            minimal: false,
        };
    }
    let pet_x = area.width.saturating_sub(52).max(2);
    let pet = offset_rect(
        Rect::new(
            area.x.saturating_add(pet_x),
            area.y.saturating_add(1),
            (area.width.saturating_sub(pet_x)).min(10),
            3,
        ),
        offset,
        area,
    );
    let bed_x = area.width.saturating_sub(28).max(4);
    let bed = Rect::new(
        area.x.saturating_add(bed_x),
        area.y.saturating_add(3),
        (area.width.saturating_sub(bed_x)).min(10),
        2,
    );
    let food_sources = food_sources(area, snapshot, 2, 5, 8, 10);
    let poops = poop_slots(area, snapshot, 44, 5, 2);
    RoomGeometry {
        pet,
        bed: Some(bed),
        food_sources,
        poops,
        minimal: false,
    }
}

const MINIMAL_TARGET_ROW: u16 = 1;
const MINIMAL_FOOD_X: u16 = 7;
const MINIMAL_BED_X: u16 = 19;
const MINIMAL_POOP_X: u16 = 27;
const MINIMAL_POOP_STEP: u16 = 8;
const MINIMAL_BED_LABEL: &str = "[BED]";
const MINIMAL_POOP_LABEL: &str = "[POOP]";
const MINIMAL_AFF_X: u16 = 52;

fn minimal_food_label(count: u32) -> String {
    format!("[FOOD x{count}]")
}

fn minimal_poop_x(index: usize) -> u16 {
    MINIMAL_POOP_X.saturating_add(index as u16 * MINIMAL_POOP_STEP)
}

fn label_width(label: &str) -> u16 {
    u16::try_from(label.chars().count()).unwrap_or(u16::MAX)
}

fn minimal_geometry(area: Rect, snapshot: &SimulationSnapshot) -> RoomGeometry {
    let pet = Rect::new(area.x, area.y.saturating_add(2), 7, 1);
    let food_label = minimal_food_label(snapshot.inventory.count(FoodKind::Kibble));
    let food_sources = vec![FoodSource {
        rect: Rect::new(
            area.x.saturating_add(MINIMAL_FOOD_X),
            area.y.saturating_add(MINIMAL_TARGET_ROW),
            label_width(&food_label),
            1,
        ),
        food_id: FoodKind::Kibble.id(),
        count: snapshot.inventory.count(FoodKind::Kibble),
    }];
    let bed = Rect::new(
        area.x.saturating_add(MINIMAL_BED_X),
        area.y.saturating_add(MINIMAL_TARGET_ROW),
        label_width(MINIMAL_BED_LABEL),
        1,
    );
    let poops = snapshot
        .pending_poops
        .iter()
        .take(2)
        .enumerate()
        .map(|(index, poop)| {
            (
                poop.id(),
                Rect::new(
                    area.x.saturating_add(minimal_poop_x(index)),
                    area.y.saturating_add(MINIMAL_TARGET_ROW),
                    label_width(MINIMAL_POOP_LABEL),
                    1,
                ),
            )
        })
        .collect();
    RoomGeometry {
        pet,
        bed: Some(bed),
        food_sources,
        poops,
        minimal: true,
    }
}

/// Builds one drag source per stocked food kind (kibble, treat, fruit,
/// energy drink) in a horizontal row starting at `(x, y)`. Only stocked
/// foods (count > 0) become drag sources.
fn food_sources(
    area: Rect,
    snapshot: &SimulationSnapshot,
    x: u16,
    y: u16,
    width: u16,
    spacing: u16,
) -> Vec<FoodSource> {
    [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(index, food)| {
        let count = snapshot.inventory.count(food);
        if count == 0 {
            return None;
        }
        Some(FoodSource {
            rect: Rect::new(
                area.x.saturating_add(x + index as u16 * spacing),
                area.y.saturating_add(y),
                width,
                1,
            ),
            food_id: food.id(),
            count,
        })
    })
    .collect()
}

/// Builds wide-layout food hit regions around the entire bowl, label, and
/// count projection. The narrow renderer keeps its historical anchor because
/// its input tests and text-only projection intentionally use that row.
fn wide_food_sources(
    area: Rect,
    snapshot: &SimulationSnapshot,
    x: u16,
    top: u16,
    width: u16,
    height: u16,
    spacing: u16,
) -> Vec<FoodSource> {
    [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(index, food)| {
        let count = snapshot.inventory.count(food);
        (count > 0).then(|| FoodSource {
            rect: Rect::new(
                area.x.saturating_add(x + index as u16 * spacing),
                area.y.saturating_add(top),
                width,
                height,
            ),
            food_id: food.id(),
            count,
        })
    })
    .collect()
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

/// Builds wide-layout poop hit regions around the four-row rendered object.
fn wide_poop_slots(
    area: Rect,
    snapshot: &SimulationSnapshot,
    x: u16,
    top: u16,
    limit: usize,
    width: u16,
    height: u16,
    spacing: u16,
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
                    area.x.saturating_add(x + index as u16 * spacing),
                    area.y.saturating_add(top),
                    width,
                    height,
                ),
            )
        })
        .collect()
}

/// Applies a presentation wander offset to a hit rectangle, clamping it inside
/// the room area.
fn offset_rect(rect: Rect, offset: (i16, i16), area: Rect) -> Rect {
    if area.is_empty() {
        return rect;
    }
    let max_x =
        i64::from(area.x) + i64::from(area.width.saturating_sub(rect.width.min(area.width)));
    let max_y =
        i64::from(area.y) + i64::from(area.height.saturating_sub(rect.height.min(area.height)));
    let x = (i64::from(rect.x) + i64::from(offset.0)).clamp(i64::from(area.x), max_x);
    let y = (i64::from(rect.y) + i64::from(offset.1)).clamp(i64::from(area.y), max_y);
    Rect::new(x as u16, y as u16, rect.width, rect.height)
}

fn reset_area(area: Rect, buffer: &mut Buffer, palette: ResolvedPalette) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buffer.cell_mut(Position { x, y }) {
                cell.set_symbol(" ")
                    .set_style(palette.cell_style(SemanticTone::Tone0));
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

// Rendering a projection needs the area, snapshot, activity projection, nap
// state, presentation frame, geometry, and drag state; a context struct would
// add noise for a private renderer.
#[allow(clippy::too_many_arguments)]
fn render_full(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
    frame: &PresentationFrame,
    geometry: &RoomGeometry,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
) {
    if area.width >= 80 {
        render_full_wide(
            area, buffer, snapshot, activity, napping, frame, geometry, options, drag,
        );
        return;
    }
    let palette = options.palette();
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
            palette.cell_style(SemanticTone::Tone3),
        );
    }
    put_text(
        area,
        buffer,
        20,
        0,
        &format!("{:?}", activity),
        palette.cell_style(SemanticTone::Tone2),
    );
    render_furniture_full(area, buffer, palette, options.ambience());

    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        put_sprite(area, buffer, &BED_FULL, bed_x, bed_y, palette);
    }
    if napping {
        if let Some(bed) = geometry.bed {
            let bed_x = bed.x.saturating_sub(area.x);
            let bed_y = bed.y.saturating_sub(area.y);
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite(PetPose::Sleep),
                bed_x,
                bed_y,
                palette,
            );
            put_text(
                area,
                buffer,
                bed_x.saturating_add(2),
                bed_y.saturating_sub(1),
                "z z z",
                palette.cell_style(SemanticTone::Tone2),
            );
        }
    } else {
        let pet_x = geometry.pet.x.saturating_sub(area.x);
        let pet_y = geometry.pet.y.saturating_sub(area.y);
        if snapshot.behavior == PetBehavior::Sleeping {
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite(PetPose::Doze),
                pet_x,
                pet_y,
                palette,
            );
            put_text(
                area,
                buffer,
                pet_x.saturating_add(8),
                pet_y.saturating_sub(1),
                "z",
                palette.cell_style(SemanticTone::Tone2),
            );
        } else {
            draw_sprite_with_palette(area, buffer, pet_sprite(frame.pose), pet_x, pet_y, palette);
        }
    }

    render_food_sources(area, buffer, geometry, true, palette);
    render_pending_demands_full(area, buffer, snapshot, palette);
    for (_, rect) in &geometry.poops {
        let x = rect.x.saturating_sub(area.x);
        let y = rect.y.saturating_sub(area.y);
        put(
            area,
            buffer,
            x,
            y,
            "●",
            palette.cell_style(SemanticTone::Tone1),
        );
    }
    render_drag_ghost(area, buffer, drag, palette);
}

#[allow(clippy::too_many_arguments)]
fn render_compact(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
    frame: &PresentationFrame,
    geometry: &RoomGeometry,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
) {
    if area.width >= 80 {
        render_compact_wide(
            area, buffer, snapshot, activity, napping, frame, geometry, options, drag,
        );
        return;
    }
    let palette = options.palette();
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
        palette.cell_style(SemanticTone::Tone3),
    );
    let (affection, snack) = demand_counts(snapshot);
    let activity_line = if affection > 0 || snack > 0 {
        format!("{:?}  A{affection} S{snack}", activity)
    } else {
        format!("{:?}", activity)
    };
    put_line(
        area,
        buffer,
        1,
        &activity_line,
        palette.cell_style(SemanticTone::Tone2),
    );
    render_furniture_compact(area, buffer, palette);

    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        put_sprite(area, buffer, &BED_COMPACT, bed_x, bed_y, palette);
    }
    if napping {
        if let Some(bed) = geometry.bed {
            let bed_x = bed.x.saturating_sub(area.x);
            let bed_y = bed.y.saturating_sub(area.y);
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite_compact(PetPose::Sleep),
                bed_x,
                bed_y,
                palette,
            );
            put_text(
                area,
                buffer,
                bed_x.saturating_add(1),
                bed_y.saturating_sub(1),
                "z z z",
                palette.cell_style(SemanticTone::Tone2),
            );
        }
    } else {
        let pet_x = geometry.pet.x.saturating_sub(area.x);
        let pet_y = geometry.pet.y.saturating_sub(area.y);
        if snapshot.behavior == PetBehavior::Sleeping {
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite_compact(PetPose::Doze),
                pet_x,
                pet_y,
                palette,
            );
            put_text(
                area,
                buffer,
                pet_x.saturating_add(6),
                pet_y.saturating_sub(1),
                "z",
                palette.cell_style(SemanticTone::Tone2),
            );
        } else {
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite_compact(frame.pose),
                pet_x,
                pet_y,
                palette,
            );
        }
    }

    render_food_sources(area, buffer, geometry, false, palette);
    for (_, rect) in &geometry.poops {
        let x = rect.x.saturating_sub(area.x);
        let y = rect.y.saturating_sub(area.y);
        put(
            area,
            buffer,
            x,
            y,
            "●",
            palette.cell_style(SemanticTone::Tone1),
        );
    }
    render_drag_ghost(area, buffer, drag, palette);
}

/// Full production composition. The room has only fourteen terminal rows, so
/// decoration is kept in the upper eleven rows and the four care meters share
/// a deliberate bottom status strip. Every visible target has an adjacent
/// semantic label so the same geometry remains discoverable with a mouse.
#[allow(clippy::too_many_arguments)]
fn render_full_wide(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
    frame: &PresentationFrame,
    geometry: &RoomGeometry,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
) {
    let palette = options.palette();
    put_text(
        area,
        buffer,
        1,
        0,
        "CODEGOTCHI ROOM",
        palette.cell_style(SemanticTone::Tone3),
    );
    put_text(
        area,
        buffer,
        20,
        0,
        &format!("{:?}", activity),
        palette.cell_style(SemanticTone::Tone2),
    );
    render_furniture_full_wide(area, buffer, palette, options.ambience());

    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        put_sprite(area, buffer, &BED_WIDE, bed_x, bed_y, palette);
        if napping {
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite(PetPose::Sleep),
                bed_x.saturating_add(6),
                bed_y.saturating_add(1),
                palette,
            );
            put_text(
                area,
                buffer,
                bed_x.saturating_add(9),
                bed_y.saturating_sub(1),
                "z z z",
                palette.cell_style(SemanticTone::Tone2),
            );
        }
        put_text(
            area,
            buffer,
            bed_x.saturating_add(5),
            bed_y.saturating_add(6),
            "BED",
            palette.cell_style(SemanticTone::Tone3),
        );
    }

    if !napping {
        let pet_x = geometry.pet.x.saturating_sub(area.x);
        let pet_y = geometry.pet.y.saturating_sub(area.y);
        if snapshot.behavior == PetBehavior::Sleeping {
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite(PetPose::Doze),
                pet_x,
                pet_y,
                palette,
            );
            put_text(
                area,
                buffer,
                pet_x.saturating_add(9),
                pet_y.saturating_sub(1),
                "z",
                palette.cell_style(SemanticTone::Tone2),
            );
        } else {
            draw_sprite_with_palette(area, buffer, pet_sprite(frame.pose), pet_x, pet_y, palette);
        }
        put_text(
            area,
            buffer,
            pet_x.saturating_add(3),
            pet_y.saturating_add(5),
            "PET",
            palette.cell_style(SemanticTone::Tone2),
        );
    }

    render_food_sources_wide(area, buffer, geometry, palette);
    render_poops_wide(area, buffer, geometry, palette);
    render_pending_demands_wide(area, buffer, snapshot, palette);
    render_full_status_strip(area, buffer, snapshot, palette);
    render_drag_ghost(area, buffer, drag, palette);
}

/// Compact keeps the mascot and every care target while letting most room
/// decoration collapse into a small vignette between the status rows.
#[allow(clippy::too_many_arguments)]
fn render_compact_wide(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
    frame: &PresentationFrame,
    geometry: &RoomGeometry,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
) {
    let palette = options.palette();
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
        palette.cell_style(SemanticTone::Tone3),
    );
    let (affection, snack) = demand_counts(snapshot);
    let activity_line = if affection > 0 || snack > 0 {
        format!("PET {:?}  A{affection} S{snack}", activity)
    } else {
        format!("PET {:?}", activity)
    };
    put_line(
        area,
        buffer,
        1,
        &activity_line,
        palette.cell_style(SemanticTone::Tone2),
    );

    // The tiny window and plant preserve the room's identity without taking
    // the pet's left-side priority in the compact hierarchy.
    put_sprite(area, buffer, &WINDOW_COMPACT, 32, 2, palette);
    put_sprite(area, buffer, &PLANTS_COMPACT, 46, 4, palette);

    let pet_x = geometry.pet.x.saturating_sub(area.x);
    let pet_y = geometry.pet.y.saturating_sub(area.y);
    if napping {
        if let Some(bed) = geometry.bed {
            let bed_x = bed.x.saturating_sub(area.x);
            let bed_y = bed.y.saturating_sub(area.y);
            put_sprite(area, buffer, &BED_COMPACT_WIDE, bed_x, bed_y, palette);
            draw_sprite_with_palette(
                area,
                buffer,
                pet_sprite_compact(PetPose::Sleep),
                bed_x.saturating_add(7),
                bed_y,
                palette,
            );
            put_text(
                area,
                buffer,
                bed_x.saturating_add(9),
                bed_y.saturating_sub(1),
                "z z",
                palette.cell_style(SemanticTone::Tone2),
            );
        }
    } else if snapshot.behavior == PetBehavior::Sleeping {
        draw_sprite_with_palette(
            area,
            buffer,
            pet_sprite_compact(PetPose::Doze),
            pet_x,
            pet_y,
            palette,
        );
        put_text(
            area,
            buffer,
            pet_x.saturating_add(7),
            pet_y.saturating_sub(1),
            "z",
            palette.cell_style(SemanticTone::Tone2),
        );
    } else {
        draw_sprite_with_palette(
            area,
            buffer,
            pet_sprite_compact(frame.pose),
            pet_x,
            pet_y,
            palette,
        );
    }
    put_text(
        area,
        buffer,
        pet_x.saturating_add(1),
        5,
        "PET",
        palette.cell_style(SemanticTone::Tone2),
    );

    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        if !napping {
            put_sprite(area, buffer, &BED_COMPACT_WIDE, bed_x, bed_y, palette);
        }
        put_text(
            area,
            buffer,
            bed_x.saturating_add(8),
            6,
            "BED",
            palette.cell_style(SemanticTone::Tone3),
        );
    }
    render_food_sources_compact_wide(area, buffer, geometry, palette);
    render_poops_compact_wide(area, buffer, geometry, palette);
    render_drag_ghost(area, buffer, drag, palette);
}

fn render_minimal(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    napping: bool,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
    geometry: &RoomGeometry,
) {
    let palette = options.palette();
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
    put_line(
        area,
        buffer,
        0,
        &line,
        palette.cell_style(SemanticTone::Tone3),
    );

    let stocked = snapshot.inventory.count(FoodKind::Kibble);
    let (affection, snack) = demand_counts(snapshot);
    put_text(
        area,
        buffer,
        0,
        MINIMAL_TARGET_ROW,
        "◉ PET",
        palette.cell_style(SemanticTone::Tone2),
    );
    let food = geometry
        .food_sources
        .first()
        .expect("Minimal always exposes the starter food target");
    put_text(
        area,
        buffer,
        food.rect.x.saturating_sub(area.x),
        MINIMAL_TARGET_ROW,
        &minimal_food_label(stocked),
        palette.cell_style(SemanticTone::Tone2),
    );
    if let Some(bed) = geometry.bed {
        put_text(
            area,
            buffer,
            bed.x.saturating_sub(area.x),
            MINIMAL_TARGET_ROW,
            MINIMAL_BED_LABEL,
            palette.cell_style(SemanticTone::Tone2),
        );
    }
    if geometry.poops.is_empty() {
        put_text(
            area,
            buffer,
            MINIMAL_POOP_X,
            MINIMAL_TARGET_ROW,
            MINIMAL_POOP_LABEL,
            palette.cell_style(SemanticTone::Tone1),
        );
    } else {
        for (_, poop) in &geometry.poops {
            put_text(
                area,
                buffer,
                poop.x.saturating_sub(area.x),
                MINIMAL_TARGET_ROW,
                MINIMAL_POOP_LABEL,
                palette.cell_style(SemanticTone::Tone2),
            );
        }
    }
    put_text(
        area,
        buffer,
        MINIMAL_AFF_X,
        MINIMAL_TARGET_ROW,
        &format!("AFF x{affection}"),
        palette.cell_style(SemanticTone::Tone2),
    );
    if snack > 0 {
        put_text(
            area,
            buffer,
            MINIMAL_AFF_X.saturating_add(8),
            MINIMAL_TARGET_ROW,
            "SNACK",
            palette.cell_style(SemanticTone::Tone2),
        );
    }
    put_text(
        area,
        buffer,
        0,
        2,
        "(=^.^=) PET   [FOOD] [BED] [POOP]",
        palette.cell_style(SemanticTone::Tone3),
    );
    render_drag_ghost(area, buffer, drag, palette);
}

/// Short terminal labels for the authoritative food kinds.
fn food_label(food_id: &str) -> &'static str {
    match food_id {
        "kibble" => "KIB",
        "treat" => "TRT",
        "fruit" => "FRT",
        "energy_drink" => "ENE",
        _ => "???",
    }
}

fn render_full_status_strip(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    palette: ResolvedPalette,
) {
    let needs = snapshot.needs;
    let statuses = [
        ("Hunger", need_bar(needs.hunger())),
        ("Energy", need_bar(needs.energy())),
        ("Happy ", need_bar(needs.happiness())),
        ("Clean ", need_bar(needs.cleanliness())),
    ];
    for (index, (label, bar)) in statuses.iter().enumerate() {
        let x = u16::try_from(index).unwrap_or(u16::MAX).saturating_mul(30);
        put_text(
            area,
            buffer,
            x,
            area.height.saturating_sub(1),
            &format!("{label} {bar}"),
            palette.cell_style(SemanticTone::Tone3),
        );
        if index < statuses.len().saturating_sub(1) {
            put_text(
                area,
                buffer,
                x.saturating_add(27),
                area.height.saturating_sub(1),
                "│",
                palette.cell_style(SemanticTone::Tone1),
            );
        }
    }
}

fn render_pending_demands_wide(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    palette: ResolvedPalette,
) {
    let (affection, snack) = demand_counts(snapshot);
    let mut line = String::from("Care:");
    if affection > 0 {
        line.push_str(&format!("  Affection x{affection}"));
    }
    if snack > 0 {
        line.push_str(&format!("  Snack x{snack}"));
    }
    if affection == 0 && snack == 0 {
        line.push_str("  PET when the heart appears");
    }
    put_text(
        area,
        buffer,
        42,
        area.height.saturating_sub(2),
        &line,
        palette.cell_style(SemanticTone::Tone2),
    );
}

fn render_food_sources_wide(
    area: Rect,
    buffer: &mut Buffer,
    geometry: &RoomGeometry,
    palette: ResolvedPalette,
) {
    for source in &geometry.food_sources {
        let x = source.rect.x.saturating_sub(area.x);
        let y = source.rect.y.saturating_sub(area.y);
        put_sprite(area, buffer, &FOOD_BOWL, x, y, palette);
        put_text(
            area,
            buffer,
            x,
            y.saturating_add(3),
            &format!("FOOD {} x{}", food_label(source.food_id), source.count),
            palette.cell_style(SemanticTone::Tone3),
        );
    }
}

fn render_poops_wide(
    area: Rect,
    buffer: &mut Buffer,
    geometry: &RoomGeometry,
    palette: ResolvedPalette,
) {
    for (index, (_, rect)) in geometry.poops.iter().enumerate() {
        let x = rect.x.saturating_sub(area.x);
        let y = rect.y.saturating_sub(area.y);
        put_sprite(area, buffer, &POOP_OBJECT, x, y, palette);
        if index == 0 {
            put_text(
                area,
                buffer,
                x,
                y.saturating_add(3),
                "POOP",
                palette.cell_style(SemanticTone::Tone3),
            );
        }
    }
}

fn render_food_sources_compact_wide(
    area: Rect,
    buffer: &mut Buffer,
    geometry: &RoomGeometry,
    palette: ResolvedPalette,
) {
    let Some(first) = geometry.food_sources.first() else {
        return;
    };
    let x = first.rect.x.saturating_sub(area.x);
    let y = first.rect.y.saturating_sub(area.y);
    put_sprite(area, buffer, &FOOD_BOWL_COMPACT, x, y, palette);
    put_text(
        area,
        buffer,
        x,
        y.saturating_add(3),
        &format!("FOOD x{}", first.count),
        palette.cell_style(SemanticTone::Tone3),
    );
    // Other stocked foods remain visible as small tray dots/counts instead
    // of displacing the pet or bed from the compact priority order.
    for (index, source) in geometry.food_sources.iter().skip(1).enumerate() {
        let x = source.rect.x.saturating_sub(area.x);
        let y = source.rect.y.saturating_sub(area.y);
        put_sprite(area, buffer, &FOOD_BOWL_COMPACT, x, y, palette);
        put_text(
            area,
            buffer,
            x,
            y.saturating_add(2),
            &format!("{}x{}", food_label(source.food_id), source.count),
            palette.cell_style(SemanticTone::Tone2),
        );
        if index > 8 {
            break;
        }
    }
}

fn render_poops_compact_wide(
    area: Rect,
    buffer: &mut Buffer,
    geometry: &RoomGeometry,
    palette: ResolvedPalette,
) {
    let Some((_, rect)) = geometry.poops.first() else {
        return;
    };
    let x = rect.x.saturating_sub(area.x);
    let y = rect.y.saturating_sub(area.y);
    put_sprite(area, buffer, &POOP_OBJECT_COMPACT, x, y, palette);
    put_text(
        area,
        buffer,
        x,
        y.saturating_add(3),
        "POOP",
        palette.cell_style(SemanticTone::Tone3),
    );
}

/// Renders every stocked drag source with its authoritative count.
fn render_food_sources(
    area: Rect,
    buffer: &mut Buffer,
    geometry: &RoomGeometry,
    verbose: bool,
    palette: ResolvedPalette,
) {
    for source in &geometry.food_sources {
        let x = source.rect.x.saturating_sub(area.x);
        let y = source.rect.y.saturating_sub(area.y);
        let label = if verbose {
            format!("{} x{} drag", food_label(source.food_id), source.count)
        } else {
            format!("{} x{}", food_label(source.food_id), source.count)
        };
        put_text(
            area,
            buffer,
            x,
            y,
            &label,
            palette.cell_style(SemanticTone::Tone2),
        );
    }
}

/// `(affection, snack)` counts from authoritative pending demands.
fn demand_counts(snapshot: &SimulationSnapshot) -> (u32, u32) {
    snapshot
        .pending_demands
        .iter()
        .fold((0, 0), |(affection, snack), demand| match demand.kind() {
            codegotchi_domain::PetDemandKind::Affection => (affection + 1, snack),
            codegotchi_domain::PetDemandKind::Snack => (affection, snack + 1),
        })
}

/// Full layout projects pending affection/snack demands as care affordances.
fn render_pending_demands_full(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    palette: ResolvedPalette,
) {
    let (affection, snack) = demand_counts(snapshot);
    if affection == 0 && snack == 0 {
        return;
    }
    let mut line = String::new();
    if affection > 0 {
        line.push_str(&format!("Affection x{affection}  "));
    }
    if snack > 0 {
        line.push_str(&format!("Snack x{snack}  "));
    }
    put_text(
        area,
        buffer,
        2,
        area.height.saturating_sub(1),
        &line,
        palette.cell_style(SemanticTone::Tone3),
    );
}

/// Draws the dragged-food ghost at the current pointer cell (room-relative
/// clamp applied by the caller's coordinate subtraction).
fn render_drag_ghost(
    area: Rect,
    buffer: &mut Buffer,
    drag: Option<(&str, Position)>,
    palette: ResolvedPalette,
) {
    let Some((food_id, position)) = drag else {
        return;
    };
    let x = position.x.saturating_sub(area.x);
    let y = position.y.saturating_sub(area.y);
    if x >= area.width || y >= area.height {
        return;
    }
    put_text(
        area,
        buffer,
        x,
        y,
        food_label(food_id),
        palette.cell_style(SemanticTone::Tone3),
    );
}

fn put_sprite(
    area: Rect,
    buffer: &mut Buffer,
    sprite: &[&str],
    x: u16,
    y: u16,
    palette: ResolvedPalette,
) {
    for (row, line) in sprite.iter().enumerate() {
        let row = u16::try_from(row).unwrap_or(u16::MAX);
        for (offset, ch) in line.chars().enumerate() {
            let style = match ch {
                '█' | '▀' | '▄' => palette.cell_style(SemanticTone::Tone1),
                '┌' | '┐' | '└' | '┘' | '─' | '│' => {
                    palette.cell_style(SemanticTone::Tone2)
                }
                _ => palette.cell_style(SemanticTone::Tone3),
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

fn render_furniture_full_wide(
    area: Rect,
    buffer: &mut Buffer,
    palette: ResolvedPalette,
    ambience: RoomAmbience,
) {
    let window = match ambience {
        RoomAmbience::Day => &WINDOW_FULL_DAY,
        RoomAmbience::Night => &WINDOW_FULL_NIGHT,
    };
    // Window + curtains on the left wall.
    put_sprite(area, buffer, window, 22, 1, palette);
    put_text(
        area,
        buffer,
        20,
        5,
        "WINDOW",
        palette.cell_style(SemanticTone::Tone2),
    );

    // Shelf with books and a plant above the desk.
    put_sprite(area, buffer, &SHELF_FULL, 40, 1, palette);
    put_text(
        area,
        buffer,
        41,
        5,
        "SHELF",
        palette.cell_style(SemanticTone::Tone2),
    );

    // Wardrobe forms the room's tall center-right anchor.
    put_sprite(area, buffer, &WARDROBE_FULL, 62, 2, palette);

    // The desk remains left of the pet, with the original recognizable laptop
    // silhouette retained for the textual renderer contract.
    put_sprite(area, buffer, &DESK_FULL, 2, 6, palette);
    put_text(
        area,
        buffer,
        4,
        10,
        "DESK",
        palette.cell_style(SemanticTone::Tone2),
    );
    // A low floor line gives the roaming area a stable depth cue without
    // introducing a clipping-prone full-width border.
    for x in (0..area.width).step_by(4) {
        put(
            area,
            buffer,
            x,
            11,
            "·",
            palette.cell_style(SemanticTone::Tone1),
        );
    }
    put_sprite(area, buffer, &PLANTS_FULL, 2, 9, palette);
    put_sprite(area, buffer, &PLANT_ACCENT, 14, 8, palette);
}

/// Decorative bedroom furniture for the Full layout. Deterministic simple
/// silhouettes; VISUAL_FIDELITY_UNVERIFIED. Placement deliberately avoids the
/// status bars, pet home, bed, food tray, and poop slots.
fn render_furniture_full(
    area: Rect,
    buffer: &mut Buffer,
    palette: ResolvedPalette,
    ambience: RoomAmbience,
) {
    let window = match ambience {
        RoomAmbience::Day => &WINDOW_FULL_DAY,
        RoomAmbience::Night => &WINDOW_FULL_NIGHT,
    };
    put_sprite(area, buffer, window, 22, 1, palette);
    put_sprite(area, buffer, &SHELF_FULL, 40, 2, palette);
    put_sprite(area, buffer, &WARDROBE_FULL, 62, 3, palette);
    put_sprite(area, buffer, &DESK_FULL, 42, 7, palette);
    put_sprite(area, buffer, &PLANTS_FULL, 22, 7, palette);
}

/// Minimal decoration for the Compact layout: decoration disappears before
/// care functionality.
fn render_furniture_compact(area: Rect, buffer: &mut Buffer, palette: ResolvedPalette) {
    put_sprite(area, buffer, &WINDOW_COMPACT, 22, 0, palette);
    put_sprite(area, buffer, &PLANTS_COMPACT, 34, 4, palette);
}

fn need_bar(value: f32) -> String {
    // Domain needs are 0..100 (hunger is inverted: 0 = full, 100 = starving).
    let filled = ((value.clamp(0.0, 100.0) / 100.0) * 8.0).round() as usize;
    let mut bar = "█".repeat(filled);
    bar.push_str(&"░".repeat(8usize.saturating_sub(filled)));
    bar
}

fn need_percent(value: f32) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
}

const WINDOW_FULL_DAY: [&str; 4] = [
    "┌────────────┐",
    "│   ☀   ·    │",
    "│▄▄▄▄▄▄▄▄▄▄▄▄│",
    "└────────────┘",
];

const WINDOW_FULL_NIGHT: [&str; 4] = [
    "┌────────────┐",
    "│  ·  ☾  ·   │",
    "│  ·    ·   ·│",
    "└────────────┘",
];

const SHELF_FULL: [&str; 3] = ["┌─┬─────┬────┐", "│▄│▄▄▄▄▄│▄▄▄▄│", "└─┴─────┴────┘"];

const WARDROBE_FULL: [&str; 6] = [
    "┌──────────┐",
    "│┌──┐  ┌──┐│",
    "││  │  │  ││",
    "│└──┘  └──┘│",
    "│┌──┐  ┌──┐│",
    "└──────────┘",
];

const DESK_FULL: [&str; 4] = [
    "┌────────────────┐",
    "│    ▄▄▄▄▄▄▄▄    │",
    "│    ██████████  │",
    "└────────────────┘",
];

const PLANTS_FULL: [&str; 3] = ["  ▀  ", " ▀▀▀ ", "┌───┐"];

const WINDOW_COMPACT: [&str; 3] = ["┌──────────┐", "│▀▀▀▀▀▀▀▀▀▀│", "└──────────┘"];

const PLANTS_COMPACT: [&str; 3] = PLANTS_FULL;

const BED_FULL: [&str; 4] = [
    "┌──────────┐",
    "│▄▄▄▄▄▄▄▄▄▄│",
    "│▀▀▀▀▀▀▀▀▀▀│",
    "└──────────┘",
];

const BED_COMPACT: [&str; 2] = ["┌────────┐", "└────────┘"];

/// Wide bed vignette: headboard, pillow, star blanket, and footboard all fit
/// inside the 23-column hit region used by the Full production layout.
const BED_WIDE: [&str; 7] = [
    "┌─┐                 ┌─┐",
    "│ │  ┌───────────┐  │ │",
    "│ └──┤  pillow   ├──┘ │",
    "│    └───────────┘    │",
    "│  *  *  *  *  *  *   │",
    "└──────────────────────┘",
    "   └────── BED ─────┘   ",
];

const BED_COMPACT_WIDE: [&str; 4] = [
    "┌─┐              ┌─┐",
    "│ └──────────────┘ │",
    "│   *  *  *  *     │",
    "└─────── BED ──────┘",
];

const FOOD_BOWL: [&str; 4] = ["  ○  ", " ◒◒ ", "└──┘ ", "     "];
const FOOD_BOWL_COMPACT: [&str; 4] = [" ○ ", "◒◒ ", "└─┘ ", "    "];
const POOP_OBJECT: [&str; 4] = ["  ~ ", "  ~ ", " (●) ", "  ╰─ "];
const POOP_OBJECT_COMPACT: [&str; 4] = [" ~ ", "(●)", "╰─ ", "   "];
const PLANT_ACCENT: [&str; 4] = ["  ▄  ", " ▄█▄ ", "  █  ", " ▄▄▄ "];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sprite row must have the same width; a ragged row (like the old
    /// sleep face) corrupts the rendered glyph alignment.
    #[test]
    fn every_sprite_has_consistent_row_widths() {
        let sprites: &[&[&str]] = &[
            &BED_FULL,
            &BED_COMPACT,
            &WINDOW_FULL_DAY,
            &WINDOW_FULL_NIGHT,
            &SHELF_FULL,
            &WARDROBE_FULL,
            &DESK_FULL,
            &PLANTS_FULL,
            &WINDOW_COMPACT,
            &PLANTS_COMPACT,
        ];
        for sprite in sprites {
            let width = sprite[0].chars().count();
            for (index, row) in sprite.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    width,
                    "sprite row {index} has a different width than row 0"
                );
            }
        }
    }
}

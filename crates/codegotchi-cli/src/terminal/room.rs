use codegotchi_domain::{FoodKind, PetBehavior, PetNeeds, SimulationSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
};
use uuid::Uuid;

use super::behavior::{
    PetPose, PresentationActivity, PresentationFrame, has_authoritative_nap, presentation_activity,
};
use super::sprites::{
    draw_sprite_with_palette, pet_sprite, pet_sprite_compact, pet_sprite_minimal,
};
use super::theme::{ResolvedPalette, SemanticTone, TerminalThemePreset};

const FULL_ROOM_HEIGHT: u16 = 14;
const COMPACT_ROOM_HEIGHT: u16 = 7;
const COMPACT_PET_WIDTH: u16 = 12;
const COMPACT_PET_HEIGHT: u16 = 5;

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
        RoomMode::Minimal => render_minimal(
            area, buffer, snapshot, activity, napping, frame, options, drag, &geometry,
        ),
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
        let bed_x = area.width.saturating_sub(24);
        let pet_x = bed_x.saturating_sub(18);
        let pet = offset_rect(
            Rect::new(
                area.x.saturating_add(pet_x),
                area.y.saturating_add(4),
                18,
                7,
            ),
            offset,
            area,
        );
        let bed = Rect::new(
            area.x.saturating_add(bed_x),
            area.y.saturating_add(5),
            23,
            7,
        );
        let food_x = 2.min(area.width.saturating_sub(52));
        let compact_food = area.width < 100;
        let ultra_compact_food = area.width < 90;
        let food_sources = wide_food_sources(
            area,
            snapshot,
            food_x,
            8,
            compact_food,
            ultra_compact_food,
            false,
            compact_food,
        );
        let food_right = food_sources
            .last()
            .map_or(food_x, |source| source.rect.right().saturating_sub(area.x));
        let poop_x = if area.width >= 100 {
            pet_x.saturating_sub(24).max(food_right.saturating_add(2))
        } else {
            food_right.saturating_add(2)
        };
        let poop_limit = if area.width >= 100 { 3 } else { 1 };
        let poops = wide_poop_slots(area, snapshot, poop_x, 8, poop_limit, 7);
        return RoomGeometry {
            pet,
            bed: Some(bed),
            food_sources,
            poops,
            minimal: false,
        };
    }
    let bed_x = if area.width <= 64 {
        area.width.saturating_sub(20).max(4)
    } else {
        area.width.saturating_sub(28).max(4)
    };
    let pet_x = bed_x.saturating_sub(18);
    let pet = offset_rect(
        Rect::new(
            area.x.saturating_add(pet_x),
            area.y.saturating_add(4),
            (area.width.saturating_sub(pet_x)).min(18),
            7,
        ),
        offset,
        area,
    );
    let bed = Rect::new(
        area.x.saturating_add(bed_x),
        area.y.saturating_add(8),
        (area.width.saturating_sub(bed_x)).min(20),
        5,
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
            Rect::new(
                area.x.saturating_add(2),
                area.y.saturating_add(2),
                COMPACT_PET_WIDTH,
                COMPACT_PET_HEIGHT,
            ),
            offset,
            area,
        );
        let bed_x = area.width.saturating_sub(24);
        let bed = Rect::new(
            area.x.saturating_add(bed_x),
            area.y.saturating_add(2),
            23,
            COMPACT_PET_HEIGHT,
        );
        let ultra_compact_food = area.width < 90;
        let food_sources = wide_food_sources(
            area,
            snapshot,
            14,
            3,
            true,
            ultra_compact_food,
            true,
            area.width < 110,
        );
        let poop_width = wide_poop_target_width();
        let poop_limit = if area.width >= 100 {
            3_u16
        } else if area.width >= 90 {
            2_u16
        } else {
            1_u16
        };
        let latest_poop_start = bed
            .x
            .saturating_sub(area.x)
            .saturating_sub(poop_width)
            .saturating_sub(poop_limit.saturating_sub(1).saturating_mul(7));
        let poop_x = poop_anchor_after_food(area, &food_sources, 52)
            .min(latest_poop_start)
            .max(12);
        let poops = wide_poop_slots(area, snapshot, poop_x, 3, usize::from(poop_limit), 7);
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
            area.y.saturating_add(2),
            (area.width.saturating_sub(pet_x)).min(COMPACT_PET_WIDTH),
            COMPACT_PET_HEIGHT.min(area.height.saturating_sub(2)),
        ),
        offset,
        area,
    );
    let bed_x = area.width.saturating_sub(28).max(4);
    let bed = Rect::new(
        area.x.saturating_add(bed_x),
        area.y.saturating_add(2),
        (area.width.saturating_sub(bed_x)).min(10),
        COMPACT_PET_HEIGHT.min(area.height.saturating_sub(2)),
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
const MINIMAL_TARGET_GAP: u16 = 1;
const MINIMAL_BED_LABEL: &str = "[BED]";
const MINIMAL_POOP_LABEL: &str = "[POOP]";
const MINIMAL_PET_WIDTH: u16 = 9;
const MINIMAL_PET_HEIGHT: u16 = 3;
const MINIMAL_MAX_POOPS: usize = 2;

fn minimal_food_label(count: u32) -> String {
    format!("[FOOD x{count}]")
}

fn first_stocked_food(snapshot: &SimulationSnapshot) -> Option<(FoodKind, u32)> {
    [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ]
    .into_iter()
    .find_map(|food| {
        let count = snapshot.inventory.count(food);
        (count > 0).then_some((food, count))
    })
}

fn label_width(label: &str) -> u16 {
    u16::try_from(label.chars().count()).unwrap_or(u16::MAX)
}

#[derive(Clone, Copy)]
enum MinimalLabelMode {
    Full,
    Compact,
    Tiny,
}

#[derive(Clone, Debug)]
struct MinimalTargetLayout {
    pet: Rect,
    food: Option<(FoodKind, u32, Rect, String)>,
    disabled_food: Option<(u16, String)>,
    bed: Option<(Rect, &'static str)>,
    poops: Vec<(Uuid, Rect, &'static str)>,
}

fn minimal_count_label(count: u32) -> String {
    compact_food_count(count)
}

fn minimal_food_label_for_mode(count: u32, mode: MinimalLabelMode) -> String {
    match mode {
        MinimalLabelMode::Full => minimal_food_label(count),
        MinimalLabelMode::Compact => format!("FOOD{}", minimal_count_label(count)),
        MinimalLabelMode::Tiny => format!("F{}", minimal_count_label(count)),
    }
}

fn minimal_disabled_food_label(mode: MinimalLabelMode) -> &'static str {
    match mode {
        MinimalLabelMode::Full => "[FOOD none]",
        MinimalLabelMode::Compact => "FOOD none",
        MinimalLabelMode::Tiny => "F-",
    }
}

fn minimal_bed_label_for_mode(mode: MinimalLabelMode) -> &'static str {
    match mode {
        MinimalLabelMode::Full => MINIMAL_BED_LABEL,
        MinimalLabelMode::Compact => "BED",
        MinimalLabelMode::Tiny => "B",
    }
}

fn minimal_poop_label_for_mode(mode: MinimalLabelMode) -> &'static str {
    match mode {
        MinimalLabelMode::Full => MINIMAL_POOP_LABEL,
        MinimalLabelMode::Compact => "POOP",
        MinimalLabelMode::Tiny => "P",
    }
}

fn minimal_layout_for_mode(
    area: Rect,
    snapshot: &SimulationSnapshot,
    mode: MinimalLabelMode,
) -> MinimalTargetLayout {
    let pet_width = MINIMAL_PET_WIDTH.min(area.width);
    let pet = Rect::new(
        area.x,
        area.y,
        pet_width,
        MINIMAL_PET_HEIGHT.min(area.height),
    );
    let mut next_x = pet_width.saturating_add(MINIMAL_TARGET_GAP);
    let mut food = None;
    let mut disabled_food = None;

    if let Some((food_kind, count)) = first_stocked_food(snapshot) {
        let label = minimal_food_label_for_mode(count, mode);
        let width = label_width(&label);
        if next_x.saturating_add(width) <= area.width {
            let rect = Rect::new(
                area.x.saturating_add(next_x),
                area.y.saturating_add(MINIMAL_TARGET_ROW),
                width,
                1,
            );
            food = Some((food_kind, count, rect, label));
            next_x = next_x.saturating_add(width);
        }
    } else {
        let label = minimal_disabled_food_label(mode);
        let width = label_width(label);
        if next_x.saturating_add(width) <= area.width {
            disabled_food = Some((next_x, label.to_owned()));
            next_x = next_x.saturating_add(width);
        }
    }

    next_x = next_x.saturating_add(MINIMAL_TARGET_GAP);
    let bed_label = minimal_bed_label_for_mode(mode);
    let bed_width = label_width(bed_label);
    let bed = (next_x.saturating_add(bed_width) <= area.width).then(|| {
        let rect = Rect::new(
            area.x.saturating_add(next_x),
            area.y.saturating_add(MINIMAL_TARGET_ROW),
            bed_width,
            1,
        );
        next_x = next_x.saturating_add(bed_width);
        (rect, bed_label)
    });

    next_x = next_x.saturating_add(MINIMAL_TARGET_GAP);
    let poop_label = minimal_poop_label_for_mode(mode);
    let poop_width = label_width(poop_label);
    let poops = snapshot
        .pending_poops
        .iter()
        .take(MINIMAL_MAX_POOPS)
        .filter_map(|poop| {
            if next_x.saturating_add(poop_width) > area.width {
                return None;
            }
            let rect = Rect::new(
                area.x.saturating_add(next_x),
                area.y.saturating_add(MINIMAL_TARGET_ROW),
                poop_width,
                1,
            );
            next_x = next_x
                .saturating_add(poop_width)
                .saturating_add(MINIMAL_TARGET_GAP);
            Some((poop.id(), rect, poop_label))
        })
        .collect();

    MinimalTargetLayout {
        pet,
        food,
        disabled_food,
        bed,
        poops,
    }
}

fn minimal_layout(area: Rect, snapshot: &SimulationSnapshot) -> MinimalTargetLayout {
    if area.width <= 24 {
        return minimal_layout_for_mode(area, snapshot, MinimalLabelMode::Tiny);
    }
    let requested_poops = snapshot.pending_poops.len().min(MINIMAL_MAX_POOPS);
    let has_food = first_stocked_food(snapshot).is_some();
    let mut fallback = None;
    for mode in [
        MinimalLabelMode::Full,
        MinimalLabelMode::Compact,
        MinimalLabelMode::Tiny,
    ] {
        let layout = minimal_layout_for_mode(area, snapshot, mode);
        let complete = (!has_food || layout.food.is_some())
            && layout.bed.is_some()
            && layout.poops.len() == requested_poops;
        if complete {
            return layout;
        }
        fallback = Some(layout);
    }
    fallback.expect("Minimal layout always has a label mode")
}

fn minimal_geometry(area: Rect, snapshot: &SimulationSnapshot) -> RoomGeometry {
    let layout = minimal_layout(area, snapshot);
    let food_sources = layout
        .food
        .iter()
        .map(|(food, count, rect, _)| FoodSource {
            rect: *rect,
            food_id: food.id(),
            count: *count,
        })
        .collect();
    let poops = layout
        .poops
        .iter()
        .map(|(id, rect, _)| (*id, *rect))
        .collect();
    RoomGeometry {
        pet: layout.pet,
        bed: layout.bed.map(|(rect, _)| rect),
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
#[allow(clippy::too_many_arguments)]
fn wide_food_sources(
    area: Rect,
    snapshot: &SimulationSnapshot,
    x: u16,
    top: u16,
    compact: bool,
    ultra_compact: bool,
    generic_first: bool,
    abbreviate_count: bool,
) -> Vec<FoodSource> {
    let mut next_x = x;
    let mut sources = Vec::new();
    for food in [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ] {
        let count = snapshot.inventory.count(food);
        if count > 0 {
            let first = sources.is_empty();
            let source = FoodSource {
                rect: Rect::new(
                    area.x.saturating_add(next_x),
                    area.y.saturating_add(top),
                    food_target_width(
                        food,
                        count,
                        compact,
                        first,
                        ultra_compact,
                        generic_first,
                        abbreviate_count,
                    ),
                    food_target_height(compact),
                ),
                food_id: food.id(),
                count,
            };
            next_x = next_x.saturating_add(source.rect.width).saturating_add(2);
            sources.push(source);
        }
    }
    sources
}

fn sprite_width(sprite: &[&str]) -> u16 {
    sprite
        .iter()
        .map(|row| label_width(row))
        .max()
        .unwrap_or_default()
}

fn food_target_width(
    food: FoodKind,
    count: u32,
    compact: bool,
    first: bool,
    ultra_compact: bool,
    generic_first: bool,
    abbreviate_count: bool,
) -> u16 {
    let label = food_source_label(
        food,
        count,
        compact,
        first,
        ultra_compact,
        generic_first,
        abbreviate_count,
    );
    let sprite = if compact {
        &FOOD_BOWL_COMPACT_WIDE
    } else {
        &FOOD_BOWL_WIDE
    };
    [sprite_width(sprite), label_width(&label)]
        .into_iter()
        .max()
        .unwrap_or_default()
}

fn food_source_label(
    food: FoodKind,
    count: u32,
    compact: bool,
    first: bool,
    ultra_compact: bool,
    generic_first: bool,
    abbreviate_count: bool,
) -> String {
    let count = if abbreviate_count {
        compact_food_count(count)
    } else {
        count.to_string()
    };
    if ultra_compact {
        if first {
            format!("FOOD{count}")
        } else {
            format!("{}{count}", food_label(food.id()))
        }
    } else if compact {
        if first {
            format!("FOOD x{count}")
        } else {
            format!("{}x{count}", food_label(food.id()))
        }
    } else if first && generic_first {
        format!("FOOD x{count}")
    } else {
        format!("{} x{count}", food_label(food.id()))
    }
}

fn compact_food_count(count: u32) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    if count < 1_000_000 {
        return format!("{}k", count / 1_000);
    }
    if count < 1_000_000_000 {
        return format!("{}M", count / 1_000_000);
    }
    let whole = count / 1_000_000_000;
    let tenth = (count % 1_000_000_000) / 100_000_000;
    if tenth == 0 {
        format!("{whole}B")
    } else {
        format!("{whole}.{tenth}B")
    }
}

const fn food_target_height(compact: bool) -> u16 {
    if compact {
        FOOD_BOWL_COMPACT_WIDE.len() as u16
    } else {
        FOOD_BOWL_WIDE.len() as u16
    }
}

fn poop_anchor_after_food(area: Rect, food_sources: &[FoodSource], fallback: u16) -> u16 {
    food_sources
        .iter()
        .map(|source| source.rect.right().saturating_sub(area.x))
        .max()
        .map_or(fallback, |right| right.saturating_add(2))
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
                    wide_poop_target_width(),
                    wide_poop_target_height(),
                ),
            )
        })
        .collect()
}

fn wide_poop_target_width() -> u16 {
    sprite_width(&POOP_OBJECT_WIDE)
        .max(sprite_width(&POOP_OBJECT_COMPACT))
        .max(label_width("POOP"))
}

fn wide_poop_target_height() -> u16 {
    POOP_OBJECT_WIDE.len().max(POOP_OBJECT_COMPACT.len()).max(1) as u16
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
                bed_x.saturating_add(2),
                bed_y,
                palette,
            );
            put_text(
                area,
                buffer,
                bed_x.saturating_add(7),
                bed_y.saturating_sub(1),
                "z z z",
                palette.cell_style(SemanticTone::Tone2),
            );
        }
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
                pet_x.saturating_add(13),
                pet_y.saturating_sub(1),
                "z",
                palette.cell_style(SemanticTone::Tone2),
            );
        } else {
            draw_sprite_with_palette(area, buffer, pet_sprite(frame.pose), pet_x, pet_y, palette);
        }
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
    let status_line = compact_status_line(snapshot.needs);
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

    // The tiny window and plant preserve the room's identity without taking
    // the pet's left-side priority in the compact hierarchy.
    let (window_x, plant_x) = compact_decoration_slots(area, geometry);
    if let Some(window_x) = window_x {
        put_sprite(area, buffer, &WINDOW_COMPACT, window_x, 2, palette);
    }
    if let Some(plant_x) = plant_x {
        put_sprite(area, buffer, &PLANTS_COMPACT, plant_x, 4, palette);
    }

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
                bed_x.saturating_add(11),
                bed_y,
                palette,
            );
            put_text(
                area,
                buffer,
                bed_x.saturating_add(8),
                bed_y.saturating_add(3),
                "BED",
                palette.cell_style(SemanticTone::Tone3),
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
    if let Some(bed) = geometry.bed {
        let bed_x = bed.x.saturating_sub(area.x);
        let bed_y = bed.y.saturating_sub(area.y);
        if !napping {
            put_sprite(area, buffer, &BED_COMPACT_WIDE, bed_x, bed_y, palette);
        }
    }
    render_food_sources_compact_wide(area, buffer, geometry, palette);
    render_poops_compact_wide(area, buffer, geometry, palette);
    render_drag_ghost(area, buffer, drag, palette);
}

#[allow(clippy::too_many_arguments)]
fn render_minimal(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &SimulationSnapshot,
    activity: PresentationActivity,
    napping: bool,
    frame: &PresentationFrame,
    options: RoomRenderOptions,
    drag: Option<(&str, Position)>,
    geometry: &RoomGeometry,
) {
    let palette = options.palette();
    let layout = minimal_layout(area, snapshot);
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
    let pet_x = geometry.pet.x.saturating_sub(area.x);
    let pet_y = geometry.pet.y.saturating_sub(area.y);
    let pet_pose = if napping {
        PetPose::Sleep
    } else if snapshot.behavior == PetBehavior::Sleeping {
        PetPose::Doze
    } else {
        frame.pose
    };
    let pet_sprite = minimal_pet_sprite(pet_pose);
    draw_sprite_with_palette(area, buffer, &pet_sprite, pet_x, pet_y, palette);
    put_text(
        area,
        buffer,
        geometry.pet.width.saturating_add(1),
        0,
        &line,
        palette.cell_style(SemanticTone::Tone3),
    );

    let (affection, snack) = demand_counts(snapshot);
    if let Some((_, _count, rect, label)) = layout.food.as_ref() {
        put_text(
            area,
            buffer,
            rect.x.saturating_sub(area.x),
            MINIMAL_TARGET_ROW,
            label,
            palette.cell_style(SemanticTone::Tone2),
        );
    } else if let Some((x, label)) = layout.disabled_food.as_ref() {
        put_text(
            area,
            buffer,
            *x,
            MINIMAL_TARGET_ROW,
            label,
            palette.cell_style(SemanticTone::Tone1),
        );
    }
    if let Some((bed, label)) = layout.bed {
        put_text(
            area,
            buffer,
            bed.x.saturating_sub(area.x),
            MINIMAL_TARGET_ROW,
            label,
            palette.cell_style(SemanticTone::Tone2),
        );
    }
    if layout.poops.is_empty() {
        let poop_x = minimal_cue_x(area, geometry);
        put_text(
            area,
            buffer,
            poop_x,
            MINIMAL_TARGET_ROW,
            minimal_poop_label_for_mode(if area.width <= 24 {
                MinimalLabelMode::Tiny
            } else {
                MinimalLabelMode::Full
            }),
            palette.cell_style(SemanticTone::Tone1),
        );
    } else {
        for (_, poop, label) in &layout.poops {
            put_text(
                area,
                buffer,
                poop.x.saturating_sub(area.x),
                MINIMAL_TARGET_ROW,
                label,
                palette.cell_style(SemanticTone::Tone2),
            );
        }
    }
    let cue_x = minimal_cue_x(area, geometry);
    let activity_cue = format!("AFF x{affection} {activity:?}");
    put_text(
        area,
        buffer,
        cue_x,
        2,
        &activity_cue,
        palette.cell_style(SemanticTone::Tone2),
    );
    if snack > 0 {
        put_text(
            area,
            buffer,
            cue_x
                .saturating_add(label_width(&activity_cue))
                .saturating_add(2),
            2,
            "SNACK",
            palette.cell_style(SemanticTone::Tone2),
        );
    }
    render_drag_ghost(area, buffer, drag, palette);
}

fn minimal_cue_x(area: Rect, geometry: &RoomGeometry) -> u16 {
    let after_targets = geometry
        .poops
        .last()
        .map(|(_, rect)| rect.right().saturating_sub(area.x).saturating_add(2))
        .or_else(|| {
            geometry
                .bed
                .map(|rect| rect.right().saturating_sub(area.x).saturating_add(2))
        })
        .unwrap_or_else(|| geometry.pet.width.saturating_add(1));
    after_targets.max(geometry.pet.width.saturating_add(1))
}

fn minimal_pet_sprite(pose: PetPose) -> [&'static str; 6] {
    *pet_sprite_minimal(pose)
}

fn compact_status_line(needs: PetNeeds) -> String {
    let status = format!(
        "HUNGER {}  ENERGY {}  HAPPY {}  CLEAN {}",
        need_bar(needs.hunger()),
        need_bar(needs.energy()),
        need_bar(needs.happiness()),
        need_bar(needs.cleanliness()),
    );
    if [
        need_percent(needs.hunger()),
        need_percent(needs.energy()),
        need_percent(needs.happiness()),
        need_percent(needs.cleanliness()),
    ] == [0, 100, 100, 100]
    {
        status
    } else {
        format!(
            "{status}  H {} E {} P {} C {}",
            need_percent(needs.hunger()),
            need_percent(needs.energy()),
            need_percent(needs.happiness()),
            need_percent(needs.cleanliness()),
        )
    }
}

fn compact_decoration_slots(area: Rect, geometry: &RoomGeometry) -> (Option<u16>, Option<u16>) {
    if area.width < 100 {
        return (None, None);
    }
    let care_end = geometry
        .poops
        .last()
        .map(|(_, rect)| rect.right().saturating_sub(area.x))
        .or_else(|| {
            geometry
                .food_sources
                .last()
                .map(|source| source.rect.right().saturating_sub(area.x))
        })
        .unwrap_or(14);
    let bed_start = geometry
        .bed
        .map(|rect| rect.x.saturating_sub(area.x))
        .unwrap_or(area.width);
    let window_width = sprite_width(&WINDOW_COMPACT);
    let plant_width = sprite_width(&PLANTS_COMPACT);
    let window_limit = bed_start.saturating_sub(window_width.saturating_add(2));
    let window_start = care_end.saturating_add(2);
    if window_start > window_limit {
        return (None, None);
    }
    let window_x = window_start;
    let plant_x = window_x.saturating_add(window_width).saturating_add(2);
    let plant = (plant_x.saturating_add(plant_width) <= bed_start).then_some(plant_x);
    (Some(window_x), plant)
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
        line.push_str("  heart cue ready");
    }
    put_text(
        area,
        buffer,
        area.width.saturating_sub(42),
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
    let compact = area.width < 100;
    let ultra_compact = area.width < 90;
    for (index, source) in geometry.food_sources.iter().enumerate() {
        let x = source.rect.x.saturating_sub(area.x);
        let y = source.rect.y.saturating_sub(area.y);
        let bowl = if compact {
            &FOOD_BOWL_COMPACT_WIDE
        } else {
            &FOOD_BOWL_WIDE
        };
        put_sprite(area, buffer, bowl, x, y, palette);
        let food = FoodKind::from_id(source.food_id).expect("rendered food id is known");
        let label = food_source_label(
            food,
            source.count,
            compact,
            index == 0,
            ultra_compact,
            false,
            compact,
        );
        put_text(
            area,
            buffer,
            x,
            y.saturating_add(bowl.len().saturating_sub(1) as u16),
            &label,
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
        put_sprite(area, buffer, &POOP_OBJECT_WIDE, x, y, palette);
        if index == 0 {
            put_text(
                area,
                buffer,
                x,
                y.saturating_add(POOP_OBJECT_WIDE.len().saturating_sub(1) as u16),
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
    let compact = true;
    let ultra_compact = area.width < 90;
    for (index, source) in geometry.food_sources.iter().enumerate() {
        let x = source.rect.x.saturating_sub(area.x);
        let y = source.rect.y.saturating_sub(area.y);
        put_sprite(area, buffer, &FOOD_BOWL_COMPACT, x, y, palette);
        let food = FoodKind::from_id(source.food_id).expect("rendered food id is known");
        let label = food_source_label(
            food,
            source.count,
            compact,
            index == 0,
            ultra_compact,
            true,
            area.width < 110,
        );
        put_text(
            area,
            buffer,
            x,
            y.saturating_add(if index == 0 { 3 } else { 2 }),
            &label,
            palette.cell_style(if index == 0 {
                SemanticTone::Tone3
            } else {
                SemanticTone::Tone2
            }),
        );
    }
}

fn render_poops_compact_wide(
    area: Rect,
    buffer: &mut Buffer,
    geometry: &RoomGeometry,
    palette: ResolvedPalette,
) {
    for (index, (_, rect)) in geometry.poops.iter().enumerate() {
        let x = rect.x.saturating_sub(area.x);
        let y = rect.y.saturating_sub(area.y);
        put_sprite(area, buffer, &POOP_OBJECT_COMPACT, x, y, palette);
        put_text(
            area,
            buffer,
            x,
            y.saturating_add(3),
            "POOP",
            palette.cell_style(if index == 0 {
                SemanticTone::Tone3
            } else {
                SemanticTone::Tone2
            }),
        );
    }
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
            let tone = match ch {
                '█' | '▀' | '▄' => SemanticTone::Tone1,
                '┌' | '┐' | '└' | '┘' | '─' | '│' => SemanticTone::Tone2,
                _ => SemanticTone::Tone3,
            };
            let logical_x = area
                .x
                .saturating_add(x)
                .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
            let logical_y = area.y.saturating_add(y).saturating_add(row);
            let style = palette.cell_style(palette.sample_logical_tone(tone, logical_x, logical_y));
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
    render_room_backdrop(area, buffer, palette);
    let window: &'static [&'static str] = match ambience {
        RoomAmbience::Day => &WINDOW_FULL_DAY,
        RoomAmbience::Night => &WINDOW_FULL_NIGHT,
    };
    for furniture in full_wide_furniture_layout(area, window) {
        put_sprite(
            area,
            buffer,
            furniture.sprite,
            furniture.x,
            furniture.y,
            palette,
        );
    }
}

#[derive(Clone, Copy)]
struct FullFurnitureSprite {
    sprite: &'static [&'static str],
    x: u16,
    y: u16,
}

fn full_wide_furniture_layout(
    area: Rect,
    window: &'static [&'static str],
) -> Vec<FullFurnitureSprite> {
    let mut furniture = Vec::new();
    if area.width >= 120 {
        furniture.extend([
            FullFurnitureSprite {
                sprite: &DESK_FULL,
                x: 1,
                y: 1,
            },
            FullFurnitureSprite {
                sprite: window,
                x: 27,
                y: 1,
            },
            FullFurnitureSprite {
                sprite: &SHELF_FULL,
                x: 47,
                y: 1,
            },
            FullFurnitureSprite {
                sprite: &WARDROBE_FULL,
                x: 66,
                y: 1,
            },
            FullFurnitureSprite {
                sprite: &RUG_FULL,
                x: 36,
                y: 7,
            },
            FullFurnitureSprite {
                sprite: &PLANTS_FULL,
                x: 38,
                y: 8,
            },
        ]);
    } else {
        furniture.extend([
            FullFurnitureSprite {
                sprite: &DESK_COMPACT_FULL,
                x: 1,
                y: 1,
            },
            FullFurnitureSprite {
                sprite: window,
                x: 18,
                y: 1,
            },
        ]);
        if area.width >= 90 {
            furniture.extend([
                FullFurnitureSprite {
                    sprite: &SHELF_COMPACT_FULL,
                    x: 38,
                    y: 1,
                },
                FullFurnitureSprite {
                    sprite: &WARDROBE_COMPACT_FULL,
                    x: 38,
                    y: 4,
                },
            ]);
        } else {
            furniture.push(FullFurnitureSprite {
                sprite: &SHELF_COMPACT_FULL,
                x: 1,
                y: 5,
            });
        }
        if area.width >= 100 {
            let pet_x = area.width.saturating_sub(42);
            furniture.push(FullFurnitureSprite {
                sprite: &PLANTS_COMPACT_FULL,
                x: pet_x.saturating_sub(12),
                y: 4,
            });
        }
    }
    furniture
}

fn render_room_backdrop(area: Rect, buffer: &mut Buffer, palette: ResolvedPalette) {
    for y in 1..area.height.saturating_sub(3) {
        let tone = if y % 2 == 0 {
            SemanticTone::Tone1
        } else {
            SemanticTone::Tone0
        };
        for x in (1..area.width.saturating_sub(1)).step_by(6) {
            put(area, buffer, x, y, "·", palette.cell_style(tone));
        }
    }
    for y in 7..area.height.saturating_sub(3).min(11) {
        for x in 0..area.width {
            if (y == 10 && x % 4 == 0)
                || (y == 9 && (x + 3) % 14 == 0)
                || (y == 7 && x % 17 == 2)
                || (x + y) % 8 == 0
            {
                put(
                    area,
                    buffer,
                    x,
                    y,
                    "█",
                    palette.cell_style(SemanticTone::Tone1),
                );
            } else if (x + y * 2) % 11 == 0 {
                put(
                    area,
                    buffer,
                    x,
                    y,
                    "·",
                    palette.cell_style(SemanticTone::Tone2),
                );
            }
        }
    }
    for x in 0..area.width {
        put(
            area,
            buffer,
            x,
            6,
            "┈",
            palette.cell_style(SemanticTone::Tone1),
        );
        put(
            area,
            buffer,
            x,
            11,
            "─",
            palette.cell_style(SemanticTone::Tone1),
        );
    }
}

/// Decorative bedroom furniture for the Full layout. Deterministic simple
/// silhouettes. Placement deliberately avoids the
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

const WINDOW_FULL_DAY: [&str; 5] = [
    "╭────────────────╮",
    "│  ║  ☀  ║  ·  · │",
    "│  ███  ▄██  ██  │",
    "│  ███  ▄██  ██  │",
    "╰────────────────╯",
];

const WINDOW_FULL_NIGHT: [&str; 5] = [
    "╭────────────────╮",
    "│  ║  ·  ☾  ║  · │",
    "│  ██  ▄██  ███  │",
    "│  ██  ▄██  ███  │",
    "╰────────────────╯",
];

const SHELF_FULL: [&str; 4] = [
    "╭─┬─────┬─────┬──╮",
    "│▌▐▌██▌▐▌  ▣██▌▐ │",
    "│▌▐▌  ─┴███──┴── │",
    "╰─┴──────────────╯",
];

const WARDROBE_FULL: [&str; 7] = [
    "╭──────────╮",
    "│┌██┐  ┌██┐│",
    "││·██  ██·││",
    "││██│  │██││",
    "│└██┘  └██┘│",
    "│╲╱╲█╱╲╱╲█╲│",
    "╰──────────╯",
];

const DESK_FULL: [&str; 5] = [
    "╭──────────────────────╮",
    "│   ╭────────╮   ╱╲    │",
    "│   │▣▣▣▣▣▣▣│  ╱██╲    │",
    "│   ╰────────╯   ║█    │",
    "╰──────────────────────╯",
];

const PLANTS_FULL: [&str; 5] = [
    "  ╱█╲   ╱█╲  ",
    " ╱███╲ ╱███╲ ",
    "   ██   ██   ",
    "  ╭██╮ ╭██╮  ",
    "  ╰──╯ ╰──╯  ",
];

const WINDOW_COMPACT: [&str; 3] = ["┌──────────┐", "│▀▀▀▀▀▀▀▀▀▀│", "└──────────┘"];

const PLANTS_COMPACT: [&str; 3] = ["  ▀  ", " ▀▀▀ ", "┌───┐"];

const BED_FULL: [&str; 4] = [
    "┌──────────────────┐",
    "│▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄│",
    "│▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀│",
    "└──────────────────┘",
];

const BED_COMPACT: [&str; 2] = ["┌────────┐", "└────────┘"];

/// Wide bed vignette: headboard, pillow, star blanket, and footboard all fit
/// inside the 23-column hit region used by the Full production layout.
const BED_WIDE: [&str; 7] = [
    "┌─┐                 ┌─┐",
    "│ │  ┌───────────┐  │ │",
    "│ └──┤  pillow   ├──┘ │",
    "│    └───────────┘    │",
    "│  * █ * █ * █ * █    │",
    "└─────────────────────┘",
    "  └────── BED ──────┘  ",
];

const BED_COMPACT_WIDE: [&str; 4] = [
    "┌─┐              ┌─┐",
    "│ └──────────────┘ │",
    "│   *  *  *  *     │",
    "└─────── BED ──────┘",
];

const FOOD_BOWL_COMPACT: [&str; 4] = [" ○  ", "◒◒  ", "└─┘ ", "    "];
const FOOD_BOWL_WIDE: [&str; 4] = ["  ╭──╮ ", " ╱██╲  ", "│█◒█│  ", "╰────╯ "];
const FOOD_BOWL_COMPACT_WIDE: [&str; 4] = [" ╭─╮ ", "╱◒╲  ", "│◒│  ", "╰─╯  "];
const POOP_OBJECT_WIDE: [&str; 4] = ["  ╱╲   ", " ╱██╲  ", " ╲██╱  ", "  ╰╯   "];
const POOP_OBJECT_COMPACT: [&str; 4] = [" ~ ", "(●)", "╰─ ", "   "];
const SHELF_COMPACT_FULL: [&str; 3] = ["╭───────╮", "│▌▐▌ ▣  │", "╰───────╯"];
const WARDROBE_COMPACT_FULL: [&str; 3] = ["╭──────╮", "││· · ││", "╰──────╯"];
const DESK_COMPACT_FULL: [&str; 4] = [
    "╭──────────────╮",
    "│ ╭──╮  ╱╲     │",
    "│ ╰──╯  ║      │",
    "╰──────────────╯",
];
const PLANTS_COMPACT_FULL: [&str; 4] = ["  ╱╲  ╱╲ ", " ╱██╲╱██╲", " ╭─╮ ╭─╮ ", " ╰─╯ ╰─╯ "];
const RUG_FULL: [&str; 3] = [
    "╭────────────────╮",
    "│· · · · · · · · │",
    "╰────────────────╯",
];

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use codegotchi_domain::{
        DefaultNeedProgressionStrategy, FoodInventory, FoodKind, Pet, PetSimulation, PetSpecies,
        SystemClock,
    };
    use uuid::Uuid;

    use super::*;

    /// Every sprite row must have the same width; a ragged row (like the old
    /// sleep face) corrupts the rendered glyph alignment.
    #[test]
    fn every_sprite_has_consistent_row_widths() {
        let sprites: &[(&str, &[&str])] = &[
            ("BED_FULL", &BED_FULL),
            ("BED_COMPACT", &BED_COMPACT),
            ("BED_WIDE", &BED_WIDE),
            ("BED_COMPACT_WIDE", &BED_COMPACT_WIDE),
            ("WINDOW_FULL_DAY", &WINDOW_FULL_DAY),
            ("WINDOW_FULL_NIGHT", &WINDOW_FULL_NIGHT),
            ("SHELF_FULL", &SHELF_FULL),
            ("SHELF_COMPACT_FULL", &SHELF_COMPACT_FULL),
            ("WARDROBE_FULL", &WARDROBE_FULL),
            ("WARDROBE_COMPACT_FULL", &WARDROBE_COMPACT_FULL),
            ("DESK_FULL", &DESK_FULL),
            ("DESK_COMPACT_FULL", &DESK_COMPACT_FULL),
            ("PLANTS_FULL", &PLANTS_FULL),
            ("PLANTS_COMPACT_FULL", &PLANTS_COMPACT_FULL),
            ("WINDOW_COMPACT", &WINDOW_COMPACT),
            ("PLANTS_COMPACT", &PLANTS_COMPACT),
            ("FOOD_BOWL_COMPACT", &FOOD_BOWL_COMPACT),
            ("FOOD_BOWL_WIDE", &FOOD_BOWL_WIDE),
            ("FOOD_BOWL_COMPACT_WIDE", &FOOD_BOWL_COMPACT_WIDE),
            ("POOP_OBJECT_WIDE", &POOP_OBJECT_WIDE),
            ("POOP_OBJECT_COMPACT", &POOP_OBJECT_COMPACT),
            ("RUG_FULL", &RUG_FULL),
        ];
        for (name, sprite) in sprites {
            let width = sprite[0].chars().count();
            for (index, row) in sprite.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    width,
                    "{name} row {index} has a different width than row 0"
                );
            }
        }
    }

    fn rects_overlap(first: Rect, second: Rect) -> bool {
        first.x < second.right()
            && second.x < first.right()
            && first.y < second.bottom()
            && second.y < first.bottom()
    }

    #[test]
    fn sparse_wide_food_sources_do_not_double_gap_after_empty_kind() {
        let now = Utc::now();
        let mut inventory = FoodInventory::new();
        inventory.add(FoodKind::Kibble, 5);
        inventory.add(FoodKind::Fruit, 7);
        let pet = Pet::with_inventory(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, now, inventory);
        let snapshot =
            PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
        let sources = wide_food_sources(
            Rect::new(0, 0, 120, 14),
            &snapshot,
            32,
            8,
            false,
            false,
            false,
            false,
        );

        assert_eq!(sources.len(), 2);
        assert_eq!(
            sources[1].rect.x,
            sources[0].rect.right().saturating_add(2),
            "an empty Treat source must not reserve a second gap"
        );
    }

    #[test]
    fn wide_furniture_stays_outside_pet_and_bed_across_supported_widths() {
        let now = Utc::now();
        let pet = Pet::with_inventory(
            Uuid::from_u128(2),
            "Mochi",
            PetSpecies::Cat,
            now,
            FoodInventory::starter(),
        );
        let snapshot =
            PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();

        for width in 80..=121 {
            let area = Rect::new(0, 0, width, 14);
            let geometry = full_geometry(area, &snapshot, (0, 0));
            let bed = geometry.bed.expect("Full room has a bed");
            for furniture in full_wide_furniture_layout(area, &WINDOW_FULL_DAY) {
                let rect = Rect::new(
                    area.x.saturating_add(furniture.x),
                    area.y.saturating_add(furniture.y),
                    sprite_width(furniture.sprite),
                    furniture.sprite.len() as u16,
                );
                assert!(
                    rect.right() <= area.right() && rect.bottom() <= area.bottom(),
                    "furniture clipped at width {width}: {rect:?} area={area:?}"
                );
                assert!(
                    !rects_overlap(rect, geometry.pet),
                    "furniture overlaps pet at width {width}: furniture={rect:?} pet={:?}",
                    geometry.pet
                );
                assert!(
                    !rects_overlap(rect, bed),
                    "furniture overlaps bed at width {width}: furniture={rect:?} bed={bed:?}"
                );
                for source in &geometry.food_sources {
                    assert!(
                        !rects_overlap(rect, source.rect),
                        "furniture overlaps food at width {width}: furniture={rect:?} food={:?}",
                        source.rect
                    );
                }
                for (_, poop) in &geometry.poops {
                    assert!(
                        !rects_overlap(rect, *poop),
                        "furniture overlaps poop at width {width}: furniture={rect:?} poop={poop:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn minimal_idle_sprite_keeps_the_complete_mascot_silhouette() {
        assert_eq!(
            minimal_pet_sprite(PetPose::Idle),
            [
                "  #  #   ",
                " ####### ",
                "#o..o..##",
                "#oooooo# ",
                " ##oo##  ",
                "  ###    ",
            ],
            "Minimal should show ears, face, body, and feet in three packed rows"
        );
    }
}

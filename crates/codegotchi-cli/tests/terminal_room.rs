use chrono::{Duration, Utc};
use codegotchi_cli::terminal::{
    CareGateway, PetPose, PresentationActivity, PresentationFrame, PresentationState, RoomAmbience,
    RoomCareRequest, RoomInputSession, RoomRenderOptions, SemanticTone, TerminalThemePreset,
    auto_style, has_authoritative_nap, presentation_activity, render_room,
    render_room_with_options, room_geometry, room_geometry_with_frame, wide_full_care_zone,
};
use codegotchi_domain::{
    ActivityKind, AgentActivityState, DefaultNeedProgressionStrategy, FoodInventory, FoodKind, Pet,
    PetBehavior, PetDemand, PetDemandKind, PetSimulation, PetSpecies, Poop, SimulationSnapshot,
    SystemClock,
};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::{Position, Rect},
    style::Color,
};
use std::sync::Mutex;
use uuid::Uuid;

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

fn default_frame() -> PresentationFrame {
    PresentationFrame::default()
}

fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
    let mut text = String::new();
    for y in 0..height {
        for x in 0..width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn row_text(buffer: &Buffer, width: u16, y: u16) -> String {
    (0..width)
        .map(|x| buffer.cell((x, y)).expect("row cell").symbol())
        .collect()
}

fn find_row_text(buffer: &Buffer, width: u16, y: u16, needle: &str) -> Option<u16> {
    let row = row_text(buffer, width, y);
    let needle_width = u16::try_from(needle.chars().count()).ok()?;
    (0..=width.saturating_sub(needle_width)).find(|start| {
        row.chars()
            .skip(usize::from(*start))
            .take(usize::from(needle_width))
            .collect::<String>()
            == needle
    })
}

fn soft_green_room(area: Rect, snapshot: &SimulationSnapshot) -> Buffer {
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room_with_options(
        area,
        &mut buffer,
        snapshot,
        &default_frame(),
        RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Day),
        None,
    );
    buffer
}

fn is_tone3(cell: &Cell) -> bool {
    cell.style().fg == Some(Color::Rgb(166, 220, 177))
}

fn random_backdrop_fill_count(buffer: &Buffer, area: Rect) -> usize {
    (area.y.saturating_add(1)..area.bottom().saturating_sub(2))
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|(x, y)| {
            buffer.cell((*x, *y)).is_some_and(|cell| {
                matches!(cell.symbol(), "█" | "▀" | "▄")
                    && cell.style().fg == Some(Color::Rgb(24, 74, 45))
            })
        })
        .count()
}

fn largest_tone3_component_is_inside(buffer: &Buffer, pet: Rect) -> bool {
    let area = buffer.area;
    let mut tone3 = std::collections::HashSet::new();
    for y in area.y.saturating_add(1)..area.bottom().saturating_sub(1) {
        for x in area.x..area.right() {
            if buffer.cell((x, y)).is_some_and(is_tone3) {
                tone3.insert((x, y));
            }
        }
    }

    let mut largest = Vec::new();
    while let Some(start) = tone3.iter().next().copied() {
        let mut pending = vec![start];
        let mut component = Vec::new();
        tone3.remove(&start);
        while let Some((x, y)) = pending.pop() {
            component.push((x, y));
            for neighbor in [
                (x.saturating_sub(1), y),
                (x.saturating_add(1), y),
                (x, y.saturating_sub(1)),
                (x, y.saturating_add(1)),
            ] {
                if tone3.remove(&neighbor) {
                    pending.push(neighbor);
                }
            }
        }
        if component.len() > largest.len() {
            largest = component;
        }
    }

    !largest.is_empty()
        && largest
            .into_iter()
            .all(|(x, y)| pet.contains(Position::new(x, y)))
}

fn tone3_count_outside_targets(
    buffer: &Buffer,
    geometry: &codegotchi_cli::terminal::RoomGeometry,
) -> usize {
    let area = buffer.area;
    (area.y.saturating_add(1)..area.bottom().saturating_sub(1))
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let point = Position::new(*x, *y);
            let care_target = geometry.pet.contains(point)
                || geometry.bed.is_some_and(|bed| bed.contains(point))
                || geometry
                    .food_sources
                    .iter()
                    .any(|source| source.rect.contains(point))
                || geometry.poops.iter().any(|(_, rect)| rect.contains(point));
            !care_target && buffer.cell(point).is_some_and(is_tone3)
        })
        .count()
}

fn has_two_column_clearance_around_pet(buffer: &Buffer, pet: Rect) -> bool {
    let area = buffer.area;
    let clear = |x: u16, y: u16| {
        if x < area.x || x >= area.right() || y < area.y || y >= area.bottom() {
            return true;
        }
        buffer
            .cell((x, y))
            .is_none_or(|cell| matches!(cell.symbol(), " " | "┈" | "─"))
    };
    (pet.y..pet.bottom()).all(|y| {
        [pet.x.saturating_sub(2), pet.x.saturating_sub(1)]
            .into_iter()
            .chain([pet.right(), pet.right().saturating_add(1)])
            .all(|x| clear(x, y))
    })
}

fn non_empty_cells(buffer: &Buffer, rect: Rect) -> usize {
    (rect.y..rect.bottom())
        .flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)))
        .filter(|(x, y)| {
            buffer
                .cell((*x, *y))
                .is_some_and(|cell| cell.symbol() != " ")
        })
        .count()
}

#[test]
fn responsive_mascots_stay_inside_pet_geometry_at_supported_widths() {
    let snapshot = base_snapshot(Utc::now());
    let poses = [
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
    ];

    for pose in poses {
        let frame = PresentationFrame {
            pose,
            offset: (0, 0),
        };

        for width in [24, 32, 40, 80, 120] {
            let area = Rect::new(0, 0, width, 7);
            let geometry = room_geometry(area, &snapshot);
            assert_eq!(
                geometry.pet.width, 12,
                "Compact {pose:?} pet width at {width} columns"
            );
            assert_eq!(
                geometry.pet.height, 5,
                "Compact {pose:?} pet height at {width} columns"
            );
            assert!(geometry.pet.right() <= area.right());
            assert!(geometry.pet.bottom() <= area.bottom());

            let mut buffer = Buffer::filled(area, Cell::new(" "));
            render_room_with_options(
                area,
                &mut buffer,
                &snapshot,
                &frame,
                RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Day),
                None,
            );
            assert!(
                non_empty_cells(&buffer, geometry.pet) > 0,
                "Compact {pose:?} mascot is empty at {width} columns"
            );
            assert!(
                largest_tone3_component_is_inside(&buffer, geometry.pet),
                "Compact {pose:?} mascot extends outside its hitbox at {width} columns"
            );
        }

        for width in [24, 32, 40, 80, 120] {
            let area = Rect::new(0, 0, width, 3);
            let geometry = room_geometry(area, &snapshot);
            assert_eq!(
                geometry.pet.width, 9,
                "Minimal {pose:?} pet width at {width} columns"
            );
            assert_eq!(
                geometry.pet.height, 3,
                "Minimal {pose:?} pet height at {width} columns"
            );
            assert!(geometry.pet.right() <= area.right());
            assert!(geometry.pet.bottom() <= area.bottom());

            let mut buffer = Buffer::filled(area, Cell::new(" "));
            render_room_with_options(
                area,
                &mut buffer,
                &snapshot,
                &frame,
                RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Day),
                None,
            );
            assert!(
                non_empty_cells(&buffer, geometry.pet) > 0,
                "Minimal {pose:?} mascot is empty at {width} columns"
            );
            assert!(
                largest_tone3_component_is_inside(&buffer, geometry.pet),
                "Minimal {pose:?} mascot extends outside its hitbox at {width} columns"
            );
        }
    }
}

/// The presentation mapping must be exact and exhaustive: current aggregate
/// activity wins over stale recent outcomes, blocked/waiting always win, and
/// only `Idle` may fall back to a recent outcome. Every current `ActivityKind`
/// appears; adding a new variant breaks the exhaustive match in
/// `presentation_activity` until its terminal mapping is chosen deliberately.
#[test]
fn presentation_activity_mapping_is_exact_and_exhaustive() {
    let mut snapshot = base_snapshot(Utc::now());
    let cases: &[(AgentActivityState, PresentationActivity)] = &[
        (
            AgentActivityState::Blocked,
            PresentationActivity::WaitingOrBlocked,
        ),
        (
            AgentActivityState::WaitingForUser,
            PresentationActivity::WaitingOrBlocked,
        ),
        (
            AgentActivityState::Active(ActivityKind::Idle),
            PresentationActivity::Calm,
        ),
        (
            AgentActivityState::Active(ActivityKind::Thinking),
            PresentationActivity::Thinking,
        ),
        (
            AgentActivityState::Active(ActivityKind::Reading),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::Searching),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::Editing),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::Testing),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::Building),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::Installing),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::GitOperation),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::DockerOperation),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::WebResearch),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::UnknownWork),
            PresentationActivity::Working,
        ),
        (
            AgentActivityState::Active(ActivityKind::Waiting),
            PresentationActivity::WaitingOrBlocked,
        ),
        (
            AgentActivityState::Active(ActivityKind::Blocked),
            PresentationActivity::WaitingOrBlocked,
        ),
        (
            AgentActivityState::Active(ActivityKind::Celebrating),
            PresentationActivity::Success,
        ),
        (
            AgentActivityState::Active(ActivityKind::Error),
            PresentationActivity::Failure,
        ),
    ];
    for (activity, expected) in cases {
        snapshot.activity = *activity;
        assert_eq!(
            presentation_activity(&snapshot),
            *expected,
            "activity={activity:?}"
        );
    }

    // `Idle` may fall back to recent outcomes, and only then.
    snapshot.activity = AgentActivityState::Idle;
    snapshot.behavior = PetBehavior::RecentSuccess;
    assert_eq!(
        presentation_activity(&snapshot),
        PresentationActivity::Success
    );
    snapshot.behavior = PetBehavior::RecentFailure;
    assert_eq!(
        presentation_activity(&snapshot),
        PresentationActivity::Failure
    );
    snapshot.behavior = PetBehavior::Wandering;
    assert_eq!(presentation_activity(&snapshot), PresentationActivity::Calm);
}

/// `PetBehavior::Sleeping` alone is NOT authoritative sleep. Only an active
/// future `napping_until` selects the recovery-bed presentation; a Sleeping
/// snapshot without one is ordinary floor dozing and must never render the bed.
#[test]
fn sleeping_without_active_nap_never_uses_the_recovery_bed() {
    let now = Utc::now();

    let mut idle_doze = base_snapshot(now);
    idle_doze.behavior = PetBehavior::Sleeping;
    idle_doze.napping_until = None;
    assert!(!has_authoritative_nap(&idle_doze));

    let mut bed_nap = base_snapshot(now);
    bed_nap.behavior = PetBehavior::Sleeping;
    bed_nap.napping_until = Some(now + Duration::minutes(30));
    assert!(has_authoritative_nap(&bed_nap));

    let area = Rect::new(0, 0, 40, 14);
    let mut doze_buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut doze_buffer, &idle_doze, &default_frame(), None);
    let doze_text = buffer_text(&doze_buffer, area.width, area.height);
    assert!(
        doze_text.contains('┌'),
        "the bed is permanent furniture in the Full room"
    );
    assert!(
        !doze_text.contains("z z z"),
        "generic idle sleeping must not use the recovery-sleep indicator"
    );
    assert_eq!(
        doze_text.matches('z').count(),
        1,
        "generic idle sleeping should render a harmless floor doze"
    );
    assert_eq!(
        doze_buffer[(25, 9)].symbol(),
        "▄",
        "idle dozing leaves the bed mattress bare"
    );

    let mut nap_buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut nap_buffer, &bed_nap, &default_frame(), None);
    let nap_text = buffer_text(&nap_buffer, area.width, area.height);
    assert!(
        nap_text.contains("z z z"),
        "authoritative nap must render the recovery-sleep indicator"
    );
    let nap_pet = room_geometry(area, &bed_nap).pet;
    let nap_region_differences = (nap_pet.y..nap_pet.bottom())
        .flat_map(|y| (nap_pet.x..nap_pet.right()).map(move |x| (x, y)))
        .filter(|(x, y)| {
            nap_buffer.cell((*x, *y)).map(Cell::symbol)
                != doze_buffer.cell((*x, *y)).map(Cell::symbol)
        })
        .count();
    assert!(
        nap_region_differences > 0,
        "authoritative nap must draw a distinct sleep mascot in its bed pet region"
    );
}

/// The room is a deterministic projection that renders all three layout
/// heights with authoritative status content.
#[test]
fn room_renders_full_compact_and_minimal_projection() {
    let snapshot = base_snapshot(Utc::now());

    let full = Rect::new(0, 0, 40, 14);
    let mut full_buffer = Buffer::filled(full, Cell::new(" "));
    render_room(full, &mut full_buffer, &snapshot, &default_frame(), None);
    for label in ["Hunger", "Energy", "Happy", "Clean"] {
        assert!(
            (0..4).any(|row| row_text(&full_buffer, full.width, row).contains(label)),
            "Full room missing {label}"
        );
    }

    let compact = Rect::new(0, 0, 40, 7);
    let mut compact_buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(
        compact,
        &mut compact_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let compact_text = buffer_text(&compact_buffer, compact.width, compact.height);
    assert!(compact_text.contains("H "), "Compact status row missing");

    let minimal = Rect::new(0, 0, 40, 3);
    let mut minimal_buffer = Buffer::filled(minimal, Cell::new(" "));
    render_room(
        minimal,
        &mut minimal_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let minimal_text = buffer_text(&minimal_buffer, minimal.width, minimal.height);
    assert!(
        minimal_text.contains("CG "),
        "Minimal pet/status row missing"
    );
}

/// Domain needs are 0..100 (hunger inverted). The room must render
/// intermediate values gradually instead of collapsing every meter to 0/100.
#[test]
fn need_display_uses_domain_scale_gradually() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.needs.set_hunger(25.0);
    snapshot.needs.set_energy(50.0);
    snapshot.needs.set_happiness(75.0);
    snapshot.needs.set_cleanliness(0.0);

    let full = Rect::new(0, 0, 120, 14);
    let mut full_buffer = Buffer::filled(full, Cell::new(" "));
    render_room(full, &mut full_buffer, &snapshot, &default_frame(), None);
    let full_text = buffer_text(&full_buffer, full.width, full.height);
    assert!(
        full_text.contains("Hunger ██"),
        "hunger 25 must render as a 2/8 bar: {full_text}"
    );
    assert!(
        full_text.contains("Energy ████"),
        "energy 50 must render as a 4/8 bar: {full_text}"
    );
    assert!(
        full_text.contains("Happy  ██████"),
        "happiness 75 must render as a 6/8 bar: {full_text}"
    );
    assert!(
        full_text.contains("Clean  ░"),
        "cleanliness 0 must render as an empty bar: {full_text}"
    );

    let compact = Rect::new(0, 0, 120, 7);
    let mut compact_buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(
        compact,
        &mut compact_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let compact_text = buffer_text(&compact_buffer, compact.width, compact.height);
    for (label, value) in [("H 25", 25), ("E 50", 50), ("P 75", 75), ("C 0", 0)] {
        assert!(
            compact_text.contains(label),
            "compact status missing {label} (value {value}): {compact_text}"
        );
    }
}

/// Full keeps a quiet room frame around the mascot and care targets.
#[test]
fn full_room_renders_quiet_hierarchy_furniture() {
    let snapshot = base_snapshot(Utc::now());
    let full = Rect::new(0, 0, 120, 14);
    let mut buffer = Buffer::filled(full, Cell::new(" "));
    render_room(full, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, full.width, full.height);
    assert_eq!(
        buffer.cell((0, 6)).expect("wall-floor divider").symbol(),
        "┈"
    );
    assert_eq!(buffer.cell((0, 11)).expect("open floor").symbol(), " ");
    assert_eq!(
        buffer.cell((1, 1)).expect("window-desk frame").symbol(),
        "╭"
    );
    assert!(text.contains("☀"), "day window needs a sun marker: {text}");
    assert!(text.contains("╱╲"), "shelf needs one plant cue: {text}");
    assert!(
        !text.contains("▣"),
        "decorative furniture must stay sparse: {text}"
    );
    assert!(!text.contains("╲╱╲╱"), "wardrobe must be removed: {text}");
    assert_eq!(buffer.cell((96, 5)).expect("bed headboard").symbol(), "┌");
    assert!(text.contains("pillow"), "bed needs a pillow marker: {text}");
    assert!(
        text.contains("BED"),
        "bed needs one interaction marker: {text}"
    );
    assert!(!text.contains("WINDOW"));
    assert!(!text.contains("SHELF"));
    assert!(!text.contains("DESK"));
    assert!(!text.contains("PET"));
}

/// Full mode is selected by height, so a sub-80-column Full room must retain
/// its bedroom anchors rather than silently falling through to no furniture.
#[test]
fn narrow_full_retains_window_desk_and_shelf() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 70, 14);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    assert!(
        text.contains("▤"),
        "narrow Full laptop desk missing: {text}"
    );
    assert!(text.contains("╱╲"), "narrow Full shelf missing: {text}");
}

#[test]
fn full_mascot_and_care_targets_have_release_geometry() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 14);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    assert!((6..=7).contains(&geometry.pet.height));
    assert!(geometry.pet.width >= 16);
    let bed = geometry.bed.expect("Full always has a bed");
    assert!(bed.right() < area.right(), "bed needs a right-edge margin");
    for source in &geometry.food_sources {
        assert!(!rects_overlap(geometry.pet, source.rect));
        assert_eq!(source.rect.y, area.y.saturating_add(8));
        assert!(source.rect.bottom() <= area.y.saturating_add(12));
    }

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let pet_pixels = (geometry.pet.x..geometry.pet.right())
        .flat_map(|x| (geometry.pet.y..geometry.pet.bottom()).map(move |y| (x, y)))
        .filter(|&(x, y)| buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " "))
        .count();
    assert!(
        pet_pixels >= 30,
        "Full mascot is still a hollow placeholder"
    );
    assert!(
        geometry.food_sources.iter().all(|source| {
            (source.rect.x..source.rect.right()).any(|x| {
                (source.rect.y..source.rect.bottom())
                    .any(|y| buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " "))
            })
        }),
        "stocked food must render as physical objects"
    );
}

#[test]
fn full_room_has_a_quiet_visual_hierarchy() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 14);
    let geometry = room_geometry(area, &snapshot);
    let buffer = soft_green_room(area, &snapshot);

    assert_eq!(random_backdrop_fill_count(&buffer, area), 0);
    assert!(largest_tone3_component_is_inside(&buffer, geometry.pet));
    assert!(tone3_count_outside_targets(&buffer, &geometry) < 48);
    assert!(has_two_column_clearance_around_pet(&buffer, geometry.pet));
}

#[test]
fn full_bed_sprite_fits_its_23_column_hitbox() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 14);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Full always has a bed");
    assert_eq!(bed.width, 23);
    assert_eq!(bed.height, 7);

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let expected_rows = [
        "┌─────────────────────┐",
        "│  ┌───────────┐      │",
        "│  │  pillow   │      │",
        "│  └───────────┘      │",
        "│  ────────────────   │",
        "└─────────────────────┘",
        "       BED             ",
    ];
    for (row, expected) in expected_rows.iter().enumerate() {
        let rendered: String = (bed.x..bed.right())
            .map(|x| {
                buffer
                    .cell((x, bed.y + row as u16))
                    .expect("bed cell")
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert_eq!(rendered, *expected, "bed row {row} changed");
    }
}

#[test]
fn full_eighty_column_geometry_keeps_care_objects_between_furniture() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(8), now));
    let area = Rect::new(0, 0, 80, 14);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Full always has a bed");
    assert_eq!(bed.x, 56);
    assert!(geometry.pet.right() <= bed.x);
    assert_eq!(geometry.poops.len(), 1);
    for source in &geometry.food_sources {
        assert!(!rects_overlap(geometry.pet, source.rect));
        assert!(
            geometry
                .poops
                .iter()
                .all(|(_, poop)| !rects_overlap(*poop, source.rect))
        );
    }
    for (_, poop) in &geometry.poops {
        assert!(!rects_overlap(geometry.pet, *poop));
        assert!(!rects_overlap(bed, *poop));
    }

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    assert!(
        text.contains("☀") || text.contains("☾"),
        "compact furniture must retain a sparse window cue"
    );
}

#[test]
fn wide_full_poops_fit_the_actual_pantry_to_pet_interval() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    let poop_ids = [
        Uuid::from_u128(101),
        Uuid::from_u128(102),
        Uuid::from_u128(103),
    ];
    for id in poop_ids {
        snapshot.pending_poops.push(Poop::new(id, now));
    }

    let width120 = Rect::new(0, 0, 120, 14);
    let width120_geometry = room_geometry(width120, &snapshot);
    assert_eq!(
        width120_geometry
            .poops
            .iter()
            .map(|(_, rect)| rect.x)
            .collect::<Vec<_>>(),
        [52, 59, 66]
    );

    let widths = std::iter::once(100_u16).chain(80..=99).chain(101..=121);
    for width in widths {
        let area = Rect::new(0, 0, width, 14);
        let geometry = room_geometry(area, &snapshot);
        let bed = geometry.bed.expect("Full always has a bed");
        let food_right = geometry
            .food_sources
            .last()
            .expect("starter inventory has food")
            .rect
            .right();
        let poop_start = food_right.saturating_add(2);
        let poop_width = 5;
        let poop_spacing = 7;
        let available_poops =
            if poop_start.saturating_add(poop_width) > geometry.pet.x.saturating_sub(2) {
                0
            } else {
                usize::from(
                    geometry
                        .pet
                        .x
                        .saturating_sub(2)
                        .saturating_sub(poop_start.saturating_add(poop_width))
                        / poop_spacing,
                ) + 1
            };
        let expected_poops = available_poops.max(1).min(poop_ids.len());

        assert_eq!(
            geometry.poops.len(),
            expected_poops,
            "wide Full should use the available interval at width {width}"
        );
        assert_eq!(
            geometry.poops.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            poop_ids[..expected_poops],
            "wide Full must preserve poop target order at width {width}"
        );

        let targets = geometry
            .food_sources
            .iter()
            .map(|source| source.rect)
            .chain([geometry.pet, bed])
            .chain(geometry.poops.iter().map(|(_, rect)| *rect))
            .collect::<Vec<_>>();
        for (index, first) in targets.iter().enumerate() {
            for second in targets.iter().skip(index + 1) {
                assert!(
                    !rects_overlap(*first, *second),
                    "wide Full care targets overlap at width {width}: {first:?} and {second:?}"
                );
            }
        }
        assert!(
            geometry.poops.iter().all(|(_, poop)| {
                poop.right() <= area.right() && poop.bottom() <= area.bottom()
            })
        );
        assert!(bed.x >= geometry.pet.right().saturating_add(2));

        if available_poops > 0 {
            let (_, last_poop) = geometry.poops.last().expect("normal poop exists");
            assert!(
                last_poop.right().saturating_add(2) <= geometry.pet.x,
                "wide Full needs two clear columns before the pet at width {width}: poop={last_poop:?} pet={:?}",
                geometry.pet
            );
            let mut buffer = Buffer::filled(area, Cell::new(" "));
            render_room(area, &mut buffer, &snapshot, &default_frame(), None);
            assert!(
                has_two_column_clearance_around_pet(&buffer, geometry.pet),
                "wide Full pet clearance is not visible at width {width}"
            );
        }
    }

    let area = Rect::new(0, 0, 100, 14);
    let geometry = room_geometry(area, &snapshot);
    assert_eq!(geometry.pet.x, 56);
    assert_eq!(geometry.poops.len(), 2);
    let last_poop = geometry
        .poops
        .last()
        .expect("width 100 has a second poop")
        .1;
    assert!(last_poop.right().saturating_add(2) <= geometry.pet.x);

    let mut without_poops = base_snapshot(now);
    without_poops.pending_poops.clear();
    let mut clear_buffer = Buffer::filled(area, Cell::new(" "));
    let mut poop_buffer = Buffer::filled(area, Cell::new(" "));
    render_room(
        area,
        &mut clear_buffer,
        &without_poops,
        &default_frame(),
        None,
    );
    render_room(area, &mut poop_buffer, &snapshot, &default_frame(), None);
    for y in geometry.pet.y..geometry.pet.bottom() {
        for x in geometry.pet.x..geometry.pet.right() {
            assert_eq!(
                poop_buffer.cell((x, y)).expect("poop pet cell").symbol(),
                clear_buffer.cell((x, y)).expect("clear pet cell").symbol(),
                "width-100 poop rendering must not erase the pet at ({x}, {y})"
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

fn rect_contains(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && inner.right() <= outer.right()
        && outer.y <= inner.y
        && inner.bottom() <= outer.bottom()
}

/// Full-room geometry is an absolute physical-terminal model even when the
/// room is the lower pane of a composed terminal. The 80/81-column fallback
/// poop intentionally occupies the same rectangle as the reserved care lane.
#[test]
fn full_wide_geometry_uses_absolute_coordinates_at_nonzero_origins() {
    let now = Utc::now();
    let poop_id = Uuid::from_u128(0x8001);
    for area in [
        Rect::new(0, 31, 80, 14),
        Rect::new(0, 31, 81, 14),
        Rect::new(7, 31, 80, 14),
    ] {
        let mut snapshot = base_snapshot(now);
        snapshot.pending_poops.push(Poop::new(poop_id, now));

        let geometry = room_geometry(area, &snapshot);
        let reserved = wide_full_care_zone(area);
        let (_, poop) = geometry
            .poops
            .iter()
            .find(|(id, _)| *id == poop_id)
            .expect("authoritative poop must remain visible");

        assert!(rect_contains(area, geometry.pet), "pet escaped {area:?}");
        assert!(rect_contains(area, geometry.bed.expect("Full bed")));
        assert!(
            geometry
                .food_sources
                .iter()
                .all(|source| rect_contains(area, source.rect))
        );
        assert!(
            rect_contains(area, *poop),
            "poop escaped {area:?}: {poop:?}"
        );
        assert_eq!(poop.y, area.y + 8, "fallback must use absolute y");
        assert!(rect_contains(reserved, *poop), "poop must be contained");
        assert_eq!(reserved, *poop, "fallback and reserved care target align");
    }
}

/// The renderer subtracts the physical room origin only for presentation;
/// the visible fallback object must still land in the same physical cells as
/// the absolute geometry used by `poop_hit`.
#[test]
fn full_wide_fallback_poop_render_and_hitbox_share_nonzero_origin() {
    let now = Utc::now();
    let poop_id = Uuid::from_u128(0x8002);
    let mut snapshot = base_snapshot(now);
    snapshot.pending_poops.push(Poop::new(poop_id, now));
    let area = Rect::new(7, 31, 80, 14);
    let geometry = room_geometry(area, &snapshot);
    let (_, poop) = geometry.poops.first().expect("fallback poop");

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);

    let visible_cell = Position::new(poop.x + 1, poop.y);
    assert_eq!(geometry.poop_hit(visible_cell), Some(poop_id));
    assert_eq!(
        buffer
            .cell(visible_cell)
            .expect("physical poop cell")
            .symbol(),
        "╭",
        "visible fallback poop must be drawn at its geometry origin"
    );
}

#[derive(Default)]
struct RecordingCareGateway {
    requests: Mutex<Vec<RoomCareRequest>>,
}

impl CareGateway for RecordingCareGateway {
    fn feed(&self, _action_id: Uuid, _food_id: &str) {}

    fn clean(&self, action_id: Uuid, poop_id: Uuid) {
        self.requests
            .lock()
            .unwrap()
            .push(RoomCareRequest::Clean { action_id, poop_id });
    }

    fn nap(&self, _action_id: Uuid) {}

    fn pet(&self, _action_id: Uuid, _interaction_ms: u64, _pointer_distance: f32) {}

    fn pet_stroke(&self, _action_id: Uuid, _duration_ms: u64, _distance: f64) {}
}

/// The care-first 80/81-column fallback must reserve a lane the wandering pet
/// can never enter. Otherwise an authoritative poop remains visible but a left
/// press is captured by pet hit-testing instead of producing Clean.
#[test]
fn care_first_poop_survives_every_allowed_full_wander_offset() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    let poop_id = Uuid::from_u128(81);
    snapshot.pending_poops.push(Poop::new(poop_id, now));

    for width in [80_u16, 81] {
        let area = Rect::new(0, 0, width, 14);
        for offset_x in -3_i16..=0 {
            for offset_y in [2_i16, 3] {
                let frame = PresentationFrame {
                    pose: PetPose::WalkA,
                    offset: (offset_x, offset_y),
                };
                let geometry = room_geometry_with_frame(area, &snapshot, &frame);
                let (_, poop) = geometry.poops.first().expect("authoritative poop");
                assert!(
                    rects_overlap(wide_full_care_zone(area), *poop),
                    "reserved zone must contain the actual poop target: width={width} zone={:?} poop={poop:?}",
                    wide_full_care_zone(area)
                );
                let mut input = RoomInputSession::default();
                let gateway = RecordingCareGateway::default();
                let down_requests = input.process(
                    area,
                    &snapshot,
                    &frame,
                    &MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: poop.x + poop.width / 2,
                        row: poop.y + 1,
                        modifiers: KeyModifiers::NONE,
                    },
                );
                assert!(down_requests.is_empty());
                let requests = input.process(
                    area,
                    &snapshot,
                    &frame,
                    &MouseEvent {
                        kind: MouseEventKind::Up(MouseButton::Left),
                        column: poop.x + poop.width / 2,
                        row: poop.y + 1,
                        modifiers: KeyModifiers::NONE,
                    },
                );
                match requests.as_slice() {
                    [
                        RoomCareRequest::Clean {
                            poop_id: cleaned, ..
                        },
                    ] => {
                        assert_eq!(*cleaned, poop_id);
                    }
                    unexpected => panic!(
                        "click did not produce Clean: width={width} offset=({offset_x},{offset_y}) requests={unexpected:?}"
                    ),
                }
                assert!(gateway.requests.lock().unwrap().is_empty());
            }
        }
    }
}

/// A real room input lifecycle clicks the visible physical fallback target.
/// The pet hitbox must not capture this press, and the release must produce
/// exactly one authoritative clean request at both narrow Full widths.
#[test]
fn nonzero_origin_full_fallback_poop_click_cleans_once_without_pet_capture() {
    let now = Utc::now();
    let poop_id = Uuid::from_u128(0x8003);
    for width in [80_u16, 81] {
        let area = Rect::new(7, 31, width, 14);
        let mut snapshot = base_snapshot(now);
        snapshot.pending_poops.push(Poop::new(poop_id, now));
        let visible_target = wide_full_care_zone(area);
        let point = Position::new(visible_target.x + 2, visible_target.y + 1);
        let mut input = RoomInputSession::default();

        assert!(
            input
                .process(
                    area,
                    &snapshot,
                    &default_frame(),
                    &MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: point.x,
                        row: point.y,
                        modifiers: KeyModifiers::NONE,
                    },
                )
                .is_empty()
        );
        assert!(
            !input.has_active_capture(),
            "fallback poop press must not be captured by the pet"
        );

        let requests = input.process(
            area,
            &snapshot,
            &default_frame(),
            &MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: point.x,
                row: point.y,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(requests.len(), 1, "width={width} requests={requests:?}");
        assert!(matches!(
            &requests[0],
            RoomCareRequest::Clean { poop_id: cleaned, .. } if *cleaned == poop_id
        ));
    }
}

/// Exercise reachable presentation frames rather than a guessed offset set.
/// The moving pet remains outside the actual fallback care lane while offsets
/// still change, proving the exclusion is not implemented by freezing it.
#[test]
fn nonzero_origin_full_wander_avoids_actual_fallback_care_zone_without_freezing() {
    let now = Utc::now();
    let poop_id = Uuid::from_u128(0x8004);
    let area = Rect::new(7, 31, 80, 14);
    let care_zone = wide_full_care_zone(area);
    let mut snapshot = base_snapshot(now);
    snapshot.pending_poops.push(Poop::new(poop_id, now));
    let mut observed_offsets = std::collections::HashSet::new();
    let mut saw_walking = false;

    for seed in 0..64_u64 {
        let mut presentation = PresentationState::new(seed);
        for tick in 0..=240_u64 {
            let frame = presentation.tick(
                std::time::Duration::from_millis(tick * 250),
                Some(&snapshot),
                area,
            );
            let geometry = room_geometry_with_frame(area, &snapshot, &frame);
            assert!(
                !rects_overlap(geometry.pet, care_zone),
                "reachable frame entered care zone: seed={seed} tick={tick} frame={frame:?} pet={:?} zone={care_zone:?}",
                geometry.pet
            );
            observed_offsets.insert(frame.offset);
            saw_walking |= matches!(frame.pose, PetPose::WalkA | PetPose::WalkB);
        }
    }

    assert!(saw_walking, "presentation must still wander in Full");
    assert!(
        observed_offsets.len() > 1,
        "presentation offsets must change instead of freezing"
    );
}

/// Compact keeps one subdued window cue; Full-only furniture disappears before
/// care functionality.
#[test]
fn compact_room_keeps_window_decoration() {
    let snapshot = base_snapshot(Utc::now());
    let compact = Rect::new(0, 0, 120, 7);
    let mut buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(compact, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, compact.width, compact.height);
    assert!(
        text.contains("┌──────────┐"),
        "Compact room should keep the window: {text}"
    );
}

#[test]
fn compact_is_a_seven_row_vignette_with_segmented_needs_and_care_objects() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(13), now));
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(14),
        PetDemandKind::Affection,
        now,
    ));
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(15),
        PetDemandKind::Snack,
        now,
    ));

    let area = Rect::new(0, 0, 120, 7);
    let geometry = room_geometry(area, &snapshot);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);

    for marker in [
        "HUNGER",
        "ENERGY",
        "HAPPY",
        "CLEAN",
        "Calm  A1 S1",
        "FOOD",
        "BED",
        "POOP",
    ] {
        assert!(text.contains(marker), "Compact missing {marker}: {text}");
    }
    assert!(text.contains("A1") && text.contains("S1"));
    assert!(
        !text.contains("PET"),
        "Compact should use the mascot/effect as its pet cue, not a debug label: {text}"
    );
    assert!(
        geometry.pet.height >= 4,
        "Compact pet hitbox must cover the focal sprite: {:?}",
        geometry.pet
    );
    assert!(
        (geometry.pet.y..geometry.pet.bottom()).any(|y| {
            (geometry.pet.x..geometry.pet.right())
                .any(|x| buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " "))
        }),
        "Compact pet sprite must occupy its hitbox"
    );
    assert!(
        !text.contains("╭──────────────────────╮"),
        "Compact must remove Full desk decoration"
    );
}

#[test]
fn compact_status_strip_has_exact_named_segments() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 7);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);

    let expected = "HUNGER ░░░░░░░░  ENERGY ████████  HAPPY ████████  CLEAN ████████";
    assert_eq!(
        row_text(&buffer, area.width, 0)
            .chars()
            .take(expected.chars().count())
            .collect::<String>(),
        expected
    );
    assert_eq!(row_text(&buffer, area.width, 1).trim_end(), "Calm");
}

#[test]
fn compact_uses_named_segmented_need_indicators() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 7);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);

    for marker in ["HUNGER", "ENERGY", "HAPPY", "CLEAN"] {
        assert!(
            text.contains(marker),
            "Compact missing named need {marker}: {text}"
        );
    }
    assert!(
        text.contains('█') && text.contains('░'),
        "Compact needs must use visible filled and empty segments: {text}"
    );
    assert!(!text.contains("H 0 E 100 P 100 C 100"));
}

#[test]
fn compact_renders_every_authoritative_pooped_target() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    for index in 0..3_u128 {
        snapshot
            .pending_poops
            .push(Poop::new(Uuid::from_u128(100 + index), now));
    }
    let area = Rect::new(0, 0, 120, 7);
    let geometry = room_geometry(area, &snapshot);
    assert_eq!(
        geometry.poops.len(),
        3,
        "Compact must retain every visible poop"
    );

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    for (_, rect) in &geometry.poops {
        assert!(
            (rect.y..rect.bottom()).any(|y| {
                (rect.x..rect.right())
                    .any(|x| buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " "))
            }),
            "Compact poop target has no rendered counterpart: {rect:?}"
        );
    }
    let text = buffer_text(&buffer, area.width, area.height);
    assert_eq!(text.matches("POOP").count(), 3, "{text}");
}

#[test]
fn compact_decorations_are_separate_from_care_targets() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(101), now));
    let area = Rect::new(0, 0, 120, 7);
    let geometry = room_geometry(area, &snapshot);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);

    let window_x =
        find_row_text(&buffer, area.width, 2, "┌──────────┐").expect("Compact window border");
    let window = Rect::new(window_x, 2, 12, 3);
    let text = buffer_text(&buffer, area.width, area.height);
    assert!(!text.contains("┌───┐"), "Compact must remove the plant cue");
    assert!(
        !text.contains("╭──────────────╮"),
        "Compact must remove the Full desk"
    );
    for target in geometry
        .food_sources
        .iter()
        .map(|source| source.rect)
        .chain(geometry.poops.iter().map(|(_, rect)| *rect))
    {
        assert!(
            !rects_overlap(window, target),
            "window overlaps care target {target:?}"
        );
    }
}

#[test]
fn minimal_packs_the_pet_across_three_rows_and_keeps_every_core_target() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(16), now));
    let area = Rect::new(0, 0, 120, 3);
    let geometry = room_geometry(area, &snapshot);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);

    assert_eq!(geometry.pet.width, 9);
    assert_eq!(geometry.pet.height, 3);
    assert_eq!(geometry.pet.x, area.x);
    assert_eq!(geometry.pet.y, area.y);
    for y in geometry.pet.y..geometry.pet.bottom() {
        assert!(
            (geometry.pet.x..geometry.pet.right())
                .any(|x| { buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " ") }),
            "Minimal sprite row {y} is empty"
        );
    }
    assert!(
        !text.contains("◉ PET") && !text.contains("(=^.^=) PET"),
        "Minimal should render a mascot instead of a text-only PET control: {text}"
    );
    for marker in ["H", "E", "P", "C", "[FOOD", "[BED]", "[POOP]", "AFF"] {
        assert!(text.contains(marker), "Minimal missing {marker}: {text}");
    }
    for source in &geometry.food_sources {
        assert!(source.rect.right() <= area.right());
    }
    if let Some(bed) = geometry.bed {
        assert!(bed.right() <= area.right());
    }
    for (_, poop) in &geometry.poops {
        assert!(poop.right() <= area.right());
    }
}

#[test]
fn compact_to_minimal_removes_scenery_but_preserves_care_hit_regions() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(17), now));

    let compact = Rect::new(0, 0, 120, 7);
    let mut compact_buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(
        compact,
        &mut compact_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let compact_text = buffer_text(&compact_buffer, compact.width, compact.height);
    assert!(compact_text.contains("┌──────────┐"));

    let minimal = Rect::new(0, 0, 120, 3);
    let minimal_geometry = room_geometry(minimal, &snapshot);
    let mut minimal_buffer = Buffer::filled(minimal, Cell::new(" "));
    render_room(
        minimal,
        &mut minimal_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let minimal_text = buffer_text(&minimal_buffer, minimal.width, minimal.height);
    assert!(!minimal_text.contains("┌──────────┐"));
    assert!(!minimal_text.contains("╭──────────────╮"));
    assert!(minimal_geometry.bed.is_some());
    assert!(!minimal_geometry.food_sources.is_empty());
    assert!(!minimal_geometry.poops.is_empty());
    assert!(minimal_text.contains("[FOOD") && minimal_text.contains("[BED]"));
}

#[test]
fn minimal_target_labels_are_drawn_inside_their_hit_regions() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(18), now));
    let area = Rect::new(0, 0, 120, 3);
    let geometry = room_geometry(area, &snapshot);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);

    let visible = |rect: Rect| {
        (rect.y..rect.bottom()).any(|y| {
            (rect.x..rect.right())
                .any(|x| buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " "))
        })
    };
    assert!(
        visible(geometry.pet),
        "Minimal mascot is outside its hitbox"
    );
    assert!(
        geometry
            .food_sources
            .iter()
            .all(|source| visible(source.rect)),
        "Minimal food affordance is outside its hitbox"
    );
    assert!(
        geometry.bed.is_some_and(visible),
        "Minimal bed affordance is outside its hitbox"
    );
    assert!(
        geometry.poops.iter().all(|(_, rect)| visible(*rect)),
        "Minimal poop affordance is outside its hitbox"
    );
}

#[test]
fn minimal_narrow_targets_are_packed_without_collisions() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    for index in 0..3_u128 {
        snapshot
            .pending_poops
            .push(Poop::new(Uuid::from_u128(30 + index), now));
    }

    for width in [24, 32, 40] {
        let area = Rect::new(0, 0, width, 3);
        let geometry = room_geometry(area, &snapshot);
        let mut buffer = Buffer::filled(area, Cell::new(" "));
        render_room(area, &mut buffer, &snapshot, &default_frame(), None);
        let targets = geometry
            .food_sources
            .iter()
            .map(|source| source.rect)
            .chain(geometry.bed.iter().copied())
            .chain(geometry.poops.iter().map(|(_, rect)| *rect))
            .collect::<Vec<_>>();

        for (index, target) in targets.iter().enumerate() {
            assert!(
                target.width > 0,
                "zero-width target at width {width}: {target:?}"
            );
            assert!(
                target.right() <= area.right(),
                "target clipped at width {width}: {target:?}"
            );
            assert!(
                (target.x..target.right()).any(|x| {
                    buffer
                        .cell((x, target.y))
                        .is_some_and(|cell| cell.symbol() != " ")
                }),
                "target has no visible label at width {width}: {target:?}"
            );
            for other in targets.iter().skip(index + 1) {
                assert!(
                    !rects_overlap(*target, *other),
                    "targets overlap at width {width}: {target:?} and {other:?}"
                );
            }
        }
    }
}

#[test]
fn minimal_renderer_uses_the_selected_narrow_target_labels() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(40), now));
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(41), now));

    let area = Rect::new(0, 0, 24, 3);
    let geometry = room_geometry(area, &snapshot);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);

    let row = |rect: Rect| {
        (rect.x..rect.right())
            .map(|x| {
                buffer
                    .cell((x, rect.y))
                    .expect("minimal target cell")
                    .symbol()
            })
            .collect::<String>()
    };
    assert_eq!(row(geometry.food_sources[0].rect), "F50");
    assert_eq!(row(geometry.bed.expect("minimal bed")), "B");
    assert_eq!(row(geometry.poops[0].1), "P");
    assert_eq!(row(geometry.poops[1].1), "P");
}

#[test]
fn minimal_renderer_uses_the_selected_no_food_label() {
    let now = Utc::now();
    let pet = Pet::with_inventory(
        Uuid::from_u128(42),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::default(),
    );
    let snapshot = PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    let area = Rect::new(0, 0, 24, 3);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    assert!(
        row_text(&buffer, area.width, 1).contains("F-"),
        "narrow Minimal should use the packed disabled-food label: {}",
        row_text(&buffer, area.width, 1)
    );
}

/// Minimal exposes exactly one deterministic stocked food source, regardless
/// of which subset of the pantry is available, and never advertises an
/// actionable zero-stock source.
#[test]
fn minimal_food_source_matches_every_nonempty_inventory_combination() {
    let now = Utc::now();
    let foods = [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ];
    for mask in 1u8..(1u8 << foods.len()) {
        let pet = Pet::with_inventory(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, now, {
            let mut inventory = FoodInventory::new();
            for (index, food) in foods.iter().copied().enumerate() {
                if mask & (1 << index) != 0 {
                    inventory.add(food, (index as u32 + 1) * 3);
                }
            }
            inventory
        });
        let snapshot =
            PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
        let geometry = codegotchi_cli::terminal::room_geometry_with_frame(
            Rect::new(0, 0, 40, 3),
            &snapshot,
            &default_frame(),
        );
        let expected = foods
            .iter()
            .copied()
            .find(|food| {
                mask & (1
                    << foods
                        .iter()
                        .position(|candidate| candidate == food)
                        .unwrap())
                    != 0
            })
            .expect("nonempty mask has one stocked food");
        assert_eq!(geometry.food_sources.len(), 1, "mask={mask:04b}");
        assert_eq!(
            geometry.food_sources[0].food_id,
            expected.id(),
            "mask={mask:04b}"
        );
        assert!(geometry.food_sources[0].count > 0, "mask={mask:04b}");
    }
}

#[test]
fn minimal_with_no_food_has_disabled_food_copy_and_no_hit_source() {
    let now = Utc::now();
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::new(),
    );
    let snapshot = PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    let area = Rect::new(0, 0, 40, 3);
    let geometry =
        codegotchi_cli::terminal::room_geometry_with_frame(area, &snapshot, &default_frame());
    assert!(geometry.food_sources.is_empty());

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    assert!(
        text.contains("FOOD none"),
        "disabled food copy missing: {text}"
    );
    assert!(
        !text.contains("[FOOD x0]"),
        "zero-stock food must not look actionable: {text}"
    );
}

#[test]
fn wide_full_bed_has_one_visible_label() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 14);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    assert_eq!(
        text.matches("BED").count(),
        1,
        "bed label duplicated: {text}"
    );
}

/// Pending affection/snack demands are projected as care affordances in the
/// Full room.
#[test]
fn full_room_renders_pending_demand_chips() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(2),
        PetDemandKind::Affection,
        now,
    ));
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(3),
        PetDemandKind::Snack,
        now,
    ));
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(4),
        PetDemandKind::Snack,
        now,
    ));

    let full = Rect::new(0, 0, 120, 14);
    let mut buffer = Buffer::filled(full, Cell::new(" "));
    render_room(full, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, full.width, full.height);
    assert!(
        text.contains("Affection x1"),
        "Full room missing affection chip: {text}"
    );
    assert!(
        text.contains("Snack x2"),
        "Full room missing snack chip: {text}"
    );

    // Compact and Minimal also surface demand counts.
    let compact = Rect::new(0, 0, 120, 7);
    let mut compact_buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(
        compact,
        &mut compact_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let compact_text = buffer_text(&compact_buffer, compact.width, compact.height);
    assert!(
        compact_text.contains("A1 S2"),
        "Compact status missing demand counts: {compact_text}"
    );

    let minimal = Rect::new(0, 0, 120, 3);
    let mut minimal_buffer = Buffer::filled(minimal, Cell::new(" "));
    render_room(
        minimal,
        &mut minimal_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let minimal_text = buffer_text(&minimal_buffer, minimal.width, minimal.height);
    assert!(
        minimal_text.contains("AFF") && minimal_text.contains("SNACK"),
        "Minimal affordances missing demands: {minimal_text}"
    );
}

/// The active food drag renders a visible ghost at the pointer cell.
#[test]
fn drag_ghost_renders_at_the_pointer_cell() {
    let snapshot = base_snapshot(Utc::now());
    let full = Rect::new(0, 0, 120, 14);
    let mut buffer = Buffer::filled(full, Cell::new(" "));
    render_room(
        full,
        &mut buffer,
        &snapshot,
        &default_frame(),
        Some(("kibble", Position::new(30, 10))),
    );
    let cell = buffer.cell((30, 10)).expect("ghost cell exists");
    assert_eq!(
        cell.symbol(),
        "K",
        "ghost should draw the food label at the pointer"
    );
    let text = buffer_text(&buffer, full.width, full.height);
    assert!(
        text.contains("KIB"),
        "ghost label should render in the room text"
    );
}

/// Auto separates logical tone selection from concrete host colors: its
/// endpoints are the terminal defaults and intermediates are sampled later.
#[test]
fn auto_theme_maps_endpoints_to_terminal_defaults_without_named_grays() {
    let tone0 = auto_style(SemanticTone::Tone0);
    assert_eq!(tone0.bg, Some(Color::Reset), "Tone0 resets the background");
    assert_eq!(
        tone0.fg, None,
        "Tone0 leaves the terminal default foreground"
    );

    let palette = TerminalThemePreset::Auto.resolve();
    for tone in [SemanticTone::Tone1, SemanticTone::Tone2] {
        let style = palette.style(tone);
        assert_ne!(style.fg, Some(Color::DarkGray));
        assert_ne!(style.fg, Some(Color::Gray));
        assert_eq!(style.fg, Some(Color::Reset));
    }

    let tone3 = auto_style(SemanticTone::Tone3);
    assert_eq!(
        tone3.fg,
        Some(Color::Reset),
        "Tone3 resets to the foreground"
    );
}

#[test]
fn auto_samples_intermediates_with_a_four_by_four_bayer_pattern() {
    let palette = TerminalThemePreset::Auto.resolve();
    let mut tone1_foreground = 0;
    let mut tone2_foreground = 0;

    for y in 0..4u16 {
        for x in 0..4u16 {
            if palette.sample_logical_tone(SemanticTone::Tone1, x, y) == SemanticTone::Tone3 {
                tone1_foreground += 1;
            }
            if palette.sample_logical_tone(SemanticTone::Tone2, x, y) == SemanticTone::Tone3 {
                tone2_foreground += 1;
            }
        }
    }

    assert_eq!(tone1_foreground, 5);
    assert_eq!(tone2_foreground, 10);
    assert!(
        tone1_foreground < tone2_foreground,
        "Tone1 must have lower foreground coverage than Tone2"
    );
}

#[test]
fn fixed_presets_sample_authored_tones_without_coordinates() {
    let expectations = [
        (
            TerminalThemePreset::Mono,
            [
                Color::Rgb(8, 8, 8),
                Color::Rgb(72, 72, 72),
                Color::Rgb(156, 156, 156),
                Color::Rgb(236, 236, 236),
            ],
        ),
        (
            TerminalThemePreset::SoftGreen,
            [
                Color::Rgb(7, 15, 12),
                Color::Rgb(24, 74, 45),
                Color::Rgb(96, 166, 112),
                Color::Rgb(166, 220, 177),
            ],
        ),
        (
            TerminalThemePreset::Amber,
            [
                Color::Rgb(18, 12, 4),
                Color::Rgb(112, 64, 16),
                Color::Rgb(196, 126, 38),
                Color::Rgb(255, 212, 112),
            ],
        ),
        (
            TerminalThemePreset::Night,
            [
                Color::Rgb(6, 10, 24),
                Color::Rgb(32, 64, 128),
                Color::Rgb(92, 132, 204),
                Color::Rgb(202, 220, 255),
            ],
        ),
    ];

    for (preset, colors) in expectations {
        let palette = preset.resolve();
        for (index, &color) in colors.iter().enumerate() {
            let tone = match index {
                0 => SemanticTone::Tone0,
                1 => SemanticTone::Tone1,
                2 => SemanticTone::Tone2,
                _ => SemanticTone::Tone3,
            };
            assert_eq!(
                palette.sample_logical_tone(tone, 3, 2),
                tone,
                "{preset} must preserve {tone:?}"
            );
            let style = palette.style(tone);
            if index == 0 {
                assert_eq!(style.bg, Some(color), "{preset} {tone:?}");
            } else {
                assert_eq!(style.fg, Some(color), "{preset} {tone:?}");
            }
        }
    }
}

#[test]
fn auto_rendering_uses_only_terminal_defaults_and_is_deterministic() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(3, 2, 120, 14);
    let options = RoomRenderOptions::for_theme(TerminalThemePreset::Auto, RoomAmbience::Day);
    let mut first = Buffer::filled(area, Cell::new(" "));
    let mut second = Buffer::filled(area, Cell::new(" "));
    render_room_with_options(area, &mut first, &snapshot, &default_frame(), options, None);
    render_room_with_options(
        area,
        &mut second,
        &snapshot,
        &default_frame(),
        options,
        None,
    );

    assert_eq!(
        first, second,
        "identical renders must produce identical cells"
    );
    for cell in first.content.iter() {
        let style = cell.style();
        assert_ne!(style.fg, Some(Color::DarkGray));
        assert_ne!(style.fg, Some(Color::Gray));
        assert_eq!(style.fg, Some(Color::Reset));
        assert_eq!(style.bg, Some(Color::Reset));
    }
}

#[test]
fn every_terminal_theme_preset_parses_and_resolves_all_semantic_tones() {
    let values = [
        ("auto", TerminalThemePreset::Auto),
        ("mono", TerminalThemePreset::Mono),
        ("soft-green", TerminalThemePreset::SoftGreen),
        ("amber", TerminalThemePreset::Amber),
        ("night", TerminalThemePreset::Night),
    ];

    for (value, expected) in values {
        let parsed = value
            .parse::<TerminalThemePreset>()
            .unwrap_or_else(|error| panic!("{value} should parse: {error}"));
        assert_eq!(parsed, expected, "parsed preset for {value}");
        assert_eq!(parsed.to_string(), value, "displayed preset for {value}");

        let palette = parsed.resolve();
        for tone in [
            SemanticTone::Tone0,
            SemanticTone::Tone1,
            SemanticTone::Tone2,
            SemanticTone::Tone3,
        ] {
            let style = palette.style(tone);
            assert!(
                style.fg.is_some() || style.bg.is_some(),
                "{value} must resolve a concrete style for {tone:?}"
            );
        }
    }
}

#[test]
fn selected_palette_reaches_room_background_furniture_and_sprite_cells() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 14);
    let options = RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Day);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room_with_options(
        area,
        &mut buffer,
        &snapshot,
        &default_frame(),
        options,
        None,
    );

    assert_eq!(
        buffer.cell((119, 13)).expect("background cell").style().bg,
        Some(Color::Rgb(7, 15, 12)),
        "empty room cells use the selected background"
    );
    assert_eq!(
        buffer.cell((22, 1)).expect("window border cell").style().fg,
        Some(Color::Rgb(96, 166, 112)),
        "furniture borders use the selected Tone2 style"
    );
    assert_eq!(
        buffer.cell((22, 1)).expect("window border cell").style().bg,
        Some(Color::Rgb(7, 15, 12)),
        "foreground cells retain the selected room background"
    );
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| matches!(cell.symbol(), "█" | "▀" | "▄")
                && cell.style().fg == Some(Color::Rgb(166, 220, 177))),
        "packed pet/furniture pixels use the selected Tone3 style"
    );
}

#[test]
fn full_window_ambience_is_deterministic_and_does_not_change_care_projection() {
    let snapshot = base_snapshot(Utc::now());
    let before = snapshot.clone();
    let area = Rect::new(0, 0, 120, 14);
    let mut day = Buffer::filled(area, Cell::new(" "));
    let mut night = Buffer::filled(area, Cell::new(" "));
    render_room_with_options(
        area,
        &mut day,
        &snapshot,
        &default_frame(),
        RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Day),
        None,
    );
    render_room_with_options(
        area,
        &mut night,
        &snapshot,
        &default_frame(),
        RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Night),
        None,
    );

    let day_text = buffer_text(&day, area.width, area.height);
    let night_text = buffer_text(&night, area.width, area.height);
    assert!(day_text.contains('☀'), "day window should show a sun");
    assert!(night_text.contains('☾'), "night window should show a moon");
    assert_ne!(
        day_text, night_text,
        "day and night must be visibly distinct"
    );
    assert_eq!(snapshot, before, "ambience must not mutate care state");

    for height in [7, 3] {
        let area = Rect::new(0, 0, 120, height);
        let mut day = Buffer::filled(area, Cell::new(" "));
        let mut night = Buffer::filled(area, Cell::new(" "));
        render_room_with_options(
            area,
            &mut day,
            &snapshot,
            &default_frame(),
            RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Day),
            None,
        );
        render_room_with_options(
            area,
            &mut night,
            &snapshot,
            &default_frame(),
            RoomRenderOptions::for_theme(TerminalThemePreset::SoftGreen, RoomAmbience::Night),
            None,
        );
        assert_eq!(
            buffer_text(&day, area.width, area.height),
            buffer_text(&night, area.width, area.height),
            "Compact/Minimal care projections do not depend on Full ambience"
        );
    }
}

#[test]
fn full_pet_bed_and_sleep_markers_stay_within_their_contexts() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = Some(now + Duration::minutes(30));
    let area = Rect::new(0, 0, 120, 14);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Full always has a bed");
    assert!(
        geometry.pet.x >= bed.x && geometry.pet.right() <= bed.right(),
        "Full sleeping pet should remain inside the bed hit context: pet={:?} bed={bed:?}",
        geometry.pet
    );

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    assert_ne!(buffer.cell((0, 12)).expect("room cell").symbol(), "z");
    assert!(
        (bed.x..bed.x.saturating_add(bed.width)).any(|x| buffer
            .cell((x, bed.y.saturating_sub(1)))
            .is_some_and(|cell| cell.symbol() == "z")),
        "bed sleep markers should sit immediately above the bed"
    );
}

#[test]
fn full_authoritative_sleep_hitbox_matches_the_rendered_bed_pet() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = Some(now + Duration::minutes(30));
    let area = Rect::new(0, 0, 120, 14);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Full always has a bed");

    assert_eq!(
        geometry.pet,
        Rect::new(bed.x + 2, bed.y, 18, 7),
        "Full bed-sleep hitbox must match the rendered sleep sprite"
    );
}

#[test]
fn compact_authoritative_sleep_hitbox_matches_the_rendered_bed_pet() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = Some(now + Duration::minutes(30));
    let area = Rect::new(0, 0, 120, 7);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Compact always has a bed");

    assert_eq!(
        geometry.pet,
        Rect::new(bed.x + 11, bed.y, 12, 5),
        "Compact bed-sleep hitbox must match the rendered sleep sprite"
    );
}

#[test]
fn minimal_keeps_a_pet_and_recognizable_care_affordance_icons() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 3);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    for marker in ["FOOD", "BED", "POOP", "AFF"] {
        assert!(
            text.contains(marker),
            "Minimal affordance missing {marker}: {text}"
        );
    }
    assert!(
        !text.contains("PET"),
        "Minimal should render a mascot instead of a text-only PET control: {text}"
    );
}

/// Wide production layouts must expose visible object-shaped care targets,
/// rather than reducing the room to counters and placeholder dots.
#[test]
fn wide_room_keeps_named_targets_and_minimal_keeps_a_pet_mark() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(9), now));

    let full = Rect::new(0, 0, 120, 14);
    let mut full_buffer = Buffer::filled(full, Cell::new(" "));
    render_room(full, &mut full_buffer, &snapshot, &default_frame(), None);
    let full_text = buffer_text(&full_buffer, full.width, full.height);
    let full_geometry = codegotchi_cli::terminal::room_geometry(full, &snapshot);
    let food = full_geometry
        .food_sources
        .first()
        .expect("starter food source");
    let poop = full_geometry.poops.first().expect("seeded poop").1;
    assert_eq!(
        full_buffer
            .cell((food.rect.x + 2, food.rect.y))
            .expect("food bowl")
            .symbol(),
        "╭"
    );
    assert_eq!(
        full_buffer
            .cell((poop.x + 1, poop.y + 1))
            .expect("poop body")
            .symbol(),
        "─"
    );
    assert!(
        full_text.contains("pillow") && full_text.contains("BED"),
        "wide Full bed must retain physical detail and its care cue: {full_text}"
    );
    assert!(!full_text.contains("PET"));

    let compact = Rect::new(0, 0, 120, 7);
    let mut compact_buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(
        compact,
        &mut compact_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let compact_text = buffer_text(&compact_buffer, compact.width, compact.height);
    for marker in ["Calm", "FOOD", "BED", "POOP"] {
        assert!(
            compact_text.contains(marker),
            "wide Compact affordance missing {marker}: {compact_text}"
        );
    }

    let minimal = Rect::new(0, 0, 120, 3);
    let mut minimal_buffer = Buffer::filled(minimal, Cell::new(" "));
    render_room(
        minimal,
        &mut minimal_buffer,
        &snapshot,
        &default_frame(),
        None,
    );
    let minimal_text = buffer_text(&minimal_buffer, minimal.width, minimal.height);
    assert!(
        minimal_text.contains("CG ok") && !minimal_text.contains("PET"),
        "Minimal must retain the packed mascot without a debug PET label: {minimal_text}"
    );
}

#[test]
fn rendered_care_extents_are_inside_their_hit_regions() {
    let now = Utc::now();
    // The rendered count is part of the clickable food affordance. A test
    // that only checks a minimum rectangle size would miss a count growing
    // beyond that rectangle. Run each food kind in isolation so the assertions
    // cover every renderer label without another target overwriting it.
    for food_kind in [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ] {
        let mut snapshot = base_snapshot(now);
        snapshot.inventory = FoodInventory::default();
        snapshot.inventory.add(food_kind, 1_000_000);
        for area in [Rect::new(0, 0, 120, 14), Rect::new(0, 0, 120, 7)] {
            let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
            let food = geometry.food_sources.first().expect("starter food source");
            let mut buffer = Buffer::filled(area, Cell::new(" "));
            render_room(area, &mut buffer, &snapshot, &default_frame(), None);
            let food_name = match food.food_id {
                "kibble" => "KIB",
                "treat" => "TRT",
                "fruit" => "FRT",
                "energy_drink" => "ENE",
                other => panic!("unexpected food id {other}"),
            };
            let label = if area.height >= 14 {
                format!("{food_name} x{}", food.count)
            } else if area.width < 110 {
                if food_name == "FOOD" {
                    format!("FOOD{}", compact_count_for_test(food.count))
                } else {
                    format!("{}{}", food_name, compact_count_for_test(food.count))
                }
            } else {
                format!("FOOD x{}", food.count)
            };
            let food_rows = match food_kind {
                FoodKind::Kibble => ["  ╭─╮", " ╱·╲ ", "│···│"],
                FoodKind::Treat => ["  ╭─╮", " │≋│ ", " │ │ "],
                FoodKind::Fruit => ["  ╭╮ ", " ╱●╲ ", " │●│ "],
                FoodKind::EnergyDrink => ["  ╭─╮", " │+│ ", " │=│ "],
            };
            for (row, symbols) in food_rows.iter().enumerate() {
                for (offset, symbol) in symbols.chars().enumerate() {
                    if symbol == ' ' {
                        continue;
                    }
                    let point = Position::new(
                        food.rect.x.saturating_add(u16::try_from(offset).unwrap()),
                        food.rect.y.saturating_add(u16::try_from(row).unwrap()),
                    );
                    assert_eq!(
                        buffer.cell(point).expect("rendered food cell").symbol(),
                        symbol.to_string(),
                        "food projection must be rendered at its geometry anchor"
                    );
                    assert!(
                        food.rect.contains(point),
                        "food hit region must contain every rendered cell: rect={:?} point={point:?}",
                        food.rect
                    );
                }
            }
            let label_y = food.rect.y.saturating_add(3);
            for (offset, symbol) in label.chars().enumerate() {
                let point = Position::new(
                    food.rect.x.saturating_add(u16::try_from(offset).unwrap()),
                    label_y,
                );
                assert_eq!(
                    buffer.cell(point).expect("rendered food label").symbol(),
                    symbol.to_string(),
                    "food label must be rendered at its geometry anchor"
                );
                assert!(food.rect.contains(point));
            }
        }
    }

    let mut snapshot = base_snapshot(now);
    snapshot.inventory = FoodInventory::default();
    let poop_id = Uuid::from_u128(10);
    snapshot.pending_poops.push(Poop::new(poop_id, now));
    for area in [Rect::new(0, 0, 120, 14), Rect::new(0, 0, 120, 7)] {
        let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
        let mut buffer = Buffer::filled(area, Cell::new(" "));
        render_room(area, &mut buffer, &snapshot, &default_frame(), None);
        let (_, poop) = geometry.poops.first().expect("seeded poop");
        // The label is deliberately drawn over the final sprite row, so this
        // fixture describes the final rendered cells rather than the source
        // sprite in isolation.
        let poop_rows = if area.height >= 14 {
            [" ╭─╮ ", "╰──╯ ", "  ~  "]
        } else {
            [" ╭╮ ", "╰╯  ", " ~  "]
        };
        for (row, symbols) in poop_rows.iter().enumerate() {
            for (offset, symbol) in symbols.chars().enumerate() {
                if symbol == ' ' {
                    continue;
                }
                let point = Position::new(
                    poop.x.saturating_add(u16::try_from(offset).unwrap()),
                    poop.y.saturating_add(u16::try_from(row).unwrap()),
                );
                assert_eq!(
                    buffer.cell(point).expect("rendered poop cell").symbol(),
                    symbol.to_string(),
                    "poop sprite must be rendered at its geometry anchor"
                );
                assert!(
                    poop.contains(point),
                    "poop hit region must contain every rendered sprite cell: rect={poop:?} point={point:?}"
                );
            }
        }
        for (offset, symbol) in "POOP".chars().enumerate() {
            let point = Position::new(
                poop.x.saturating_add(u16::try_from(offset).unwrap()),
                poop.y.saturating_add(3),
            );
            assert_eq!(
                buffer.cell(point).expect("rendered poop label").symbol(),
                symbol.to_string(),
                "poop label must be rendered at its geometry anchor"
            );
            assert!(poop.contains(point));
        }
    }
}

fn compact_count_for_test(count: u32) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 1_000_000 {
        format!("{}k", count / 1_000)
    } else if count < 1_000_000_000 {
        format!("{}M", count / 1_000_000)
    } else {
        let whole = count / 1_000_000_000;
        let tenth = (count % 1_000_000_000) / 100_000_000;
        if tenth == 0 {
            format!("{whole}B")
        } else {
            format!("{whole}.{tenth}B")
        }
    }
}

#[test]
fn rendered_food_labels_do_not_overlap_poops_in_wide_layouts() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(12), now));

    for area in [Rect::new(0, 0, 120, 14), Rect::new(0, 0, 120, 7)] {
        let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
        let mut buffer = Buffer::filled(area, Cell::new(" "));
        render_room(area, &mut buffer, &snapshot, &default_frame(), None);
        for (index, source) in geometry.food_sources.iter().enumerate() {
            let label = if area.height >= 14 {
                format!(
                    "{} x{}",
                    match source.food_id {
                        "kibble" => "KIB",
                        "treat" => "TRT",
                        "fruit" => "FRT",
                        "energy_drink" => "ENE",
                        other => panic!("unexpected food id {other}"),
                    },
                    source.count
                )
            } else if index == 0 {
                format!("FOOD x{}", source.count)
            } else {
                format!(
                    "{}x{}",
                    match source.food_id {
                        "kibble" => "KIB",
                        "treat" => "TRT",
                        "fruit" => "FRT",
                        "energy_drink" => "ENE",
                        other => panic!("unexpected food id {other}"),
                    },
                    source.count
                )
            };
            let row_offset = if area.height >= 14 || index == 0 {
                3
            } else {
                2
            };
            let row = source.rect.y + row_offset;
            for (offset, symbol) in label.chars().enumerate() {
                assert_eq!(
                    buffer
                        .cell((source.rect.x + u16::try_from(offset).unwrap(), row))
                        .expect("food label cell")
                        .symbol(),
                    symbol.to_string(),
                    "wide food label must remain visible beside poop: area={area:?} source={source:?}"
                );
            }
        }
    }
}

#[test]
fn minimal_care_labels_start_inside_their_hit_regions() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(11), now));
    let area = Rect::new(0, 0, 120, 3);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);

    let row = |x: u16, width: usize| {
        (0..width)
            .map(|offset| {
                buffer
                    .cell((x + offset as u16, 1))
                    .expect("Minimal label cell")
                    .symbol()
                    .to_owned()
            })
            .collect::<String>()
    };
    let food = geometry.food_sources.first().expect("starter food source");
    assert_eq!(row(food.rect.x, 5), "[FOOD");
    let bed = geometry.bed.expect("Minimal bed");
    assert_eq!(row(bed.x, 5), "[BED]");
    let (_, poop) = geometry.poops.first().expect("seeded poop");
    assert_eq!(row(poop.x, 6), "[POOP]");
}

#[test]
fn minimal_empty_poop_copy_reflows_after_a_wide_food_count() {
    let now = Utc::now();
    let mut inventory = FoodInventory::new();
    inventory.add(FoodKind::Kibble, u32::MAX);
    let pet = Pet::with_inventory(
        Uuid::from_u128(19),
        "Mochi",
        PetSpecies::Cat,
        now,
        inventory,
    );
    let snapshot = PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    let area = Rect::new(0, 0, 120, 3);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Minimal bed");
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let poop_start = (0..area.width.saturating_sub(5))
        .find(|x| {
            "[POOP]".chars().enumerate().all(|(offset, expected)| {
                buffer
                    .cell((x.saturating_add(u16::try_from(offset).unwrap()), 1))
                    .is_some_and(|cell| cell.symbol() == expected.to_string())
            })
        })
        .expect("disabled poop copy");

    assert!(
        poop_start >= bed.right().saturating_sub(area.x).saturating_add(2),
        "empty poop copy overlaps the bed: poop_start={poop_start} bed={bed:?}"
    );
}

#[test]
fn compact_bed_hit_region_covers_the_sleeping_mascot() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = Some(now + Duration::minutes(30));
    let area = Rect::new(0, 0, 120, 7);
    let geometry = codegotchi_cli::terminal::room_geometry(area, &snapshot);
    let bed = geometry.bed.expect("Compact bed");

    assert!(
        bed.bottom() >= area.bottom(),
        "Compact sleep mascot extends below its bed hit region: {bed:?} area={area:?}"
    );
}

#[test]
fn minimal_demand_cues_keep_activity_and_snack_separated() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(20),
        PetDemandKind::Affection,
        now,
    ));
    snapshot.pending_demands.push(PetDemand::new(
        Uuid::from_u128(21),
        PetDemandKind::Snack,
        now,
    ));
    let area = Rect::new(0, 0, 120, 3);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let row = (0..area.width)
        .map(|x| buffer.cell((x, 2)).expect("Minimal cue cell").symbol())
        .collect::<String>();

    assert!(
        row.contains("AFF x1 Calm  SNACK"),
        "Minimal demand cues overlap: {row:?}"
    );
}

#[test]
fn compact_authoritative_sleep_keeps_the_bed_label_visible() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot.behavior = PetBehavior::Sleeping;
    snapshot.napping_until = Some(now + Duration::minutes(30));
    let area = Rect::new(0, 0, 120, 7);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);

    assert!(
        text.contains("BED"),
        "Compact authoritative sleep must keep the bed discoverable: {text}"
    );
}

#[test]
fn compact_eighty_column_geometry_keeps_care_targets_in_bounds() {
    let now = Utc::now();
    let mut snapshot = base_snapshot(now);
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(22), now));
    let area = Rect::new(0, 0, 80, 7);
    let geometry = room_geometry(area, &snapshot);

    let bed = geometry.bed.expect("Compact bed");
    assert!(
        bed.right() <= area.right(),
        "bed clipped: {bed:?} area={area:?}"
    );
    assert!(
        geometry
            .food_sources
            .iter()
            .all(|source| source.rect.right() <= area.right())
    );
    assert!(
        geometry
            .poops
            .iter()
            .all(|(_, poop)| poop.right() <= area.right())
    );
    assert!(geometry.food_sources.iter().all(|source| {
        geometry
            .poops
            .iter()
            .all(|(_, poop)| !rects_overlap(source.rect, *poop))
    }));

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    for marker in ["FOOD", "TRT", "FRT", "ENE", "BED", "POOP"] {
        assert!(text.contains(marker), "Compact 80 missing {marker}: {text}");
    }
}

#[test]
fn compact_eighty_column_geometry_abbreviates_large_authoritative_counts() {
    let now = Utc::now();
    let mut inventory = FoodInventory::new();
    for food in [
        FoodKind::Kibble,
        FoodKind::Treat,
        FoodKind::Fruit,
        FoodKind::EnergyDrink,
    ] {
        inventory.add(food, u32::MAX);
    }
    let pet = Pet::with_inventory(
        Uuid::from_u128(23),
        "Mochi",
        PetSpecies::Cat,
        now,
        inventory,
    );
    let mut snapshot =
        PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    snapshot
        .pending_poops
        .push(Poop::new(Uuid::from_u128(24), now));
    let area = Rect::new(0, 0, 80, 7);
    let geometry = room_geometry(area, &snapshot);

    assert_eq!(geometry.food_sources.len(), 4);
    assert!(
        geometry
            .food_sources
            .iter()
            .all(|source| source.rect.right() <= area.right())
    );
    for (index, source) in geometry.food_sources.iter().enumerate() {
        for other in geometry.food_sources.iter().skip(index + 1) {
            assert!(!rects_overlap(source.rect, other.rect));
        }
        assert!(
            geometry
                .poops
                .iter()
                .all(|(_, poop)| !rects_overlap(source.rect, *poop))
        );
    }

    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    for marker in ["FOOD", "TRT", "FRT", "ENE", "BED", "POOP"] {
        assert!(text.contains(marker), "Compact 80 missing {marker}: {text}");
    }
}

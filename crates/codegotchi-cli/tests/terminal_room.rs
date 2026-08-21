use chrono::{Duration, Utc};
use codegotchi_cli::terminal::{
    PresentationActivity, PresentationFrame, RoomAmbience, RoomRenderOptions, SemanticTone,
    TerminalThemePreset, auto_style, has_authoritative_nap, presentation_activity, render_room,
    render_room_with_options,
};
use codegotchi_domain::{
    ActivityKind, AgentActivityState, DefaultNeedProgressionStrategy, FoodInventory, FoodKind, Pet,
    PetBehavior, PetDemand, PetDemandKind, PetSimulation, PetSpecies, Poop, SimulationSnapshot,
    SystemClock,
};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::{Position, Rect},
    style::Color,
};
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
    assert_eq!(
        nap_buffer[(25, 9)].symbol(),
        "█",
        "authoritative nap places the pet body on the bed mattress"
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
    let full_text = buffer_text(&full_buffer, full.width, full.height);
    for label in ["Hunger", "Energy", "Happy", "Clean"] {
        assert!(full_text.contains(label), "Full room missing {label}");
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

/// The Full bedroom renders the decorative furniture specified by the design
/// (window, shelf, wardrobe, desk, plants) without replacing care objects.
#[test]
fn full_room_renders_decorative_furniture() {
    let snapshot = base_snapshot(Utc::now());
    let full = Rect::new(0, 0, 120, 14);
    let mut buffer = Buffer::filled(full, Cell::new(" "));
    render_room(full, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, full.width, full.height);
    let regions = [
        (Rect::new(2, 1, 20, 6), "window"),
        (Rect::new(34, 1, 22, 5), "shelf"),
        (Rect::new(58, 1, 20, 8), "wardrobe"),
        (Rect::new(1, 6, 28, 6), "desk"),
        (Rect::new(1, 9, 16, 4), "plants"),
        (Rect::new(98, 5, 21, 8), "bed"),
    ];
    for (region, name) in regions {
        let occupied = (region.x..region.right())
            .flat_map(|x| (region.y..region.bottom()).map(move |y| (x, y)))
            .filter(|&(x, y)| buffer.cell((x, y)).is_some_and(|cell| cell.symbol() != " "))
            .count();
        assert!(occupied > 4, "Full room missing layered {name} region");
    }
    assert!(!text.contains("WINDOW"));
    assert!(!text.contains("SHELF"));
    assert!(!text.contains("DESK"));
    assert!(!text.contains("PET"));
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

fn rects_overlap(first: Rect, second: Rect) -> bool {
    first.x < second.right()
        && second.x < first.right()
        && first.y < second.bottom()
        && second.y < first.bottom()
}

/// Compact keeps at least a window and a plant; decoration disappears before
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
        !text.contains("x0"),
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
        geometry.pet.x < 80 && bed.x < 100,
        "Full pet should remain on open floor left of the bed: pet={:?} bed={bed:?}",
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
fn minimal_keeps_a_pet_and_recognizable_care_affordance_icons() {
    let snapshot = base_snapshot(Utc::now());
    let area = Rect::new(0, 0, 120, 3);
    let mut buffer = Buffer::filled(area, Cell::new(" "));
    render_room(area, &mut buffer, &snapshot, &default_frame(), None);
    let text = buffer_text(&buffer, area.width, area.height);
    for marker in ["◉ PET", "FOOD", "BED", "POOP", "AFF"] {
        assert!(
            text.contains(marker),
            "Minimal affordance missing {marker}: {text}"
        );
    }
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
    for marker in ["FOOD", "BED", "POOP"] {
        assert!(
            full_text.contains(marker),
            "wide Full affordance missing {marker}: {full_text}"
        );
    }
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
    for marker in ["PET", "FOOD", "BED", "POOP"] {
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
        minimal_text.contains("(=^.^=)"),
        "Minimal must retain a visible pet mark: {minimal_text}"
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
                format!("FOOD {food_name} x{}", food.count)
            } else {
                format!("FOOD x{}", food.count)
            };
            let food_rows = if area.height >= 14 {
                ["  ○  ", " ◒◒ ", "└──┘ ", label.as_str()]
            } else {
                [" ○ ", "◒◒ ", "└─┘ ", label.as_str()]
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
            ["  ~ ", "  ~ ", " (●) ", "POOP"]
        } else {
            [" ~ ", "(●)", "╰─ ", "POOP"]
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
                    "FOOD {} x{}",
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

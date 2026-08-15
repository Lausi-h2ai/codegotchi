use chrono::{Duration, Utc};
use codegotchi_cli::terminal::{
    PresentationActivity, PresentationFrame, SemanticTone, auto_style, has_authoritative_nap,
    presentation_activity, render_room,
};
use codegotchi_domain::{
    ActivityKind, AgentActivityState, DefaultNeedProgressionStrategy, FoodInventory, Pet,
    PetBehavior, PetSimulation, PetSpecies, SimulationSnapshot, SystemClock,
};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
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
    render_room(area, &mut doze_buffer, &idle_doze, &default_frame());
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
    render_room(area, &mut nap_buffer, &bed_nap, &default_frame());
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
    render_room(full, &mut full_buffer, &snapshot, &default_frame());
    let full_text = buffer_text(&full_buffer, full.width, full.height);
    for label in ["Hunger", "Energy", "Happy", "Clean"] {
        assert!(full_text.contains(label), "Full room missing {label}");
    }

    let compact = Rect::new(0, 0, 40, 7);
    let mut compact_buffer = Buffer::filled(compact, Cell::new(" "));
    render_room(compact, &mut compact_buffer, &snapshot, &default_frame());
    let compact_text = buffer_text(&compact_buffer, compact.width, compact.height);
    assert!(compact_text.contains("H "), "Compact status row missing");

    let minimal = Rect::new(0, 0, 40, 3);
    let mut minimal_buffer = Buffer::filled(minimal, Cell::new(" "));
    render_room(minimal, &mut minimal_buffer, &snapshot, &default_frame());
    let minimal_text = buffer_text(&minimal_buffer, minimal.width, minimal.height);
    assert!(
        minimal_text.contains("CG "),
        "Minimal pet/status row missing"
    );
}

/// Auto theme uses terminal defaults plus neutral gray steps so the same art
/// stays readable on dark and light terminals.
#[test]
fn auto_theme_maps_semantic_tones_to_defaults_and_gray_steps() {
    let tone0 = auto_style(SemanticTone::Tone0);
    assert_eq!(tone0.bg, Some(Color::Reset), "Tone0 resets the background");
    assert_eq!(
        tone0.fg, None,
        "Tone0 leaves the terminal default foreground"
    );

    let tone1 = auto_style(SemanticTone::Tone1);
    assert_eq!(tone1.fg, Some(Color::DarkGray));

    let tone2 = auto_style(SemanticTone::Tone2);
    assert_eq!(tone2.fg, Some(Color::Gray));

    let tone3 = auto_style(SemanticTone::Tone3);
    assert_eq!(
        tone3.fg,
        Some(Color::Reset),
        "Tone3 resets to the foreground"
    );
}

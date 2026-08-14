use codegotchi_cli::terminal::{RoomLayoutMode, TerminalLayout, choose_layout};
use ratatui::layout::Rect;

#[test]
fn selects_modes_and_codex_priority_sizes_at_thresholds() {
    let cases = [
        (60, None, RoomLayoutMode::Full, 46, 14),
        (40, None, RoomLayoutMode::Full, 26, 14),
        (39, None, RoomLayoutMode::Compact, 32, 7),
        (36, None, RoomLayoutMode::Compact, 29, 7),
        (
            35,
            Some(RoomLayoutMode::Full),
            RoomLayoutMode::Compact,
            28,
            7,
        ),
        (26, None, RoomLayoutMode::Compact, 19, 7),
        (25, None, RoomLayoutMode::Minimal, 22, 3),
        (22, None, RoomLayoutMode::Minimal, 19, 3),
        (
            22,
            Some(RoomLayoutMode::Compact),
            RoomLayoutMode::Compact,
            18,
            4,
        ),
        (21, None, RoomLayoutMode::Minimal, 18, 3),
        (10, None, RoomLayoutMode::Minimal, 7, 3),
    ];

    for (height, previous, mode, codex_height, room_height) in cases {
        let layout = choose_layout(Rect::new(0, 0, 120, height), previous);

        assert_eq!(
            layout.room_mode, mode,
            "height={height}, previous={previous:?}"
        );
        assert_eq!(
            layout.codex.height, codex_height,
            "height={height}, previous={previous:?}"
        );
        assert_eq!(
            layout.room.height, room_height,
            "height={height}, previous={previous:?}"
        );
        assert_eq!(layout.codex.height + layout.room.height, height);
    }
}

#[test]
fn applies_hysteresis_for_each_existing_mode() {
    let cases = [
        (40, Some(RoomLayoutMode::Compact), RoomLayoutMode::Full),
        (39, Some(RoomLayoutMode::Compact), RoomLayoutMode::Compact),
        (22, Some(RoomLayoutMode::Compact), RoomLayoutMode::Compact),
        (21, Some(RoomLayoutMode::Compact), RoomLayoutMode::Minimal),
        (40, Some(RoomLayoutMode::Minimal), RoomLayoutMode::Full),
        (26, Some(RoomLayoutMode::Minimal), RoomLayoutMode::Compact),
        (25, Some(RoomLayoutMode::Minimal), RoomLayoutMode::Minimal),
        (36, Some(RoomLayoutMode::Full), RoomLayoutMode::Full),
        (35, Some(RoomLayoutMode::Full), RoomLayoutMode::Compact),
    ];

    for (height, previous, expected) in cases {
        assert_eq!(
            choose_layout(Rect::new(0, 0, 120, height), previous).room_mode,
            expected,
            "height={height}, previous={previous:?}"
        );
    }
}

#[test]
fn preserves_origin_width_and_exactly_partitions_the_height() {
    let terminal = Rect::new(17, 9, 0, 45);
    let layout = choose_layout(terminal, None);

    assert_eq!(layout.codex.x, terminal.x);
    assert_eq!(layout.room.x, terminal.x);
    assert_eq!(layout.codex.y, terminal.y);
    assert_eq!(layout.room.y, terminal.y + layout.codex.height);
    assert_eq!(layout.codex.width, terminal.width);
    assert_eq!(layout.room.width, terminal.width);
    assert_eq!(layout.codex.height + layout.room.height, terminal.height);
}

#[test]
fn width_does_not_change_the_vertical_mode() {
    let heights = [60, 40, 39, 36, 35, 26, 25, 22, 21, 10];

    for height in heights {
        let narrow = choose_layout(Rect::new(0, 0, 1, height), None);
        let wide = choose_layout(Rect::new(0, 0, 240, height), None);

        assert_eq!(narrow.room_mode, wide.room_mode, "height={height}");
        assert_eq!(narrow.codex.height, wide.codex.height, "height={height}");
        assert_eq!(narrow.room.height, wide.room.height, "height={height}");
    }
}

#[test]
fn keeps_codex_at_least_eighteen_rows_when_selected_room_can_share_them() {
    for (height, previous) in [
        (21, None),
        (22, Some(RoomLayoutMode::Compact)),
        (25, None),
        (26, None),
        (36, Some(RoomLayoutMode::Full)),
        (40, None),
    ] {
        let layout = choose_layout(Rect::new(0, 0, 120, height), previous);

        assert!(
            layout.codex.height >= 18,
            "height={height}, previous={previous:?}"
        );
    }
}

#[test]
fn tiny_heights_are_clamped_without_underflow_or_wrap() {
    for height in 0..=3 {
        let terminal = Rect {
            x: u16::MAX,
            y: u16::MAX,
            width: 0,
            height,
        };
        let layout = choose_layout(terminal, None);

        assert_eq!(layout.room_mode, RoomLayoutMode::Minimal);
        assert_eq!(layout.codex.x, u16::MAX);
        assert_eq!(layout.room.x, u16::MAX);
        assert_eq!(layout.codex.y, u16::MAX);
        assert_eq!(layout.room.y, u16::MAX);
        assert_eq!(layout.codex.width, 0);
        assert_eq!(layout.room.width, 0);
        assert!(layout.codex.height.saturating_add(layout.room.height) <= height);
        assert!(layout.room.height <= height);
    }
}

#[test]
fn layout_is_copyable_and_debuggable_as_a_stable_value() {
    let layout = choose_layout(Rect::new(2, 3, 80, 30), None);
    let copied: TerminalLayout = layout;

    assert_eq!(copied, layout);
    assert!(format!("{copied:?}").contains("Compact"));
}

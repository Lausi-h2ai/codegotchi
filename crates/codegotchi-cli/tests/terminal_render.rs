use codegotchi_cli::terminal::{CodexScreen, render_codex};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, Cell},
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};

fn render(screen: &CodexScreen, area: Rect, buffer: &mut Buffer) -> Option<Position> {
    render_codex(screen, area, buffer)
}

#[test]
fn renders_origin_cells_with_ansi_indexed_truecolor_and_modifiers() {
    let mut screen = CodexScreen::new(2, 8);
    screen.process(
        b"\x1b[31;44;1;3;4;7mA\x1b[0m\x1b[2;38;5;16;48;5;200mB\x1b[0m\x1b[38;2;1;2;3;48;2;4;5;6mC",
    );
    let area = Rect::new(5, 7, 4, 1);
    let mut sentinel = Cell::new("?");
    sentinel.set_style(Style::new().fg(Color::Magenta));
    let mut buffer = Buffer::filled(Rect::new(0, 0, 20, 20), sentinel);

    let cursor = render(&screen, area, &mut buffer);

    let ansi = &buffer[(5, 7)];
    assert_eq!(ansi.symbol(), "A");
    assert_eq!(ansi.fg, Color::Red);
    assert_eq!(ansi.bg, Color::Blue);
    assert_eq!(
        ansi.modifier,
        Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED | Modifier::REVERSED
    );

    let indexed = &buffer[(6, 7)];
    assert_eq!(indexed.symbol(), "B");
    assert_eq!(indexed.fg, Color::Indexed(16));
    assert_eq!(indexed.bg, Color::Indexed(200));
    assert_eq!(indexed.modifier, Modifier::DIM);

    let rgb = &buffer[(7, 7)];
    assert_eq!(rgb.symbol(), "C");
    assert_eq!(rgb.fg, Color::Rgb(1, 2, 3));
    assert_eq!(rgb.bg, Color::Rgb(4, 5, 6));
    assert_eq!(cursor, Some(Position::new(8, 7)));
}

#[test]
fn maps_all_ansi_palette_entries_to_ratatui_named_colors() {
    let expected = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];

    for (index, expected) in expected.into_iter().enumerate() {
        let mut screen = CodexScreen::new(1, 1);
        let sequence = format!("\x1b[38;5;{index}mX");
        screen.process(sequence.as_bytes());
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        render(&screen, buffer.area, &mut buffer);
        assert_eq!(buffer[(0, 0)].fg, expected, "palette index {index}");
    }
}

#[test]
fn maps_default_colors_to_ratatui_reset() {
    let mut screen = CodexScreen::new(1, 1);
    screen.process(b"X");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));

    render(&screen, buffer.area, &mut buffer);

    assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
}

#[test]
fn preserves_blank_cell_background_and_style() {
    let mut screen = CodexScreen::new(1, 3);
    screen.process(b"\x1b[48;5;16;1m ");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 1));

    render(&screen, buffer.area, &mut buffer);

    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(0, 0)].bg, Color::Indexed(16));
    assert!(buffer[(0, 0)].modifier.contains(Modifier::BOLD));
}

#[test]
fn clips_writes_to_nonzero_area_and_preserves_outside_cells() {
    let mut screen = CodexScreen::new(3, 4);
    screen.process(b"abcd\x1b[2;1Hefgh\x1b[3;1Hijkl");
    let mut sentinel = Cell::new("?");
    sentinel.set_style(Style::new().fg(Color::Magenta).bg(Color::Yellow));
    let mut buffer = Buffer::filled(Rect::new(0, 0, 5, 4), sentinel.clone());
    let area = Rect::new(1, 1, 2, 2);

    render(&screen, area, &mut buffer);

    assert_eq!(buffer[(1, 1)].symbol(), "a");
    assert_eq!(buffer[(2, 1)].symbol(), "b");
    assert_eq!(buffer[(1, 2)].symbol(), "e");
    assert_eq!(buffer[(2, 2)].symbol(), "f");
    assert_eq!(buffer[(0, 0)], sentinel);
    assert_eq!(buffer[(3, 1)], sentinel);
    assert_eq!(buffer[(1, 3)], sentinel);
}

#[test]
fn renders_combining_text_and_wide_geometry_without_duplicate_glyphs() {
    let mut screen = CodexScreen::new(1, 6);
    screen.process("界e\u{301}z".as_bytes());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));

    render(&screen, buffer.area, &mut buffer);

    assert_eq!(buffer[(0, 0)].symbol(), "界");
    assert_eq!(buffer[(1, 0)].symbol(), " ");
    assert_eq!(buffer[(2, 0)].symbol(), "e\u{301}");
    assert_eq!(buffer[(3, 0)].symbol(), "z");
}

#[test]
fn clips_a_wide_lead_when_its_continuation_is_outside_the_area() {
    let mut screen = CodexScreen::new(1, 2);
    screen.process("界".as_bytes());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));

    render(&screen, buffer.area, &mut buffer);

    assert_eq!(buffer[(0, 0)].symbol(), " ");
}

#[test]
fn returns_only_visible_in_bounds_cursor_translated_by_area_origin() {
    let mut screen = CodexScreen::new(3, 4);
    screen.process(b"\x1b[2;3H");
    let area = Rect::new(9, 11, 3, 2);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 20));

    assert_eq!(
        render(&screen, area, &mut buffer),
        Some(Position::new(11, 12))
    );

    screen.process(b"\x1b[?25l");
    assert_eq!(render(&screen, area, &mut buffer), None);

    let mut out_of_bounds = CodexScreen::new(3, 4);
    out_of_bounds.process(b"\x1b[3;4H");
    assert_eq!(
        render(&out_of_bounds, Rect::new(9, 11, 3, 2), &mut buffer),
        None
    );
}

#[test]
fn zero_sized_and_overflowing_areas_do_not_panic_or_return_cursor() {
    let mut screen = CodexScreen::new(2, 2);
    screen.process(b"XY");

    let mut zero = Buffer::empty(Rect::new(0, 0, 0, 0));
    assert_eq!(render(&screen, zero.area, &mut zero), None);

    let mut overflow = Buffer::empty(Rect::new(u16::MAX, u16::MAX, 1, 1));
    assert_eq!(render(&screen, overflow.area, &mut overflow), None);
}

#[test]
fn rendering_does_not_mutate_codex_screen() {
    let mut screen = CodexScreen::new(2, 5);
    screen.process(b"\x1b[31mX\x1b[2;2H\x1b[?25l");
    let contents = screen.contents();
    let cursor = screen.cursor_position();
    let cell = screen.cell(0, 0).cloned().expect("screen cell");
    let hidden = screen.screen().hide_cursor();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 2));

    render(&screen, buffer.area, &mut buffer);

    assert_eq!(screen.contents(), contents);
    assert_eq!(screen.cursor_position(), cursor);
    assert_eq!(screen.cell(0, 0), Some(&cell));
    assert_eq!(screen.screen().hide_cursor(), hidden);
}

#[test]
fn production_renderer_composes_through_test_backend_and_cursor() {
    let mut screen = CodexScreen::new(2, 6);
    screen.process(b"hello");
    let mut terminal = Terminal::new(TestBackend::new(6, 2)).expect("test terminal");

    terminal
        .draw(|frame| {
            let cursor = render_codex(&screen, frame.area(), frame.buffer_mut());
            if let Some(cursor) = cursor {
                frame.set_cursor_position(cursor);
            }
        })
        .expect("draw through test backend");

    terminal.backend().assert_buffer_lines(["hello ", "      "]);
    assert!(terminal.backend().cursor_visible());
    assert_eq!(terminal.backend().cursor_position(), Position::new(5, 0));
}

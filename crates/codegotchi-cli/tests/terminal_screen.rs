use codegotchi_cli::terminal::{
    CodexInputModes, CodexScreen, MouseEncoding, MouseTrackingMode, encode_focus_event,
    encode_key_event, encode_mouse_event, encode_paste,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    mouse_with_modifiers(kind, column, row, KeyModifiers::NONE)
}

fn mouse_with_modifiers(
    kind: MouseEventKind,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }
}

#[test]
fn screen_handles_ansi_state_and_read_only_cell_access() {
    let mut screen = CodexScreen::new(4, 20);
    screen.process(b"hello\x1b[2;1Hworld");
    assert_eq!(screen.text_at(0, 0, 5), "hello");
    assert_eq!(screen.text_at(1, 0, 5), "world");
    assert_eq!(screen.cursor_position(), (1, 5));

    screen.process(b"\x1b[2J");
    assert!(!screen.cell(0, 0).expect("first cell").has_contents());

    screen.process(b"\x1b[H\x1b[31mR\x1b[38;5;200mI\x1b[38;2;1;2;3mT\x1b[1;4mB");
    assert_eq!(
        screen.cell(0, 0).expect("red cell").fgcolor(),
        vt100::Color::Idx(1)
    );
    assert_eq!(
        screen.cell(0, 1).expect("indexed cell").fgcolor(),
        vt100::Color::Idx(200)
    );
    assert_eq!(
        screen.cell(0, 2).expect("rgb cell").fgcolor(),
        vt100::Color::Rgb(1, 2, 3)
    );
    assert!(screen.cell(0, 3).expect("bold underline cell").bold());
    assert!(screen.cell(0, 3).expect("bold underline cell").underline());

    screen.process("界".as_bytes());
    assert!(screen.cell(0, 4).expect("wide cell").is_wide());
    assert!(
        screen
            .cell(0, 5)
            .expect("wide continuation cell")
            .is_wide_continuation()
    );

    screen.process(b"\x1b[?1049hALT");
    assert!(screen.alternate_screen());
    assert_eq!(screen.text_at(0, 0, 3), "ALT");
    screen.process(b"\x1b[?1049l");
    assert!(!screen.alternate_screen());

    screen.resize(6, 12);
    assert_eq!(screen.size(), (6, 12));
    screen.process(b"\x1b[?9999z\x1b[?1000");
    screen.process(b"truncated-but-benign");
}

#[test]
fn screen_scrollback_by_moves_between_live_and_historical_output() {
    let mut screen = CodexScreen::new(3, 20);
    screen.process(b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
    let live = screen.contents();

    assert_eq!(screen.scrollback(), 0);
    assert!(screen.scrollback_by(1));
    assert_eq!(screen.scrollback(), 1);
    assert_ne!(screen.contents(), live);

    assert!(screen.scrollback_by(-1));
    assert_eq!(screen.scrollback(), 0);
    assert_eq!(screen.contents(), live);
    assert!(!screen.scrollback_by(-1));
}

#[test]
fn screen_scrollback_by_works_while_codex_uses_alternate_screen() {
    let mut screen = CodexScreen::new(3, 20);
    screen.process(b"\x1b[?1049h\x1b[Hfirst frame");
    screen.process(b"\x1b[2J\x1b[Hsecond frame");
    let live = screen.contents();

    assert!(screen.alternate_screen());
    assert!(screen.scrollback_by(1));
    assert_ne!(screen.contents(), live);
    assert!(screen.contents().contains("first frame"));

    assert!(screen.scrollback_by(-1));
    assert_eq!(screen.contents(), live);
}

#[test]
fn screen_reports_historical_view_and_returns_to_live() {
    let mut screen = CodexScreen::new(3, 20);
    screen.process(b"\x1b[?1049h\x1b[Hfirst frame");
    screen.process(b"\x1b[2J\x1b[Hsecond frame");
    let live = screen.contents();

    assert!(!screen.is_scrolled_back());
    assert!(screen.scrollback_by(1));
    assert!(screen.is_scrolled_back());
    assert_ne!(screen.contents(), live);

    screen.scroll_to_live();

    assert!(!screen.is_scrolled_back());
    assert_eq!(screen.contents(), live);
}

#[test]
fn alternate_history_stays_pinned_when_new_output_arrives() {
    let mut screen = CodexScreen::new(3, 20);
    screen.process(b"\x1b[?1049h\x1b[Hfirst frame");
    screen.process(b"\x1b[2J\x1b[Hsecond frame");
    assert!(screen.scrollback_by(1));
    let historical = screen.contents();

    screen.process(b"\x1b[2J\x1b[Hthird frame");

    assert!(screen.is_scrolled_back());
    assert_eq!(screen.contents(), historical);
}

#[test]
fn screen_scrollback_by_collects_alternate_frames_from_one_pty_chunk() {
    let mut screen = CodexScreen::new(3, 20);
    screen
        .process(b"\x1b[?1049h\x1b[Hfirst frame\x1b[2J\x1b[Hsecond frame\x1b[2J\x1b[Hthird frame");
    let live = screen.contents();

    assert!(screen.scrollback_by(1));
    assert_ne!(screen.contents(), live);
    assert!(screen.contents().contains("second frame"));
}

#[test]
fn input_modes_track_vt_controls_and_split_focus_without_visible_text() {
    let mut screen = CodexScreen::new(8, 40);
    assert_eq!(screen.input_modes(), CodexInputModes::default());

    screen.process(b"1004h");
    assert!(!screen.input_modes().focus_reporting);

    screen.process(b"\x1b[?1h\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[?10");
    assert_eq!(
        screen.input_modes(),
        CodexInputModes {
            application_cursor_keys: true,
            bracketed_paste: true,
            focus_reporting: false,
            mouse_tracking: MouseTrackingMode::PressRelease,
            mouse_encoding: MouseEncoding::Sgr,
        }
    );

    screen.process(b"04h");
    assert!(screen.input_modes().focus_reporting);
    screen.process(b"\x1b[?1004l");
    assert!(!screen.input_modes().focus_reporting);

    for (sequence, expected) in [
        (b"\x1b[?9h".as_slice(), MouseTrackingMode::Press),
        (b"\x1b[?1000h".as_slice(), MouseTrackingMode::PressRelease),
        (b"\x1b[?1002h".as_slice(), MouseTrackingMode::ButtonMotion),
        (b"\x1b[?1003h".as_slice(), MouseTrackingMode::AnyMotion),
    ] {
        screen.process(sequence);
        assert_eq!(screen.input_modes().mouse_tracking, expected);
    }

    screen.process(b"\x1b[?1l\x1b[?2004l\x1b[?1003l\x1b[?1006l");
    assert_eq!(screen.input_modes(), CodexInputModes::default());
}

#[test]
fn mode_fixtures_enable_and_disable_each_protocol_without_precedence_masking() {
    let mut screen = CodexScreen::new(8, 40);

    screen.process(b"\x1b[?1h");
    assert!(screen.input_modes().application_cursor_keys);
    screen.process(b"\x1b[?1l");
    assert!(!screen.input_modes().application_cursor_keys);

    screen.process(b"\x1b[?2004h");
    assert!(screen.input_modes().bracketed_paste);
    screen.process(b"\x1b[?2004l");
    assert!(!screen.input_modes().bracketed_paste);

    screen.process(b"\x1b[?1004");
    screen.process(b"h");
    assert!(screen.input_modes().focus_reporting);
    screen.process(b"\x1b[?1004");
    screen.process(b"l");
    assert!(!screen.input_modes().focus_reporting);

    for (enable, disable, expected) in [
        (
            b"\x1b[?9h".as_slice(),
            b"\x1b[?9l".as_slice(),
            MouseTrackingMode::Press,
        ),
        (
            b"\x1b[?1000h".as_slice(),
            b"\x1b[?1000l".as_slice(),
            MouseTrackingMode::PressRelease,
        ),
        (
            b"\x1b[?1002h".as_slice(),
            b"\x1b[?1002l".as_slice(),
            MouseTrackingMode::ButtonMotion,
        ),
        (
            b"\x1b[?1003h".as_slice(),
            b"\x1b[?1003l".as_slice(),
            MouseTrackingMode::AnyMotion,
        ),
    ] {
        let mut isolated = CodexScreen::new(8, 40);
        isolated.process(enable);
        assert_eq!(isolated.input_modes().mouse_tracking, expected);
        isolated.process(disable);
        assert_eq!(
            isolated.input_modes().mouse_tracking,
            MouseTrackingMode::Disabled
        );
    }

    let mut encoding = CodexScreen::new(8, 40);
    encoding.process(b"\x1b[?1005h");
    assert_eq!(encoding.input_modes().mouse_encoding, MouseEncoding::Utf8);
    encoding.process(b"\x1b[?1005l");
    assert_eq!(
        encoding.input_modes().mouse_encoding,
        MouseEncoding::Default
    );
    encoding.process(b"\x1b[?1006h");
    assert_eq!(encoding.input_modes().mouse_encoding, MouseEncoding::Sgr);
    encoding.process(b"\x1b[?1006l");
    assert_eq!(
        encoding.input_modes().mouse_encoding,
        MouseEncoding::Default
    );
}

#[test]
fn ris_clears_split_focus_reporting_and_still_resets_vt_screen() {
    let mut screen = CodexScreen::new(3, 10);
    screen.process(b"\x1b[?1004h\x1b[31mX");
    assert!(screen.input_modes().focus_reporting);
    assert_eq!(
        screen.cell(0, 0).expect("colored cell").fgcolor(),
        vt100::Color::Idx(1)
    );

    screen.process(b"\x1b");
    assert!(screen.input_modes().focus_reporting);
    screen.process(b"c");
    assert!(!screen.input_modes().focus_reporting);
    screen.process(b"Y");

    assert_eq!(screen.text_at(0, 0, 1), "Y");
    assert_eq!(
        screen.cell(0, 0).expect("reset cell").fgcolor(),
        vt100::Color::Default
    );
}

#[test]
fn malformed_and_split_control_input_never_panics_or_activates_focus() {
    let mut screen = CodexScreen::new(3, 10);
    for chunk in [
        b"\x1b".as_slice(),
        b"[".as_slice(),
        b"?".as_slice(),
        b"100".as_slice(),
        b"5".as_slice(),
        b"z".as_slice(),
        b"visible 1004h".as_slice(),
        b"\x1b[?1004".as_slice(),
        b"\x1b[".as_slice(),
    ] {
        screen.process(chunk);
    }
    assert!(!screen.input_modes().focus_reporting);
    screen.process(b"z");
    screen.process(b"ordinary");
    assert!(screen.contents().contains("ordinary"));
    screen.process(b"\x1b[?1004h");
    assert!(screen.input_modes().focus_reporting);
}

#[test]
fn unknown_and_truncated_controls_leave_subsequent_screen_text_usable() {
    let mut screen = CodexScreen::new(3, 20);
    screen.process(b"\x1b[?9999zordinary");
    assert_eq!(screen.text_at(0, 0, 8), "ordinary");

    screen.process(b"\x1b[");
    screen.process(b"z");
    screen.process(b"truncated");
    assert!(screen.contents().contains("truncated"));
}

#[test]
fn query_point_reports_cursor_before_later_movement() {
    let mut screen = CodexScreen::new(4, 20);

    let replies = screen.process(b"\x1b[2;3H\x1b[6n\x1b[H");

    assert_eq!(replies, b"\x1b[2;3R");
    assert_eq!(screen.cursor_position(), (0, 0));
}

#[test]
fn query_point_preserves_split_sequence_state() {
    let mut screen = CodexScreen::new(4, 20);

    assert!(screen.process(b"\x1b[2;3H\x1b[6").is_empty());
    assert_eq!(screen.process(b"n"), b"\x1b[2;3R");
}

#[test]
fn primary_device_attributes_are_conservative_and_kitty_query_is_unanswered() {
    let mut screen = CodexScreen::new(4, 20);

    assert_eq!(screen.process(b"\x1b[c\x1b[0c"), b"\x1b[?1;2c\x1b[?1;2c");
    assert!(screen.process(b"\x1b[?u\x1b[>u\x1b[<u").is_empty());
}

#[test]
fn split_osc_color_queries_are_unanswered() {
    let mut screen = CodexScreen::new(4, 20);

    assert!(screen.process(b"\x1b]10;?").is_empty());
    assert!(screen.process(b"\x07\x1b]11;?\x07").is_empty());
    assert!(screen.process(b"\x1b]10;?\x1b\\").is_empty());
    assert!(screen.process(b"\x1b]11;?\x1b\\").is_empty());
}

#[test]
fn deferred_wrap_cursor_reply_is_clamped_to_visible_bounds() {
    let mut screen = CodexScreen::new(2, 3);

    let replies = screen.process(b"abc\x1b[6n");

    assert_eq!(replies, b"\x1b[1;3R");
}

#[test]
fn queries_do_not_change_input_modes() {
    let mut screen = CodexScreen::new(4, 20);
    screen.process(b"\x1b[?1h\x1b[?2004h\x1b[?1004h\x1b[?1000h\x1b[?1006h");
    let before = screen.input_modes();

    let replies = screen.process(b"\x1b[6n\x1b[c\x1b[?u\x1b]10;?\x07\x1b[?2026h");

    assert_eq!(replies, b"\x1b[1;1R\x1b[?1;2c");
    assert_eq!(screen.input_modes(), before);
}

#[test]
fn unsupported_mouse_modes_only_leave_safe_encoding_fallback_and_active_tracking() {
    let mut screen = CodexScreen::new(4, 20);
    screen.process(b"\x1b[?1h\x1b[?2004h\x1b[?1004h\x1b[?1000h");

    screen.process(b"\x1b[?1015h\x1b[?1016h");
    assert_eq!(
        screen.input_modes(),
        CodexInputModes {
            application_cursor_keys: true,
            bracketed_paste: true,
            focus_reporting: true,
            mouse_tracking: MouseTrackingMode::PressRelease,
            mouse_encoding: MouseEncoding::Default,
        }
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 4, 5),
            screen.input_modes(),
        ),
        [27, b'[', b'M', 32, 37, 38]
    );

    screen.process(b"\x1b[?1006h\x1b[?1015l\x1b[?1016l");
    assert_eq!(
        screen.input_modes().mouse_tracking,
        MouseTrackingMode::PressRelease
    );
    assert_eq!(screen.input_modes().mouse_encoding, MouseEncoding::Sgr);
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 4, 5),
            screen.input_modes(),
        ),
        b"\x1b[<0;5;6M"
    );
}

#[test]
fn query_replies_are_drained_without_stale_replies() {
    let mut screen = CodexScreen::new(4, 20);

    assert_eq!(screen.process(b"\x1b[c"), b"\x1b[?1;2c");
    assert!(screen.process(b"ordinary text").is_empty());
    assert!(screen.process(b"ordinary text").is_empty());
}

#[test]
fn pending_query_replies_are_bounded_and_drained() {
    let mut screen = CodexScreen::new(4, 20);
    let query = b"\x1b[c";
    let mut queries = Vec::new();
    for _ in 0..40 {
        queries.extend_from_slice(query);
    }

    let replies = screen.process(&queries);
    assert_eq!(replies, b"\x1b[?1;2c".repeat(32));
    assert_eq!(screen.process(b"\x1b[6n"), b"\x1b[1;1R");
}

#[test]
fn key_encoding_follows_application_mode_and_common_codex_keys() {
    let normal = CodexInputModes::default();
    let application = CodexInputModes {
        application_cursor_keys: true,
        ..normal
    };
    assert_eq!(
        encode_key_event(key(KeyCode::Char('é')), normal),
        "é".as_bytes()
    );
    assert_eq!(encode_key_event(key(KeyCode::Enter), normal), b"\r");
    assert_eq!(encode_key_event(key(KeyCode::Tab), normal), b"\t");
    assert_eq!(encode_key_event(key(KeyCode::BackTab), normal), b"\x1b[Z");
    assert_eq!(encode_key_event(key(KeyCode::Backspace), normal), b"\x7f");
    assert_eq!(encode_key_event(key(KeyCode::Esc), normal), b"\x1b");
    assert_eq!(encode_key_event(key(KeyCode::Up), normal), b"\x1b[A");
    assert_eq!(encode_key_event(key(KeyCode::Up), application), b"\x1bOA");
    assert_eq!(encode_key_event(key(KeyCode::Down), application), b"\x1bOB");
    assert_eq!(encode_key_event(key(KeyCode::Home), normal), b"\x1b[H");
    assert_eq!(encode_key_event(key(KeyCode::End), normal), b"\x1b[F");
    assert_eq!(encode_key_event(key(KeyCode::Home), application), b"\x1bOH");
    assert_eq!(encode_key_event(key(KeyCode::End), application), b"\x1bOF");
    assert_eq!(
        encode_key_event(
            KeyEvent::new(KeyCode::Home, KeyModifiers::SHIFT),
            application
        ),
        b"\x1b[1;2H"
    );
    assert_eq!(
        encode_key_event(
            KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL | KeyModifiers::ALT),
            application
        ),
        b"\x1b[1;7F"
    );
    assert_eq!(encode_key_event(key(KeyCode::Delete), normal), b"\x1b[3~");
    assert_eq!(encode_key_event(key(KeyCode::F(1)), normal), b"\x1bOP");
    assert_eq!(encode_key_event(key(KeyCode::F(5)), normal), b"\x1b[15~");

    let control = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let alt = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
    assert_eq!(encode_key_event(control, normal), b"\x03");
    assert_eq!(encode_key_event(alt, normal), b"\x1bx");
}

#[test]
fn paste_and_focus_encoding_are_strictly_mode_driven() {
    let off = CodexInputModes::default();
    let on = CodexInputModes {
        bracketed_paste: true,
        focus_reporting: true,
        ..off
    };
    assert_eq!(encode_paste("a\nb", off), b"a\nb");
    assert_eq!(encode_paste("a\nb", on), b"\x1b[200~a\nb\x1b[201~");
    assert_eq!(encode_focus_event(true, off), b"");
    assert_eq!(encode_focus_event(false, off), b"");
    assert_eq!(encode_focus_event(true, on), b"\x1b[I");
    assert_eq!(encode_focus_event(false, on), b"\x1b[O");
}

#[test]
fn mouse_encoding_honors_protocol_and_tracking_level() {
    let event = mouse(MouseEventKind::Down(MouseButton::Left), 0, 0);
    assert_eq!(encode_mouse_event(event, CodexInputModes::default()), b"");

    let default_modes = CodexInputModes {
        mouse_tracking: MouseTrackingMode::Press,
        mouse_encoding: MouseEncoding::Default,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(event, default_modes),
        [27, b'[', b'M', 32, 33, 33]
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollUp, 1, 2), default_modes),
        [27, b'[', b'M', 96, 34, 35]
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Left), 0, 0),
            default_modes
        ),
        b""
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 223, 0),
            default_modes
        ),
        b""
    );

    let utf8_modes = CodexInputModes {
        mouse_tracking: MouseTrackingMode::PressRelease,
        mouse_encoding: MouseEncoding::Utf8,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 200, 100),
            utf8_modes
        ),
        [27, b'[', b'M', 32, 0xc3, 0xa9, 0xc2, 0x85]
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Left), 4, 5),
            utf8_modes
        ),
        [27, b'[', b'M', 35, 37, 38]
    );

    let sgr_modes = CodexInputModes {
        mouse_tracking: MouseTrackingMode::AnyMotion,
        mouse_encoding: MouseEncoding::Sgr,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Right), 4, 5),
            sgr_modes
        ),
        b"\x1b[<2;5;6M"
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Right), 4, 5),
            sgr_modes
        ),
        b"\x1b[<2;5;6m"
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::Moved, 4, 5), sgr_modes),
        b"\x1b[<35;5;6M"
    );

    let press_release = CodexInputModes {
        mouse_tracking: MouseTrackingMode::PressRelease,
        mouse_encoding: MouseEncoding::Sgr,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Drag(MouseButton::Left), 0, 0),
            press_release
        ),
        b""
    );
    let button_motion = CodexInputModes {
        mouse_tracking: MouseTrackingMode::ButtonMotion,
        ..press_release
    };
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Drag(MouseButton::Left), 0, 0),
            button_motion
        ),
        b"\x1b[<32;1;1M"
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::Moved, 0, 0), button_motion),
        b""
    );
    let any_motion = CodexInputModes {
        mouse_tracking: MouseTrackingMode::AnyMotion,
        ..button_motion
    };
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::Moved, 0, 0), any_motion),
        b"\x1b[<35;1;1M"
    );
}

#[test]
fn mouse_wire_encodings_preserve_modifiers_wheels_and_release_forms() {
    let default_release = CodexInputModes {
        mouse_tracking: MouseTrackingMode::PressRelease,
        mouse_encoding: MouseEncoding::Default,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(
            mouse_with_modifiers(
                MouseEventKind::Down(MouseButton::Left),
                0,
                0,
                KeyModifiers::SHIFT,
            ),
            default_release
        ),
        [27, b'[', b'M', 36, 33, 33]
    );
    assert_eq!(
        encode_mouse_event(
            mouse_with_modifiers(
                MouseEventKind::Down(MouseButton::Left),
                0,
                0,
                KeyModifiers::ALT,
            ),
            default_release
        ),
        [27, b'[', b'M', 40, 33, 33]
    );
    assert_eq!(
        encode_mouse_event(
            mouse_with_modifiers(
                MouseEventKind::Down(MouseButton::Left),
                0,
                0,
                KeyModifiers::CONTROL,
            ),
            default_release
        ),
        [27, b'[', b'M', 48, 33, 33]
    );
    assert_eq!(
        encode_mouse_event(
            mouse_with_modifiers(
                MouseEventKind::Up(MouseButton::Right),
                4,
                5,
                KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL,
            ),
            default_release
        ),
        [27, b'[', b'M', 63, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Left), 4, 5),
            default_release
        ),
        [27, b'[', b'M', 35, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollLeft, 4, 5), default_release),
        [27, b'[', b'M', 98, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollRight, 4, 5), default_release),
        [27, b'[', b'M', 99, 37, 38]
    );

    let utf8_release = CodexInputModes {
        mouse_tracking: MouseTrackingMode::PressRelease,
        mouse_encoding: MouseEncoding::Utf8,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Right), 4, 5),
            utf8_release
        ),
        [27, b'[', b'M', 35, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(
            mouse_with_modifiers(
                MouseEventKind::Up(MouseButton::Middle),
                4,
                5,
                KeyModifiers::ALT,
            ),
            utf8_release
        ),
        [27, b'[', b'M', 43, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollLeft, 4, 5), utf8_release),
        [27, b'[', b'M', 98, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollRight, 4, 5), utf8_release),
        [27, b'[', b'M', 99, 37, 38]
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 2015, 0),
            utf8_release
        ),
        b""
    );

    let sgr_release = CodexInputModes {
        mouse_tracking: MouseTrackingMode::PressRelease,
        mouse_encoding: MouseEncoding::Sgr,
        ..Default::default()
    };
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Left), 4, 5),
            sgr_release
        ),
        b"\x1b[<0;5;6m"
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Middle), 4, 5),
            sgr_release
        ),
        b"\x1b[<1;5;6m"
    );
    assert_eq!(
        encode_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Right), 4, 5),
            sgr_release
        ),
        b"\x1b[<2;5;6m"
    );
    assert_eq!(
        encode_mouse_event(
            mouse_with_modifiers(
                MouseEventKind::Up(MouseButton::Right),
                4,
                5,
                KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL,
            ),
            sgr_release
        ),
        b"\x1b[<30;5;6m"
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollLeft, 4, 5), sgr_release),
        b"\x1b[<66;5;6M"
    );
    assert_eq!(
        encode_mouse_event(mouse(MouseEventKind::ScrollRight, 4, 5), sgr_release),
        b"\x1b[<67;5;6M"
    );
}

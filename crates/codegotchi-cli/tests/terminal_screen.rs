use codegotchi_cli::terminal::{
    CodexInputModes, CodexScreen, MouseEncoding, MouseTrackingMode, encode_focus_event,
    encode_key_event, encode_mouse_event, encode_paste,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
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
    screen.process(b"\x1b[?1004h");
    assert!(screen.input_modes().focus_reporting);
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
        b"\x1b[<3;5;6m"
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

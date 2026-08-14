use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

use codegotchi_cli::CodexInvocation;
use codegotchi_cli::terminal::{
    TerminalBackend, TerminalSessionCore, TerminalSessionError, TerminalSessionSignal,
    initialize_terminal_and_spawn, render_codex, run_terminal_session,
    terminal_session_signal_channel,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};

#[derive(Default)]
struct BackendState {
    calls: Vec<&'static str>,
    fail_size: bool,
}

struct FakeBackend {
    state: Rc<RefCell<BackendState>>,
}

impl FakeBackend {
    fn record(&mut self, call: &'static str) -> io::Result<()> {
        self.state.borrow_mut().calls.push(call);
        Ok(())
    }
}

impl TerminalBackend for FakeBackend {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        self.state.borrow_mut().calls.push("size");
        if self.state.borrow().fail_size {
            Err(io::Error::other("size failed"))
        } else {
            Ok((80, 24))
        }
    }

    fn enable_raw_mode(&mut self) -> io::Result<()> {
        self.record("enable_raw_mode")
    }
    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        self.record("enter_alternate_screen")
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.record("hide_cursor")
    }
    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        self.record("enable_mouse_capture")
    }
    fn enable_focus_change(&mut self) -> io::Result<()> {
        self.record("enable_focus_change")
    }
    fn enable_bracketed_paste(&mut self) -> io::Result<()> {
        self.record("enable_bracketed_paste")
    }
    fn disable_bracketed_paste(&mut self) -> io::Result<()> {
        self.record("disable_bracketed_paste")
    }
    fn disable_focus_change(&mut self) -> io::Result<()> {
        self.record("disable_focus_change")
    }
    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        self.record("disable_mouse_capture")
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.record("show_cursor")
    }
    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        self.record("leave_alternate_screen")
    }
    fn disable_raw_mode(&mut self) -> io::Result<()> {
        self.record("disable_raw_mode")
    }
}

fn invocation() -> CodexInvocation {
    CodexInvocation {
        program: PathBuf::from("fake-codex"),
        arguments: vec![OsString::from("--literal")],
        environment: vec![(OsString::from("CODEGOTCHI_TEST"), OsString::from("yes"))],
    }
}

#[test]
fn terminal_initialization_failure_never_invokes_spawn_callback() {
    let state = Rc::new(RefCell::new(BackendState {
        fail_size: true,
        ..BackendState::default()
    }));
    let backend = FakeBackend {
        state: Rc::clone(&state),
    };
    let mut spawn_calls = 0;

    let result =
        initialize_terminal_and_spawn(backend, &invocation(), |_invocation, _rows, _cols| {
            spawn_calls += 1;
            Err(TerminalSessionError::SpawnUnavailable)
        });

    assert!(matches!(
        result,
        Err(TerminalSessionError::Initialization(_))
    ));
    assert_eq!(spawn_calls, 0);
    assert_eq!(state.borrow().calls, ["size"]);
}

#[test]
fn initialized_session_invokes_one_spawn_with_exact_invocation_and_dimensions() {
    let state = Rc::new(RefCell::new(BackendState::default()));
    let backend = FakeBackend {
        state: Rc::clone(&state),
    };
    let expected = invocation();
    let mut spawn_calls = 0;

    let result = initialize_terminal_and_spawn(backend, &expected, |received, rows, columns| {
        spawn_calls += 1;
        assert_eq!(received, &expected);
        assert_eq!((rows, columns), (24, 80));
        Ok(())
    });

    assert!(result.is_ok());
    assert_eq!(spawn_calls, 1);
    assert_eq!(state.borrow().calls[0], "size");
    assert_eq!(state.borrow().calls[1], "enable_raw_mode");
}

#[test]
fn production_session_core_uses_negotiated_modes_for_every_input_kind() {
    let mut core = TerminalSessionCore::new(8, 40);

    assert_eq!(
        core.encode_event(&Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))),
        b"\x1b[A"
    );
    assert_eq!(core.encode_event(&Event::Paste("a\nb".to_owned())), b"a\nb");
    assert_eq!(core.encode_event(&Event::FocusGained), b"");

    core.process_output(b"\x1b[?1h\x1b[?2004h\x1b[?1004h\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        core.encode_event(&Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))),
        b"\x1bOA"
    );
    assert_eq!(
        core.encode_event(&Event::Paste("a\nb".to_owned())),
        b"\x1b[200~a\nb\x1b[201~"
    );
    assert_eq!(core.encode_event(&Event::FocusGained), b"\x1b[I");

    let mouse = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(core.encode_event(&mouse), b"\x1b[<0;5;6M");
}

#[test]
fn production_session_core_resizes_rows_before_columns_and_bounds_screen_state() {
    let mut core = TerminalSessionCore::new(4, 10);
    core.resize(31, 120);
    assert_eq!(core.screen().size(), (31, 120));

    let output = "line\r\n".repeat(20_000);
    core.process_output(output.as_bytes());
    assert!(core.screen().scrollback() <= 10_000);
}

#[test]
fn production_session_core_output_reaches_the_production_renderer() {
    let mut core = TerminalSessionCore::new(2, 20);
    core.process_output(b"\x1b[31mREADY");

    let area = Rect::new(0, 0, 20, 2);
    let mut buffer = Buffer::empty(area);
    let cursor = render_codex(core.screen(), area, &mut buffer);

    assert_eq!(buffer[(0, 0)].symbol(), "R");
    assert_eq!(buffer[(0, 0)].fg, Color::Red);
    assert_eq!(buffer[(1, 0)].symbol(), "E");
    assert_eq!(cursor, Some(ratatui::layout::Position::new(5, 0)));
}

#[tokio::test]
async fn closed_signal_receiver_is_observed_once_and_can_be_disabled() {
    let (sender, mut receiver) = terminal_session_signal_channel(0);
    drop(sender);
    assert!(receiver.recv().await.is_none());
}

#[test]
fn mouse_disabled_mode_emits_no_bytes_even_through_core_seam() {
    let core = TerminalSessionCore::new(3, 10);
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert!(core.encode_event(&event).is_empty());
    let (sender, mut receiver) = terminal_session_signal_channel(1);
    sender
        .try_send(TerminalSessionSignal::WindowChange)
        .expect("signal channel has capacity");
    assert_eq!(receiver.try_recv(), Ok(TerminalSessionSignal::WindowChange));
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn real_session_adapter_spawns_fixture_and_reaps_after_external_interrupt() {
    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex-pty.sh"),
        arguments: vec!["--session-adapter".into()],
        environment: Vec::new(),
    };
    let (sender, receiver) = terminal_session_signal_channel(2);
    tokio::spawn(async move {
        let delay = if std::env::var_os("CODEGOTCHI_H6C_SCREENSHOT").is_some() {
            3_000
        } else {
            100
        };
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        sender
            .send(TerminalSessionSignal::Interrupt)
            .await
            .expect("session is still receiving external signals");
    });

    let status = run_terminal_session(&invocation, receiver)
        .await
        .expect("fixture session should restore and return its child status");
    assert!(!status.success());
}

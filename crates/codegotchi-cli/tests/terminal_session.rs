use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use codegotchi_cli::CodexInvocation;
use codegotchi_cli::terminal::{
    TerminalBackend, TerminalSessionCore, TerminalSessionError, TerminalSessionEventSource,
    TerminalSessionSignal, initialize_terminal_and_spawn, render_codex, run_terminal_session,
    run_terminal_session_with_events, terminal_session_signal_channel,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use tokio::sync::Mutex;

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

struct ScriptedEvents {
    events: VecDeque<Event>,
    ready_at: Option<tokio::time::Instant>,
}

impl ScriptedEvents {
    fn new(events: impl IntoIterator<Item = Event>) -> Self {
        Self {
            events: events.into_iter().collect(),
            ready_at: None,
        }
    }
}

impl TerminalSessionEventSource for ScriptedEvents {
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Event, io::Error>>> + '_>> {
        let this = self;
        Box::pin(async move {
            let ready_at = *this
                .ready_at
                .get_or_insert_with(|| tokio::time::Instant::now() + Duration::from_millis(150));
            tokio::time::sleep_until(ready_at).await;
            let event = this.events.pop_front();
            this.ready_at = None;
            match event {
                Some(event) => Some(Ok(event)),
                None => std::future::pending().await,
            }
        })
    }
}

struct PendingEvents;

impl TerminalSessionEventSource for PendingEvents {
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Event, io::Error>>> + '_>> {
        Box::pin(std::future::pending())
    }
}

fn outer_pty_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn composed_session_adapter_fairly_handles_signals_during_continuous_output() {
    let _outer_pty = outer_pty_lock().lock().await;
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex-flood-pty.sh");
    for (mode, first, second, expected) in [
        ("--interrupt", TerminalSessionSignal::Interrupt, None, 130),
        (
            "--ignore-interrupt",
            TerminalSessionSignal::Interrupt,
            Some(TerminalSessionSignal::Terminate),
            143,
        ),
    ] {
        let invocation = CodexInvocation {
            program: fixture.clone(),
            arguments: vec![mode.into()],
            environment: Vec::new(),
        };
        let (sender, receiver) = terminal_session_signal_channel(4);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            sender
                .send(first)
                .await
                .expect("session receives first signal");
            if let Some(second) = second {
                tokio::time::sleep(Duration::from_millis(30)).await;
                sender
                    .send(second)
                    .await
                    .expect("session receives escalation signal");
            }
        });

        let status = tokio::time::timeout(
            Duration::from_secs(3),
            run_terminal_session_with_events(&invocation, receiver, PendingEvents),
        )
        .await
        .expect("signal should not starve behind PTY output")
        .expect("flood fixture should restore and return its status");
        assert_eq!(status.exit_code(), expected);
    }
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn composed_session_adapter_delivers_exact_invocation_modes_input_and_status() {
    let _outer_pty = outer_pty_lock().lock().await;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_nanos();
    let log = std::env::temp_dir().join(format!(
        "codegotchi-composed-session-{}-{timestamp}.log",
        std::process::id()
    ));
    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-codex-composed-pty.sh"),
        arguments: vec!["--literal".into(), "argument with spaces".into()],
        environment: vec![
            ("FAKE_COMPOSED_LOG".into(), log.as_os_str().into()),
            ("FAKE_COMPOSED_ENV".into(), "exact-value".into()),
        ],
    };
    let events = ScriptedEvents::new([
        Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        Event::Paste("a\nb".to_owned()),
        Event::FocusGained,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }),
        // Crossterm reports columns, rows. The production adapter re-queries
        // the changed outer PTY and forwards rows, columns to the child.
        Event::Resize(120, 31),
    ]);
    let (sender, receiver) = terminal_session_signal_channel(2);
    let resize_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(450)).await;
        let result = std::process::Command::new("stty")
            .args(["rows", "31", "cols", "120"])
            .status();
        sender
            .send(TerminalSessionSignal::WindowChange)
            .await
            .expect("session should still receive a resize signal");
        result.expect("resize the outer test PTY").success()
    });

    let status = run_terminal_session_with_events(&invocation, receiver, events)
        .await
        .expect("composed fixture should exit and restore");
    assert!(resize_task.await.expect("resize task should not panic"));
    assert_eq!(status.exit_code(), 0);
    let output = fs::read_to_string(&log).expect("fixture should record direct adapter inputs");
    assert!(output.starts_with(
        "argc=2\narg[1]=--literal\narg[2]=argument with spaces\nenv=exact-value\nsize=24 80\n"
    ));
    assert!(output.contains("1b4f411b5b3230307e610a621b5b3230317e1b5b491b5b3c303b353b364d\n"));
    assert!(output.ends_with("resized-size=31 120\n"));
    fs::remove_file(log).expect("remove composed adapter log");

    // Do not leave the outer PTY resized for a later test in the same script.
    std::process::Command::new("stty")
        .args(["rows", "24", "cols", "80"])
        .status()
        .expect("restore the outer test PTY");
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn real_session_adapter_spawns_fixture_and_reaps_after_external_interrupt() {
    let _outer_pty = outer_pty_lock().lock().await;
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
    assert_eq!(status.exit_code(), 130);
}

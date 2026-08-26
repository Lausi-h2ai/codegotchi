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

use chrono::Utc;
use codegotchi_cli::terminal::{
    TerminalBackend, TerminalSessionCore, TerminalSessionError, TerminalSessionEventSource,
    TerminalSessionSignal, TerminalSessionStartError, initialize_terminal_and_spawn, render_codex,
    room_geometry_with_frame, run_terminal_session, run_terminal_session_with_events,
    run_terminal_session_with_spawn_guard_and_initialization_recovery,
    terminal_session_signal_channel, wide_full_care_zone,
};
use codegotchi_cli::{AuthoritativeRuntime, CodexInvocation, SqliteStore};
use codegotchi_domain::{
    DefaultNeedProgressionStrategy, FoodInventory, Pet, PetSimulation, PetSpecies, Poop,
    SimulationSnapshot, SystemClock,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use tokio::sync::Mutex;
use uuid::Uuid;

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

#[tokio::test]
async fn launcher_terminal_entry_retains_signals_when_initialization_fails() {
    let (sender, mut receiver) = terminal_session_signal_channel(1);
    let mut before_spawn_calls = 0;
    let result = run_terminal_session_with_spawn_guard_and_initialization_recovery(
        &invocation(),
        receiver,
        None,
        || {
            before_spawn_calls += 1;
            Ok(())
        },
    )
    .await;

    let Err(TerminalSessionStartError::Initialization {
        error,
        signals: returned,
    }) = result
    else {
        panic!("the test process must not expose a physical terminal");
    };
    receiver = returned;
    assert!(matches!(error, TerminalSessionError::Initialization(_)));
    assert_eq!(before_spawn_calls, 0);
    assert!(receiver.try_recv().is_err());
    drop(sender);
    assert!(receiver.recv().await.is_none());
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
fn production_session_core_encodes_every_negotiated_mouse_tracking_level() {
    let cases = [
        (
            b"\x1b[?9h".as_slice(),
            MouseEventKind::Down(MouseButton::Left),
            [27, b'[', b'M', 32, 37, 38].as_slice(),
        ),
        (
            b"\x1b[?1000h".as_slice(),
            MouseEventKind::Up(MouseButton::Left),
            [27, b'[', b'M', 35, 37, 38].as_slice(),
        ),
        (
            b"\x1b[?1002h".as_slice(),
            MouseEventKind::Drag(MouseButton::Left),
            [27, b'[', b'M', 64, 37, 38].as_slice(),
        ),
        (
            b"\x1b[?1003h".as_slice(),
            MouseEventKind::Moved,
            [27, b'[', b'M', 67, 37, 38].as_slice(),
        ),
    ];

    for (mode, kind, expected) in cases {
        let mut core = TerminalSessionCore::new(8, 40);
        core.process_output(mode);
        let event = Event::Mouse(MouseEvent {
            kind,
            column: 4,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(core.encode_event(&event), expected, "mode bytes: {mode:?}");
    }
}

#[test]
fn production_session_core_encodes_every_negotiated_mouse_coordinate_encoding() {
    let cases = [
        (
            b"\x1b[?1000h".as_slice(),
            [27, b'[', b'M', 32, 37, 38].as_slice(),
        ),
        (
            b"\x1b[?1000h\x1b[?1005h".as_slice(),
            [27, b'[', b'M', 32, 0xc3, 0xa9, 0xc2, 0x85].as_slice(),
        ),
        (
            b"\x1b[?1000h\x1b[?1006h".as_slice(),
            b"\x1b[<0;5;6M".as_slice(),
        ),
    ];

    for (modes, expected) in cases {
        let mut core = TerminalSessionCore::new(8, 40);
        core.process_output(modes);
        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: if modes.ends_with(b"1005h") { 200 } else { 4 },
            row: if modes.ends_with(b"1005h") { 100 } else { 5 },
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(core.encode_event(&event), expected, "mode bytes: {modes:?}");
    }
}

#[test]
fn production_session_core_resizes_rows_before_columns_and_bounds_screen_state() {
    let mut core = TerminalSessionCore::new(4, 10);
    core.resize(31, 120);
    // Resize normalizes the virtual screen to the Codex pane: a 31 row
    // terminal selects Compact (7 row room, 24 row Codex pane), proving the
    // (rows, columns) argument order reaches the pane-sized screen.
    assert_eq!(core.screen().size(), (24, 120));

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

#[test]
fn eighty_by_forty_five_core_uses_lower_pane_origin_for_full_care_geometry() {
    let now = Utc::now();
    let pet = Pet::with_inventory(
        Uuid::from_u128(0x8100),
        "Mochi",
        codegotchi_domain::PetSpecies::Cat,
        now,
        FoodInventory::starter(),
    );
    let mut snapshot =
        PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    let poop_id = Uuid::from_u128(0x8101);
    snapshot.pending_poops.push(Poop::new(poop_id, now));

    let mut core = TerminalSessionCore::with_seed(45, 80, 0x8102);
    core.set_snapshot(snapshot.clone());
    assert_eq!(core.layout().room, Rect::new(0, 31, 80, 14));

    let geometry =
        room_geometry_with_frame(core.layout().room, &snapshot, &core.presentation_frame());
    let (_, poop) = geometry
        .poops
        .iter()
        .find(|(id, _)| *id == poop_id)
        .expect("core Full room retains authoritative poop");
    assert_eq!(poop.y, core.layout().room.y + 8);
    assert_eq!(*poop, wide_full_care_zone(core.layout().room));
}

#[test]
fn production_session_core_exposes_terminal_query_replies() {
    let mut core = TerminalSessionCore::new(4, 20);

    assert_eq!(core.process_output(b"\x1b[2;3H\x1b[6n"), b"\x1b[2;3R");
    assert!(core.process_output(b"ordinary").is_empty());
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

struct OuterPtySizeGuard {
    rows: u16,
    columns: u16,
    restored: bool,
}

impl OuterPtySizeGuard {
    fn capture() -> Self {
        let output = std::process::Command::new("stty")
            .args(stty_tty_arguments(&["size".to_owned()]))
            .output()
            .expect("read outer PTY size");
        assert!(output.status.success(), "stty size should succeed");
        let values = String::from_utf8(output.stdout).expect("stty size is UTF-8");
        let mut values = values.split_whitespace();
        let rows = values
            .next()
            .expect("stty should report rows")
            .parse()
            .expect("rows should be numeric");
        let columns = values
            .next()
            .expect("stty should report columns")
            .parse()
            .expect("columns should be numeric");
        Self {
            rows,
            columns,
            restored: false,
        }
    }

    fn restore(&mut self) {
        let status = std::process::Command::new("stty")
            .args(stty_tty_arguments(&[
                "rows".to_owned(),
                self.rows.to_string(),
                "cols".to_owned(),
                self.columns.to_string(),
            ]))
            .status()
            .expect("restore outer PTY size");
        assert!(status.success(), "stty should restore outer PTY size");
        self.restored = true;
    }
}

impl Drop for OuterPtySizeGuard {
    fn drop(&mut self) {
        if !self.restored {
            let _ = std::process::Command::new("stty")
                .args(stty_tty_arguments(&[
                    "rows".to_owned(),
                    self.rows.to_string(),
                    "cols".to_owned(),
                    self.columns.to_string(),
                ]))
                .status();
        }
    }
}

struct OuterPtyResizeTask(Option<tokio::task::JoinHandle<bool>>);

fn stty_tty_arguments(arguments: &[String]) -> Vec<String> {
    let device_flag = if cfg!(target_os = "macos") {
        "-f"
    } else {
        "-F"
    };
    let mut all = vec![device_flag.to_owned(), "/dev/tty".to_owned()];
    all.extend(arguments.iter().cloned());
    all
}

impl OuterPtyResizeTask {
    fn new(handle: tokio::task::JoinHandle<bool>) -> Self {
        Self(Some(handle))
    }

    async fn finish(mut self) -> bool {
        self.0
            .take()
            .expect("resize task handle is owned")
            .await
            .expect("resize task should not panic")
    }
}

impl Drop for OuterPtyResizeTask {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
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
            run_terminal_session_with_events(&invocation, receiver, PendingEvents, None),
        )
        .await
        .expect("signal should not starve behind PTY output")
        .expect("flood fixture should restore and return its status");
        assert_eq!(status.exit_code(), expected);
    }
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn composed_session_adapter_cancellation_under_output_flood_completes() {
    let _outer_pty = outer_pty_lock().lock().await;
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-codex-flood-pty.sh");
    let invocation = CodexInvocation {
        program: fixture,
        arguments: vec!["--ignore-interrupt".into()],
        environment: Vec::new(),
    };
    let (_sender, receiver) = terminal_session_signal_channel(1);
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let task = tokio::task::spawn_local(async move {
                run_terminal_session_with_events(&invocation, receiver, PendingEvents, None).await
            });

            tokio::time::sleep(Duration::from_millis(150)).await;
            task.abort();
            let result = tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .expect("cancellation under PTY output flood should complete")
                .expect_err("aborted session should report cancellation");
            assert!(result.is_cancelled());
        })
        .await;
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn composed_session_adapter_closed_signal_receiver_completes_without_spin() {
    let _outer_pty = outer_pty_lock().lock().await;
    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-codex-close-pty.sh"),
        arguments: Vec::new(),
        environment: Vec::new(),
    };
    let (sender, receiver) = terminal_session_signal_channel(1);
    drop(sender);

    let status = tokio::time::timeout(
        Duration::from_secs(3),
        run_terminal_session_with_events(&invocation, receiver, PendingEvents, None),
    )
    .await
    .expect("closed signal input should not spin or hang")
    .expect("composed child should exit normally");
    assert_eq!(status.exit_code(), 0);
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn composed_session_adapter_cleans_descendant_after_natural_leader_exit() {
    let _outer_pty = outer_pty_lock().lock().await;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_nanos();
    let pid_file = std::env::temp_dir().join(format!(
        "codegotchi-natural-leader-{}-{timestamp}.pid",
        std::process::id()
    ));
    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-codex-natural-leader-pty.sh"),
        arguments: vec![pid_file.as_os_str().into()],
        environment: Vec::new(),
    };
    let (sender, receiver) = terminal_session_signal_channel(1);
    drop(sender);

    let status = tokio::time::timeout(
        Duration::from_secs(2),
        run_terminal_session_with_events(&invocation, receiver, PendingEvents, None),
    )
    .await
    .expect("natural leader exit should not leave the PTY reader blocked")
    .expect("natural leader session should exit cleanly");
    assert_eq!(status.exit_code(), 0);
    let descendant_pid = wait_for_pid(&pid_file);
    assert_eventually(Duration::from_secs(2), || !process_exists(descendant_pid));
    fs::remove_file(pid_file).expect("remove natural-leader PID file");
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn composed_session_adapter_delivers_exact_invocation_modes_input_and_status() {
    let _outer_pty = outer_pty_lock().lock().await;
    let mut outer_size = OuterPtySizeGuard::capture();
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
    let resize_task = OuterPtyResizeTask::new(tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(450)).await;
        let result = std::process::Command::new("stty")
            .args(stty_tty_arguments(&[
                "rows".to_owned(),
                "31".to_owned(),
                "cols".to_owned(),
                "120".to_owned(),
            ]))
            .status();
        sender
            .send(TerminalSessionSignal::WindowChange)
            .await
            .expect("session should still receive a resize signal");
        result.expect("resize the outer test PTY").success()
    }));

    let status = run_terminal_session_with_events(&invocation, receiver, events, None)
        .await
        .expect("composed fixture should exit and restore");
    assert!(resize_task.finish().await);
    assert_eq!(status.exit_code(), 0);
    let output = fs::read_to_string(&log).expect("fixture should record direct adapter inputs");
    // The child PTY is sized to the Codex pane, not the full terminal. A 24
    // row outer PTY yields the Minimal layout (3 row room, 21 row Codex pane);
    // a 31 x 120 resize enters Compact (7 row room, 24 row Codex pane).
    assert!(output.starts_with(
        "argc=2\narg[1]=--literal\narg[2]=argument with spaces\nenv=exact-value\nsize=21 80\n"
    ));
    assert!(output.contains("1b4f411b5b3230307e610a621b5b3230317e1b5b491b5b3c303b353b364d\n"));
    assert!(output.ends_with("resized-size=24 120\n"));
    fs::remove_file(log).expect("remove composed adapter log");

    // Keep explicit restoration in the success path, with Drop covering
    // assertion failures and unwinding in the outer PTY test.
    outer_size.restore();
}

#[tokio::test]
#[ignore = "requires running the test process inside a real outer PTY"]
async fn room_care_events_reach_the_authoritative_runtime() {
    let _outer_pty = outer_pty_lock().lock().await;
    let mut outer_size = OuterPtySizeGuard::capture();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_nanos();
    let database = std::env::temp_dir().join(format!(
        "codegotchi-care-runtime-{}-{timestamp}.sqlite",
        std::process::id()
    ));
    let now = Utc::now();
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::starter(),
    );
    let snapshot: SimulationSnapshot =
        PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    let runtime =
        AuthoritativeRuntime::new(SqliteStore::open(&database).expect("store opens"), snapshot)
            .expect("runtime starts");

    let invocation = CodexInvocation {
        program: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/fake-codex-composed-pty.sh"),
        arguments: Vec::new(),
        environment: vec![(
            "FAKE_COMPOSED_LOG".into(),
            std::env::temp_dir()
                .join(format!(
                    "codegotchi-care-fixture-{}-{timestamp}.log",
                    std::process::id()
                ))
                .into_os_string(),
        )],
    };
    // At a 24 row outer PTY the room is Minimal (rows 21-23). The bed
    // affordance sits at room-relative (9..13, 1), i.e. absolute row 22.
    let events = ScriptedEvents::new([Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 10,
        row: 22,
        modifiers: KeyModifiers::NONE,
    })]);
    let (sender, receiver) = terminal_session_signal_channel(2);
    let status =
        run_terminal_session_with_events(&invocation, receiver, events, Some(runtime.clone()))
            .await
            .expect("care session runs and restores");
    assert_eq!(status.exit_code(), 0);
    let _ = sender;

    let (after, _) = runtime.subscribe().expect("runtime snapshot after care");
    assert!(
        after.napping_until.is_some(),
        "clicking the bed must start an authoritative nap"
    );

    fs::remove_file(&database).expect("remove care runtime database");
    outer_size.restore();
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

#[cfg(unix)]
fn wait_for_pid(path: &std::path::Path) -> u32 {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(value) = fs::read_to_string(path)
            && let Ok(pid) = value.trim().parse()
        {
            return pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fixture did not publish descendant PID"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(unix)]
fn assert_eventually(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + timeout;
    while !predicate() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(predicate(), "condition did not become true before timeout");
}

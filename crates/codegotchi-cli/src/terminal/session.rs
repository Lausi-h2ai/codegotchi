//! The interactive Codex PTY session.
//!
//! This module deliberately keeps the terminal lifecycle owner in `host.rs`
//! and the PTY process owner in `pty.rs`.  [`TerminalSessionCore`] is the
//! production-shared, deterministic part of the session: the real adapter
//! below feeds it the same bytes and Crossterm events that focused tests can
//! feed without requiring a physical terminal.

use std::{
    fmt,
    future::Future,
    io::{self, Read, Write},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crossterm::event::{Event, EventStream};
use futures_util::StreamExt;
use ratatui::{
    Terminal,
    backend::{Backend as RatatuiBackend, CrosstermBackend},
    layout::{Position, Rect},
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use uuid::Uuid;

use codegotchi_domain::SimulationSnapshot;

use crate::CodexInvocation;
use crate::runtime::{AuthoritativeRuntime, RuntimeError};

use super::behavior::{PresentationFrame, PresentationState};
use super::pty::PtyWriter;
use super::{
    CareGateway, CodexScreen, CrosstermTerminal, PtyCodexChild, PtyCodexError, RoomCareRequest,
    RoomInputSession, TerminalBackend, TerminalEntryError, TerminalGuard, TerminalLayout,
    TerminalRestoreError, choose_layout, encode_focus_event, encode_key_event, encode_mouse_event,
    encode_paste, render_codex, render_room,
};

impl CareGateway for AuthoritativeRuntime {
    fn feed(&self, action_id: Uuid, food_id: &str) {
        // Care validation errors are normal UI feedback (for example an
        // out-of-stock click after a stale snapshot). The authoritative
        // snapshot broadcast is the source of truth; a rejected request
        // simply leaves the room unchanged.
        let _ = AuthoritativeRuntime::feed(self, action_id, food_id);
    }

    fn clean(&self, action_id: Uuid, poop_id: Uuid) {
        let _ = AuthoritativeRuntime::clean(self, action_id, poop_id);
    }

    fn nap(&self, action_id: Uuid) {
        let _ = AuthoritativeRuntime::nap(self, action_id);
    }

    fn pet(&self, action_id: Uuid, interaction_ms: u64, pointer_distance: f32) {
        let _ = AuthoritativeRuntime::pet(self, action_id, interaction_ms, pointer_distance);
    }

    fn pet_stroke(&self, action_id: Uuid) {
        let _ = AuthoritativeRuntime::pet_stroke(self, action_id);
    }
}

/// A host-owned signal delivered to a running terminal session.
///
/// The session consumes these values but never installs a signal handler. The
/// launcher (or a later host) remains the sole owner of signal installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalSessionSignal {
    /// Deliver SIGINT semantics to the Codex PTY process group and await child
    /// reaping.
    Interrupt,
    /// Deliver SIGTERM semantics to the Codex PTY process group and await
    /// reaping.
    Terminate,
    /// Re-query the physical terminal and resize both PTY and virtual screen.
    WindowChange,
}

/// Sender half of the externally-owned session signal channel.
pub type TerminalSessionSignalSender = Sender<TerminalSessionSignal>;

/// Receiver half of the externally-owned session signal channel.
pub type TerminalSessionSignalReceiver = Receiver<TerminalSessionSignal>;

/// Production event-source seam used by the real Crossterm adapter and
/// composed PTY integration tests. Implementations yield the same events the
/// session would receive from the physical terminal.
pub type TerminalSessionEventFuture<'a> =
    Pin<Box<dyn Future<Output = Option<Result<Event, io::Error>>> + 'a>>;

pub trait TerminalSessionEventSource {
    fn next(&mut self) -> TerminalSessionEventFuture<'_>;
}

impl TerminalSessionEventSource for EventStream {
    fn next(&mut self) -> TerminalSessionEventFuture<'_> {
        Box::pin(StreamExt::next(self))
    }
}

/// Creates the bounded signal seam consumed by [`run_terminal_session`].
#[must_use]
pub fn terminal_session_signal_channel(
    capacity: usize,
) -> (TerminalSessionSignalSender, TerminalSessionSignalReceiver) {
    mpsc::channel(capacity.max(1))
}

/// Error returned by the launcher-aware terminal entry point.
///
/// Initialization is reported together with the still-unused bounded signal
/// receiver so `--ui auto` can continue through the inherited stdio path
/// without reinstalling signal handlers. Once terminal entry succeeds, every
/// error is terminal-session-owned and cannot trigger fallback.
pub enum TerminalSessionStartError {
    Initialization {
        error: TerminalSessionError,
        signals: TerminalSessionSignalReceiver,
    },
    Session(TerminalSessionError),
}

/// Errors raised by terminal entry, PTY setup, event processing, or cleanup.
#[derive(Debug)]
pub enum TerminalSessionError {
    /// Physical terminal setup failed before child spawn.
    Initialization(TerminalEntryError),
    /// A test/injected spawn seam declined to spawn a child.
    SpawnUnavailable,
    /// The real PTY child could not be spawned.
    Spawn(PtyCodexError),
    /// The PTY reader reported an I/O failure.
    Reader(io::Error),
    /// The physical event stream or PTY input writer failed.
    Input(io::Error),
    /// Ratatui/Crossterm rendering failed.
    Render(io::Error),
    /// PTY resize failed.
    Resize(PtyCodexError),
    /// Subscribing to the authoritative runtime failed.
    Runtime(RuntimeError),
    /// Child polling, waiting, or reaping failed.
    Child(PtyCodexError),
    /// A reader thread could not be joined after the PTY was closed.
    ReaderTask(String),
    /// Process/reader cleanup failed while unwinding a session body.
    Cleanup {
        error: Option<Box<Self>>,
        failures: Vec<String>,
    },
    /// Restoration failed after an otherwise successful body.
    Restoration(TerminalRestoreError),
    /// Both the session body and terminal restoration failed.
    Body {
        error: Box<Self>,
        restoration: TerminalRestoreError,
    },
}

impl TerminalSessionError {
    /// Returns restoration context attached to this error, if any.
    #[must_use]
    pub fn restoration(&self) -> Option<&TerminalRestoreError> {
        match self {
            Self::Initialization(error) => error.restoration(),
            Self::Body { restoration, .. } | Self::Restoration(restoration) => Some(restoration),
            Self::Cleanup { error, .. } => error.as_deref().and_then(Self::restoration),
            Self::SpawnUnavailable
            | Self::Spawn(_)
            | Self::Reader(_)
            | Self::Input(_)
            | Self::Render(_)
            | Self::Resize(_)
            | Self::Runtime(_)
            | Self::Child(_)
            | Self::ReaderTask(_) => None,
        }
    }
}

impl fmt::Display for TerminalSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialization(error) => error.fmt(f),
            Self::SpawnUnavailable => f.write_str("Codex spawn was unavailable"),
            Self::Spawn(error) => error.fmt(f),
            Self::Reader(error) => write!(f, "Codex PTY reader failed: {error}"),
            Self::Input(error) => write!(f, "terminal input failed: {error}"),
            Self::Render(error) => write!(f, "terminal render failed: {error}"),
            Self::Resize(error) => error.fmt(f),
            Self::Runtime(error) => write!(f, "authoritative runtime failed: {error}"),
            Self::Child(error) => error.fmt(f),
            Self::ReaderTask(error) => write!(f, "Codex PTY reader task failed: {error}"),
            Self::Cleanup { error, failures } => {
                if let Some(error) = error {
                    write!(f, "{error}; ")?;
                }
                write!(f, "session cleanup failed")?;
                for failure in failures {
                    write!(f, "; {failure}")?;
                }
                Ok(())
            }
            Self::Restoration(error) => write!(f, "terminal restoration failed: {error}"),
            Self::Body { error, restoration } => write!(
                f,
                "session failed: {error}; restoration also failed: {restoration}"
            ),
        }
    }
}

impl std::error::Error for TerminalSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Initialization(error) => Some(error),
            Self::Spawn(error) | Self::Resize(error) | Self::Child(error) => Some(error),
            Self::Reader(error) | Self::Input(error) | Self::Render(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::Restoration(error) => Some(error),
            Self::Body { error, .. } => Some(error.as_ref()),
            Self::Cleanup { error, .. } => error.as_deref().and_then(|error| error.source()),
            Self::SpawnUnavailable | Self::ReaderTask(_) => None,
        }
    }
}

impl TerminalSessionError {
    fn with_cleanup(self, failures: Vec<String>) -> Self {
        if failures.is_empty() {
            self
        } else {
            Self::Cleanup {
                error: Some(Box::new(self)),
                failures,
            }
        }
    }

    fn cleanup_only(failures: Vec<String>) -> Self {
        Self::Cleanup {
            error: None,
            failures,
        }
    }
}

/// Deterministic session state shared by the real PTY adapter and tests.
///
/// It owns only bounded `vt100` state. It never accumulates raw PTY output;
/// callers process one bounded chunk at a time and use [`Self::encode_event`]
/// to consult the modes negotiated by the latest output.
pub struct TerminalSessionCore {
    screen: CodexScreen,
    layout: TerminalLayout,
    snapshot: Option<SimulationSnapshot>,
    presentation: PresentationState,
}

impl TerminalSessionCore {
    /// Creates a virtual Codex screen with `(rows, columns)` dimensions.
    #[must_use]
    pub fn new(rows: u16, columns: u16) -> Self {
        Self::with_seed(rows, columns, DEFAULT_BEHAVIOR_SEED)
    }

    /// Creates a session core with a deterministic presentation seed.
    #[must_use]
    pub fn with_seed(rows: u16, columns: u16, seed: u64) -> Self {
        Self {
            screen: CodexScreen::new(rows, columns),
            layout: choose_layout(Rect::new(0, 0, columns, rows), None),
            snapshot: None,
            presentation: PresentationState::new(seed),
        }
    }

    /// Returns the production virtual screen used by the renderer.
    #[must_use]
    pub fn screen(&self) -> &CodexScreen {
        &self.screen
    }

    /// Returns the current Full/Compact/Minimal pane split.
    #[must_use]
    pub fn layout(&self) -> TerminalLayout {
        self.layout
    }

    /// Returns the latest authoritative room snapshot, if any.
    #[must_use]
    pub fn snapshot(&self) -> Option<&SimulationSnapshot> {
        self.snapshot.as_ref()
    }

    /// Replaces the authoritative room snapshot.
    pub fn set_snapshot(&mut self, snapshot: SimulationSnapshot) {
        self.snapshot = Some(snapshot);
    }

    /// Advances the autonomous presentation clock to `now` using the current
    /// snapshot and room area.
    pub fn advance_presentation(&mut self, now: std::time::Duration) {
        let _ = self
            .presentation
            .tick(now, self.snapshot.as_ref(), self.layout.room);
    }

    /// Triggers the presentation-only eating reaction after an authoritative
    /// feed.
    pub fn react_to_feed(&mut self, now: std::time::Duration) {
        self.presentation.react_to_feed(now);
    }

    /// Triggers the presentation-only petted reaction after an authoritative
    /// pet.
    pub fn react_to_pet(&mut self, now: std::time::Duration) {
        self.presentation.react_to_pet(now);
    }

    /// The current deterministic presentation frame (pose + wander offset).
    #[must_use]
    pub fn presentation_frame(&self) -> PresentationFrame {
        self.presentation.frame()
    }

    /// Feeds one PTY output chunk into the virtual screen and mode read model,
    /// returning bounded terminal-query replies for the child PTY.
    pub fn process_output(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.screen.process(bytes)
    }

    /// Resizes the virtual screen using `(rows, columns)` order.
    pub fn resize(&mut self, rows: u16, columns: u16) {
        self.layout = choose_layout(Rect::new(0, 0, columns, rows), Some(self.layout.room_mode));
        self.screen
            .resize(self.layout.codex.height, self.layout.codex.width);
    }

    /// Encodes a physical event from the mode state current at this instant.
    /// Resize events are handled by the adapter because they must re-query the
    /// physical terminal before resizing the PTY and screen.
    #[must_use]
    pub fn encode_event(&self, event: &Event) -> Vec<u8> {
        let modes = self.screen.input_modes();
        match event {
            Event::Key(event) => encode_key_event(*event, modes),
            Event::Paste(content) => encode_paste(content, modes),
            Event::FocusGained => encode_focus_event(true, modes),
            Event::FocusLost => encode_focus_event(false, modes),
            Event::Mouse(event) => encode_mouse_event(*event, modes),
            Event::Resize(_, _) => Vec::new(),
        }
    }
}

/// Runs a small injected lifecycle seam used by deterministic tests.
///
/// Entry is completed before the callback is invoked, so an initialization
/// failure can never result in a spawn callback. The callback receives the
/// exact `CodexInvocation` and physical dimensions in `(rows, columns)` order.
pub fn initialize_terminal_and_spawn<B, F>(
    backend: B,
    invocation: &CodexInvocation,
    spawn: F,
) -> Result<(), TerminalSessionError>
where
    B: TerminalBackend,
    F: FnOnce(&CodexInvocation, u16, u16) -> Result<(), TerminalSessionError>,
{
    let mut guard = TerminalGuard::enter(backend).map_err(TerminalSessionError::Initialization)?;
    let dimensions = match guard.backend_mut().size() {
        Ok((columns, rows)) => (rows, columns),
        Err(error) => return finish_guard(&mut guard, Err(TerminalSessionError::Input(error))),
    };
    let body = spawn(invocation, dimensions.0, dimensions.1);
    finish_guard(&mut guard, body)
}

/// Runs the real interactive Codex PTY session.
///
/// `signals` is supplied by the host/launcher. This function does not install
/// signal handlers and spawns the invocation exactly once, after successful
/// physical-terminal initialization.
pub async fn run_terminal_session(
    invocation: &CodexInvocation,
    signals: TerminalSessionSignalReceiver,
) -> Result<portable_pty::ExitStatus, TerminalSessionError> {
    run_terminal_session_with_events(invocation, signals, EventStream::new(), None).await
}

/// Runs the real terminal session with an injected event source.
///
/// The PTY, terminal guard, renderer, input encoder, signal handling, and
/// cleanup path are identical to [`run_terminal_session`]. The seam exists so
/// an integration test or a later host can provide a scripted event stream
/// without creating a second session implementation.
pub async fn run_terminal_session_with_events<E>(
    invocation: &CodexInvocation,
    signals: TerminalSessionSignalReceiver,
    mut events: E,
    runtime: Option<Arc<AuthoritativeRuntime>>,
) -> Result<portable_pty::ExitStatus, TerminalSessionError>
where
    E: TerminalSessionEventSource,
{
    let mut guard = TerminalGuard::enter(CrosstermTerminal::new())
        .map_err(TerminalSessionError::Initialization)?;
    let body = run_session_after_entry(
        &mut guard,
        invocation,
        signals,
        &mut events,
        runtime,
        || Ok(()),
    )
    .await;
    finish_guard(&mut guard, body)
}

/// Runs the production terminal session while invoking `before_spawn` at the
/// exact PTY spawn boundary.
///
/// Unlike [`run_terminal_session`], this launcher seam preserves the bounded
/// signal receiver when physical-terminal initialization fails. The caller may
/// then perform the only permitted `--ui auto` inherited fallback.
pub async fn run_terminal_session_with_spawn_guard_and_initialization_recovery<F>(
    invocation: &CodexInvocation,
    signals: TerminalSessionSignalReceiver,
    runtime: Option<Arc<AuthoritativeRuntime>>,
    before_spawn: F,
) -> Result<portable_pty::ExitStatus, TerminalSessionStartError>
where
    F: FnOnce() -> Result<(), TerminalSessionError>,
{
    let mut signals = Some(signals);
    let mut guard = match TerminalGuard::enter(CrosstermTerminal::new()) {
        Ok(guard) => guard,
        Err(error) => {
            return Err(TerminalSessionStartError::Initialization {
                error: TerminalSessionError::Initialization(error),
                signals: signals
                    .take()
                    .expect("terminal signal receiver is retained on initialization failure"),
            });
        }
    };
    let mut events = EventStream::new();
    let body = run_session_after_entry(
        &mut guard,
        invocation,
        signals
            .take()
            .expect("terminal signal receiver is moved after successful entry"),
        &mut events,
        runtime,
        before_spawn,
    )
    .await;
    finish_guard(&mut guard, body).map_err(TerminalSessionStartError::Session)
}

fn finish_guard<B, T>(
    guard: &mut TerminalGuard<B>,
    body: Result<T, TerminalSessionError>,
) -> Result<T, TerminalSessionError>
where
    B: TerminalBackend,
{
    let restoration = guard.restore().err();
    match (body, restoration) {
        (Ok(value), None) => Ok(value),
        (Ok(_), Some(error)) => Err(TerminalSessionError::Restoration(error)),
        (Err(error), None) => Err(error),
        (Err(error), Some(restoration)) => Err(TerminalSessionError::Body {
            error: Box::new(error),
            restoration,
        }),
    }
}

const OUTPUT_CHANNEL_CAPACITY: usize = 16;
const OUTPUT_CHUNK_BYTES: usize = 8 * 1024;
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(10);
const BEHAVIOR_TICK_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_BEHAVIOR_SEED: u64 = 0x436f_6465_476f_7474; // "CodeGott"
const READER_JOIN_GRACE: Duration = Duration::from_millis(100);
const READER_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(5);

enum ReaderMessage {
    Data(Vec<u8>),
    Eof,
    Error(io::Error),
}

/// Owns every resource whose drop order matters while the session future is
/// suspended or cancelled. In particular, a blocked reader can be waiting on
/// either PTY EOF or a bounded-channel send. Kill/drop the child first, then
/// disconnect the receiver, and only then cancel/boundedly join the reader
/// thread.
struct SessionResources {
    child: Option<PtyCodexChild>,
    writer: Option<PtyWriter>,
    output_receiver: Option<Receiver<ReaderMessage>>,
    reader_task: Option<ReaderThreadGuard>,
}

impl Drop for SessionResources {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Explicitly consume the process-group identity before the child
            // handle is dropped. PtyCodexChild then performs bounded reaping.
            if !child.is_reaped() {
                let _ = child.kill_group();
            }
            drop(child);
        }
        self.writer.take();
        // Receiver closure must happen before ReaderThreadGuard's Drop joins;
        // blocking_send then returns instead of deadlocking cancellation.
        self.output_receiver.take();
        self.reader_task.take();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionWorkKind {
    Signal,
    Event,
    Poll,
    Output,
}

/// Returns the deterministic branch order for one scheduler turn.
///
/// Output is intentionally rotated through the order rather than put first
/// in a biased `select!`: if all branches are ready, each control branch wins
/// at least once every four turns, while output remains continuously bounded
/// to one chunk per turn.
fn session_work_order(turn: usize) -> [SessionWorkKind; 4] {
    const BASE: [SessionWorkKind; 4] = [
        SessionWorkKind::Signal,
        SessionWorkKind::Event,
        SessionWorkKind::Poll,
        SessionWorkKind::Output,
    ];
    let offset = turn % BASE.len();
    [
        BASE[offset],
        BASE[(offset + 1) % BASE.len()],
        BASE[(offset + 2) % BASE.len()],
        BASE[(offset + 3) % BASE.len()],
    ]
}

enum SessionWork {
    Signal(Option<TerminalSessionSignal>),
    Event(Option<Result<Event, io::Error>>),
    Poll,
    Output(Option<ReaderMessage>),
}

async fn next_session_work(
    output_receiver: &mut Receiver<ReaderMessage>,
    reader_done: bool,
    event_stream: &mut impl TerminalSessionEventSource,
    child_alive: bool,
    signal_receiver: &mut Option<TerminalSessionSignalReceiver>,
    poll: &mut tokio::time::Interval,
    turn: usize,
) -> SessionWork {
    match session_work_order(turn)[0] {
        SessionWorkKind::Signal => tokio::select! {
            biased;
            signal = receive_signal(signal_receiver), if child_alive => SessionWork::Signal(signal),
            event = event_stream.next(), if child_alive => SessionWork::Event(event),
            _ = poll.tick(), if child_alive => SessionWork::Poll,
            output = output_receiver.recv(), if !reader_done => SessionWork::Output(output),
        },
        SessionWorkKind::Event => tokio::select! {
            biased;
            event = event_stream.next(), if child_alive => SessionWork::Event(event),
            _ = poll.tick(), if child_alive => SessionWork::Poll,
            output = output_receiver.recv(), if !reader_done => SessionWork::Output(output),
            signal = receive_signal(signal_receiver), if child_alive => SessionWork::Signal(signal),
        },
        SessionWorkKind::Poll => tokio::select! {
            biased;
            _ = poll.tick(), if child_alive => SessionWork::Poll,
            output = output_receiver.recv(), if !reader_done => SessionWork::Output(output),
            signal = receive_signal(signal_receiver), if child_alive => SessionWork::Signal(signal),
            event = event_stream.next(), if child_alive => SessionWork::Event(event),
        },
        SessionWorkKind::Output => tokio::select! {
            biased;
            output = output_receiver.recv(), if !reader_done => SessionWork::Output(output),
            signal = receive_signal(signal_receiver), if child_alive => SessionWork::Signal(signal),
            event = event_stream.next(), if child_alive => SessionWork::Event(event),
            _ = poll.tick(), if child_alive => SessionWork::Poll,
        },
    }
}

async fn run_session_after_entry<F>(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    invocation: &CodexInvocation,
    signals: TerminalSessionSignalReceiver,
    events: &mut impl TerminalSessionEventSource,
    runtime: Option<Arc<AuthoritativeRuntime>>,
    before_spawn: F,
) -> Result<portable_pty::ExitStatus, TerminalSessionError>
where
    F: FnOnce() -> Result<(), TerminalSessionError>,
{
    let (columns, rows) = guard
        .backend_mut()
        .size()
        .map_err(TerminalSessionError::Input)?;
    let mut core = TerminalSessionCore::new(rows, columns);
    // Normalize the virtual screen and pane split to the Codex rectangle
    // before the child exists, so the spawned PTY, the virtual screen, and
    // the rendered upper pane always agree on dimensions. Without this the
    // child would believe it owns the full terminal while the room covered
    // its lower rows.
    core.resize(rows, columns);
    let codex = core.layout().codex;
    if codex.width == 0 || codex.height == 0 {
        return Err(TerminalSessionError::Input(io::Error::new(
            io::ErrorKind::InvalidInput,
            "terminal is too small for a Codex pane plus the CodeGotchi room",
        )));
    }
    let mut snapshot_receiver = None;
    if let Some(runtime) = runtime.as_ref() {
        let (snapshot, receiver) = runtime.subscribe().map_err(TerminalSessionError::Runtime)?;
        core.set_snapshot(snapshot);
        snapshot_receiver = Some(receiver);
    }

    // This is the sole production spawn call. Entry and the physical size
    // query above have already succeeded, preserving pre-spawn UI fallback.
    before_spawn()?;
    let mut child = PtyCodexChild::spawn(invocation, codex.height, codex.width)
        .map_err(TerminalSessionError::Spawn)?;
    let reader = match child.interruptible_reader() {
        Ok(reader) => reader,
        Err(error) => {
            let cleanup = terminate_after_setup_failure(&mut child).await;
            return Err(
                TerminalSessionError::Reader(io::Error::other(error.to_string()))
                    .with_cleanup(cleanup),
            );
        }
    };
    let writer = match child.writer() {
        Ok(writer) => writer,
        Err(error) => {
            let cleanup = terminate_after_setup_failure(&mut child).await;
            return Err(
                TerminalSessionError::Input(io::Error::other(error.to_string()))
                    .with_cleanup(cleanup),
            );
        }
    };

    let (output_receiver, reader_thread, reader_cancellation) = spawn_reader(reader);
    let mut resources = SessionResources {
        child: Some(child),
        writer: Some(writer),
        output_receiver: Some(output_receiver),
        reader_task: Some(ReaderThreadGuard::new(reader_thread, reader_cancellation)),
    };
    // Keep exactly one Ratatui compositor for the hosted session. Its backend
    // borrows the stdout retained by `TerminalGuard`; dropping this local at
    // function return releases that borrow before the guard is restored.
    let mut body_error = None;
    let mut compositor = match Terminal::new(CrosstermBackend::new(guard.writer_mut())) {
        Ok(compositor) => Some(compositor),
        Err(error) => {
            body_error = Some(TerminalSessionError::Render(error));
            None
        }
    };
    let mut child_status = None;
    let mut reader_done = false;
    let mut signal_receiver = Some(signals);
    let mut interrupt_sent = false;
    let mut terminate_sent = false;
    let mut poll = tokio::time::interval(CHILD_POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let session_started = Instant::now();
    let mut last_behavior_at = Duration::ZERO;
    let mut last_behavior_frame = core.presentation_frame();
    let mut fairness_turn = 0;
    let mut event_redraw = false;
    let mut room_input = RoomInputSession::default();
    let runtime = runtime.as_ref();

    if body_error.is_none()
        && let Err(error) = draw_frame(
            compositor
                .as_mut()
                .expect("compositor exists when initial draw starts"),
            &core,
            None,
        )
    {
        body_error = Some(error);
    }

    while body_error.is_none() && (child_status.is_none() || !reader_done) {
        let work = next_session_work(
            resources
                .output_receiver
                .as_mut()
                .expect("session owns output receiver until cleanup"),
            reader_done,
            events,
            child_status.is_none(),
            &mut signal_receiver,
            &mut poll,
            fairness_turn,
        )
        .await;
        fairness_turn = (fairness_turn + 1) % 4;
        match work {
            SessionWork::Output(message) => match message {
                Some(ReaderMessage::Data(bytes)) => {
                    let replies = core.process_output(&bytes);
                    if !replies.is_empty()
                        && let Err(error) = write_input(&mut resources.writer, &replies)
                    {
                        body_error = Some(error);
                    }
                    if body_error.is_none()
                        && let Err(error) = draw_frame(
                            compositor
                                .as_mut()
                                .expect("compositor exists while session is rendering"),
                            &core,
                            room_input.active_drag(),
                        )
                    {
                        body_error = Some(error);
                    }
                }
                Some(ReaderMessage::Eof) | None => reader_done = true,
                Some(ReaderMessage::Error(error)) => {
                    body_error = Some(TerminalSessionError::Reader(error));
                }
            },
            SessionWork::Event(event) => match event {
                Some(Ok(event)) => {
                    if let Err(error) = handle_event(
                        compositor
                            .as_mut()
                            .expect("compositor exists while session handles events"),
                        &mut core,
                        &mut resources.child,
                        &mut resources.writer,
                        event,
                        runtime,
                        &mut room_input,
                        session_started.elapsed(),
                    ) {
                        body_error = Some(error);
                    } else {
                        // Mouse/key events can change the room (drag ghost,
                        // eating/petted reactions), so redraw after each one.
                        event_redraw = true;
                    }
                }
                Some(Err(error)) => body_error = Some(TerminalSessionError::Input(error)),
                None => {
                    body_error = Some(TerminalSessionError::Input(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Crossterm event stream closed",
                    )))
                }
            },
            SessionWork::Signal(signal) => match signal {
                Some(signal) => {
                    let should_handle = match signal {
                        TerminalSessionSignal::WindowChange => true,
                        TerminalSessionSignal::Interrupt | TerminalSessionSignal::Terminate => {
                            match signal {
                                TerminalSessionSignal::Interrupt if !interrupt_sent => {
                                    interrupt_sent = true;
                                    true
                                }
                                TerminalSessionSignal::Terminate if !terminate_sent => {
                                    terminate_sent = true;
                                    true
                                }
                                _ => false,
                            }
                        }
                    };
                    if should_handle
                        && let Err(error) = handle_signal(
                            compositor
                                .as_mut()
                                .expect("compositor exists while session handles signals"),
                            &mut core,
                            &mut resources.child,
                            signal,
                        )
                    {
                        body_error = Some(error);
                    }
                }
                None => signal_receiver = None,
            },
            SessionWork::Poll => match poll_child(&mut resources.child) {
                Ok(Some(status)) => {
                    child_status = Some(status);
                    let mut cleanup = resources
                        .child
                        .as_mut()
                        .map(cleanup_process_group)
                        .unwrap_or_default();
                    if let Some(child) = resources.child.as_mut()
                        && let Some(error) = child.take_cleanup_error()
                        && !is_process_gone(&error)
                    {
                        cleanup.push(format!("process-group cleanup: {error}"));
                    }
                    if !cleanup.is_empty() {
                        body_error = Some(TerminalSessionError::cleanup_only(cleanup));
                    }
                    resources.writer.take();
                    // try_wait marked this child reaped; the one-shot explicit
                    // descendant cleanup above consumed the PGID if needed.
                    resources.child.take();
                }
                Ok(None) => {}
                Err(error) => body_error = Some(error),
            },
        }
        if body_error.is_none() {
            let mut redraw = event_redraw;
            // Autonomous presentation tick: advance at a bounded rate and
            // redraw only when the pose or wander offset changed.
            let behavior_now = session_started.elapsed();
            if behavior_now.saturating_sub(last_behavior_at) >= BEHAVIOR_TICK_INTERVAL {
                last_behavior_at = behavior_now;
                core.advance_presentation(behavior_now);
                let frame = core.presentation_frame();
                if frame != last_behavior_frame {
                    last_behavior_frame = frame;
                    redraw = true;
                }
                // Continuous petting: while a qualified gesture is active,
                // each tick applies a small authoritative happiness gain so
                // the bar rises during the gesture, not only on release. The
                // petted reaction keeps the pet smiling while stroked.
                if let Some(runtime) = runtime
                    && room_input.petting_qualified()
                {
                    core.react_to_pet(behavior_now);
                    let _ = runtime.pet_stroke(Uuid::new_v4());
                    redraw = true;
                }
            }
            if let Some(receiver) = snapshot_receiver.as_mut() {
                loop {
                    match receiver.try_recv() {
                        Ok(snapshot) => {
                            core.set_snapshot(snapshot);
                            redraw = true;
                        }
                        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                    }
                }
            }
            if redraw
                && let Err(error) = draw_frame(
                    compositor
                        .as_mut()
                        .expect("compositor exists while session is rendering"),
                    &core,
                    room_input.active_drag(),
                )
            {
                body_error = Some(error);
            }
        }
    }

    // An operational error still owns a live process. Terminate and reap it
    // before releasing the master so no process survives restoration. The
    // bounded poll also gives cancellation-free cleanup a liveness deadline.
    let mut cleanup_failures = Vec::new();
    if let Some(mut running) = resources.child.take() {
        let (status, failures) = shutdown_child(&mut running).await;
        cleanup_failures.extend(failures);
        if child_status.is_none() {
            child_status = status;
        }
    }
    resources.writer.take();
    resources.output_receiver.take();

    if let Some(reader_task) = resources.reader_task.as_mut()
        && let Err(error) = reader_task.join()
    {
        cleanup_failures.push(error.to_string());
    }

    if let Some(error) = body_error {
        return Err(error.with_cleanup(cleanup_failures));
    }
    if !cleanup_failures.is_empty() {
        return Err(TerminalSessionError::cleanup_only(cleanup_failures));
    }
    child_status.ok_or_else(|| {
        TerminalSessionError::Child(PtyCodexError::Wait {
            source: io::Error::new(io::ErrorKind::UnexpectedEof, "Codex child did not exit"),
        })
    })
}

async fn terminate_after_setup_failure(child: &mut PtyCodexChild) -> Vec<String> {
    shutdown_child(child).await.1
}

fn cleanup_process_group(child: &mut PtyCodexChild) -> Vec<String> {
    let mut failures = Vec::new();
    if let Err(error) = child.kill_group()
        && !is_process_gone(&error)
    {
        failures.push(format!("process-group cleanup: {error}"));
    }
    failures
}

fn is_process_gone(error: &PtyCodexError) -> bool {
    #[cfg(unix)]
    {
        let source = match error {
            PtyCodexError::Open { source, .. }
            | PtyCodexError::CurrentDirectory { source }
            | PtyCodexError::Reaper { source }
            | PtyCodexError::Spawn { source, .. }
            | PtyCodexError::Reader { source }
            | PtyCodexError::Writer { source }
            | PtyCodexError::Resize { source, .. }
            | PtyCodexError::Wait { source }
            | PtyCodexError::Kill { source }
            | PtyCodexError::Signal { source, .. } => source,
        };
        source.raw_os_error() == Some(nix::errno::Errno::ESRCH as i32)
    }
    #[cfg(not(unix))]
    {
        let _ = error;
        false
    }
}

fn finish_signal_delivery(result: Result<(), PtyCodexError>) -> Result<(), TerminalSessionError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if is_process_gone(&error) => Ok(()),
        Err(error) => Err(TerminalSessionError::Child(error)),
    }
}

async fn shutdown_child(
    child: &mut PtyCodexChild,
) -> (Option<portable_pty::ExitStatus>, Vec<String>) {
    const TERM_GRACE: Duration = Duration::from_millis(250);
    const KILL_GRACE: Duration = Duration::from_millis(500);

    let mut failures = Vec::new();
    if let Err(error) = child.terminate()
        && !is_process_gone(&error)
    {
        failures.push(format!("SIGTERM delivery: {error}"));
    }
    let term_deadline = std::time::Instant::now() + TERM_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                failures.extend(cleanup_process_group(child));
                if let Some(error) = child.take_cleanup_error()
                    && !is_process_gone(&error)
                {
                    failures.push(format!("process-group cleanup: {error}"));
                }
                return (Some(status), failures);
            }
            Ok(None) if std::time::Instant::now() < term_deadline => {
                tokio::time::sleep(CHILD_POLL_INTERVAL).await;
            }
            Ok(None) => break,
            Err(error) => {
                failures.push(format!("child poll during cleanup: {error}"));
                break;
            }
        }
    }

    failures.extend(cleanup_process_group(child));
    if let Some(error) = child.take_cleanup_error()
        && !is_process_gone(&error)
    {
        failures.push(format!("process-group cleanup: {error}"));
    }
    let kill_deadline = std::time::Instant::now() + KILL_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(error) = child.take_cleanup_error()
                    && !is_process_gone(&error)
                {
                    failures.push(format!("process-group cleanup: {error}"));
                }
                return (Some(status), failures);
            }
            Ok(None) if std::time::Instant::now() < kill_deadline => {
                tokio::time::sleep(CHILD_POLL_INTERVAL).await;
            }
            Ok(None) => {
                failures.push("child did not exit before cleanup deadline".to_owned());
                return (None, failures);
            }
            Err(error) => {
                failures.push(format!("child poll after SIGKILL: {error}"));
                if let Some(cleanup_error) = child.take_cleanup_error()
                    && !is_process_gone(&cleanup_error)
                {
                    failures.push(format!("process-group cleanup: {cleanup_error}"));
                }
                return (None, failures);
            }
        }
    }
}

struct ReaderCancellation {
    cancelled: Arc<AtomicBool>,
}

struct ReaderThreadGuard {
    thread: Option<thread::JoinHandle<()>>,
    cancellation: ReaderCancellation,
}

impl ReaderThreadGuard {
    fn new(thread: thread::JoinHandle<()>, cancellation: ReaderCancellation) -> Self {
        Self {
            thread: Some(thread),
            cancellation,
        }
    }

    fn cancel(&self) {
        self.cancellation.cancel();
    }

    fn join(&mut self) -> Result<(), TerminalSessionError> {
        self.cancel();
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        let deadline = Instant::now() + READER_JOIN_GRACE;
        while !thread.is_finished() && Instant::now() < deadline {
            thread::sleep(READER_JOIN_POLL_INTERVAL);
        }
        if !thread.is_finished() {
            // Dropping a JoinHandle detaches it. The production PTY reader
            // uses bounded fd readiness and therefore reaches this branch
            // only when its underlying backend violated the cancellation
            // contract; returning an error keeps session cleanup bounded.
            return Err(TerminalSessionError::ReaderTask(
                "reader thread did not stop before cleanup deadline".to_owned(),
            ));
        }
        thread
            .join()
            .map_err(|_| TerminalSessionError::ReaderTask("reader thread panicked".to_owned()))
    }
}

impl Drop for ReaderThreadGuard {
    fn drop(&mut self) {
        self.cancel();
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = Instant::now() + READER_JOIN_GRACE;
        while !thread.is_finished() && Instant::now() < deadline {
            thread::sleep(READER_JOIN_POLL_INTERVAL);
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}

impl ReaderCancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Drop for ReaderCancellation {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn spawn_reader<R>(
    reader: R,
) -> (
    Receiver<ReaderMessage>,
    thread::JoinHandle<()>,
    ReaderCancellation,
)
where
    R: Read + Send + 'static,
{
    let (sender, receiver) = mpsc::channel(OUTPUT_CHANNEL_CAPACITY);
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    let thread = thread::spawn(move || {
        let mut reader = reader;
        let mut chunk = vec![0; OUTPUT_CHUNK_BYTES];
        loop {
            if thread_cancelled.load(Ordering::Acquire) {
                break;
            }
            match reader.read(&mut chunk) {
                Ok(0) => {
                    let _ = sender.blocking_send(ReaderMessage::Eof);
                    break;
                }
                Ok(length) => {
                    if sender
                        .blocking_send(ReaderMessage::Data(chunk[..length].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::yield_now();
                }
                Err(error) => {
                    let _ = sender.blocking_send(ReaderMessage::Error(error));
                    break;
                }
            }
        }
    });
    (receiver, thread, ReaderCancellation { cancelled })
}

async fn receive_signal(
    receiver: &mut Option<TerminalSessionSignalReceiver>,
) -> Option<TerminalSessionSignal> {
    match receiver {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

fn poll_child(
    child: &mut Option<PtyCodexChild>,
) -> Result<Option<portable_pty::ExitStatus>, TerminalSessionError> {
    let Some(child) = child.as_mut() else {
        return Ok(None);
    };
    child.try_wait().map_err(TerminalSessionError::Child)
}

// The event dispatcher needs the compositor, core, PTY seams, the event,
// runtime, room input, and the presentation clock; a context struct would add
// noise for a private dispatcher.
#[allow(clippy::too_many_arguments)]
fn handle_event<B>(
    compositor: &mut Terminal<B>,
    core: &mut TerminalSessionCore,
    child: &mut Option<PtyCodexChild>,
    writer: &mut Option<PtyWriter>,
    event: Event,
    runtime: Option<&Arc<AuthoritativeRuntime>>,
    room_input: &mut RoomInputSession,
    now: Duration,
) -> Result<(), TerminalSessionError>
where
    B: RatatuiBackend<Error = io::Error>,
{
    if let Event::Resize(_, _) = event {
        return resize_session(compositor, core, child.as_ref());
    }
    if let Event::Mouse(mouse) = &event {
        let layout = core.layout();
        let point = Position::new(mouse.column, mouse.row);
        if layout.room.contains(point) {
            let Some(snapshot) = core.snapshot() else {
                return Ok(());
            };
            let requests =
                room_input.process(layout.room, snapshot, &core.presentation_frame(), mouse);
            if let Some(runtime) = runtime {
                for request in requests {
                    match &request {
                        RoomCareRequest::Feed { .. } => core.react_to_feed(now),
                        RoomCareRequest::Pet { .. } => core.react_to_pet(now),
                        RoomCareRequest::Clean { .. }
                        | RoomCareRequest::Nap { .. }
                        | RoomCareRequest::PetStroke { .. } => {}
                    }
                    apply_room_request(runtime.as_ref(), request);
                }
            }
            return Ok(());
        }
    }
    let bytes = core.encode_event(&event);
    if bytes.is_empty() {
        return Ok(());
    }
    write_input(writer, &bytes)
}

fn apply_room_request(runtime: &dyn CareGateway, request: RoomCareRequest) {
    match request {
        RoomCareRequest::Feed { action_id, food_id } => runtime.feed(action_id, &food_id),
        RoomCareRequest::Clean { action_id, poop_id } => runtime.clean(action_id, poop_id),
        RoomCareRequest::Nap { action_id } => runtime.nap(action_id),
        RoomCareRequest::Pet {
            action_id,
            interaction_ms,
            pointer_distance,
        } => runtime.pet(action_id, interaction_ms, pointer_distance),
        RoomCareRequest::PetStroke { action_id } => runtime.pet_stroke(action_id),
    }
}

fn handle_signal<B>(
    compositor: &mut Terminal<B>,
    core: &mut TerminalSessionCore,
    child: &mut Option<PtyCodexChild>,
    signal: TerminalSessionSignal,
) -> Result<(), TerminalSessionError>
where
    B: RatatuiBackend<Error = io::Error>,
{
    match signal {
        TerminalSessionSignal::Interrupt => {
            let Some(child) = child.as_mut() else {
                return Ok(());
            };
            finish_signal_delivery(child.interrupt())?;
        }
        TerminalSessionSignal::Terminate => {
            let Some(child) = child.as_mut() else {
                return Ok(());
            };
            finish_signal_delivery(child.terminate())?;
        }
        TerminalSessionSignal::WindowChange => {
            resize_session(compositor, core, child.as_ref())?;
        }
    }
    Ok(())
}

fn resize_session<B>(
    compositor: &mut Terminal<B>,
    core: &mut TerminalSessionCore,
    child: Option<&PtyCodexChild>,
) -> Result<(), TerminalSessionError>
where
    B: RatatuiBackend<Error = io::Error>,
{
    let size = compositor
        .backend_mut()
        .size()
        .map_err(TerminalSessionError::Input)?;
    let (columns, rows) = (size.width, size.height);
    resize_compositor(compositor, core, rows, columns, child)
}

fn resize_compositor<B>(
    compositor: &mut Terminal<B>,
    core: &mut TerminalSessionCore,
    rows: u16,
    columns: u16,
    child: Option<&PtyCodexChild>,
) -> Result<(), TerminalSessionError>
where
    B: RatatuiBackend<Error = io::Error>,
{
    core.resize(rows, columns);
    let codex = core.layout().codex;
    if let Some(child) = child {
        child
            .resize(codex.height, codex.width)
            .map_err(TerminalSessionError::Resize)?;
    }
    compositor
        .resize(Rect::new(0, 0, columns, rows))
        .map_err(TerminalSessionError::Render)?;
    draw_frame(compositor, core, None)
}

fn write_input(writer: &mut Option<PtyWriter>, bytes: &[u8]) -> Result<(), TerminalSessionError> {
    let Some(writer) = writer.as_mut() else {
        return Ok(());
    };
    writer
        .write_all(bytes)
        .map_err(TerminalSessionError::Input)?;
    writer.flush().map_err(TerminalSessionError::Input)
}

fn draw_frame<B>(
    compositor: &mut Terminal<B>,
    core: &TerminalSessionCore,
    drag: Option<(&str, Position)>,
) -> Result<(), TerminalSessionError>
where
    B: RatatuiBackend<Error = io::Error>,
{
    compositor
        .draw(|frame| {
            let layout = core.layout();
            let cursor = render_codex(core.screen(), layout.codex, frame.buffer_mut());
            if let Some(snapshot) = core.snapshot() {
                render_room(
                    layout.room,
                    frame.buffer_mut(),
                    snapshot,
                    &core.presentation_frame(),
                    drag,
                );
            }
            if let Some(cursor) = cursor {
                frame.set_cursor_position(cursor);
            }
        })
        .map_err(TerminalSessionError::Render)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        io::{self, Read, Write},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    use chrono::Utc;
    use codegotchi_domain::{
        DefaultNeedProgressionStrategy, FoodInventory, Pet, PetSimulation, PetSpecies,
        SimulationSnapshot, SystemClock,
    };
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use crossterm::style::{Colored, force_color_output};
    use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};

    use super::{
        ReaderMessage, ReaderThreadGuard, RoomInputSession, SessionResources, TerminalSessionCore,
        TerminalSessionError, draw_frame, finish_guard, finish_signal_delivery, handle_event,
        resize_compositor, session_work_order, spawn_reader, write_input,
    };
    use crate::terminal::{TerminalBackend, TerminalGuard, TerminalStep};
    use uuid::Uuid;

    fn test_snapshot() -> SimulationSnapshot {
        let now = Utc::now();
        let pet = Pet::with_inventory(
            Uuid::from_u128(1),
            "Mochi",
            PetSpecies::Cat,
            now,
            FoodInventory::starter(),
        );
        PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot()
    }

    struct ScriptedReader {
        steps: VecDeque<io::Result<Vec<u8>>>,
    }

    impl ScriptedReader {
        fn new(steps: impl IntoIterator<Item = io::Result<Vec<u8>>>) -> Self {
            Self {
                steps: steps.into_iter().collect(),
            }
        }
    }

    impl Read for ScriptedReader {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let output = self.steps.pop_front().unwrap_or_else(|| Ok(Vec::new()))?;
            bytes[..output.len()].copy_from_slice(&output);
            Ok(output.len())
        }
    }

    struct RepeatingReader;

    impl Read for RepeatingReader {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            bytes[0] = b'x';
            Ok(1)
        }
    }

    struct BlockingReader {
        released: Arc<AtomicBool>,
    }

    impl Read for BlockingReader {
        fn read(&mut self, _bytes: &mut [u8]) -> io::Result<usize> {
            while !self.released.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(0)
        }
    }

    #[tokio::test]
    async fn reader_forwards_output_and_eof_then_joins_without_hanging() {
        let (mut receiver, thread, cancellation) = spawn_reader(Box::new(ScriptedReader::new([
            Ok(b"ansi".to_vec()),
            Ok(Vec::new()),
        ])));

        assert!(matches!(
            receiver.recv().await,
            Some(ReaderMessage::Data(bytes)) if bytes == b"ansi"
        ));
        assert!(matches!(receiver.recv().await, Some(ReaderMessage::Eof)));
        drop(receiver);
        drop(cancellation);
        thread.join().expect("reader thread should terminate");
    }

    #[tokio::test]
    async fn reader_forwards_errors_and_does_not_spin_after_failure() {
        let (mut receiver, thread, cancellation) =
            spawn_reader(Box::new(ScriptedReader::new([Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "reader failed",
            ))])));

        let message = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .expect("reader error should arrive promptly")
            .expect("reader should send one error message");
        assert!(matches!(
            message,
            ReaderMessage::Error(error) if error.kind() == io::ErrorKind::BrokenPipe
        ));
        drop(receiver);
        drop(cancellation);
        thread.join().expect("reader thread should terminate");
    }

    #[tokio::test]
    async fn reader_backpressure_stops_the_producer_at_the_bounded_channel() {
        let (receiver, thread, cancellation) = spawn_reader(Box::new(RepeatingReader));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(
            !thread.is_finished(),
            "producer should block instead of accumulating unbounded output"
        );
        drop(receiver);
        drop(cancellation);
        thread
            .join()
            .expect("reader should stop after receiver closes");
    }

    #[tokio::test]
    async fn session_resources_disconnect_backpressure_before_joining_reader() {
        let (receiver, thread, cancellation) = spawn_reader(Box::new(RepeatingReader));
        let resources = SessionResources {
            child: None,
            writer: None,
            output_receiver: Some(receiver),
            reader_task: Some(ReaderThreadGuard::new(thread, cancellation)),
        };

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        tokio::time::timeout(std::time::Duration::from_secs(1), async move {
            tokio::task::spawn_blocking(move || drop(resources))
                .await
                .expect("resource drop should not panic");
        })
        .await
        .expect("resource drop should disconnect output before joining reader");
    }

    #[test]
    fn session_resources_drop_is_bounded_when_reader_read_stays_blocked() {
        let released = Arc::new(AtomicBool::new(false));
        let (receiver, reader_handle, cancellation) = spawn_reader(Box::new(BlockingReader {
            released: Arc::clone(&released),
        }));
        let resources = SessionResources {
            child: None,
            writer: None,
            output_receiver: Some(receiver),
            reader_task: Some(ReaderThreadGuard::new(reader_handle, cancellation)),
        };
        let (done_sender, done_receiver) = std::sync::mpsc::channel();
        let drop_thread = thread::spawn(move || {
            drop(resources);
            done_sender
                .send(())
                .expect("drop completion should be observable");
        });

        let completed_before_release = done_receiver
            .recv_timeout(Duration::from_millis(150))
            .is_ok();
        released.store(true, Ordering::Release);
        drop_thread
            .join()
            .expect("resource drop thread should join");

        assert!(
            completed_before_release,
            "reader cleanup synchronously waited for a blocked read"
        );
    }

    #[test]
    fn session_work_order_rotates_output_behind_every_control_branch() {
        use super::SessionWorkKind::{Event, Output, Poll, Signal};

        assert_eq!(session_work_order(0), [Signal, Event, Poll, Output]);
        assert_eq!(session_work_order(1), [Event, Poll, Output, Signal]);
        assert_eq!(session_work_order(2), [Poll, Output, Signal, Event]);
        assert_eq!(session_work_order(3), [Output, Signal, Event, Poll]);
    }

    #[cfg(unix)]
    #[test]
    fn queued_signal_esrch_is_benign_and_does_not_replace_exit_status() {
        for signal in ["SIGINT", "SIGTERM"] {
            let error = super::PtyCodexError::Signal {
                signal,
                source: io::Error::from_raw_os_error(nix::errno::Errno::ESRCH as i32),
            };
            assert!(finish_signal_delivery(Err(error)).is_ok());
        }

        let status = portable_pty::ExitStatus::with_exit_code(0);
        assert_eq!(status.exit_code(), 0);
    }

    struct RecordingWriter {
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes
                .lock()
                .expect("recording writer lock")
                .extend(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct ColorOutputRestore {
        was_disabled: bool,
    }

    impl Drop for ColorOutputRestore {
        fn drop(&mut self) {
            force_color_output(!self.was_disabled);
        }
    }

    #[test]
    fn writer_seam_delivers_exact_encoded_bytes() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let writer = RecordingWriter {
            bytes: Arc::clone(&bytes),
        };
        let mut writer = Some(Box::new(writer) as super::PtyWriter);
        write_input(&mut writer, b"\x1b[200~paste\x1b[201~").expect("write should succeed");
        assert_eq!(
            bytes.lock().expect("recording writer lock").as_slice(),
            b"\x1b[200~paste\x1b[201~"
        );
    }

    fn fixed_compositor(
        bytes: Arc<Mutex<Vec<u8>>>,
        columns: u16,
        rows: u16,
    ) -> Terminal<CrosstermBackend<RecordingWriter>> {
        Terminal::with_options(
            CrosstermBackend::new(RecordingWriter { bytes }),
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, columns, rows)),
            },
        )
        .expect("fixed compositor should initialize")
    }

    fn apply_new_output(
        bytes: &Arc<Mutex<Vec<u8>>>,
        offset: &mut usize,
        parser: &mut vt100::Parser,
    ) {
        let output = bytes.lock().expect("recording writer lock");
        parser.process(&output[*offset..]);
        *offset = output.len();
    }

    #[test]
    fn persistent_compositor_clears_removed_text_from_physical_screen() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut compositor = fixed_compositor(Arc::clone(&bytes), 6, 24);
        let mut parser = vt100::Parser::new(24, 6, 0);
        let mut offset = 0;
        let mut core = TerminalSessionCore::new(24, 6);
        core.process_output(b"HELLO");

        draw_frame(&mut compositor, &core, None).expect("first frame should render");
        apply_new_output(&bytes, &mut offset, &mut parser);
        assert_eq!(
            parser.screen().cell(0, 0).expect("cell exists").contents(),
            "H"
        );

        core.process_output(b"\x1b[2J\x1b[H");
        draw_frame(&mut compositor, &core, None).expect("blank frame should render");
        apply_new_output(&bytes, &mut offset, &mut parser);

        let cell = parser.screen().cell(0, 0).expect("cell exists");
        assert_eq!(
            cell.contents(),
            " ",
            "removed text must be physically erased"
        );
    }

    #[test]
    fn persistent_compositor_resize_clears_stale_style_from_default_cells() {
        let was_disabled = Colored::ansi_color_disabled_memoized();
        force_color_output(true);
        let _restore = ColorOutputRestore { was_disabled };
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut compositor = fixed_compositor(Arc::clone(&bytes), 6, 24);
        let mut parser = vt100::Parser::new(24, 6, 0);
        let mut offset = 0;
        let mut core = TerminalSessionCore::new(24, 6);
        core.process_output(b"\x1b[31;44;1mSTALE");

        draw_frame(&mut compositor, &core, None).expect("styled frame should render");
        apply_new_output(&bytes, &mut offset, &mut parser);
        let styled = parser.screen().cell(0, 0).expect("cell exists");
        assert_eq!(styled.fgcolor(), vt100::Color::Idx(1));
        assert_eq!(styled.bgcolor(), vt100::Color::Idx(4));
        assert!(styled.bold());

        core.process_output(b"\x1b[0m\x1b[2J\x1b[H");
        resize_compositor(&mut compositor, &mut core, 4, 6, None)
            .expect("resize/default frame should render");
        parser.screen_mut().set_size(1, 6);
        apply_new_output(&bytes, &mut offset, &mut parser);

        let cleared = parser.screen().cell(0, 0).expect("cell exists");
        assert_eq!(cleared.contents(), " ");
        assert_eq!(cleared.fgcolor(), vt100::Color::Default);
        assert_eq!(cleared.bgcolor(), vt100::Color::Default);
        assert!(!cleared.bold(), "default cell must not retain stale style");
    }

    #[test]
    fn room_mouse_events_never_reach_the_codex_writer() {
        let compositor_bytes = Arc::new(Mutex::new(Vec::new()));
        let mut compositor = fixed_compositor(Arc::clone(&compositor_bytes), 80, 24);
        let writer_bytes = Arc::new(Mutex::new(Vec::new()));
        let mut core = TerminalSessionCore::new(24, 80);
        core.set_snapshot(test_snapshot());
        let layout = core.layout();
        let mut writer = Some(Box::new(RecordingWriter {
            bytes: Arc::clone(&writer_bytes),
        }) as super::PtyWriter);
        let mut room_input = RoomInputSession::default();

        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: layout.room.y + 2,
            modifiers: KeyModifiers::NONE,
        });
        handle_event(
            &mut compositor,
            &mut core,
            &mut None,
            &mut writer,
            event,
            None,
            &mut room_input,
            Duration::ZERO,
        )
        .expect("room mouse event handles without error");

        assert!(
            writer_bytes.lock().expect("writer lock").is_empty(),
            "room mouse input must never be forwarded to the Codex PTY"
        );
    }

    #[test]
    fn codex_pane_mouse_events_forward_with_negotiated_encoding() {
        let compositor_bytes = Arc::new(Mutex::new(Vec::new()));
        let mut compositor = fixed_compositor(Arc::clone(&compositor_bytes), 80, 24);
        let writer_bytes = Arc::new(Mutex::new(Vec::new()));
        let mut core = TerminalSessionCore::new(24, 80);
        core.process_output(b"\x1b[?1000h\x1b[?1006h");
        let mut writer = Some(Box::new(RecordingWriter {
            bytes: Arc::clone(&writer_bytes),
        }) as super::PtyWriter);
        let mut room_input = RoomInputSession::default();

        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        handle_event(
            &mut compositor,
            &mut core,
            &mut None,
            &mut writer,
            event,
            None,
            &mut room_input,
            Duration::ZERO,
        )
        .expect("codex mouse event encodes");

        assert_eq!(
            writer_bytes.lock().expect("writer lock").as_slice(),
            b"\x1b[<0;6;3M",
            "Codex-pane mouse must use the negotiated SGR encoding"
        );
    }

    struct RestorationBackend {
        fail_focus_restore: Arc<Mutex<bool>>,
    }

    impl RestorationBackend {
        fn operation(&self, step: TerminalStep) -> io::Result<()> {
            if step == TerminalStep::FocusChange
                && *self.fail_focus_restore.lock().expect("failure lock")
            {
                Err(io::Error::other("focus restore failed"))
            } else {
                Ok(())
            }
        }
    }

    impl TerminalBackend for RestorationBackend {
        fn size(&mut self) -> io::Result<(u16, u16)> {
            Ok((80, 24))
        }
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn hide_cursor(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn enable_mouse_capture(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn enable_focus_change(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn enable_bracketed_paste(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn disable_bracketed_paste(&mut self) -> io::Result<()> {
            self.operation(TerminalStep::BracketedPaste)
        }
        fn disable_focus_change(&mut self) -> io::Result<()> {
            self.operation(TerminalStep::FocusChange)
        }
        fn disable_mouse_capture(&mut self) -> io::Result<()> {
            self.operation(TerminalStep::MouseCapture)
        }
        fn show_cursor(&mut self) -> io::Result<()> {
            self.operation(TerminalStep::Cursor)
        }
        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.operation(TerminalStep::AlternateScreen)
        }
        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.operation(TerminalStep::RawMode)
        }
    }

    #[test]
    fn session_error_retains_body_and_restoration_context_together() {
        let failure = Arc::new(Mutex::new(true));
        let backend = RestorationBackend {
            fail_focus_restore: Arc::clone(&failure),
        };
        let mut guard = TerminalGuard::enter(backend).expect("entry should succeed");
        let result: Result<(), TerminalSessionError> =
            finish_guard(&mut guard, Err(TerminalSessionError::SpawnUnavailable));

        match result {
            Err(TerminalSessionError::Body { error, restoration }) => {
                assert!(matches!(*error, TerminalSessionError::SpawnUnavailable));
                assert_eq!(restoration.failures().len(), 1);
                assert_eq!(restoration.failures()[0].step(), TerminalStep::FocusChange);
            }
            other => panic!("expected body plus restoration context, got {other:?}"),
        }
    }
}

//! The interactive Codex PTY session.
//!
//! This module deliberately keeps the terminal lifecycle owner in `host.rs`
//! and the PTY process owner in `pty.rs`.  [`TerminalSessionCore`] is the
//! production-shared, deterministic part of the session: the real adapter
//! below feeds it the same bytes and Crossterm events that focused tests can
//! feed without requiring a physical terminal.

use std::{
    fmt,
    io::{self, Read, Write},
    thread,
    time::Duration,
};

use crossterm::event::{Event, EventStream};
use futures_util::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::CodexInvocation;

use super::pty::{PtyReader, PtyWriter};
use super::{
    CodexScreen, CrosstermTerminal, PtyCodexChild, PtyCodexError, TerminalBackend,
    TerminalEntryError, TerminalGuard, TerminalRestoreError, encode_focus_event, encode_key_event,
    encode_mouse_event, encode_paste, render_codex,
};

/// A host-owned signal delivered to a running terminal session.
///
/// The session consumes these values but never installs a signal handler. The
/// launcher (or a later host) remains the sole owner of signal installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalSessionSignal {
    /// Forward an interrupt byte to the Codex PTY and await child reaping.
    Interrupt,
    /// Terminate the Codex child through portable-pty and await reaping.
    Terminate,
    /// Re-query the physical terminal and resize both PTY and virtual screen.
    WindowChange,
}

/// Sender half of the externally-owned session signal channel.
pub type TerminalSessionSignalSender = Sender<TerminalSessionSignal>;

/// Receiver half of the externally-owned session signal channel.
pub type TerminalSessionSignalReceiver = Receiver<TerminalSessionSignal>;

/// Creates the bounded signal seam consumed by [`run_terminal_session`].
#[must_use]
pub fn terminal_session_signal_channel(
    capacity: usize,
) -> (TerminalSessionSignalSender, TerminalSessionSignalReceiver) {
    mpsc::channel(capacity.max(1))
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
    /// Child polling, waiting, or reaping failed.
    Child(PtyCodexError),
    /// A reader thread could not be joined after the PTY was closed.
    ReaderTask(String),
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
            Self::SpawnUnavailable
            | Self::Spawn(_)
            | Self::Reader(_)
            | Self::Input(_)
            | Self::Render(_)
            | Self::Resize(_)
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
            Self::Child(error) => error.fmt(f),
            Self::ReaderTask(error) => write!(f, "Codex PTY reader task failed: {error}"),
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
            Self::Restoration(error) => Some(error),
            Self::Body { error, .. } => Some(error.as_ref()),
            Self::SpawnUnavailable | Self::ReaderTask(_) => None,
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
}

impl TerminalSessionCore {
    /// Creates a virtual Codex screen with `(rows, columns)` dimensions.
    #[must_use]
    pub fn new(rows: u16, columns: u16) -> Self {
        Self {
            screen: CodexScreen::new(rows, columns),
        }
    }

    /// Returns the production virtual screen used by the renderer.
    #[must_use]
    pub fn screen(&self) -> &CodexScreen {
        &self.screen
    }

    /// Feeds one PTY output chunk into the virtual screen and mode read model.
    pub fn process_output(&mut self, bytes: &[u8]) {
        self.screen.process(bytes);
    }

    /// Resizes the virtual screen using `(rows, columns)` order.
    pub fn resize(&mut self, rows: u16, columns: u16) {
        self.screen.resize(rows, columns);
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
    let mut guard = TerminalGuard::enter(CrosstermTerminal::new())
        .map_err(TerminalSessionError::Initialization)?;
    let body = run_session_after_entry(&mut guard, invocation, signals).await;
    finish_guard(&mut guard, body)
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

enum ReaderMessage {
    Data(Vec<u8>),
    Eof,
    Error(io::Error),
}

async fn run_session_after_entry(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    invocation: &CodexInvocation,
    signals: TerminalSessionSignalReceiver,
) -> Result<portable_pty::ExitStatus, TerminalSessionError> {
    let (columns, rows) = guard
        .backend_mut()
        .size()
        .map_err(TerminalSessionError::Input)?;
    let mut core = TerminalSessionCore::new(rows, columns);

    // This is the sole production spawn call. Entry and the physical size
    // query above have already succeeded, preserving pre-spawn UI fallback.
    let mut child =
        PtyCodexChild::spawn(invocation, rows, columns).map_err(TerminalSessionError::Spawn)?;
    let reader = match child.reader() {
        Ok(reader) => reader,
        Err(error) => {
            terminate_after_setup_failure(&mut child);
            return Err(TerminalSessionError::Reader(io::Error::other(
                error.to_string(),
            )));
        }
    };
    let writer = match child.writer() {
        Ok(writer) => writer,
        Err(error) => {
            terminate_after_setup_failure(&mut child);
            return Err(TerminalSessionError::Input(io::Error::other(
                error.to_string(),
            )));
        }
    };

    let (mut output_receiver, reader_thread) = spawn_reader(reader);
    let mut reader_thread = Some(reader_thread);
    let mut writer = Some(writer);
    let mut child = Some(child);
    let mut child_status = None;
    let mut reader_done = false;
    let mut signal_receiver = Some(signals);
    let mut termination_requested = false;
    let mut event_stream = EventStream::new();
    let mut poll = tokio::time::interval(CHILD_POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut body_error = None;

    if let Err(error) = draw_frame(guard, &core) {
        body_error = Some(error);
    }

    while body_error.is_none() && (child_status.is_none() || !reader_done) {
        tokio::select! {
            biased;
            message = output_receiver.recv(), if !reader_done => {
                match message {
                    Some(ReaderMessage::Data(bytes)) => {
                        core.process_output(&bytes);
                        if let Err(error) = draw_frame(guard, &core) {
                            body_error = Some(error);
                        }
                    }
                    Some(ReaderMessage::Eof) | None => reader_done = true,
                    Some(ReaderMessage::Error(error)) => {
                        body_error = Some(TerminalSessionError::Reader(error));
                    }
                }
            }
            event = event_stream.next(), if child_status.is_none() => {
                match event {
                    Some(Ok(event)) => {
                        if let Err(error) = handle_event(
                            guard,
                            &mut core,
                            &mut child,
                            &mut writer,
                            event,
                        ) {
                            body_error = Some(error);
                        }
                    }
                    Some(Err(error)) => body_error = Some(TerminalSessionError::Input(error)),
                    None => body_error = Some(TerminalSessionError::Input(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Crossterm event stream closed",
                    ))),
                }
            }
            signal = receive_signal(&mut signal_receiver), if child_status.is_none() => {
                match signal {
                    Some(signal) => {
                        let should_handle = matches!(
                            signal,
                            TerminalSessionSignal::WindowChange
                        ) || !termination_requested;
                        if should_handle {
                            if let Err(error) = handle_signal(
                                guard,
                                &mut core,
                                &mut child,
                                &mut writer,
                                signal,
                            ) {
                                body_error = Some(error);
                            }
                            if matches!(
                                signal,
                                TerminalSessionSignal::Interrupt
                                    | TerminalSessionSignal::Terminate
                            ) {
                                termination_requested = true;
                            }
                        }
                    }
                    None => signal_receiver = None,
                }
            }
            _ = poll.tick(), if child_status.is_none() => {
                match poll_child(&mut child) {
                    Ok(Some(status)) => {
                        child_status = Some(status);
                        writer.take();
                        child.take();
                    }
                    Ok(None) => {}
                    Err(error) => body_error = Some(error),
                }
            }
        }
    }

    // An operational error still owns a live process. Terminate and reap it
    // before releasing the master so no process survives restoration.
    if body_error.is_some()
        && let Some(mut running) = child.take()
    {
        let _ = running.kill();
        let _ = running.wait();
    }
    writer.take();
    drop(output_receiver);

    if let Some(reader_thread) = reader_thread.take() {
        join_reader(reader_thread).await?;
    }

    if let Some(error) = body_error {
        return Err(error);
    }
    child_status.ok_or_else(|| {
        TerminalSessionError::Child(PtyCodexError::Wait {
            source: io::Error::new(io::ErrorKind::UnexpectedEof, "Codex child did not exit"),
        })
    })
}

fn terminate_after_setup_failure(child: &mut PtyCodexChild) {
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_reader(reader: PtyReader) -> (Receiver<ReaderMessage>, thread::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel(OUTPUT_CHANNEL_CAPACITY);
    let thread = thread::spawn(move || {
        let mut reader = reader;
        let mut chunk = vec![0; OUTPUT_CHUNK_BYTES];
        loop {
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
                Err(error) => {
                    let _ = sender.blocking_send(ReaderMessage::Error(error));
                    break;
                }
            }
        }
    });
    (receiver, thread)
}

async fn join_reader(thread: thread::JoinHandle<()>) -> Result<(), TerminalSessionError> {
    tokio::task::spawn_blocking(move || thread.join())
        .await
        .map_err(|error| TerminalSessionError::ReaderTask(error.to_string()))?
        .map_err(|_| TerminalSessionError::ReaderTask("reader thread panicked".to_owned()))
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

fn handle_event(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    core: &mut TerminalSessionCore,
    child: &mut Option<PtyCodexChild>,
    writer: &mut Option<PtyWriter>,
    event: Event,
) -> Result<(), TerminalSessionError> {
    if let Event::Resize(_, _) = event {
        return resize_session(guard, core, child.as_ref());
    }
    let bytes = core.encode_event(&event);
    if bytes.is_empty() {
        return Ok(());
    }
    write_input(writer, &bytes)
}

fn handle_signal(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    core: &mut TerminalSessionCore,
    child: &mut Option<PtyCodexChild>,
    writer: &mut Option<PtyWriter>,
    signal: TerminalSessionSignal,
) -> Result<(), TerminalSessionError> {
    match signal {
        TerminalSessionSignal::Interrupt => {
            write_input(writer, b"\x03")?;
        }
        TerminalSessionSignal::Terminate => {
            let Some(child) = child.as_mut() else {
                return Ok(());
            };
            child.kill().map_err(TerminalSessionError::Child)?;
        }
        TerminalSessionSignal::WindowChange => {
            resize_session(guard, core, child.as_ref())?;
        }
    }
    Ok(())
}

fn resize_session(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    core: &mut TerminalSessionCore,
    child: Option<&PtyCodexChild>,
) -> Result<(), TerminalSessionError> {
    let (columns, rows) = guard
        .backend_mut()
        .size()
        .map_err(TerminalSessionError::Input)?;
    if let Some(child) = child {
        child
            .resize(rows, columns)
            .map_err(TerminalSessionError::Resize)?;
    }
    core.resize(rows, columns);
    draw_frame(guard, core)
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

fn draw_frame(
    guard: &mut TerminalGuard<CrosstermTerminal>,
    core: &TerminalSessionCore,
) -> Result<(), TerminalSessionError> {
    let backend = CrosstermBackend::new(guard.writer_mut());
    let mut terminal = Terminal::new(backend).map_err(TerminalSessionError::Render)?;
    terminal
        .draw(|frame| {
            let cursor = render_codex(core.screen(), frame.area(), frame.buffer_mut());
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
        sync::{Arc, Mutex},
    };

    use super::{ReaderMessage, TerminalSessionError, finish_guard, spawn_reader, write_input};
    use crate::terminal::{TerminalBackend, TerminalGuard, TerminalStep};

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

    #[tokio::test]
    async fn reader_forwards_output_and_eof_then_joins_without_hanging() {
        let (mut receiver, thread) = spawn_reader(Box::new(ScriptedReader::new([
            Ok(b"ansi".to_vec()),
            Ok(Vec::new()),
        ])));

        assert!(matches!(
            receiver.recv().await,
            Some(ReaderMessage::Data(bytes)) if bytes == b"ansi"
        ));
        assert!(matches!(receiver.recv().await, Some(ReaderMessage::Eof)));
        drop(receiver);
        thread.join().expect("reader thread should terminate");
    }

    #[tokio::test]
    async fn reader_forwards_errors_and_does_not_spin_after_failure() {
        let (mut receiver, thread) = spawn_reader(Box::new(ScriptedReader::new([Err(
            io::Error::new(io::ErrorKind::BrokenPipe, "reader failed"),
        )])));

        let message = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .expect("reader error should arrive promptly")
            .expect("reader should send one error message");
        assert!(matches!(
            message,
            ReaderMessage::Error(error) if error.kind() == io::ErrorKind::BrokenPipe
        ));
        drop(receiver);
        thread.join().expect("reader thread should terminate");
    }

    #[tokio::test]
    async fn reader_backpressure_stops_the_producer_at_the_bounded_channel() {
        let (receiver, thread) = spawn_reader(Box::new(RepeatingReader));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(
            !thread.is_finished(),
            "producer should block instead of accumulating unbounded output"
        );
        drop(receiver);
        thread
            .join()
            .expect("reader should stop after receiver closes");
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

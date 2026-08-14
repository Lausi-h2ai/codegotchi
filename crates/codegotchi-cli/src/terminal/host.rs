use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::fmt;
use std::io;

/// The physical-terminal operation represented by a lifecycle responsibility.
///
/// This is lifecycle infrastructure, not application-domain state. It is
/// public so deterministic tests and a later host can identify cleanup
/// failures without depending on Crossterm command types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalStep {
    RawMode,
    AlternateScreen,
    Cursor,
    MouseCapture,
    FocusChange,
    BracketedPaste,
}

impl fmt::Display for TerminalStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::RawMode => "raw mode",
            Self::AlternateScreen => "alternate screen",
            Self::Cursor => "cursor",
            Self::MouseCapture => "mouse capture",
            Self::FocusChange => "focus-change reporting",
            Self::BracketedPaste => "bracketed paste",
        };
        f.write_str(name)
    }
}

/// The point at which physical-terminal initialization failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStage {
    Size,
    RawMode,
    AlternateScreen,
    Cursor,
    MouseCapture,
    FocusChange,
    BracketedPaste,
}

impl From<TerminalStep> for EntryStage {
    fn from(step: TerminalStep) -> Self {
        match step {
            TerminalStep::RawMode => Self::RawMode,
            TerminalStep::AlternateScreen => Self::AlternateScreen,
            TerminalStep::Cursor => Self::Cursor,
            TerminalStep::MouseCapture => Self::MouseCapture,
            TerminalStep::FocusChange => Self::FocusChange,
            TerminalStep::BracketedPaste => Self::BracketedPaste,
        }
    }
}

impl fmt::Display for EntryStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Size => "terminal size",
            Self::RawMode => "raw mode",
            Self::AlternateScreen => "alternate screen",
            Self::Cursor => "cursor",
            Self::MouseCapture => "mouse capture",
            Self::FocusChange => "focus-change reporting",
            Self::BracketedPaste => "bracketed paste",
        };
        f.write_str(name)
    }
}

/// Errors from one entry responsibility, retaining the operation context.
#[derive(Debug)]
pub enum EntryDetail {
    SizeQuery(io::Error),
    UnusableSize { columns: u16, rows: u16 },
    Operation(io::Error),
}

impl fmt::Display for EntryDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeQuery(error) => write!(f, "size query failed: {error}"),
            Self::UnusableSize { columns, rows } => {
                write!(f, "reported unusable size {columns} columns x {rows} rows")
            }
            Self::Operation(error) => f.write_str(&error.to_string()),
        }
    }
}

impl std::error::Error for EntryDetail {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SizeQuery(error) | Self::Operation(error) => Some(error),
            Self::UnusableSize { .. } => None,
        }
    }
}

/// Initialization failure, including any best-effort cleanup failures.
#[derive(Debug)]
pub struct TerminalEntryError {
    stage: EntryStage,
    detail: EntryDetail,
    restoration: Option<TerminalRestoreError>,
}

impl TerminalEntryError {
    fn new(stage: EntryStage, detail: EntryDetail) -> Self {
        Self {
            stage,
            detail,
            restoration: None,
        }
    }

    fn with_restoration(mut self, restoration: Option<TerminalRestoreError>) -> Self {
        self.restoration = restoration;
        self
    }

    /// Returns the initialization stage that failed.
    #[must_use]
    pub fn stage(&self) -> EntryStage {
        self.stage
    }

    /// Returns the detailed initialization cause.
    #[must_use]
    pub fn detail(&self) -> &EntryDetail {
        &self.detail
    }

    /// Returns cleanup failures observed while unwinding partial entry.
    #[must_use]
    pub fn restoration(&self) -> Option<&TerminalRestoreError> {
        self.restoration.as_ref()
    }
}

impl fmt::Display for TerminalEntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "terminal initialization failed at {}: {}",
            self.stage, self.detail
        )?;
        if let Some(restoration) = &self.restoration {
            write!(f, "; cleanup also failed: {restoration}")?;
        }
        Ok(())
    }
}

impl std::error::Error for TerminalEntryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.detail.source()
    }
}

/// One cleanup operation that failed while later cleanup continued.
#[derive(Debug)]
pub struct RestoreFailure {
    step: TerminalStep,
    source: io::Error,
}

impl RestoreFailure {
    /// Returns the responsibility whose cleanup failed.
    #[must_use]
    pub fn step(&self) -> TerminalStep {
        self.step
    }

    /// Returns the underlying Crossterm/backend error.
    #[must_use]
    pub fn source(&self) -> &io::Error {
        &self.source
    }
}

/// All cleanup failures collected from one restore attempt.
#[derive(Debug)]
pub struct TerminalRestoreError {
    failures: Vec<RestoreFailure>,
}

impl TerminalRestoreError {
    /// Returns every failed cleanup operation in attempted order.
    #[must_use]
    pub fn failures(&self) -> &[RestoreFailure] {
        &self.failures
    }
}

impl fmt::Display for TerminalRestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} terminal cleanup operation(s) failed",
            self.failures.len()
        )?;
        for failure in &self.failures {
            write!(
                f,
                "; {step}: {source}",
                step = failure.step,
                source = failure.source
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for TerminalRestoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failures
            .first()
            .map(|failure| &failure.source as &(dyn std::error::Error + 'static))
    }
}

/// Result of running a host body after terminal initialization.
#[derive(Debug)]
pub enum TerminalRunError<E> {
    Initialization(TerminalEntryError),
    Body {
        error: E,
        restoration: Option<TerminalRestoreError>,
    },
    Restoration(TerminalRestoreError),
}

impl<E> TerminalRunError<E> {
    /// Returns cleanup failures attached to a body failure, if any.
    #[must_use]
    pub fn restoration(&self) -> Option<&TerminalRestoreError> {
        match self {
            Self::Initialization(error) => error.restoration(),
            Self::Body { restoration, .. } => restoration.as_ref(),
            Self::Restoration(error) => Some(error),
        }
    }
}

/// The narrow backend seam used by [`TerminalGuard`] and deterministic tests.
///
/// Implementations own the output authority used for all physical-terminal
/// commands. A backend must not be cloned into another restoration owner.
pub trait TerminalBackend {
    fn size(&mut self) -> io::Result<(u16, u16)>;
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    fn enable_focus_change(&mut self) -> io::Result<()>;
    fn enable_bracketed_paste(&mut self) -> io::Result<()>;
    fn disable_bracketed_paste(&mut self) -> io::Result<()>;
    fn disable_focus_change(&mut self) -> io::Result<()>;
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
}

/// Real Crossterm physical-terminal backend.
///
/// The same `Stdout` remains owned here for the later Ratatui compositor;
/// callers may borrow it through [`Self::writer_mut`] while this backend stays
/// inside its one [`TerminalGuard`].
pub struct CrosstermTerminal {
    output: io::Stdout,
}

impl CrosstermTerminal {
    #[must_use]
    pub fn new() -> Self {
        Self {
            output: io::stdout(),
        }
    }

    /// Borrows the output authority retained by this lifecycle backend.
    pub fn writer_mut(&mut self) -> &mut io::Stdout {
        &mut self.output
    }

    /// Enters a real Crossterm terminal session with one restoration owner.
    pub fn enter() -> Result<TerminalGuard<Self>, TerminalEntryError> {
        TerminalGuard::enter(Self::new())
    }
}

impl Default for CrosstermTerminal {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalBackend for CrosstermTerminal {
    fn size(&mut self) -> io::Result<(u16, u16)> {
        terminal::size()
    }

    fn enable_raw_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(&mut self.output, EnterAlternateScreen)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(&mut self.output, Hide)
    }

    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(&mut self.output, EnableMouseCapture)
    }

    fn enable_focus_change(&mut self) -> io::Result<()> {
        execute!(&mut self.output, EnableFocusChange)
    }

    fn enable_bracketed_paste(&mut self) -> io::Result<()> {
        execute!(&mut self.output, EnableBracketedPaste)
    }

    fn disable_bracketed_paste(&mut self) -> io::Result<()> {
        execute!(&mut self.output, DisableBracketedPaste)
    }

    fn disable_focus_change(&mut self) -> io::Result<()> {
        execute!(&mut self.output, DisableFocusChange)
    }

    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(&mut self.output, DisableMouseCapture)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(&mut self.output, Show)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(&mut self.output, LeaveAlternateScreen)
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()
    }
}

const RAW_MODE: u8 = 1 << 0;
const ALTERNATE_SCREEN: u8 = 1 << 1;
const CURSOR: u8 = 1 << 2;
const MOUSE_CAPTURE: u8 = 1 << 3;
const FOCUS_CHANGE: u8 = 1 << 4;
const BRACKETED_PASTE: u8 = 1 << 5;

#[derive(Clone, Copy, Debug, Default)]
struct Acquired(u8);

impl Acquired {
    fn add(&mut self, step: TerminalStep) {
        self.0 |= bit(step);
    }

    fn contains(self, step: TerminalStep) -> bool {
        self.0 & bit(step) != 0
    }
}

fn bit(step: TerminalStep) -> u8 {
    match step {
        TerminalStep::RawMode => RAW_MODE,
        TerminalStep::AlternateScreen => ALTERNATE_SCREEN,
        TerminalStep::Cursor => CURSOR,
        TerminalStep::MouseCapture => MOUSE_CAPTURE,
        TerminalStep::FocusChange => FOCUS_CHANGE,
        TerminalStep::BracketedPaste => BRACKETED_PASTE,
    }
}

/// Sole owner of physical-terminal restoration authority.
pub struct TerminalGuard<B: TerminalBackend> {
    backend: B,
    acquired: Acquired,
    restored: bool,
}

impl<B: TerminalBackend> TerminalGuard<B> {
    /// Validates physical size and enters all terminal responsibilities in a
    /// fixed order. Any partial entry is unwound before the error is returned.
    pub fn enter(backend: B) -> Result<Self, TerminalEntryError> {
        let mut guard = Self {
            backend,
            acquired: Acquired::default(),
            restored: false,
        };

        let (columns, rows) = match guard.backend.size() {
            Ok(size) => size,
            Err(error) => {
                return Err(guard.entry_failure(EntryStage::Size, EntryDetail::SizeQuery(error)));
            }
        };
        if columns == 0 || rows == 0 {
            return Err(guard.entry_failure(
                EntryStage::Size,
                EntryDetail::UnusableSize { columns, rows },
            ));
        }

        guard.acquired.add(TerminalStep::RawMode);
        if let Err(error) = guard.backend.enable_raw_mode() {
            return Err(guard.entry_failure(EntryStage::RawMode, EntryDetail::Operation(error)));
        }

        guard.acquired.add(TerminalStep::AlternateScreen);
        if let Err(error) = guard.backend.enter_alternate_screen() {
            return Err(
                guard.entry_failure(EntryStage::AlternateScreen, EntryDetail::Operation(error))
            );
        }

        guard.acquired.add(TerminalStep::Cursor);
        if let Err(error) = guard.backend.hide_cursor() {
            return Err(guard.entry_failure(EntryStage::Cursor, EntryDetail::Operation(error)));
        }

        guard.acquired.add(TerminalStep::MouseCapture);
        if let Err(error) = guard.backend.enable_mouse_capture() {
            return Err(
                guard.entry_failure(EntryStage::MouseCapture, EntryDetail::Operation(error))
            );
        }

        guard.acquired.add(TerminalStep::FocusChange);
        if let Err(error) = guard.backend.enable_focus_change() {
            return Err(guard.entry_failure(EntryStage::FocusChange, EntryDetail::Operation(error)));
        }

        guard.acquired.add(TerminalStep::BracketedPaste);
        if let Err(error) = guard.backend.enable_bracketed_paste() {
            return Err(
                guard.entry_failure(EntryStage::BracketedPaste, EntryDetail::Operation(error))
            );
        }

        Ok(guard)
    }

    /// Runs one host body with explicit restore on every returned outcome.
    /// Panic/unwind remains covered by this guard's `Drop` implementation.
    pub fn run_with<T, E, F>(backend: B, body: F) -> Result<T, TerminalRunError<E>>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
    {
        let mut guard = Self::enter(backend).map_err(TerminalRunError::Initialization)?;
        let body_result = body(&mut guard);
        let restoration = guard.restore().err();
        match (body_result, restoration) {
            (Ok(value), None) => Ok(value),
            (Ok(_), Some(error)) => Err(TerminalRunError::Restoration(error)),
            (Err(error), restoration) => Err(TerminalRunError::Body { error, restoration }),
        }
    }

    /// Borrows the backend while retaining this guard as the sole owner.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutably borrows the backend for the later compositor/output writer.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Explicitly restores every responsibility acquired by this guard.
    ///
    /// The guard is marked restored before attempting operations, so a
    /// partial failure is still idempotent and `Drop` cannot repeat cleanup.
    pub fn restore(&mut self) -> Result<(), TerminalRestoreError> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        let mut failures = Vec::new();
        self.restore_step(TerminalStep::BracketedPaste, &mut failures, |backend| {
            backend.disable_bracketed_paste()
        });
        self.restore_step(TerminalStep::FocusChange, &mut failures, |backend| {
            backend.disable_focus_change()
        });
        self.restore_step(TerminalStep::MouseCapture, &mut failures, |backend| {
            backend.disable_mouse_capture()
        });
        self.restore_step(TerminalStep::Cursor, &mut failures, |backend| {
            backend.show_cursor()
        });
        self.restore_step(TerminalStep::AlternateScreen, &mut failures, |backend| {
            backend.leave_alternate_screen()
        });
        self.restore_step(TerminalStep::RawMode, &mut failures, |backend| {
            backend.disable_raw_mode()
        });
        if failures.is_empty() {
            Ok(())
        } else {
            Err(TerminalRestoreError { failures })
        }
    }

    /// Returns whether explicit or drop cleanup has already been attempted.
    #[must_use]
    pub fn is_restored(&self) -> bool {
        self.restored
    }

    fn restore_step<F>(&mut self, step: TerminalStep, failures: &mut Vec<RestoreFailure>, action: F)
    where
        F: FnOnce(&mut B) -> io::Result<()>,
    {
        if !self.acquired.contains(step) {
            return;
        }
        if let Err(source) = action(&mut self.backend) {
            failures.push(RestoreFailure { step, source });
        }
    }

    fn entry_failure(self, stage: EntryStage, detail: EntryDetail) -> TerminalEntryError {
        let mut guard = self;
        let restoration = guard.restore().err();
        TerminalEntryError::new(stage, detail).with_restoration(restoration)
    }
}

impl TerminalGuard<CrosstermTerminal> {
    /// Borrows the retained physical output for Ratatui/Crossterm.
    pub fn writer_mut(&mut self) -> &mut io::Stdout {
        self.backend.writer_mut()
    }
}

impl<B: TerminalBackend> Drop for TerminalGuard<B> {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CrosstermTerminal, EntryStage, TerminalBackend, TerminalGuard, TerminalRunError,
        TerminalStep,
    };
    use std::cell::RefCell;
    use std::io;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Call {
        Size,
        EnableRawMode,
        EnterAlternateScreen,
        HideCursor,
        EnableMouseCapture,
        EnableFocusChange,
        EnableBracketedPaste,
        DisableBracketedPaste,
        DisableFocusChange,
        DisableMouseCapture,
        ShowCursor,
        LeaveAlternateScreen,
        DisableRawMode,
    }

    struct FakeState {
        calls: Vec<Call>,
        size: io::Result<(u16, u16)>,
        fail_entry: Option<TerminalStep>,
        fail_restore: Option<TerminalStep>,
    }

    impl Default for FakeState {
        fn default() -> Self {
            Self {
                calls: Vec::new(),
                size: Ok((120, 40)),
                fail_entry: None,
                fail_restore: None,
            }
        }
    }

    struct FakeBackend {
        state: Rc<RefCell<FakeState>>,
    }

    impl FakeBackend {
        fn usable(log: Rc<RefCell<FakeState>>) -> Self {
            Self { state: log }
        }

        fn call(&mut self, call: Call, step: TerminalStep) -> io::Result<()> {
            let mut state = self.state.borrow_mut();
            state.calls.push(call);
            let restore = matches!(
                call,
                Call::DisableBracketedPaste
                    | Call::DisableFocusChange
                    | Call::DisableMouseCapture
                    | Call::ShowCursor
                    | Call::LeaveAlternateScreen
                    | Call::DisableRawMode
            );
            let fail = if restore {
                state.fail_restore == Some(step)
            } else {
                state.fail_entry == Some(step)
            };
            if fail {
                Err(io::Error::other(format!("injected {step:?} failure")))
            } else {
                Ok(())
            }
        }
    }

    impl TerminalBackend for FakeBackend {
        fn size(&mut self) -> io::Result<(u16, u16)> {
            let mut state = self.state.borrow_mut();
            state.calls.push(Call::Size);
            state
                .size
                .as_ref()
                .map(|size| *size)
                .map_err(|error| io::Error::new(error.kind(), error.to_string()))
        }

        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.call(Call::EnableRawMode, TerminalStep::RawMode)
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.call(Call::EnterAlternateScreen, TerminalStep::AlternateScreen)
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            self.call(Call::HideCursor, TerminalStep::Cursor)
        }

        fn enable_mouse_capture(&mut self) -> io::Result<()> {
            self.call(Call::EnableMouseCapture, TerminalStep::MouseCapture)
        }

        fn enable_focus_change(&mut self) -> io::Result<()> {
            self.call(Call::EnableFocusChange, TerminalStep::FocusChange)
        }

        fn enable_bracketed_paste(&mut self) -> io::Result<()> {
            self.call(Call::EnableBracketedPaste, TerminalStep::BracketedPaste)
        }

        fn disable_bracketed_paste(&mut self) -> io::Result<()> {
            self.call(Call::DisableBracketedPaste, TerminalStep::BracketedPaste)
        }

        fn disable_focus_change(&mut self) -> io::Result<()> {
            self.call(Call::DisableFocusChange, TerminalStep::FocusChange)
        }

        fn disable_mouse_capture(&mut self) -> io::Result<()> {
            self.call(Call::DisableMouseCapture, TerminalStep::MouseCapture)
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.call(Call::ShowCursor, TerminalStep::Cursor)
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.call(Call::LeaveAlternateScreen, TerminalStep::AlternateScreen)
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.call(Call::DisableRawMode, TerminalStep::RawMode)
        }
    }

    fn calls(state: &Rc<RefCell<FakeState>>) -> Vec<Call> {
        state.borrow().calls.clone()
    }

    #[test]
    fn terminal_lifecycle_successful_enter_restores_in_reverse_order_and_drop_is_a_noop() {
        let log = Rc::new(RefCell::new(FakeState::default()));
        let backend = FakeBackend::usable(Rc::clone(&log));
        let mut guard = TerminalGuard::enter(backend).expect("entry should succeed");

        guard.restore().expect("restore should succeed");
        assert_eq!(
            calls(&log),
            vec![
                Call::Size,
                Call::EnableRawMode,
                Call::EnterAlternateScreen,
                Call::HideCursor,
                Call::EnableMouseCapture,
                Call::EnableFocusChange,
                Call::EnableBracketedPaste,
                Call::DisableBracketedPaste,
                Call::DisableFocusChange,
                Call::DisableMouseCapture,
                Call::ShowCursor,
                Call::LeaveAlternateScreen,
                Call::DisableRawMode,
            ]
        );

        drop(guard);
        assert_eq!(calls(&log).len(), 13, "Drop must not restore twice");
    }

    #[test]
    fn terminal_lifecycle_every_body_outcome_restores_the_physical_terminal() {
        for (name, outcome) in [
            ("normal child exit", Ok(())),
            ("spawn error", Err("spawn")),
            ("render error", Err("render")),
            ("input-loop error", Err("input")),
            ("termination signal", Err("signal")),
        ] {
            let log = Rc::new(RefCell::new(FakeState::default()));
            let backend = FakeBackend::usable(Rc::clone(&log));
            let result = TerminalGuard::run_with(backend, |_guard| outcome);

            match outcome {
                Ok(()) => assert!(result.is_ok(), "{name} should succeed"),
                Err(expected) => match result {
                    Err(TerminalRunError::Body { error, .. }) => assert_eq!(error, expected),
                    other => panic!("{name} returned unexpected result: {other:?}"),
                },
            }
            assert_eq!(calls(&log).len(), 13, "{name} must restore once");
            assert_eq!(
                calls(&log)[7..],
                [
                    Call::DisableBracketedPaste,
                    Call::DisableFocusChange,
                    Call::DisableMouseCapture,
                    Call::ShowCursor,
                    Call::LeaveAlternateScreen,
                    Call::DisableRawMode,
                ],
                "{name} cleanup order"
            );
        }
    }

    #[test]
    fn terminal_lifecycle_panic_unwind_uses_best_effort_drop_restoration() {
        let log = Rc::new(RefCell::new(FakeState::default()));
        let backend = FakeBackend::usable(Rc::clone(&log));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = TerminalGuard::enter(backend).expect("entry should succeed");
            panic!("body panic");
        }));
        assert!(result.is_err());
        assert_eq!(calls(&log).len(), 13);
        assert_eq!(calls(&log)[7], Call::DisableBracketedPaste);
    }

    #[test]
    fn terminal_lifecycle_each_partial_entry_failure_restores_every_acquired_responsibility() {
        let cases = [
            (
                TerminalStep::RawMode,
                vec![Call::Size, Call::EnableRawMode, Call::DisableRawMode],
            ),
            (
                TerminalStep::AlternateScreen,
                vec![
                    Call::Size,
                    Call::EnableRawMode,
                    Call::EnterAlternateScreen,
                    Call::LeaveAlternateScreen,
                    Call::DisableRawMode,
                ],
            ),
            (
                TerminalStep::Cursor,
                vec![
                    Call::Size,
                    Call::EnableRawMode,
                    Call::EnterAlternateScreen,
                    Call::HideCursor,
                    Call::ShowCursor,
                    Call::LeaveAlternateScreen,
                    Call::DisableRawMode,
                ],
            ),
            (
                TerminalStep::MouseCapture,
                vec![
                    Call::Size,
                    Call::EnableRawMode,
                    Call::EnterAlternateScreen,
                    Call::HideCursor,
                    Call::EnableMouseCapture,
                    Call::DisableMouseCapture,
                    Call::ShowCursor,
                    Call::LeaveAlternateScreen,
                    Call::DisableRawMode,
                ],
            ),
            (
                TerminalStep::FocusChange,
                vec![
                    Call::Size,
                    Call::EnableRawMode,
                    Call::EnterAlternateScreen,
                    Call::HideCursor,
                    Call::EnableMouseCapture,
                    Call::EnableFocusChange,
                    Call::DisableFocusChange,
                    Call::DisableMouseCapture,
                    Call::ShowCursor,
                    Call::LeaveAlternateScreen,
                    Call::DisableRawMode,
                ],
            ),
            (
                TerminalStep::BracketedPaste,
                vec![
                    Call::Size,
                    Call::EnableRawMode,
                    Call::EnterAlternateScreen,
                    Call::HideCursor,
                    Call::EnableMouseCapture,
                    Call::EnableFocusChange,
                    Call::EnableBracketedPaste,
                    Call::DisableBracketedPaste,
                    Call::DisableFocusChange,
                    Call::DisableMouseCapture,
                    Call::ShowCursor,
                    Call::LeaveAlternateScreen,
                    Call::DisableRawMode,
                ],
            ),
        ];

        for (step, expected) in cases {
            let log = Rc::new(RefCell::new(FakeState {
                fail_entry: Some(step),
                ..FakeState::default()
            }));
            let backend = FakeBackend::usable(Rc::clone(&log));
            let error = match TerminalGuard::enter(backend) {
                Ok(_) => panic!("entry should fail"),
                Err(error) => error,
            };
            assert_eq!(error.stage(), EntryStage::from(step));
            assert_eq!(calls(&log), expected, "partial failure at {step:?}");
        }
    }

    #[test]
    fn terminal_lifecycle_restoration_failure_attempts_later_steps_and_is_not_retried_by_drop() {
        let log = Rc::new(RefCell::new(FakeState::default()));
        let backend = FakeBackend::usable(Rc::clone(&log));
        let mut guard = TerminalGuard::enter(backend).expect("entry should succeed");
        log.borrow_mut().fail_restore = Some(TerminalStep::FocusChange);

        let error = guard.restore().expect_err("injected cleanup failure");
        assert_eq!(error.failures().len(), 1);
        assert_eq!(error.failures()[0].step(), TerminalStep::FocusChange);
        assert_eq!(
            calls(&log)[7..],
            [
                Call::DisableBracketedPaste,
                Call::DisableFocusChange,
                Call::DisableMouseCapture,
                Call::ShowCursor,
                Call::LeaveAlternateScreen,
                Call::DisableRawMode,
            ]
        );

        let call_count = calls(&log).len();
        drop(guard);
        assert_eq!(
            calls(&log).len(),
            call_count,
            "failed restore must not repeat on Drop"
        );
    }

    #[test]
    fn terminal_lifecycle_partial_entry_cleanup_failure_is_attached_and_later_steps_continue() {
        let log = Rc::new(RefCell::new(FakeState {
            fail_entry: Some(TerminalStep::FocusChange),
            fail_restore: Some(TerminalStep::FocusChange),
            ..FakeState::default()
        }));
        let backend = FakeBackend::usable(Rc::clone(&log));
        let error = match TerminalGuard::enter(backend) {
            Ok(_) => panic!("entry should fail"),
            Err(error) => error,
        };

        let restoration = error
            .restoration()
            .expect("entry error should retain inverse cleanup failure");
        assert_eq!(restoration.failures().len(), 1);
        assert_eq!(restoration.failures()[0].step(), TerminalStep::FocusChange);
        assert_eq!(
            calls(&log),
            [
                Call::Size,
                Call::EnableRawMode,
                Call::EnterAlternateScreen,
                Call::HideCursor,
                Call::EnableMouseCapture,
                Call::EnableFocusChange,
                Call::DisableFocusChange,
                Call::DisableMouseCapture,
                Call::ShowCursor,
                Call::LeaveAlternateScreen,
                Call::DisableRawMode,
            ]
        );
    }

    #[test]
    fn terminal_lifecycle_unusable_or_failed_size_is_initialization_failure_before_body() {
        for size in [Ok((0, 40)), Ok((120, 0)), Err(io::Error::other("size"))] {
            let log = Rc::new(RefCell::new(FakeState {
                size: match size {
                    Ok(size) => Ok(size),
                    Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
                },
                ..FakeState::default()
            }));
            let backend = FakeBackend::usable(Rc::clone(&log));
            let body_called = Rc::new(RefCell::new(false));
            let body_called_for_test = Rc::clone(&body_called);
            let result = TerminalGuard::run_with(backend, move |_guard| {
                *body_called_for_test.borrow_mut() = true;
                Ok::<(), &'static str>(())
            });
            assert!(matches!(result, Err(TerminalRunError::Initialization(_))));
            assert!(!*body_called.borrow());
            assert_eq!(calls(&log), [Call::Size]);
        }
    }

    #[test]
    fn terminal_lifecycle_restoration_authority_is_owned_by_one_non_clonable_guard() {
        fn assert_send<T: Send>() {}
        assert_send::<TerminalGuard<CrosstermTerminal>>();

        let log = Rc::new(RefCell::new(FakeState::default()));
        let backend = FakeBackend::usable(Rc::clone(&log));
        let guard = TerminalGuard::enter(backend).expect("entry should succeed");
        let _moved = guard;
        assert_eq!(calls(&log).len(), 7);
    }
}

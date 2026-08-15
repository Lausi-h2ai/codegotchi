use std::collections::VecDeque;

use vt100::{Callbacks, MouseProtocolEncoding, MouseProtocolMode};

/// The xterm mouse tracking level negotiated by Codex.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseTrackingMode {
    /// Mouse reporting is disabled.
    #[default]
    Disabled,
    /// Report button presses and wheel events.
    Press,
    /// Report button presses, releases, and wheel events.
    PressRelease,
    /// Also report movement while a button is held.
    ButtonMotion,
    /// Also report unbuttoned movement.
    AnyMotion,
}

/// The xterm mouse coordinate encoding negotiated by Codex.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseEncoding {
    /// The original three-byte X10-compatible encoding.
    #[default]
    Default,
    /// UTF-8 extended coordinates using the X10 framing.
    Utf8,
    /// SGR decimal coordinates.
    Sgr,
}

/// The input modes currently requested by the Codex terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodexInputModes {
    pub application_cursor_keys: bool,
    pub bracketed_paste: bool,
    pub focus_reporting: bool,
    pub mouse_tracking: MouseTrackingMode,
    pub mouse_encoding: MouseEncoding,
}

const DEFAULT_SCROLLBACK: usize = 1_000;
const MAX_SCROLLBACK: usize = 10_000;
const MAX_TRACKED_SEQUENCE: usize = 128;
const MAX_PENDING_QUERY_RESPONSES: usize = 32;

#[derive(Clone, Copy, Debug)]
enum TrackerState {
    Ground,
    Escape,
    Csi,
    Discard,
}

/// Incrementally recognizes the one DEC private mode that vt100 0.16.2 does
/// not expose. It deliberately only accepts an actual ESC `[` CSI prefix;
/// visible text containing the same digits is never interpreted as a mode.
#[derive(Debug)]
struct FocusTracker {
    state: TrackerState,
    sequence: [u8; MAX_TRACKED_SEQUENCE],
    length: usize,
    focus_reporting: bool,
}

impl Default for FocusTracker {
    fn default() -> Self {
        Self {
            state: TrackerState::Ground,
            sequence: [0; MAX_TRACKED_SEQUENCE],
            length: 0,
            focus_reporting: false,
        }
    }
}

impl FocusTracker {
    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.feed_byte(byte);
        }
    }

    fn feed_byte(&mut self, byte: u8) {
        match self.state {
            TrackerState::Ground => {
                if byte == 0x1b {
                    self.state = TrackerState::Escape;
                }
            }
            TrackerState::Escape => match byte {
                b'[' => {
                    self.sequence[0] = b'[';
                    self.length = 1;
                    self.state = TrackerState::Csi;
                }
                b'c' => {
                    self.focus_reporting = false;
                    self.length = 0;
                    self.state = TrackerState::Ground;
                }
                0x1b => {}
                _ => {
                    self.length = 0;
                    self.state = TrackerState::Ground;
                }
            },
            TrackerState::Csi => {
                if byte == 0x1b {
                    self.state = TrackerState::Escape;
                    self.length = 0;
                } else if self.length >= MAX_TRACKED_SEQUENCE {
                    self.state = TrackerState::Discard;
                } else {
                    self.sequence[self.length] = byte;
                    self.length += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        self.finish_sequence();
                    }
                }
            }
            TrackerState::Discard => {
                if byte == 0x1b {
                    self.state = TrackerState::Escape;
                    self.length = 0;
                } else if (0x40..=0x7e).contains(&byte) {
                    self.state = TrackerState::Ground;
                    self.length = 0;
                }
            }
        }
    }

    fn finish_sequence(&mut self) {
        if self.length == 7
            && self.sequence[..6] == *b"[?1004"
            && matches!(self.sequence[6], b'h' | b'l')
        {
            self.focus_reporting = self.sequence[6] == b'h';
        }
        self.state = TrackerState::Ground;
        self.length = 0;
    }
}

/// Collects the small set of terminal queries that the hosted PTY can answer
/// truthfully from its virtual screen. Responses are drained after each
/// bounded PTY output chunk, and the queue cap prevents a malicious child from
/// retaining an unbounded amount of query traffic.
#[derive(Debug, Default)]
struct TerminalQueryCallbacks {
    responses: VecDeque<Vec<u8>>,
}

impl TerminalQueryCallbacks {
    fn push(&mut self, response: Vec<u8>) {
        if self.responses.len() < MAX_PENDING_QUERY_RESPONSES {
            self.responses.push_back(response);
        }
    }

    fn drain(&mut self) -> Vec<u8> {
        let response_bytes = self.responses.iter().map(Vec::len).sum();
        let mut responses = Vec::with_capacity(response_bytes);
        while let Some(response) = self.responses.pop_front() {
            responses.extend(response);
        }
        responses
    }
}

impl Callbacks for TerminalQueryCallbacks {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if i1.is_some() || i2.is_some() {
            return;
        }

        if c == 'n' && params.len() == 1 && params[0] == [6] {
            let (rows, columns) = screen.size();
            let (row, column) = screen.cursor_position();
            let row = row.min(rows.saturating_sub(1));
            let column = column.min(columns.saturating_sub(1));
            self.push(format!("\x1b[{};{}R", row + 1, column + 1).into_bytes());
        } else if c == 'c' && (params.is_empty() || (params.len() == 1 && params[0] == [0])) {
            self.push(b"\x1b[?1;2c".to_vec());
        }
    }
}

/// Incremental, non-interactive representation of a Codex PTY terminal.
pub struct CodexScreen {
    parser: vt100::Parser<TerminalQueryCallbacks>,
    focus_tracker: FocusTracker,
}

impl CodexScreen {
    /// Creates a screen with the requested rows and columns and bounded
    /// scrollback suitable for an interactive Codex pane.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self::with_scrollback(rows, cols, DEFAULT_SCROLLBACK)
    }

    /// Creates a screen with an explicitly bounded scrollback length.
    #[must_use]
    pub fn with_scrollback(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            parser: vt100::Parser::new_with_callbacks(
                rows.max(1),
                cols.max(1),
                scrollback.min(MAX_SCROLLBACK),
                TerminalQueryCallbacks::default(),
            ),
            focus_tracker: FocusTracker::default(),
        }
    }

    /// Feeds an arbitrary PTY output chunk to both the VT parser and the
    /// protocol-side focus-mode tracker.
    pub fn process(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.process(bytes);
        self.focus_tracker.feed(bytes);
        self.parser.callbacks_mut().drain()
    }

    /// Resizes the virtual terminal while retaining its parser state.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows.max(1), cols.max(1));
    }

    /// Returns a read-only view of the underlying VT screen for rendering.
    #[must_use]
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Returns the current terminal size as `(rows, columns)`.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        self.screen().size()
    }

    /// Returns the current cursor position as `(row, column)`, both zero-based.
    #[must_use]
    pub fn cursor_position(&self) -> (u16, u16) {
        self.screen().cursor_position()
    }

    /// Returns a read-only cell at a zero-based location.
    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&vt100::Cell> {
        self.screen().cell(row, col)
    }

    /// Returns text from one row, restricted to a zero-based column and width.
    #[must_use]
    pub fn text_at(&self, row: u16, col: u16, width: u16) -> String {
        self.screen()
            .rows(col, width)
            .nth(usize::from(row))
            .unwrap_or_default()
    }

    /// Returns the current visible text without granting mutation access.
    #[must_use]
    pub fn contents(&self) -> String {
        self.screen().contents()
    }

    /// Returns whether the alternate screen buffer is active.
    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.screen().alternate_screen()
    }

    /// Returns the current scrollback offset.
    #[must_use]
    pub fn scrollback(&self) -> usize {
        self.screen().scrollback()
    }

    /// Returns the negotiated mode read model used by all Codex input
    /// encoders. Mode state comes only from terminal control protocols.
    #[must_use]
    pub fn input_modes(&self) -> CodexInputModes {
        let screen = self.screen();
        CodexInputModes {
            application_cursor_keys: screen.application_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            focus_reporting: self.focus_tracker.focus_reporting,
            mouse_tracking: screen.mouse_protocol_mode().into(),
            mouse_encoding: screen.mouse_protocol_encoding().into(),
        }
    }

    /// Returns whether DEC focus reporting is active.
    #[must_use]
    pub fn focus_reporting(&self) -> bool {
        self.focus_tracker.focus_reporting
    }
}

impl Default for CodexScreen {
    fn default() -> Self {
        Self::new(24, 80)
    }
}

impl From<MouseProtocolMode> for MouseTrackingMode {
    fn from(mode: MouseProtocolMode) -> Self {
        match mode {
            MouseProtocolMode::None => Self::Disabled,
            MouseProtocolMode::Press => Self::Press,
            MouseProtocolMode::PressRelease => Self::PressRelease,
            MouseProtocolMode::ButtonMotion => Self::ButtonMotion,
            MouseProtocolMode::AnyMotion => Self::AnyMotion,
        }
    }
}

impl From<MouseProtocolEncoding> for MouseEncoding {
    fn from(encoding: MouseProtocolEncoding) -> Self {
        match encoding {
            MouseProtocolEncoding::Default => Self::Default,
            MouseProtocolEncoding::Utf8 => Self::Utf8,
            MouseProtocolEncoding::Sgr => Self::Sgr,
        }
    }
}

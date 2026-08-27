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
const MAX_ALTERNATE_SCREEN_HISTORY: usize = 128;
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct AlternateScreenFrame {
    rows: u16,
    columns: u16,
    cells: Vec<vt100::Cell>,
    row_text: Vec<String>,
    contents: String,
    cursor_position: (u16, u16),
    hide_cursor: bool,
}

impl AlternateScreenFrame {
    fn capture(screen: &vt100::Screen) -> Option<Self> {
        let (rows, columns) = screen.size();
        let mut cells = Vec::with_capacity(usize::from(rows) * usize::from(columns));
        for row in 0..rows {
            for column in 0..columns {
                cells.push(screen.cell(row, column)?.clone());
            }
        }

        Some(Self {
            rows,
            columns,
            cells,
            row_text: screen.rows(0, columns).collect(),
            contents: screen.contents(),
            cursor_position: screen.cursor_position(),
            hide_cursor: screen.hide_cursor(),
        })
    }

    fn cell(&self, row: u16, column: u16) -> Option<&vt100::Cell> {
        if row >= self.rows || column >= self.columns {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.columns) + usize::from(column))
    }

    fn text_at(&self, row: u16, column: u16, width: u16) -> String {
        self.row_text
            .get(usize::from(row))
            .map(|text| {
                text.chars()
                    .skip(usize::from(column))
                    .take(usize::from(width))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Incremental, non-interactive representation of a Codex PTY terminal.
pub struct CodexScreen {
    parser: vt100::Parser<TerminalQueryCallbacks>,
    focus_tracker: FocusTracker,
    alternate_history: VecDeque<AlternateScreenFrame>,
    alternate_history_offset: usize,
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
            alternate_history: VecDeque::new(),
            alternate_history_offset: 0,
        }
    }

    /// Feeds an arbitrary PTY output chunk to both the VT parser and the
    /// protocol-side focus-mode tracker.
    pub fn process(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut was_alternate = self.alternate_screen();
        let mut offset_before_output = self.alternate_history_offset;
        let mut remaining = bytes;
        while let Some(clear_offset) = full_screen_clear_offset(remaining) {
            let (before_clear, after_prefix) = remaining.split_at(clear_offset);
            self.process_bytes(before_clear);
            self.record_alternate_history_frame(was_alternate, offset_before_output);

            let (clear, after_clear) = after_prefix.split_at(4);
            self.process_bytes(clear);
            remaining = after_clear;
            was_alternate = self.alternate_screen();
            offset_before_output = self.alternate_history_offset;
        }
        self.process_bytes(remaining);
        self.record_alternate_history_frame(was_alternate, offset_before_output);
        self.parser.callbacks_mut().drain()
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.focus_tracker.feed(bytes);
    }

    fn record_alternate_history_frame(&mut self, was_alternate: bool, offset_before_output: usize) {
        if !self.alternate_screen() {
            self.alternate_history.clear();
            self.alternate_history_offset = 0;
            return;
        }
        if !was_alternate {
            self.alternate_history.clear();
            self.alternate_history_offset = 0;
        }
        self.record_alternate_frame(was_alternate, offset_before_output);
    }

    fn record_alternate_frame(&mut self, was_alternate: bool, offset_before_output: usize) {
        let Some(frame) = AlternateScreenFrame::capture(self.parser.screen()) else {
            return;
        };
        if self.alternate_history.back() == Some(&frame) {
            return;
        }

        self.alternate_history.push_back(frame);
        if self.alternate_history.len() > MAX_ALTERNATE_SCREEN_HISTORY {
            self.alternate_history.pop_front();
        }
        self.alternate_history_offset = if was_alternate && offset_before_output > 0 {
            offset_before_output.saturating_add(1)
        } else {
            0
        }
        .min(self.alternate_history.len().saturating_sub(1));
    }

    /// Resizes the virtual terminal while retaining its parser state.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows.max(1), cols.max(1));
        self.alternate_history.clear();
        self.alternate_history_offset = 0;
    }

    /// Returns a read-only view of the underlying VT screen for rendering.
    #[must_use]
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Returns the current terminal size as `(rows, columns)`.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        self.visible_size()
    }

    /// Returns the current cursor position as `(row, column)`, both zero-based.
    #[must_use]
    pub fn cursor_position(&self) -> (u16, u16) {
        self.visible_cursor_position()
    }

    /// Returns a read-only cell at a zero-based location.
    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&vt100::Cell> {
        self.visible_cell(row, col)
    }

    /// Returns text from one row, restricted to a zero-based column and width.
    #[must_use]
    pub fn text_at(&self, row: u16, col: u16, width: u16) -> String {
        if let Some(frame) = self.alternate_history_frame() {
            frame.text_at(row, col, width)
        } else {
            self.screen()
                .rows(col, width)
                .nth(usize::from(row))
                .unwrap_or_default()
        }
    }

    /// Returns the current visible text without granting mutation access.
    #[must_use]
    pub fn contents(&self) -> String {
        self.alternate_history_frame()
            .map_or_else(|| self.screen().contents(), |frame| frame.contents.clone())
    }

    /// Returns whether the alternate screen buffer is active.
    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.screen().alternate_screen()
    }

    /// Returns the current scrollback offset.
    #[must_use]
    pub fn scrollback(&self) -> usize {
        if self.alternate_screen() {
            self.alternate_history_offset
        } else {
            self.screen().scrollback()
        }
    }

    /// Returns whether the visible viewport is showing historical output
    /// instead of the live terminal screen.
    #[must_use]
    pub fn is_scrolled_back(&self) -> bool {
        self.scrollback() > 0
    }

    /// Returns the visible viewport to the live terminal screen.
    pub fn scroll_to_live(&mut self) {
        if self.alternate_screen() {
            self.alternate_history_offset = 0;
        } else {
            self.parser.screen_mut().set_scrollback(0);
        }
    }

    /// Moves the virtual terminal viewport by a signed number of rows.
    ///
    /// Positive values reveal older output; negative values return toward the
    /// live bottom of the Codex screen. Returns whether the viewport moved.
    pub fn scrollback_by(&mut self, lines: i16) -> bool {
        if self.alternate_screen() {
            let before = self.alternate_history_offset;
            let maximum = self.alternate_history.len().saturating_sub(1);
            self.alternate_history_offset = if lines.is_negative() {
                before.saturating_sub(usize::from(lines.unsigned_abs()))
            } else {
                before
                    .saturating_add(usize::from(lines.unsigned_abs()))
                    .min(maximum)
            };
            return self.alternate_history_offset != before;
        }

        let before = self.scrollback();
        let target = if lines.is_negative() {
            before.saturating_sub(usize::from(lines.unsigned_abs()))
        } else {
            before.saturating_add(usize::from(lines.unsigned_abs()))
        };
        self.parser.screen_mut().set_scrollback(target);
        self.scrollback() != before
    }

    pub(crate) fn visible_size(&self) -> (u16, u16) {
        self.alternate_history_frame()
            .map_or_else(|| self.screen().size(), |frame| (frame.rows, frame.columns))
    }

    pub(crate) fn visible_cell(&self, row: u16, column: u16) -> Option<&vt100::Cell> {
        self.alternate_history_frame().map_or_else(
            || self.screen().cell(row, column),
            |frame| frame.cell(row, column),
        )
    }

    pub(crate) fn visible_cursor_position(&self) -> (u16, u16) {
        self.alternate_history_frame().map_or_else(
            || self.screen().cursor_position(),
            |frame| frame.cursor_position,
        )
    }

    pub(crate) fn visible_hide_cursor(&self) -> bool {
        self.alternate_history_frame()
            .map_or_else(|| self.screen().hide_cursor(), |frame| frame.hide_cursor)
    }

    fn alternate_history_frame(&self) -> Option<&AlternateScreenFrame> {
        if !self.alternate_screen() || self.alternate_history_offset == 0 {
            return None;
        }
        self.alternate_history.get(
            self.alternate_history
                .len()
                .saturating_sub(1 + self.alternate_history_offset),
        )
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

fn full_screen_clear_offset(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|sequence| sequence == b"\x1b[2J")
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

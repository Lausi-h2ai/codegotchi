mod host;
mod input;
mod layout;
mod pty;
mod render;
mod screen;
mod session;

pub use host::{
    CrosstermTerminal, EntryDetail, EntryStage, RestoreFailure, TerminalBackend,
    TerminalEntryError, TerminalGuard, TerminalRestoreError, TerminalRunError, TerminalStep,
};
pub use input::{
    encode_focus, encode_focus_event, encode_key, encode_key_event, encode_mouse,
    encode_mouse_event, encode_paste,
};
pub use layout::{RoomLayoutMode, TerminalLayout, choose_layout};
pub use pty::{PtyCodexChild, PtyCodexError};
pub use render::render_codex;
pub use screen::{CodexInputModes, CodexScreen, MouseEncoding, MouseTrackingMode};
pub use session::{
    TerminalSessionCore, TerminalSessionError, TerminalSessionEventFuture,
    TerminalSessionEventSource, TerminalSessionSignal, TerminalSessionSignalReceiver,
    TerminalSessionSignalSender, initialize_terminal_and_spawn, run_terminal_session,
    run_terminal_session_with_events, terminal_session_signal_channel,
};

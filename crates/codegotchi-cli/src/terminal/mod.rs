mod behavior;
mod host;
mod input;
mod layout;
mod pty;
mod render;
mod room;
mod screen;
mod session;
mod sprites;
mod theme;

pub use behavior::{
    IdleIntent, PetPose, PresentationActivity, PresentationFrame, PresentationState, RoomObject,
    has_authoritative_nap, presentation_activity,
};
pub use host::{
    CrosstermTerminal, EntryDetail, EntryStage, RestoreFailure, TerminalBackend,
    TerminalEntryError, TerminalGuard, TerminalRestoreError, TerminalRunError, TerminalStep,
};
pub use input::{
    CareGateway, POINTER_DISTANCE_PER_CELL, PetGesture, RoomCareRequest, RoomInputSession,
    encode_focus, encode_focus_event, encode_key, encode_key_event, encode_mouse,
    encode_mouse_event, encode_paste, pointer_distance,
};
pub use layout::{RoomLayoutMode, TerminalLayout, choose_layout};
pub use pty::{PtyCodexChild, PtyCodexError};
pub use render::render_codex;
pub use room::{
    FoodSource, RoomAmbience, RoomGeometry, RoomRenderOptions, render_room,
    render_room_with_options, render_room_with_palette, room_geometry, room_geometry_with_frame,
};
pub use screen::{CodexInputModes, CodexScreen, MouseEncoding, MouseTrackingMode};
pub use session::{
    TerminalSessionCore, TerminalSessionError, TerminalSessionEventFuture,
    TerminalSessionEventSource, TerminalSessionSignal, TerminalSessionSignalReceiver,
    TerminalSessionSignalSender, TerminalSessionStartError, initialize_terminal_and_spawn,
    run_terminal_session, run_terminal_session_with_events,
    run_terminal_session_with_spawn_guard_and_initialization_recovery,
    run_terminal_session_with_spawn_guard_and_initialization_recovery_with_theme,
    terminal_session_signal_channel,
};
pub use theme::{
    ResolvedPalette, SemanticTone, TerminalThemeParseError, TerminalThemePreset, auto_style,
};

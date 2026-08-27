pub mod assets;
pub mod classify;
pub mod cli;
pub mod codex_hook;
pub mod codex_profile;
pub mod launcher;
pub mod persistence;
pub mod protocol;
pub mod runtime;
pub mod runtime_metadata;
pub mod server;
pub mod terminal;

pub use classify::classify_command;
pub use codex_hook::{
    HookTransportError, hook_output_for_payload, permission_context_for_hook,
    run_hook_from_environment, runtime_metadata_is_active, send_debug_generate_poop_to_runtime,
    send_debug_neglect_to_runtime, send_event_to_runtime, send_mode_to_runtime,
    send_name_to_runtime, translate_hook, translate_hook_json,
};
pub use codex_profile::{CodexInvocation, PersistentCodexProfile, PersistentCodexProfileGuard};
pub use launcher::{LaunchRequest, LauncherError, UiMode, ValidatedLaunch, parse_launch_request};
pub use persistence::{PersistenceError, SQLITE_SCHEMA_VERSION, SqliteStore};
pub use protocol::{
    CleanRequest, DebugRequest, ErrorEnvelope, EventIngestRequest, EventIngestResponse,
    FeedRequest, HealthResponse, HookInput, HookOutput, ModeRequest, NameRequest,
    PermissionContext, PetRequest, RuntimeMetadataV1, SnapshotMutationResponse,
};
pub use runtime::{
    AuthoritativeRuntime, EventIngestReceipt, MutationReceipt, RuntimeError, RuntimeInitial,
};
pub use server::{MAX_REQUEST_BODY_BYTES, RunningServer, ServerError};
pub use terminal::{PtyCodexChild, PtyCodexError, TerminalThemePreset};

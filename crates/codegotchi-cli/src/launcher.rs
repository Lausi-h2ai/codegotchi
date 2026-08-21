use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

use chrono::Utc;
use codegotchi_domain::{Pet, PetSpecies};
#[cfg(unix)]
use nix::sys::signal::{SigSet, Signal, killpg};
#[cfg(unix)]
use nix::unistd::{Pid, getpgid, getpgrp, setpgid, tcgetpgrp, tcsetpgrp};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use thiserror::Error;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::PersistentCodexProfile;
use crate::cli::CODEGOTCHI_ENABLE_DEBUG;
use crate::persistence::SqliteStore;
use crate::protocol::RuntimeMetadataV1;
use crate::runtime::AuthoritativeRuntime;
use crate::runtime_metadata::{read_metadata, remove_metadata, write_metadata};
use crate::server::RunningServer;
use crate::terminal::{
    TerminalSessionError, TerminalSessionSignal, TerminalSessionSignalReceiver,
    TerminalSessionSignalSender, TerminalThemePreset,
    run_terminal_session_with_spawn_guard_and_initialization_recovery_with_theme,
    terminal_session_signal_channel,
};

const CODEGOTCHI_STATE_DIRECTORY: &str = "codegotchi";
const DATABASE_FILE_NAME: &str = "state.sqlite";
const RUNTIME_DIRECTORY_NAME: &str = "codegotchi";
const SESSION_FILE_PREFIX: &str = "session-";
const SESSION_FILE_SUFFIX: &str = ".json";
const REPOSITORY_ID_NAMESPACE: &str = "codegotchi-repository-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LauncherSignal {
    Interrupt,
    Terminate,
    WindowChange,
}

impl LauncherSignal {
    fn exit_status(self) -> i32 {
        128 + self.number()
    }

    fn number(self) -> i32 {
        match self {
            Self::Interrupt => 2,
            Self::Terminate => 15,
            Self::WindowChange => 28,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "INT",
            Self::Terminate => "TERM",
            Self::WindowChange => "WINCH",
        }
    }

    fn is_terminal_group_signal(self) -> bool {
        matches!(self, Self::Interrupt | Self::WindowChange)
    }
}

#[cfg(unix)]
struct SignalController {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
    window_change: tokio::signal::unix::Signal,
}

#[cfg(not(unix))]
struct SignalController;

impl SignalController {
    #[cfg(unix)]
    fn install() -> Result<Self, LauncherError> {
        Ok(Self {
            interrupt: signal(SignalKind::interrupt()).map_err(|error| {
                LauncherError::message(format!("could not install SIGINT handling: {error}"))
            })?,
            terminate: signal(SignalKind::terminate()).map_err(|error| {
                LauncherError::message(format!("could not install SIGTERM handling: {error}"))
            })?,
            window_change: signal(SignalKind::window_change()).map_err(|error| {
                LauncherError::message(format!("could not install SIGWINCH handling: {error}"))
            })?,
        })
    }

    #[cfg(not(unix))]
    fn install() -> Result<Self, LauncherError> {
        Ok(Self)
    }

    #[cfg(unix)]
    async fn next(&mut self) -> Option<LauncherSignal> {
        tokio::select! {
            value = self.interrupt.recv() => value.map(|_| LauncherSignal::Interrupt),
            value = self.terminate.recv() => value.map(|_| LauncherSignal::Terminate),
            value = self.window_change.recv() => value.map(|_| LauncherSignal::WindowChange),
        }
    }

    #[cfg(not(unix))]
    async fn next(&mut self) -> Option<LauncherSignal> {
        None
    }

    #[cfg(unix)]
    async fn try_next(&mut self) -> Option<LauncherSignal> {
        tokio::time::timeout(Duration::from_millis(1), self.next())
            .await
            .ok()
            .flatten()
    }

    #[cfg(not(unix))]
    async fn try_next(&mut self) -> Option<LauncherSignal> {
        None
    }

    async fn try_setup_termination(&mut self) -> Option<LauncherSignal> {
        loop {
            match self.try_next().await {
                Some(LauncherSignal::WindowChange) => {}
                signal => return signal,
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("{0}")]
    Message(String),
    #[error("could not create the launcher runtime: {0}")]
    Runtime(#[source] io::Error),
}

impl LauncherError {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

#[derive(Debug)]
pub struct ValidatedLaunch {
    pub codex_path: PathBuf,
    pub codegotchi_executable: PathBuf,
    pub ui_mode: UiMode,
    pub terminal_theme: TerminalThemePreset,
    pub trailing_arguments: Vec<OsString>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMode {
    Auto,
    Terminal,
    Browser,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchRequest {
    pub ui_mode: UiMode,
    pub terminal_theme: TerminalThemePreset,
    pub trailing_codex_arguments: Vec<OsString>,
}

/// The inherited child wait path can consume either the installed launcher
/// signal controller or the bounded stream used by the terminal session.
/// Auto fallback uses the latter so initialization never installs or loses a
/// second set of handlers.
enum InheritedSignalSource {
    Controller(SignalController),
    Terminal(TerminalSessionSignalReceiver),
    /// No inherited wait path is available for a terminal-only attempt. This
    /// avoids allocating a disconnected fallback channel before Auto needs
    /// one for a real initialization recovery.
    Unavailable,
}

enum TerminalAttemptError {
    /// Physical-terminal initialization failed before the PTY child spawn.
    /// The receiver remains available for Auto's inherited fallback.
    Initialization {
        error: TerminalSessionError,
        signals: TerminalSessionSignalReceiver,
    },
    /// Every other terminal failure occurs after the no-fallback boundary.
    Session(TerminalSessionError),
}

#[derive(Debug)]
enum RouteError {
    Inherited(LauncherError),
    Terminal(TerminalSessionError),
}

/// Executes the shared mode routing used by production launch.
///
/// The callbacks are the production spawn/browser seams, so deterministic
/// tests exercise this exact route coordinator rather than a parallel test
/// implementation. `Auto` retries inherited stdio only when the terminal
/// session explicitly reports pre-spawn initialization failure.
async fn execute_ui_route<Browser, Inherited, InheritedFuture, Terminal, TerminalFuture>(
    mode: UiMode,
    inherited_signals: InheritedSignalSource,
    launch_browser: Browser,
    inherited: Inherited,
    terminal: Terminal,
) -> Result<i32, RouteError>
where
    Browser: FnOnce(),
    Inherited: FnOnce(InheritedSignalSource) -> InheritedFuture,
    InheritedFuture: Future<Output = Result<i32, LauncherError>>,
    Terminal: FnOnce() -> TerminalFuture,
    TerminalFuture: Future<Output = Result<i32, TerminalAttemptError>>,
{
    match mode {
        UiMode::Browser => {
            launch_browser();
            inherited(inherited_signals)
                .await
                .map_err(RouteError::Inherited)
        }
        UiMode::Terminal => terminal().await.map_err(|error| match error {
            TerminalAttemptError::Initialization { error, .. }
            | TerminalAttemptError::Session(error) => RouteError::Terminal(error),
        }),
        UiMode::Both => {
            launch_browser();
            terminal().await.map_err(|error| match error {
                TerminalAttemptError::Initialization { error, .. }
                | TerminalAttemptError::Session(error) => RouteError::Terminal(error),
            })
        }
        UiMode::Auto => match terminal().await {
            Ok(status) => Ok(status),
            Err(TerminalAttemptError::Initialization { error: _, signals }) => {
                launch_browser();
                inherited(InheritedSignalSource::Terminal(signals))
                    .await
                    .map_err(RouteError::Inherited)
            }
            Err(TerminalAttemptError::Session(error)) => Err(RouteError::Terminal(error)),
        },
    }
}

/// Validates the exact launcher shape and resolves both executables without
/// creating a state, runtime, metadata, or profile file.
pub fn validate(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<ValidatedLaunch, LauncherError> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let request = parse_launch_request(&arguments)?;
    let codegotchi_executable = current_codegotchi_executable()?;
    let codex_path = resolve_codex(&codegotchi_executable)?;
    Ok(ValidatedLaunch {
        codex_path,
        codegotchi_executable,
        ui_mode: request.ui_mode,
        terminal_theme: request.terminal_theme,
        trailing_arguments: request.trailing_codex_arguments,
    })
}

/// Runs one Codex child and returns its numeric exit status.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<i32, LauncherError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(LauncherError::Runtime)?;
    runtime.block_on(run_async(arguments.into_iter().collect()))
}

async fn run_async(arguments: Vec<OsString>) -> Result<i32, LauncherError> {
    let validated = validate(arguments)?;
    let mut signals = SignalController::install()?;
    if let Some(signal) = signals.try_setup_termination().await {
        return Ok(signal.exit_status());
    }
    let paths = resolve_launch_paths()?;
    if let Some(signal) = signals.try_setup_termination().await {
        return Ok(signal.exit_status());
    }

    ensure_private_directory(&paths.state_directory)?;
    fs::create_dir_all(&paths.codex_home).map_err(|error| {
        LauncherError::message(format!(
            "could not create CODEX_HOME at {}: {error}",
            paths.codex_home.display()
        ))
    })?;
    if !paths.codex_home.is_dir() {
        return Err(LauncherError::message(format!(
            "CODEX_HOME is not a directory: {}",
            paths.codex_home.display()
        )));
    }

    let store = SqliteStore::open_for_repository(&paths.database, paths.repository_id.clone())
        .map_err(|error| {
            LauncherError::message(format!("could not open CodeGotchi state: {error}"))
        })?;
    ensure_private_directory(&paths.runtime_directory)?;
    clean_stale_metadata(&paths.runtime_directory)?;
    if let Some(signal) = signals.try_setup_termination().await {
        return Ok(signal.exit_status());
    }

    let repository_uuid = Uuid::parse_str(&paths.repository_id).map_err(|error| {
        LauncherError::message(format!("could not derive the repository identity: {error}"))
    })?;
    let runtime = AuthoritativeRuntime::new(
        store,
        Pet::new(
            repository_uuid,
            deterministic_pet_name(repository_uuid),
            PetSpecies::Cat,
            Utc::now(),
        ),
    )
    .map_err(|error| {
        LauncherError::message(format!("could not start CodeGotchi runtime: {error}"))
    })?;

    let token = launch_token();
    let debug_enabled = std::env::var(CODEGOTCHI_ENABLE_DEBUG).ok().as_deref() == Some("1");
    let server = if debug_enabled {
        RunningServer::start_with_debug(std::sync::Arc::clone(&runtime), token.clone()).await
    } else {
        RunningServer::start(std::sync::Arc::clone(&runtime), token.clone()).await
    }
    .map_err(|error| {
        LauncherError::message(format!("could not start CodeGotchi server: {error}"))
    })?;
    if let Some(signal) = signals.try_setup_termination().await {
        let _ = server.shutdown().await;
        return Ok(signal.exit_status());
    }
    let (runtime_id, metadata_path) = match session_path(&paths.runtime_directory) {
        Ok(path) => path,
        Err(error) => {
            let _ = server.shutdown().await;
            return Err(error);
        }
    };
    let metadata = RuntimeMetadataV1::new(
        runtime_id,
        paths.repository_root.clone(),
        server.base_url(),
        token,
        std::process::id(),
    );
    if let Err(error) = write_metadata(&metadata_path, &metadata) {
        let _ = server.shutdown().await;
        return Err(LauncherError::message(format!(
            "could not publish CodeGotchi runtime metadata at {}: {error}",
            metadata_path.display()
        )));
    }
    let mut owned_metadata = OwnedMetadata::new(metadata_path);
    if let Some(signal) = signals.try_setup_termination().await {
        let _ = owned_metadata.cleanup();
        let _ = server.shutdown().await;
        return Ok(signal.exit_status());
    }

    let hook_command = format!("{} hook", shell_quote(&validated.codegotchi_executable));
    let profile = match PersistentCodexProfile::ensure(
        &paths.codex_home,
        owned_metadata.path(),
        &hook_command,
    ) {
        Ok(profile) => profile,
        Err(error) => {
            let _ = owned_metadata.cleanup();
            let _ = server.shutdown().await;
            return Err(LauncherError::message(format!(
                "could not ensure the persistent Codex profile: {error}"
            )));
        }
    };
    if let Some(signal) = signals.try_setup_termination().await {
        let _ = owned_metadata.cleanup();
        let _ = server.shutdown().await;
        return Ok(signal.exit_status());
    }

    let profile_guard = match profile.acquire_spawn_guard() {
        Ok(guard) => guard,
        Err(error) => {
            let _ = owned_metadata.cleanup();
            let _ = server.shutdown().await;
            return Err(LauncherError::message(format!(
                "could not acquire the persistent Codex profile before spawn: {error}"
            )));
        }
    };
    let invocation = profile_guard.invocation(&validated.codex_path, &validated.trailing_arguments);
    if let Some(signal) = signals.try_setup_termination().await {
        drop(profile_guard);
        drop(profile);
        let _ = owned_metadata.cleanup();
        let _ = server.shutdown().await;
        return Ok(signal.exit_status());
    }

    let ui_url = format!("{}/#token={}", server.base_url(), metadata.bearer_token);
    let mut browser_wait = None;
    let mut terminal_forwarder = None;
    let mut installed_signals = Some(signals);
    let inherited_signals = if validated.ui_mode == UiMode::Browser {
        InheritedSignalSource::Controller(
            installed_signals
                .take()
                .expect("browser route retains the installed signal controller"),
        )
    } else {
        InheritedSignalSource::Unavailable
    };
    let route_result = execute_ui_route(
        validated.ui_mode,
        inherited_signals,
        || {
            println!("CodeGotchi UI: {ui_url}");
            let _ = io::Write::flush(&mut io::stdout());
            browser_wait = launch_browser(&ui_url);
        },
        |source| {
            run_inherited_session(
                &profile_guard,
                &invocation,
                &validated.codex_path,
                source,
            )
        },
        || {
            let controller = installed_signals
                .take()
                .expect("terminal route retains the installed signal controller");
            let (sender, receiver) = terminal_session_signal_channel(16);
            terminal_forwarder = Some(tokio::spawn(forward_terminal_signals(controller, sender)));
            let room_runtime = std::sync::Arc::clone(&runtime);
            async {
                let result =
                    run_terminal_session_with_spawn_guard_and_initialization_recovery_with_theme(
                        &invocation,
                        receiver,
                        Some(room_runtime),
                        validated.terminal_theme,
                        || {
                            profile_guard
                                .verify_before_spawn()
                                .map_err(|error| {
                                    TerminalSessionError::Input(io::Error::other(format!(
                                        "could not verify the persistent Codex profile before spawn: {error}"
                                    )))
                                })
                        },
                    )
                    .await;
                match result {
                    Ok(status) => Ok(numeric_terminal_exit_status(status)),
                    Err(crate::terminal::TerminalSessionStartError::Initialization {
                        error,
                        signals,
                    }) => Err(TerminalAttemptError::Initialization { error, signals }),
                    Err(crate::terminal::TerminalSessionStartError::Session(error)) => {
                        Err(TerminalAttemptError::Session(error))
                    }
                }
            }
        },
    )
    .await;
    if let Some(task) = terminal_forwarder.take() {
        task.abort();
        let _ = task.await;
    }
    drop(profile_guard);
    drop(profile);
    let metadata_cleanup = owned_metadata.cleanup();
    let server_cleanup = server.shutdown().await;
    wait_for_browser(browser_wait).await;

    if let Err(error) = metadata_cleanup {
        return Err(LauncherError::message(format!(
            "Codex exited, but CodeGotchi could not remove owned metadata: {error}"
        )));
    }
    if let Err(error) = server_cleanup {
        return Err(LauncherError::message(format!(
            "Codex exited, but CodeGotchi server shutdown failed: {error}"
        )));
    }
    route_result.map_err(|error| match error {
        RouteError::Inherited(error) => error,
        RouteError::Terminal(error) => {
            LauncherError::message(format!("Codex terminal session failed: {error}"))
        }
    })
}

async fn run_inherited_session(
    profile_guard: &crate::PersistentCodexProfileGuard<'_>,
    invocation: &crate::CodexInvocation,
    codex_path: &Path,
    signals: InheritedSignalSource,
) -> Result<i32, LauncherError> {
    let mut child_command = invocation.std_command();
    child_command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    let terminal_handoff = TerminalHandoff::detect();
    #[cfg(unix)]
    if terminal_handoff.is_some() {
        child_command.process_group(0);
    }

    // Keep the profile guard alive through this exact inherited spawn.
    profile_guard.verify_before_spawn().map_err(|error| {
        LauncherError::message(format!(
            "could not verify the persistent Codex profile before spawn: {error}"
        ))
    })?;
    let child = profile_guard.spawn(&mut child_command).map_err(|error| {
        LauncherError::message(format!(
            "could not spawn Codex at {}: {error}",
            codex_path.display()
        ))
    })?;

    #[cfg(unix)]
    let status = wait_with_terminal_handoff(child, signals, terminal_handoff).await?;
    #[cfg(not(unix))]
    let status = wait_for_child(child, signals).await?;
    Ok(numeric_exit_status(status))
}

async fn forward_terminal_signals(
    mut signals: SignalController,
    sender: TerminalSessionSignalSender,
) {
    while let Some(signal) = signals.next().await {
        let signal = match signal {
            LauncherSignal::Interrupt => TerminalSessionSignal::Interrupt,
            LauncherSignal::Terminate => TerminalSessionSignal::Terminate,
            LauncherSignal::WindowChange => TerminalSessionSignal::WindowChange,
        };
        if sender.send(signal).await.is_err() {
            break;
        }
    }
}

impl InheritedSignalSource {
    async fn next(&mut self) -> Option<LauncherSignal> {
        match self {
            Self::Controller(signals) => signals.next().await,
            Self::Terminal(signals) => signals.recv().await.map(|signal| match signal {
                TerminalSessionSignal::Interrupt => LauncherSignal::Interrupt,
                TerminalSessionSignal::Terminate => LauncherSignal::Terminate,
                TerminalSessionSignal::WindowChange => LauncherSignal::WindowChange,
            }),
            Self::Unavailable => std::future::pending().await,
        }
    }
}

fn numeric_terminal_exit_status(status: portable_pty::ExitStatus) -> i32 {
    if let Some(signal) = status.signal() {
        if let Some(number) = portable_signal_number(signal) {
            return 128 + number;
        }
        return 1;
    }
    i32::try_from(status.exit_code()).unwrap_or(1)
}

/// Converts portable-pty's platform-specific signal description to the Unix
/// signal number used by the inherited launcher path. On Unix portable-pty
/// obtains this text from `strsignal`, so Linux reports names such as
/// `Interrupt` and `Terminated` rather than `SIGINT` and `SIGTERM`.
fn portable_signal_number(signal: &str) -> Option<i32> {
    let normalized = signal
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    if let Some(number) = normalized
        .strip_prefix("SIGNAL ")
        .and_then(|number| number.parse::<i32>().ok())
    {
        return (1..=64).contains(&number).then_some(number);
    }

    let signal = normalized.strip_prefix("SIG").unwrap_or(&normalized);
    match signal {
        "HUP" | "HANGUP" => Some(1),
        "INT" | "INTERRUPT" => Some(2),
        "QUIT" => Some(3),
        "ILL" | "ILLEGAL INSTRUCTION" => Some(4),
        "TRAP" => Some(5),
        "ABRT" | "IOT" | "ABORT" | "ABORTED" => Some(6),
        "BUS" | "BUS ERROR" => Some(7),
        "FPE" | "FLOATING POINT EXCEPTION" => Some(8),
        "KILL" | "KILLED" => Some(9),
        "USR1" | "USER DEFINED SIGNAL 1" => Some(10),
        "SEGV" | "SEGMENTATION FAULT" => Some(11),
        "USR2" | "USER DEFINED SIGNAL 2" => Some(12),
        "PIPE" | "BROKEN PIPE" => Some(13),
        "ALRM" | "ALARM CLOCK" => Some(14),
        "TERM" | "TERMINATED" => Some(15),
        "STKFLT" | "STACK FAULT" => Some(16),
        "CHLD" | "CHILD EXITED" => Some(17),
        "CONT" | "CONTINUED" => Some(18),
        "STOP" | "STOPPED" => Some(19),
        "TSTP" => Some(20),
        "TTIN" | "BACKGROUND READ" => Some(21),
        "TTOU" | "BACKGROUND WRITE" => Some(22),
        "URG" | "URGENT I/O" => Some(23),
        "XCPU" | "CPU TIME LIMIT EXCEEDED" => Some(24),
        "XFSZ" | "FILE SIZE LIMIT EXCEEDED" => Some(25),
        "VTALRM" | "VIRTUAL TIMER EXPIRED" => Some(26),
        "PROF" | "PROFILING TIMER EXPIRED" => Some(27),
        "WINCH" | "WINDOW CHANGED" => Some(28),
        "IO" | "I/O POSSIBLE" => Some(29),
        "PWR" | "POWER FAILURE" => Some(30),
        "SYS" | "BAD SYSTEM CALL" => Some(31),
        _ => None,
    }
}

pub fn parse_launch_request<I, A>(arguments: I) -> Result<LaunchRequest, LauncherError>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    const SUPPORTED_FORM: &str = "supported command form: `codegotchi run [--ui auto|terminal|browser|both] [--terminal-theme auto|mono|soft-green|amber|night] -- codex [arguments...]`";

    let arguments = arguments
        .into_iter()
        .map(|argument| argument.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let separator_index = arguments
        .iter()
        .position(|argument| argument == OsStr::new("--"))
        .ok_or_else(|| {
            LauncherError::message(format!(
                "`codegotchi run` requires the exact separator `--` before the agent; {SUPPORTED_FORM}"
            ))
        })?;

    let mut ui_mode = UiMode::Auto;
    let mut saw_ui = false;
    let mut terminal_theme = TerminalThemePreset::Auto;
    let mut saw_terminal_theme = false;
    let mut index = 0;
    while index < separator_index {
        let argument = &arguments[index];
        if argument == OsStr::new("--ui") {
            if saw_ui {
                return Err(LauncherError::message(format!(
                    "duplicate `--ui` option before the separator; {SUPPORTED_FORM}"
                )));
            }
            let value = arguments.get(index + 1).ok_or_else(|| {
                LauncherError::message(format!(
                    "`--ui` requires one of `auto`, `terminal`, `browser`, or `both`; {SUPPORTED_FORM}"
                ))
            })?;
            if value == OsStr::new("--") || value == OsStr::new("--ui") {
                return Err(LauncherError::message(format!(
                    "`--ui` requires one of `auto`, `terminal`, `browser`, or `both`; {SUPPORTED_FORM}"
                )));
            }
            ui_mode = parse_ui_mode(value, SUPPORTED_FORM)?;
            saw_ui = true;
            index += 2;
            continue;
        }

        if let Some(value) = argument
            .to_str()
            .and_then(|argument| argument.strip_prefix("--ui="))
        {
            if saw_ui {
                return Err(LauncherError::message(format!(
                    "duplicate `--ui` option before the separator; {SUPPORTED_FORM}"
                )));
            }
            if value.is_empty() {
                return Err(LauncherError::message(format!(
                    "`--ui` requires one of `auto`, `terminal`, `browser`, or `both`; {SUPPORTED_FORM}"
                )));
            }
            ui_mode = parse_ui_mode(OsStr::new(value), SUPPORTED_FORM)?;
            saw_ui = true;
            index += 1;
            continue;
        }

        if argument == OsStr::new("--terminal-theme") {
            if saw_terminal_theme {
                return Err(LauncherError::message(format!(
                    "duplicate `--terminal-theme` option before the separator; {SUPPORTED_FORM}"
                )));
            }
            let value = arguments.get(index + 1).ok_or_else(|| {
                LauncherError::message(format!(
                    "`--terminal-theme` requires one of `auto`, `mono`, `soft-green`, `amber`, or `night`; {SUPPORTED_FORM}"
                ))
            })?;
            if value == OsStr::new("--")
                || value == OsStr::new("--ui")
                || value == OsStr::new("--terminal-theme")
            {
                return Err(LauncherError::message(format!(
                    "`--terminal-theme` requires one of `auto`, `mono`, `soft-green`, `amber`, or `night`; {SUPPORTED_FORM}"
                )));
            }
            terminal_theme = parse_terminal_theme(value, SUPPORTED_FORM)?;
            saw_terminal_theme = true;
            index += 2;
            continue;
        }

        if let Some(value) = argument
            .to_str()
            .and_then(|argument| argument.strip_prefix("--terminal-theme="))
        {
            if saw_terminal_theme {
                return Err(LauncherError::message(format!(
                    "duplicate `--terminal-theme` option before the separator; {SUPPORTED_FORM}"
                )));
            }
            if value.is_empty() {
                return Err(LauncherError::message(format!(
                    "`--terminal-theme` requires one of `auto`, `mono`, `soft-green`, `amber`, or `night`; {SUPPORTED_FORM}"
                )));
            }
            terminal_theme = parse_terminal_theme(OsStr::new(value), SUPPORTED_FORM)?;
            saw_terminal_theme = true;
            index += 1;
            continue;
        }

        return Err(LauncherError::message(format!(
            "unexpected pre-separator argument `{}`; CodeGotchi accepts only `--ui auto|terminal|browser|both` and `--terminal-theme auto|mono|soft-green|amber|night` before the separator; {SUPPORTED_FORM}",
            argument.to_string_lossy()
        )));
    }

    if arguments.get(separator_index + 1).map(OsString::as_os_str) != Some(OsStr::new("codex")) {
        let agent = arguments
            .get(separator_index + 1)
            .map(|argument| argument.to_string_lossy())
            .unwrap_or_else(|| "<missing>".into());
        return Err(LauncherError::message(format!(
            "unsupported agent `{agent}`; the only supported agent is `codex`; {SUPPORTED_FORM}"
        )));
    }

    let trailing_codex_arguments = arguments
        .iter()
        .skip(separator_index + 2)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(conflict) = trailing_codex_arguments
        .iter()
        .find(|argument| is_profile_conflict(argument))
    {
        return Err(LauncherError::message(format!(
            "Codex argument `{}` conflicts with CodeGotchi's generated additive profile; remove `-p`/`--profile` because CodeGotchi injects its own profile",
            conflict.to_string_lossy()
        )));
    }

    Ok(LaunchRequest {
        ui_mode,
        terminal_theme,
        trailing_codex_arguments,
    })
}

fn parse_ui_mode(value: &OsStr, supported_form: &str) -> Result<UiMode, LauncherError> {
    match value {
        value if value == OsStr::new("auto") => Ok(UiMode::Auto),
        value if value == OsStr::new("terminal") => Ok(UiMode::Terminal),
        value if value == OsStr::new("browser") => Ok(UiMode::Browser),
        value if value == OsStr::new("both") => Ok(UiMode::Both),
        value => Err(LauncherError::message(format!(
            "unsupported `--ui` value `{}`; choose `auto|terminal|browser|both`; {supported_form}",
            value.to_string_lossy()
        ))),
    }
}

fn parse_terminal_theme(
    value: &OsStr,
    supported_form: &str,
) -> Result<TerminalThemePreset, LauncherError> {
    value
        .to_str()
        .unwrap_or_default()
        .parse::<TerminalThemePreset>()
        .map_err(|_| {
            LauncherError::message(format!(
                "unsupported `--terminal-theme` value `{}`; choose `auto|mono|soft-green|amber|night`; {supported_form}",
                value.to_string_lossy()
            ))
        })
}

fn is_profile_conflict(argument: &OsString) -> bool {
    let argument = argument.to_string_lossy();
    argument == "-p"
        || argument.starts_with("-p") && !argument.starts_with("--")
        || argument == "--profile"
        || argument.starts_with("--profile=")
}

fn current_codegotchi_executable() -> Result<PathBuf, LauncherError> {
    let path = env::current_exe().map_err(|error| {
        LauncherError::message(format!(
            "could not resolve the running CodeGotchi executable: {error}"
        ))
    })?;
    fs::canonicalize(&path).map_err(|error| {
        LauncherError::message(format!(
            "could not resolve the running CodeGotchi executable at {}: {error}",
            path.display()
        ))
    })
}

fn resolve_codex(codegotchi_executable: &Path) -> Result<PathBuf, LauncherError> {
    let override_value = env::var_os("CODEGOTCHI_REAL_CODEX");
    let requested = override_value
        .clone()
        .unwrap_or_else(|| OsString::from("codex"));
    if requested.is_empty() {
        return Err(LauncherError::message(
            "CODEGOTCHI_REAL_CODEX is empty; set it to an executable Codex path or unset it to use PATH",
        ));
    }

    let requested_path = Path::new(&requested);
    let has_path_separator = requested_path.is_absolute()
        || requested_path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());
    if override_value.is_some() && has_path_separator {
        return resolve_candidate(requested_path, codegotchi_executable, true);
    }

    let path = env::var_os("PATH").ok_or_else(|| {
        LauncherError::message("could not locate Codex: PATH is not set and CODEGOTCHI_REAL_CODEX is not an executable path")
    })?;
    let mut saw_non_executable = false;
    for directory in env::split_paths(&path) {
        let directory = if directory.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            directory
        };
        let candidate = directory.join(&requested);
        if !candidate.exists() {
            continue;
        }
        match resolve_candidate(&candidate, codegotchi_executable, false) {
            Ok(path) => return Ok(path),
            Err(error) if error.to_string().contains("not executable") => {
                saw_non_executable = true;
            }
            Err(error) => return Err(error),
        }
    }
    if saw_non_executable {
        return Err(LauncherError::message(format!(
            "Codex candidate `{}` was found in PATH but is not executable",
            requested.to_string_lossy()
        )));
    }
    Err(LauncherError::message(format!(
        "Codex `{}` was not found in PATH; set CODEGOTCHI_REAL_CODEX to its executable path",
        requested.to_string_lossy()
    )))
}

fn resolve_candidate(
    candidate: &Path,
    codegotchi_executable: &Path,
    explicit: bool,
) -> Result<PathBuf, LauncherError> {
    let canonical = match fs::canonicalize(candidate) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(LauncherError::message(format!(
                "Codex candidate {} was not found",
                candidate.display()
            )));
        }
        Err(error) => {
            return Err(LauncherError::message(format!(
                "could not resolve Codex candidate {}: {error}",
                candidate.display()
            )));
        }
    };
    if canonical == codegotchi_executable {
        return Err(LauncherError::message(
            "CODEGOTCHI_REAL_CODEX resolves to the running CodeGotchi executable; choose the real Codex binary",
        ));
    }
    let metadata = fs::metadata(&canonical).map_err(|error| {
        LauncherError::message(format!(
            "could not inspect Codex candidate {}: {error}",
            canonical.display()
        ))
    })?;
    if !is_executable_file(&metadata) {
        let message = format!("Codex candidate {} is not executable", candidate.display());
        if explicit {
            return Err(LauncherError::message(message));
        }
        return Err(LauncherError::message(message));
    }
    Ok(canonical)
}

#[cfg(unix)]
fn is_executable_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable_file(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}

struct LaunchPaths {
    repository_root: PathBuf,
    repository_id: String,
    state_directory: PathBuf,
    database: PathBuf,
    runtime_directory: PathBuf,
    codex_home: PathBuf,
}

fn resolve_launch_paths() -> Result<LaunchPaths, LauncherError> {
    let current_directory = env::current_dir().map_err(|error| {
        LauncherError::message(format!("could not read the current directory: {error}"))
    })?;
    let current_directory = fs::canonicalize(&current_directory).map_err(|error| {
        LauncherError::message(format!(
            "could not canonicalize the current directory {}: {error}",
            current_directory.display()
        ))
    })?;
    let repository_root = git_worktree_root(&current_directory);
    let repository_id = stable_repository_id(&repository_root);
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from);
    let state_home = env::var_os("XDG_STATE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/state")))
        .ok_or_else(|| {
            LauncherError::message(
                "HOME is required when XDG_STATE_HOME is not set for CodeGotchi state",
            )
        })?;
    let state_directory = state_home.join(CODEGOTCHI_STATE_DIRECTORY);
    let runtime_directory = env::var_os("XDG_RUNTIME_DIR")
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(path).join(RUNTIME_DIRECTORY_NAME))
        .unwrap_or_else(|| state_directory.clone());
    let codex_home = env::var_os("CODEX_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".codex")))
        .ok_or_else(|| {
            LauncherError::message(
                "HOME is required when CODEX_HOME is not set for the Codex profile",
            )
        })?;
    Ok(LaunchPaths {
        repository_root,
        repository_id,
        database: state_directory.join(DATABASE_FILE_NAME),
        state_directory,
        runtime_directory,
        codex_home,
    })
}

fn git_worktree_root(current_directory: &Path) -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(current_directory)
        .output();
    if let Ok(output) = output
        && output.status.success()
    {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !value.is_empty()
            && let Ok(path) = fs::canonicalize(value)
            && path.is_dir()
        {
            return path;
        }
    }
    current_directory.to_path_buf()
}

fn stable_repository_id(repository_root: &Path) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "{REPOSITORY_ID_NAMESPACE}:{}",
            repository_root.to_string_lossy()
        )
        .as_bytes(),
    )
    .to_string()
}

fn deterministic_pet_name(repository_id: Uuid) -> &'static str {
    const NAMES: [&str; 8] = [
        "Mochi", "Pixel", "Nori", "Biscuit", "Miso", "Pico", "Tofu", "Pudding",
    ];
    let index = repository_id
        .as_bytes()
        .iter()
        .fold(0usize, |sum, byte| sum.wrapping_add(usize::from(*byte)))
        % NAMES.len();
    NAMES[index]
}

fn ensure_private_directory(path: &Path) -> Result<(), LauncherError> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        return Err(LauncherError::message(format!(
            "CodeGotchi directory must not be a symlink: {}",
            path.display()
        )));
    }
    fs::create_dir_all(path).map_err(|error| {
        LauncherError::message(format!(
            "could not create CodeGotchi directory {}: {error}",
            path.display()
        ))
    })?;
    let metadata = fs::metadata(path).map_err(|error| {
        LauncherError::message(format!(
            "could not inspect CodeGotchi directory {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(LauncherError::message(format!(
            "CodeGotchi path is not a directory: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).map_err(|error| {
            LauncherError::message(format!(
                "could not protect CodeGotchi directory {}: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

fn session_runtime_id(name: &OsStr) -> Option<Uuid> {
    let name = name.to_str()?;
    let value = name
        .strip_prefix(SESSION_FILE_PREFIX)?
        .strip_suffix(SESSION_FILE_SUFFIX)?;
    let runtime_id = Uuid::parse_str(value).ok()?;
    (value == runtime_id.to_string()).then_some(runtime_id)
}

fn clean_stale_metadata(directory: &Path) -> Result<(), LauncherError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        LauncherError::message(format!(
            "could not inspect CodeGotchi runtime directory {}: {error}",
            directory.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| LauncherError::message(error.to_string()))?;
        let path = entry.path();
        let Some(filename_runtime_id) = session_runtime_id(&entry.file_name()) else {
            continue;
        };
        let file_type = entry.file_type().map_err(|error| {
            LauncherError::message(format!("could not inspect {}: {error}", path.display()))
        })?;
        if !file_type.is_file() {
            continue;
        }
        let Ok(metadata) = read_metadata(&path) else {
            continue;
        };
        if metadata.runtime_id != filename_runtime_id {
            continue;
        }
        if !crate::codex_hook::runtime_metadata_is_active(&metadata) {
            remove_metadata(&path).map_err(|error| {
                LauncherError::message(format!(
                    "could not remove stale metadata {}: {error}",
                    path.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn session_path(directory: &Path) -> Result<(Uuid, PathBuf), LauncherError> {
    for _ in 0..8 {
        let runtime_id = Uuid::new_v4();
        let path = directory.join(format!("{SESSION_FILE_PREFIX}{runtime_id}.json"));
        if !path.exists() {
            return Ok((runtime_id, path));
        }
    }
    Err(LauncherError::message(
        "could not allocate a unique CodeGotchi runtime metadata path",
    ))
}

fn launch_token() -> String {
    format!("{}-{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\'', "'\\''");
    format!("'{value}'")
}

struct OwnedMetadata {
    path: PathBuf,
    owned: bool,
}

impl OwnedMetadata {
    fn new(path: PathBuf) -> Self {
        Self { path, owned: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup(&mut self) -> Result<(), io::Error> {
        if !self.owned {
            return Ok(());
        }
        remove_metadata(&self.path)?;
        self.owned = false;
        Ok(())
    }
}

impl Drop for OwnedMetadata {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn launch_browser(url: &str) -> Option<JoinHandle<()>> {
    let override_value = env::var_os("CODEGOTCHI_BROWSER");
    if override_value.as_deref() == Some(OsStr::new("none")) {
        return None;
    }

    let result = if let Some(program) = override_value {
        spawn_browser_command(Path::new(&program), &[OsString::from(url)])
    } else {
        launch_native_browser(url)
    };
    match result {
        Ok(child) => Some(tokio::spawn(reap_browser(child, url.to_owned()))),
        Err(error) => {
            eprintln!(
                "CodeGotchi warning: could not open the UI automatically ({error}); open {url}"
            );
            None
        }
    }
}

fn spawn_browser_command(program: &Path, arguments: &[OsString]) -> Result<Child, String> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .map_err(|error| format!("{program:?}: {error}"))
}

#[cfg(target_os = "macos")]
fn launch_native_browser(url: &str) -> Result<Child, String> {
    let (program, arguments) = native_browser_command(url);
    spawn_browser_command(&program, &arguments)
}

#[cfg(target_os = "linux")]
fn launch_native_browser(url: &str) -> Result<Child, String> {
    for (program, arguments) in [
        ("xdg-open", vec![OsString::from(url)]),
        ("gio", vec![OsString::from("open"), OsString::from(url)]),
    ] {
        if let Ok(child) = spawn_browser_command(Path::new(program), &arguments) {
            return Ok(child);
        }
    }
    if is_wsl() {
        let arguments = [
            OsString::from("/c"),
            OsString::from("start"),
            OsString::new(),
            OsString::from(url),
        ];
        if let Ok(child) = spawn_browser_command(Path::new("cmd.exe"), &arguments) {
            return Ok(child);
        }
    }
    Err(String::from("no supported browser launcher was available"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn launch_native_browser(url: &str) -> Result<Child, String> {
    let _ = url;
    Err(String::from("no supported browser launcher was available"))
}

#[cfg(target_os = "macos")]
fn native_browser_command(url: &str) -> (PathBuf, Vec<OsString>) {
    (PathBuf::from("/usr/bin/open"), vec![OsString::from(url)])
}

async fn reap_browser(mut child: Child, url: String) {
    let result = tokio::task::spawn_blocking(move || child.wait()).await;
    match result {
        Ok(Ok(status)) if status.success() => {}
        Ok(Ok(status)) => eprintln!(
            "CodeGotchi warning: browser helper exited unsuccessfully ({status}); open {url}"
        ),
        Ok(Err(error)) => eprintln!(
            "CodeGotchi warning: browser helper could not be reaped ({error}); open {url}"
        ),
        Err(error) => {
            eprintln!("CodeGotchi warning: browser helper wait failed ({error}); open {url}")
        }
    }
}

async fn wait_for_browser(wait: Option<JoinHandle<()>>) {
    if let Some(wait) = wait {
        let _ = wait.await;
    }
}

#[cfg(target_os = "linux")]
fn is_wsl() -> bool {
    env::var_os("WSL_INTEROP").is_some()
        || fs::read_to_string("/proc/version")
            .map(|version| version.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

#[cfg(unix)]
struct TerminalHandoff {
    launcher_group: Pid,
    active: bool,
}

#[cfg(unix)]
impl TerminalHandoff {
    fn detect() -> Option<Self> {
        let launcher_group = getpgrp();
        (tcgetpgrp(io::stdin()).ok() == Some(launcher_group)).then_some(Self {
            launcher_group,
            active: false,
        })
    }

    fn activate(&mut self, child_pid: u32) -> Result<(), LauncherError> {
        let raw_pid = i32::try_from(child_pid)
            .map_err(|_| LauncherError::message("Codex process ID does not fit in a Unix PID"))?;
        let child_group = Pid::from_raw(raw_pid);
        if let Err(set_error) = setpgid(child_group, child_group)
            && getpgid(Some(child_group)).ok() != Some(child_group)
        {
            return Err(LauncherError::message(format!(
                "could not create the Codex process group: {set_error}"
            )));
        }
        tcsetpgrp(io::stdin(), child_group).map_err(|error| {
            LauncherError::message(format!(
                "could not give the terminal foreground to Codex: {error}"
            ))
        })?;
        self.active = true;
        let _ = killpg(child_group, Signal::SIGCONT);
        Ok(())
    }

    fn restore(&mut self) -> Result<(), LauncherError> {
        if !self.active {
            return Ok(());
        }

        let previous_mask = SigSet::thread_get_mask().map_err(|error| {
            LauncherError::message(format!(
                "could not inspect the signal mask before restoring the terminal: {error}"
            ))
        })?;
        let mut blocked_mask = previous_mask;
        blocked_mask.add(Signal::SIGTTOU);
        blocked_mask.thread_set_mask().map_err(|error| {
            LauncherError::message(format!(
                "could not block SIGTTOU while restoring the terminal: {error}"
            ))
        })?;
        let restore_result = tcsetpgrp(io::stdin(), self.launcher_group);
        let mask_result = previous_mask.thread_set_mask();
        if restore_result.is_ok() {
            self.active = false;
        }
        restore_result.map_err(|error| {
            LauncherError::message(format!(
                "could not restore the terminal foreground to CodeGotchi: {error}"
            ))
        })?;
        mask_result.map_err(|error| {
            LauncherError::message(format!(
                "could not restore the signal mask after terminal handoff: {error}"
            ))
        })
    }
}

#[cfg(unix)]
async fn wait_with_terminal_handoff(
    mut child: Child,
    signals: InheritedSignalSource,
    mut handoff: Option<TerminalHandoff>,
) -> Result<ExitStatus, LauncherError> {
    if let Some(handoff) = handoff.as_mut()
        && let Err(error) = handoff.activate(child.id())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }

    let wait_result = wait_for_child(child, signals).await;
    let restore_result = match handoff.as_mut() {
        Some(handoff) => handoff.restore(),
        None => Ok(()),
    };
    match (wait_result, restore_result) {
        (Ok(status), Ok(())) => Ok(status),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

async fn wait_for_child(
    child: Child,
    mut signals: InheritedSignalSource,
) -> Result<ExitStatus, LauncherError> {
    #[cfg(unix)]
    {
        let pid = child.id();
        let shared_foreground_group = shared_foreground_terminal_group(pid);
        let wait = tokio::task::spawn_blocking(move || {
            let mut child = child;
            child.wait()
        });
        tokio::pin!(wait);
        loop {
            tokio::select! {
                result = &mut wait => return join_child_wait(result),
                received = signals.next() => match received {
                    Some(received) if !shared_foreground_group || !received.is_terminal_group_signal() => {
                        forward_signal(pid, received.name());
                    }
                    Some(_) => {}
                    None => return join_child_wait(wait.await),
                },
            }
        }
    }
    #[cfg(not(unix))]
    {
        wait_without_signal_forwarding(child).await
    }
}

#[cfg(unix)]
fn shared_foreground_terminal_group(child_pid: u32) -> bool {
    let Some((launcher_group, launcher_foreground)) = process_group_info(std::process::id()) else {
        return false;
    };
    let Some((child_group, _)) = process_group_info(child_pid) else {
        return false;
    };
    launcher_group > 0 && launcher_group == launcher_foreground && child_group == launcher_group
}

#[cfg(unix)]
fn process_group_info(pid: u32) -> Option<(i32, i32)> {
    let output = Command::new("ps")
        .args(["-o", "pgid=,tpgid=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut fields = stdout.split_whitespace();
    Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
}

#[cfg(not(unix))]
async fn wait_without_signal_forwarding(child: Child) -> Result<ExitStatus, LauncherError> {
    let result = tokio::task::spawn_blocking(move || {
        let mut child = child;
        child.wait()
    })
    .await
    .map_err(|error| LauncherError::message(format!("Codex wait task failed: {error}")))?;
    result.map_err(|error| LauncherError::message(format!("could not wait for Codex: {error}")))
}

#[cfg(unix)]
fn join_child_wait(
    result: Result<Result<ExitStatus, io::Error>, tokio::task::JoinError>,
) -> Result<ExitStatus, LauncherError> {
    result
        .map_err(|error| LauncherError::message(format!("Codex wait task failed: {error}")))?
        .map_err(|error| LauncherError::message(format!("could not wait for Codex: {error}")))
}

#[cfg(unix)]
fn forward_signal(pid: u32, signal_name: &str) {
    let pid = pid.to_string();
    let argument = format!("-{signal_name}");
    let mut command = if Path::new("/bin/kill").is_file() {
        Command::new("/bin/kill")
    } else {
        Command::new("kill")
    };
    let result = command
        .args([argument, pid])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if result.as_ref().is_err() || result.as_ref().is_ok_and(|status| !status.success()) {
        eprintln!("CodeGotchi warning: could not forward SIG{signal_name} to Codex");
    }
}

#[cfg(unix)]
fn numeric_exit_status(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

#[cfg(not(unix))]
fn numeric_exit_status(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{
        InheritedSignalSource, LaunchRequest, LauncherError, TerminalAttemptError, UiMode,
        execute_ui_route, parse_launch_request,
    };
    use crate::terminal::TerminalThemePreset;
    use std::ffi::OsString;
    #[cfg(target_os = "macos")]
    use std::path::PathBuf;

    use crate::terminal::{
        TerminalSessionError, TerminalSessionSignal, terminal_session_signal_channel,
    };

    fn os(arguments: &[&str]) -> Vec<OsString> {
        arguments.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_terminal_ui_before_separator_and_preserves_codex_arguments() {
        let parsed = parse_launch_request(os(&[
            "--ui", "terminal", "--", "codex", "--model", "gpt-5.6",
        ]))
        .unwrap();

        assert_eq!(
            parsed,
            LaunchRequest {
                ui_mode: UiMode::Terminal,
                terminal_theme: TerminalThemePreset::Auto,
                trailing_codex_arguments: os(&["--model", "gpt-5.6"]),
            }
        );
    }

    #[test]
    fn defaults_to_auto_without_a_pre_separator_ui_option() {
        let parsed = parse_launch_request(os(&["--", "codex", "--search"])).unwrap();

        assert_eq!(parsed.ui_mode, UiMode::Auto);
        assert_eq!(parsed.terminal_theme, TerminalThemePreset::Auto);
        assert_eq!(parsed.trailing_codex_arguments, os(&["--search"]));
    }

    #[test]
    fn preserves_codex_ui_arguments_after_the_separator() {
        let parsed = parse_launch_request(os(&["--", "codex", "--ui", "browser"])).unwrap();

        assert_eq!(parsed.ui_mode, UiMode::Auto);
        assert_eq!(parsed.trailing_codex_arguments, os(&["--ui", "browser"]));

        let equals_form = parse_launch_request(os(&["--", "codex", "--ui=browser"])).unwrap();
        assert_eq!(equals_form.ui_mode, UiMode::Auto);
        assert_eq!(equals_form.trailing_codex_arguments, os(&["--ui=browser"]));
    }

    #[test]
    fn parses_each_explicit_ui_mode() {
        for (value, expected) in [
            ("auto", UiMode::Auto),
            ("terminal", UiMode::Terminal),
            ("browser", UiMode::Browser),
            ("both", UiMode::Both),
        ] {
            let parsed = parse_launch_request(os(&["--ui", value, "--", "codex"])).unwrap();
            assert_eq!(parsed.ui_mode, expected, "--ui {value}");
        }
    }

    #[test]
    fn parses_equals_form_before_separator() {
        let parsed = parse_launch_request(os(&["--ui=browser", "--", "codex"])).unwrap();

        assert_eq!(parsed.ui_mode, UiMode::Browser);
        assert!(parsed.trailing_codex_arguments.is_empty());
    }

    #[test]
    fn parses_terminal_theme_before_separator_and_keeps_ui_semantics() {
        for (option, expected) in [
            ("auto", TerminalThemePreset::Auto),
            ("mono", TerminalThemePreset::Mono),
            ("soft-green", TerminalThemePreset::SoftGreen),
            ("amber", TerminalThemePreset::Amber),
            ("night", TerminalThemePreset::Night),
        ] {
            let parsed = parse_launch_request(os(&[
                "--ui",
                "both",
                "--terminal-theme",
                option,
                "--",
                "codex",
                "--ui",
                "browser",
            ]))
            .unwrap_or_else(|error| panic!("--terminal-theme {option}: {error}"));
            assert_eq!(parsed.ui_mode, UiMode::Both);
            assert_eq!(parsed.terminal_theme, expected);
            assert_eq!(parsed.trailing_codex_arguments, os(&["--ui", "browser"]));

            let equals = parse_launch_request([
                OsString::from(format!("--terminal-theme={option}")),
                OsString::from("--"),
                OsString::from("codex"),
            ])
            .unwrap_or_else(|error| panic!("--terminal-theme={option}: {error}"));
            assert_eq!(equals.terminal_theme, expected);
        }
    }

    #[test]
    fn rejects_duplicate_pre_separator_ui_options() {
        let error = parse_launch_request(os(&["--ui", "terminal", "--ui=browser", "--", "codex"]))
            .unwrap_err()
            .to_string();

        assert!(error.contains("duplicate `--ui`"), "{error}");
        assert!(error.contains("codegotchi run [--ui"), "{error}");
    }

    #[test]
    fn rejects_unknown_ui_values() {
        let error = parse_launch_request(os(&["--ui", "desktop", "--", "codex"]))
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("unsupported `--ui` value `desktop`"),
            "{error}"
        );
        assert!(error.contains("auto|terminal|browser|both"), "{error}");
    }

    #[test]
    fn rejects_unknown_missing_and_duplicate_terminal_theme_options() {
        let unknown = parse_launch_request(os(&["--terminal-theme", "violet", "--", "codex"]))
            .unwrap_err()
            .to_string();
        assert!(
            unknown.contains("unsupported `--terminal-theme` value `violet`"),
            "{unknown}"
        );

        for arguments in [
            os(&["--terminal-theme", "--", "codex"]),
            os(&["--terminal-theme=", "--", "codex"]),
        ] {
            let error = parse_launch_request(arguments).unwrap_err().to_string();
            assert!(
                error.contains("`--terminal-theme` requires one of"),
                "{error}"
            );
        }

        let duplicate = parse_launch_request(os(&[
            "--terminal-theme",
            "mono",
            "--terminal-theme=night",
            "--",
            "codex",
        ]))
        .unwrap_err()
        .to_string();
        assert!(
            duplicate.contains("duplicate `--terminal-theme`"),
            "{duplicate}"
        );
    }

    #[test]
    fn rejects_missing_ui_values() {
        for arguments in [
            os(&["--ui", "--", "codex"]),
            os(&["--ui", "--ui", "terminal", "--", "codex"]),
            os(&["--ui=", "--", "codex"]),
        ] {
            let error = parse_launch_request(arguments).unwrap_err().to_string();
            assert!(error.contains("`--ui` requires one of"), "{error}");
        }
    }

    #[test]
    fn rejects_unexpected_pre_separator_tokens() {
        let error = parse_launch_request(os(&["terminal", "--", "codex"]))
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("unexpected pre-separator argument `terminal`"),
            "{error}"
        );
        assert!(error.contains("codegotchi run [--ui"), "{error}");
    }

    #[test]
    fn rejects_missing_separator() {
        let error = parse_launch_request(os(&["codex"]))
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("requires the exact separator `--`"),
            "{error}"
        );
        assert!(error.contains("codegotchi run [--ui"), "{error}");
    }

    #[test]
    fn rejects_missing_or_non_codex_agent() {
        for arguments in [os(&["--"]), os(&["--", "claude"])] {
            let error = parse_launch_request(arguments).unwrap_err().to_string();

            assert!(error.contains("unsupported agent"), "{error}");
            assert!(error.contains("codegotchi run [--ui"), "{error}");
        }
    }

    #[test]
    fn rejects_each_generated_profile_conflict_in_codex_arguments() {
        for conflict in ["-p", "-pfoo", "--profile", "--profile=foo"] {
            let error = parse_launch_request(os(&["--", "codex", conflict]))
                .unwrap_err()
                .to_string();

            assert!(
                error.contains(&format!("Codex argument `{conflict}` conflicts")),
                "{error}"
            );
        }
    }

    #[test]
    fn parsing_does_not_mutate_the_argument_vector() {
        let arguments = os(&["--ui", "both", "--", "codex", "--ui=browser"]);
        let before = arguments.clone();

        let _ = parse_launch_request(arguments.clone()).unwrap();

        assert_eq!(arguments, before);
    }

    #[tokio::test]
    async fn ui_routes_launch_browser_and_spawn_exactly_once_per_mode() {
        for (mode, browser_calls, inherited_calls, terminal_calls) in [
            (UiMode::Browser, 1, 1, 0),
            (UiMode::Terminal, 0, 0, 1),
            (UiMode::Both, 1, 0, 1),
            (UiMode::Auto, 1, 1, 1),
        ] {
            let mut browser_calls_seen = 0;
            let mut inherited_calls_seen = 0;
            let mut terminal_calls_seen = 0;
            let (sender, receiver) = terminal_session_signal_channel(1);
            drop(sender);

            let result = execute_ui_route(
                mode,
                InheritedSignalSource::Unavailable,
                || browser_calls_seen += 1,
                |_signals: InheritedSignalSource| {
                    inherited_calls_seen += 1;
                    async { Ok::<_, LauncherError>(17) }
                },
                || {
                    terminal_calls_seen += 1;
                    async move {
                        if mode == UiMode::Auto {
                            Err::<i32, _>(TerminalAttemptError::Initialization {
                                error: TerminalSessionError::SpawnUnavailable,
                                signals: receiver,
                            })
                        } else {
                            Ok::<i32, TerminalAttemptError>(29)
                        }
                    }
                },
            )
            .await
            .expect("route completes");

            assert_eq!(
                result,
                if matches!(mode, UiMode::Browser | UiMode::Auto) {
                    17
                } else {
                    29
                },
                "{mode:?}"
            );
            assert_eq!(browser_calls_seen, browser_calls, "{mode:?}");
            assert_eq!(inherited_calls_seen, inherited_calls, "{mode:?}");
            assert_eq!(terminal_calls_seen, terminal_calls, "{mode:?}");
        }
    }

    #[tokio::test]
    async fn auto_falls_back_only_from_pre_spawn_initialization() {
        let (sender, receiver) = terminal_session_signal_channel(1);
        drop(sender);
        let mut inherited_calls = 0;
        let mut terminal_calls = 0;
        let result = execute_ui_route(
            UiMode::Auto,
            InheritedSignalSource::Unavailable,
            || {},
            |_signals: InheritedSignalSource| {
                inherited_calls += 1;
                async { Ok::<_, LauncherError>(23) }
            },
            || {
                terminal_calls += 1;
                async {
                    Err::<i32, _>(TerminalAttemptError::Initialization {
                        error: TerminalSessionError::SpawnUnavailable,
                        signals: receiver,
                    })
                }
            },
        )
        .await
        .expect("initialization fallback succeeds");

        assert_eq!(result, 23);
        assert_eq!(terminal_calls, 1);
        assert_eq!(inherited_calls, 1);
    }

    #[tokio::test]
    async fn auto_does_not_fall_back_after_terminal_session_error() {
        let mut inherited_calls = 0;
        let mut terminal_calls = 0;
        let result = execute_ui_route(
            UiMode::Auto,
            InheritedSignalSource::Unavailable,
            || {},
            |_signals: InheritedSignalSource| {
                inherited_calls += 1;
                async { Ok::<_, LauncherError>(23) }
            },
            || {
                terminal_calls += 1;
                async {
                    Err::<i32, _>(TerminalAttemptError::Session(
                        TerminalSessionError::SpawnUnavailable,
                    ))
                }
            },
        )
        .await
        .expect_err("post-spawn session errors surface");

        assert!(matches!(result, super::RouteError::Terminal(_)));
        assert_eq!(terminal_calls, 1);
        assert_eq!(inherited_calls, 0);
    }

    #[tokio::test]
    async fn terminal_signal_source_maps_each_bounded_session_signal() {
        let (sender, receiver) = terminal_session_signal_channel(3);
        sender.send(TerminalSessionSignal::Interrupt).await.unwrap();
        sender.send(TerminalSessionSignal::Terminate).await.unwrap();
        sender
            .send(TerminalSessionSignal::WindowChange)
            .await
            .unwrap();
        let mut source = InheritedSignalSource::Terminal(receiver);

        assert_eq!(source.next().await, Some(super::LauncherSignal::Interrupt));
        assert_eq!(source.next().await, Some(super::LauncherSignal::Terminate));
        assert_eq!(
            source.next().await,
            Some(super::LauncherSignal::WindowChange)
        );
    }

    #[test]
    fn terminal_exit_status_preserves_portable_pty_signal_codes() {
        assert_eq!(
            super::numeric_terminal_exit_status(portable_pty::ExitStatus::with_signal("Interrupt")),
            130
        );
        assert_eq!(
            super::numeric_terminal_exit_status(portable_pty::ExitStatus::with_signal(
                "Terminated"
            )),
            143
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_selects_open_with_the_exact_url_argument() {
        let url = "http://127.0.0.1:4173/#token=test";
        let (program, arguments) = super::native_browser_command(url);

        assert_eq!(program, PathBuf::from("/usr/bin/open"));
        assert_eq!(arguments, vec![OsString::from(url)]);
    }
}

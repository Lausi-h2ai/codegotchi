use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(any(
    target_os = "android",
    target_os = "freebsd",
    target_os = "haiku",
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc")),
))]
use nix::sys::wait::{Id, WaitPidFlag, WaitStatus, waitid};
#[cfg(unix)]
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

#[cfg(unix)]
use filedescriptor::{FileDescriptor, POLLERR, POLLHUP, POLLIN, poll, pollfd};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};

use crate::CodexInvocation;

pub type PtyReader = Box<dyn Read + Send>;
pub type PtyWriter = Box<dyn Write + Send>;

#[cfg(unix)]
const PTY_READER_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[cfg(unix)]
struct RawFdSource(RawFd);

#[cfg(unix)]
impl AsRawFd for RawFdSource {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

/// A PTY reader that checks readiness with a bounded timeout before reading.
///
/// The portable-pty reader is deliberately a blocking `Read`, which means a
/// cancellation flag alone cannot wake it if a descendant still holds the
/// slave side open. Polling the cloned master descriptor gives the reader a
/// bounded cancellation checkpoint without changing the master descriptor's
/// file status flags (and therefore without making PTY writes nonblocking).
#[cfg(unix)]
struct InterruptiblePtyReader {
    fd: FileDescriptor,
}

#[cfg(unix)]
fn poll_error_to_io(error: filedescriptor::Error) -> io::Error {
    match error {
        filedescriptor::Error::Poll(source) => source,
        error => io::Error::other(error),
    }
}

#[cfg(unix)]
impl Read for InterruptiblePtyReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut descriptor = [pollfd {
            fd: self.fd.as_raw_fd(),
            events: POLLIN | POLLHUP | POLLERR,
            revents: 0,
        }];
        let ready =
            poll(&mut descriptor, Some(PTY_READER_POLL_INTERVAL)).map_err(poll_error_to_io)?;
        if ready == 0 {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }

        match self.fd.read(bytes) {
            Err(error) if error.raw_os_error() == Some(nix::libc::EIO) => Ok(0),
            result => result,
        }
    }
}

#[derive(Debug, Error)]
pub enum PtyCodexError {
    #[error("could not initialize the PTY child reaper: {source}")]
    Reaper {
        #[source]
        source: io::Error,
    },
    #[error("could not open PTY at {rows} rows x {cols} columns: {source}")]
    Open {
        rows: u16,
        cols: u16,
        #[source]
        source: io::Error,
    },
    #[error("could not determine Codex working directory: {source}")]
    CurrentDirectory {
        #[source]
        source: io::Error,
    },
    #[error("could not spawn Codex program {program}: {source}")]
    Spawn {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not clone Codex PTY reader: {source}")]
    Reader {
        #[source]
        source: io::Error,
    },
    #[error("could not take Codex PTY writer: {source}")]
    Writer {
        #[source]
        source: io::Error,
    },
    #[error("could not resize Codex PTY to {rows} rows x {cols} columns: {source}")]
    Resize {
        rows: u16,
        cols: u16,
        #[source]
        source: io::Error,
    },
    #[error("could not wait for Codex child: {source}")]
    Wait {
        #[source]
        source: io::Error,
    },
    #[error("could not terminate Codex child: {source}")]
    Kill {
        #[source]
        source: io::Error,
    },
    #[error("could not send {signal} to Codex process group: {source}")]
    Signal {
        signal: &'static str,
        #[source]
        source: io::Error,
    },
}

type ReapChild = Box<dyn Child + Send + Sync>;

static PTY_REAPER: std::sync::OnceLock<Sender<ReapChild>> = std::sync::OnceLock::new();

fn pty_reaper_sender() -> io::Result<Sender<ReapChild>> {
    if let Some(sender) = PTY_REAPER.get() {
        return Ok(sender.clone());
    }

    let (sender, receiver) = mpsc::channel::<ReapChild>();
    thread::Builder::new()
        .name("codegotchi-pty-reaper".to_owned())
        .spawn(move || {
            for mut child in receiver {
                let _ = child.wait();
            }
        })?;
    if PTY_REAPER.set(sender.clone()).is_err() {
        return Ok(PTY_REAPER
            .get()
            .expect("PTY reaper sender installed by another thread")
            .clone());
    }
    Ok(sender)
}

/// A signal understood by the narrow PTY process-control seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PtySignal {
    Interrupt,
    Terminate,
    Kill,
}

#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
struct ProcessGroupState {
    identity: Option<i32>,
    reaped: bool,
    cleanup_consumed: bool,
}

#[cfg(unix)]
impl ProcessGroupState {
    #[cfg(test)]
    fn new(identity: i32) -> Self {
        Self::from_option(Some(identity))
    }

    fn from_option(identity: Option<i32>) -> Self {
        Self {
            identity,
            reaped: false,
            cleanup_consumed: false,
        }
    }

    fn identity(&self) -> Option<i32> {
        self.identity
    }

    #[cfg(test)]
    fn is_reaped(&self) -> bool {
        self.reaped
    }

    fn mark_reaped(&mut self) {
        self.reaped = true;
        #[cfg(not(any(
            target_os = "android",
            target_os = "freebsd",
            target_os = "haiku",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "uclibc")),
        )))]
        {
            // Platforms without waitid(WNOWAIT) cannot safely retain a PGID
            // across the portable-pty reap; disarm it before any post-reap
            // cleanup path can run.
            self.cleanup_consumed = true;
            self.identity = None;
        }
    }

    fn mark_gone(&mut self) {
        self.cleanup_consumed = true;
        self.identity = None;
    }

    fn mark_direct_kill(&mut self) {
        self.cleanup_consumed = true;
        self.identity = None;
    }

    fn cleanup_consumed(&self) -> bool {
        self.cleanup_consumed
    }

    fn take_for_descendant_cleanup(&mut self) -> Option<i32> {
        if self.cleanup_consumed {
            return None;
        }
        self.cleanup_consumed = true;
        self.identity.take()
    }
}

impl PtySignal {
    #[cfg(unix)]
    const fn unix(self) -> Signal {
        match self {
            Self::Interrupt => Signal::SIGINT,
            Self::Terminate => Signal::SIGTERM,
            Self::Kill => Signal::SIGKILL,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "SIGINT",
            Self::Terminate => "SIGTERM",
            Self::Kill => "SIGKILL",
        }
    }
}

/// A Codex process attached directly to a native PTY.
///
/// The caller owns the profile guard and must keep it alive through invocation
/// construction and this method's return, as required by the profile
/// lifecycle. This type deliberately does not create or validate profiles.
pub struct PtyCodexChild {
    master: Box<dyn MasterPty + Send>,
    child: Option<Box<dyn Child + Send + Sync>>,
    reaper: Sender<ReapChild>,
    reaped_status: Option<portable_pty::ExitStatus>,
    cleanup_error: Option<PtyCodexError>,
    #[cfg(unix)]
    process_group: ProcessGroupState,
}

impl PtyCodexChild {
    /// Opens a PTY with the requested text dimensions and executes the exact
    /// invocation program directly on its slave side.
    pub fn spawn(
        invocation: &CodexInvocation,
        rows: u16,
        cols: u16,
    ) -> Result<Self, PtyCodexError> {
        let current_directory =
            std::env::current_dir().map_err(|source| PtyCodexError::CurrentDirectory { source })?;
        let reaper = pty_reaper_sender().map_err(|source| PtyCodexError::Reaper { source })?;
        let mut command = CommandBuilder::new(&invocation.program);
        command.cwd(current_directory);
        command.args(&invocation.arguments);
        for (key, value) in &invocation.environment {
            command.env(key, value);
        }

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|source| PtyCodexError::Open {
                rows,
                cols,
                source: io::Error::other(source),
            })?;
        let slave = pair.slave;
        let master = pair.master;
        let child = slave
            .spawn_command(command)
            .map_err(|source| PtyCodexError::Spawn {
                program: invocation.program.clone(),
                source: io::Error::other(source),
            })?;
        drop(slave);

        #[cfg(unix)]
        let process_group = ProcessGroupState::from_option(
            master
                .process_group_leader()
                .or_else(|| child.process_id().map(|pid| pid as i32)),
        );

        Ok(Self {
            master,
            child: Some(child),
            reaper,
            reaped_status: None,
            cleanup_error: None,
            #[cfg(unix)]
            process_group,
        })
    }

    /// Returns an owned blocking reader that can be moved to a reader thread.
    pub fn reader(&self) -> Result<PtyReader, PtyCodexError> {
        self.master
            .try_clone_reader()
            .map_err(|source| PtyCodexError::Reader {
                source: io::Error::other(source),
            })
    }

    /// Returns the session-owned reader with bounded readiness checkpoints.
    ///
    /// The public [`Self::reader`] API retains portable-pty's blocking
    /// semantics. The interactive session uses this narrower seam because its
    /// cancellation flag must be able to wake a reader whose slave holder
    /// survives group teardown.
    pub(crate) fn interruptible_reader(&self) -> Result<PtyReader, PtyCodexError> {
        #[cfg(unix)]
        if let Some(raw_fd) = self.master.as_raw_fd() {
            let source = RawFdSource(raw_fd);
            let fd = FileDescriptor::dup(&source).map_err(|source| PtyCodexError::Reader {
                source: io::Error::other(source.to_string()),
            })?;
            return Ok(Box::new(InterruptiblePtyReader { fd }));
        }

        self.reader()
    }

    /// Alias for [`Self::reader`] that mirrors portable-pty's API.
    pub fn try_clone_reader(&self) -> Result<PtyReader, PtyCodexError> {
        self.reader()
    }

    /// Takes the single writable input handle for this PTY.
    pub fn writer(&self) -> Result<PtyWriter, PtyCodexError> {
        self.master
            .take_writer()
            .map_err(|source| PtyCodexError::Writer {
                source: io::Error::other(source),
            })
    }

    /// Alias for [`Self::writer`] that mirrors portable-pty's API.
    pub fn take_writer(&self) -> Result<PtyWriter, PtyCodexError> {
        self.writer()
    }

    /// Informs the child PTY of a new text size. Pixel dimensions stay zero.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), PtyCodexError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|source| PtyCodexError::Resize {
                rows,
                cols,
                source: io::Error::other(source),
            })
    }

    /// Waits for the child and returns portable-pty's numeric exit status.
    pub fn wait(&mut self) -> Result<portable_pty::ExitStatus, PtyCodexError> {
        if let Some(status) = &self.reaped_status {
            return Ok(status.clone());
        }
        self.observe_exit_before_reap(true)?;
        let child = self.child.as_mut().ok_or_else(|| PtyCodexError::Wait {
            source: io::Error::new(io::ErrorKind::NotFound, "Codex child handle unavailable"),
        })?;
        let status = child
            .wait()
            .map_err(|source| PtyCodexError::Wait { source })?;
        self.mark_reaped(status.clone());
        Ok(status)
    }

    /// Polls the child without blocking. A returned status has been reaped by
    /// the underlying process implementation.
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>, PtyCodexError> {
        if let Some(status) = &self.reaped_status {
            return Ok(Some(status.clone()));
        }
        if !self.observe_exit_before_reap(false)? {
            return Ok(None);
        }
        let child = self.child.as_mut().ok_or_else(|| PtyCodexError::Wait {
            source: io::Error::new(io::ErrorKind::NotFound, "Codex child handle unavailable"),
        })?;
        let status = child
            .try_wait()
            .map_err(|source| PtyCodexError::Wait { source })?;
        if let Some(status) = &status {
            self.mark_reaped(status.clone());
        }
        Ok(status)
    }

    #[cfg(any(
        target_os = "android",
        target_os = "freebsd",
        target_os = "haiku",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "uclibc")),
    ))]
    fn observe_exit_before_reap(&mut self, blocking: bool) -> Result<bool, PtyCodexError> {
        let Some(pid) = self.process_id() else {
            return Ok(true);
        };
        let mut flags = WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT;
        if !blocking {
            flags |= WaitPidFlag::WNOHANG;
        }
        let status = waitid(Id::Pid(Pid::from_raw(pid as i32)), flags).map_err(|source| {
            PtyCodexError::Wait {
                source: io::Error::from_raw_os_error(source as i32),
            }
        })?;
        if matches!(status, WaitStatus::StillAlive) {
            return Ok(false);
        }

        // Keep the exited leader as a zombie while this one-shot cleanup
        // consumes its cached PGID. The zombie reserves the identity against
        // reuse until portable-pty performs the actual reap below.
        if self.cleanup_error.is_none() {
            self.cleanup_error = self.cleanup_descendant_group().err();
        }
        Ok(true)
    }

    #[cfg(not(any(
        target_os = "android",
        target_os = "freebsd",
        target_os = "haiku",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "uclibc")),
    )))]
    fn observe_exit_before_reap(&mut self, _blocking: bool) -> Result<bool, PtyCodexError> {
        Ok(true)
    }

    pub(crate) fn take_cleanup_error(&mut self) -> Option<PtyCodexError> {
        self.cleanup_error.take()
    }

    /// Requests direct child termination for backends without a usable group
    /// identity. This is intentionally private: callers must use the group
    /// signal methods so a portable-pty kill cannot leave a cached PGID armed.
    fn kill_direct(&mut self) -> Result<(), PtyCodexError> {
        let result = {
            let child = self.child.as_mut().ok_or_else(|| PtyCodexError::Kill {
                source: io::Error::new(io::ErrorKind::NotFound, "Codex child handle unavailable"),
            })?;
            child
                .kill()
                .map_err(|source| PtyCodexError::Kill { source })
        };
        if result.is_ok() {
            #[cfg(unix)]
            self.process_group.mark_direct_kill();
        }
        result
    }

    /// Delivers SIGINT semantics to the complete PTY process group on Unix.
    /// Other platforms fall back to portable-pty's child termination API.
    pub fn interrupt(&mut self) -> Result<(), PtyCodexError> {
        self.signal(PtySignal::Interrupt)
    }

    /// Delivers SIGTERM semantics to the complete PTY process group on Unix.
    /// Other platforms fall back to portable-pty's child termination API.
    pub fn terminate(&mut self) -> Result<(), PtyCodexError> {
        self.signal(PtySignal::Terminate)
    }

    /// Delivers SIGKILL semantics to the complete PTY process group on Unix.
    /// Other platforms fall back to portable-pty's child termination API.
    pub(crate) fn kill_group(&mut self) -> Result<(), PtyCodexError> {
        self.cleanup_descendant_group()
    }

    /// Delivers one SIGKILL to the cached process group while the direct child
    /// is still owned. On waitid-capable Unix targets, callers invoke this
    /// while an exited leader remains a zombie, so the retained identity cannot
    /// be reused before the syscall. Taking the identity before the syscall
    /// prevents a later Drop or reaped-child path from signalling twice.
    pub(crate) fn cleanup_descendant_group(&mut self) -> Result<(), PtyCodexError> {
        #[cfg(unix)]
        {
            if self.process_group.cleanup_consumed() {
                return Ok(());
            }
            let Some(process_group) = self.process_group.take_for_descendant_cleanup() else {
                return self.kill_direct();
            };
            killpg(Pid::from_raw(process_group), PtySignal::Kill.unix()).map_err(|error| {
                PtyCodexError::Signal {
                    signal: PtySignal::Kill.name(),
                    source: io::Error::from_raw_os_error(error as i32),
                }
            })
        }

        #[cfg(not(unix))]
        {
            self.kill_direct()
        }
    }

    fn signal(&mut self, signal: PtySignal) -> Result<(), PtyCodexError> {
        if self.reaped_status.is_some() {
            return Ok(());
        }

        #[cfg(unix)]
        {
            if self.process_group.cleanup_consumed() {
                return Ok(());
            }
            let Some(process_group) = self.process_group.identity() else {
                let Some(pid) = self.child.as_ref().and_then(|child| child.process_id()) else {
                    return Err(PtyCodexError::Signal {
                        signal: signal.name(),
                        source: io::Error::new(
                            io::ErrorKind::NotFound,
                            "Codex child has no process identifier",
                        ),
                    });
                };
                return kill(Pid::from_raw(pid as i32), Some(signal.unix())).map_err(|error| {
                    if error == nix::errno::Errno::ESRCH {
                        self.process_group.mark_gone();
                    }
                    PtyCodexError::Signal {
                        signal: signal.name(),
                        source: io::Error::from_raw_os_error(error as i32),
                    }
                });
            };
            let process_group = Pid::from_raw(process_group);
            killpg(process_group, signal.unix()).map_err(|error| {
                if error == nix::errno::Errno::ESRCH {
                    self.process_group.mark_gone();
                }
                PtyCodexError::Signal {
                    signal: signal.name(),
                    source: io::Error::from_raw_os_error(error as i32),
                }
            })
        }

        #[cfg(not(unix))]
        {
            self.kill_direct().map_err(|source| PtyCodexError::Signal {
                signal: signal.name(),
                source: match source {
                    PtyCodexError::Kill { source } => source,
                    _ => io::Error::other("child kill failed"),
                },
            })
        }
    }

    fn mark_reaped(&mut self, status: portable_pty::ExitStatus) {
        self.reaped_status = Some(status);
        #[cfg(unix)]
        self.process_group.mark_reaped();
    }

    /// Returns the native child process identifier when the PTY backend has
    /// one. This is metadata only; the session retains process ownership here.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().and_then(|child| child.process_id())
    }

    pub(crate) fn is_reaped(&self) -> bool {
        self.reaped_status.is_some()
    }
}

impl Drop for PtyCodexChild {
    fn drop(&mut self) {
        const DROP_REAP_GRACE: Duration = Duration::from_millis(250);
        const DROP_POLL_INTERVAL: Duration = Duration::from_millis(5);

        // A dropped async session has no opportunity to await cleanup. Signal
        // the still-live group once; cleanup_descendant_group consumes the
        // cached identity so no later Drop can target a reused PGID.
        if self.reaped_status.is_none() {
            let _ = self.cleanup_descendant_group();
        }

        let Some(mut child) = self.child.take() else {
            return;
        };
        if self.reaped_status.is_some() {
            return;
        }

        let deadline = Instant::now() + DROP_REAP_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.reaped_status = Some(status);
                    #[cfg(unix)]
                    self.process_group.mark_reaped();
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(DROP_POLL_INTERVAL);
                }
                Ok(None) | Err(_) => break,
            }
        }

        // Drop cannot block indefinitely. Transfer the child handle to the
        // process-wide dedicated reaper; its unbounded queue send is
        // nonblocking, and ownership remains with the worker until wait()
        // completes.
        if let Err(error) = self.reaper.send(child) {
            // The worker should live for the process lifetime. If an
            // unexpected channel teardown races this Drop, retaining the
            // already-killed handle is the only bounded-latency fallback
            // available from Drop; normal startup initializes the worker and
            // therefore keeps eventual reaping owned.
            std::mem::forget(error.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::ProcessGroupState;

    #[cfg(unix)]
    #[test]
    fn poll_errors_preserve_os_error_kind_and_code() {
        for (code, kind) in [
            (nix::libc::EINTR, io::ErrorKind::Interrupted),
            (nix::libc::EINVAL, io::ErrorKind::InvalidInput),
        ] {
            let error = super::poll_error_to_io(filedescriptor::Error::Poll(
                io::Error::from_raw_os_error(code),
            ));

            assert_eq!(error.kind(), kind);
            assert_eq!(error.raw_os_error(), Some(code));
        }
    }

    #[test]
    fn process_group_state_retains_identity_for_one_immediate_cleanup_after_reap() {
        let mut state = ProcessGroupState::new(42);

        assert_eq!(state.identity(), Some(42));
        state.mark_reaped();

        assert!(state.is_reaped());
        #[cfg(any(
            target_os = "android",
            target_os = "freebsd",
            target_os = "haiku",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "uclibc")),
        ))]
        assert_eq!(state.identity(), Some(42));
        #[cfg(not(any(
            target_os = "android",
            target_os = "freebsd",
            target_os = "haiku",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "uclibc")),
        )))]
        assert_eq!(state.identity(), None);
        #[cfg(any(
            target_os = "android",
            target_os = "freebsd",
            target_os = "haiku",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "uclibc")),
        ))]
        assert_eq!(state.take_for_descendant_cleanup(), Some(42));
        #[cfg(not(any(
            target_os = "android",
            target_os = "freebsd",
            target_os = "haiku",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "uclibc")),
        )))]
        assert_eq!(state.take_for_descendant_cleanup(), None);
        assert_eq!(state.identity(), None);
    }

    #[test]
    fn process_group_cleanup_consumes_identity_once_before_reap() {
        let mut state = ProcessGroupState::new(42);

        assert_eq!(state.take_for_descendant_cleanup(), Some(42));
        assert_eq!(state.take_for_descendant_cleanup(), None);
        assert!(!state.is_reaped());

        state.mark_reaped();
        assert_eq!(state.identity(), None);
    }

    #[test]
    fn direct_kill_disarms_cached_group_identity_before_reap() {
        let mut state = ProcessGroupState::new(42);

        state.mark_direct_kill();

        assert!(state.cleanup_consumed());
        assert_eq!(state.identity(), None);
    }
}

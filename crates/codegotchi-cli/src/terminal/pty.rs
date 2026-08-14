use std::io::{self, Read, Write};
use std::path::PathBuf;

#[cfg(unix)]
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

use crate::CodexInvocation;

pub type PtyReader = Box<dyn Read + Send>;
pub type PtyWriter = Box<dyn Write + Send>;

#[derive(Debug, Error)]
pub enum PtyCodexError {
    #[error("could not open PTY at {rows} rows x {cols} columns: {source}")]
    Open {
        rows: u16,
        cols: u16,
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

/// A signal understood by the narrow PTY process-control seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PtySignal {
    Interrupt,
    Terminate,
    Kill,
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
    child: Box<dyn Child + Send + Sync>,
    #[cfg(unix)]
    process_group: Option<i32>,
}

impl PtyCodexChild {
    /// Opens a PTY with the requested text dimensions and executes the exact
    /// invocation program directly on its slave side.
    pub fn spawn(
        invocation: &CodexInvocation,
        rows: u16,
        cols: u16,
    ) -> Result<Self, PtyCodexError> {
        let mut command = CommandBuilder::new(&invocation.program);
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
        let process_group = master
            .process_group_leader()
            .or_else(|| child.process_id().map(|pid| pid as i32));

        Ok(Self {
            master,
            child,
            #[cfg(unix)]
            process_group,
        })
    }

    /// Returns an owned reader that can be moved to a blocking reader thread.
    pub fn reader(&self) -> Result<PtyReader, PtyCodexError> {
        self.master
            .try_clone_reader()
            .map_err(|source| PtyCodexError::Reader {
                source: io::Error::other(source),
            })
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
        self.child
            .wait()
            .map_err(|source| PtyCodexError::Wait { source })
    }

    /// Polls the child without blocking. A returned status has been reaped by
    /// the underlying process implementation.
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>, PtyCodexError> {
        self.child
            .try_wait()
            .map_err(|source| PtyCodexError::Wait { source })
    }

    /// Requests cooperative child termination through portable-pty's native
    /// process-control implementation.
    pub fn kill(&mut self) -> Result<(), PtyCodexError> {
        self.child
            .kill()
            .map_err(|source| PtyCodexError::Kill { source })
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
        self.signal(PtySignal::Kill)
    }

    fn signal(&mut self, signal: PtySignal) -> Result<(), PtyCodexError> {
        #[cfg(unix)]
        {
            let Some(process_group) = self.process_group else {
                let Some(pid) = self.child.process_id() else {
                    return Err(PtyCodexError::Signal {
                        signal: signal.name(),
                        source: io::Error::new(
                            io::ErrorKind::NotFound,
                            "Codex child has no process identifier",
                        ),
                    });
                };
                return kill(Pid::from_raw(pid as i32), Some(signal.unix())).map_err(|error| {
                    PtyCodexError::Signal {
                        signal: signal.name(),
                        source: io::Error::from_raw_os_error(error as i32),
                    }
                });
            };
            let process_group = Pid::from_raw(process_group);
            killpg(process_group, signal.unix()).map_err(|error| PtyCodexError::Signal {
                signal: signal.name(),
                source: io::Error::from_raw_os_error(error as i32),
            })
        }

        #[cfg(not(unix))]
        {
            self.child.kill().map_err(|source| PtyCodexError::Signal {
                signal: signal.name(),
                source,
            })
        }
    }

    /// Returns the native child process identifier when the PTY backend has
    /// one. This is metadata only; the session retains process ownership here.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }
}

impl Drop for PtyCodexChild {
    fn drop(&mut self) {
        // A dropped async session has no opportunity to await its cleanup
        // future. Kill the retained process group and reap the direct child
        // synchronously as a final liveness guard; normal session paths still
        // report cleanup errors before this fallback runs.
        let _ = self.kill_group();
        let _ = self.child.wait();
    }
}

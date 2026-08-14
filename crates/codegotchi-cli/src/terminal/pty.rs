use std::io::{self, Read, Write};
use std::path::PathBuf;

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
}

/// A Codex process attached directly to a native PTY.
///
/// The caller owns the profile guard and must keep it alive through invocation
/// construction and this method's return, as required by the profile
/// lifecycle. This type deliberately does not create or validate profiles.
pub struct PtyCodexChild {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
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

        Ok(Self { master, child })
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

    /// Returns the native child process identifier when the PTY backend has
    /// one. This is metadata only; the session retains process ownership here.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }
}

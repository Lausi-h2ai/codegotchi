use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use codegotchi_domain::{
    DefaultNeedProgressionStrategy, PetSimulation, SimulationSnapshot, SnapshotRestoreError,
    SystemClock,
};
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

pub const SQLITE_SCHEMA_VERSION: u16 = 1;
const DEFAULT_REPOSITORY_ID: &str = "default";

#[derive(Clone)]
pub struct SqliteStore {
    path: Arc<PathBuf>,
    repository_id: Arc<str>,
    connection: Arc<Mutex<Connection>>,
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("could not create SQLite parent directory: {0}")]
    ParentDirectory(#[source] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("authoritative SQLite connection lock is poisoned")]
    LockPoisoned,
    #[error("corrupt simulation snapshot: {0}")]
    CorruptSnapshot(String),
    #[error("unsupported persisted schema version {0}; expected {SQLITE_SCHEMA_VERSION}")]
    UnsupportedSchemaVersion(u16),
    #[error("invalid simulation snapshot: {0}")]
    InvalidSnapshot(#[source] SnapshotRestoreError),
    #[error("could not encode simulation snapshot: {0}")]
    Encode(#[source] serde_json::Error),
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        Self::open_for_repository(path, DEFAULT_REPOSITORY_ID)
    }

    pub fn open_for_repository(
        path: impl AsRef<Path>,
        repository_id: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(PersistenceError::ParentDirectory)?;
        }
        let connection = Connection::open(&path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS simulation_snapshots (
                 repository_id TEXT PRIMARY KEY NOT NULL,
                 schema_version INTEGER NOT NULL,
                 snapshot_json TEXT NOT NULL
             );",
        )?;
        Ok(Self {
            path: Arc::new(path),
            repository_id: Arc::from(repository_id.into()),
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    pub fn repository_id(&self) -> &str {
        &self.repository_id
    }

    pub fn load_or_initialize(
        &self,
        initial: SimulationSnapshot,
    ) -> Result<SimulationSnapshot, PersistenceError> {
        validate_snapshot(&initial)?;
        let encoded = serde_json::to_string(&initial).map_err(PersistenceError::Encode)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let existing = transaction
            .query_row(
                "SELECT schema_version, snapshot_json
                 FROM simulation_snapshots WHERE repository_id = ?1",
                params![self.repository_id.as_ref()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let row = if let Some(row) = existing {
            row
        } else {
            transaction.execute(
                "INSERT INTO simulation_snapshots
                 (repository_id, schema_version, snapshot_json) VALUES (?1, ?2, ?3)",
                params![
                    self.repository_id.as_ref(),
                    i64::from(initial.schema_version),
                    encoded,
                ],
            )?;
            (i64::from(initial.schema_version), encoded)
        };
        transaction.commit()?;
        decode_snapshot(row.0, &row.1)
    }

    pub fn load(&self) -> Result<Option<SimulationSnapshot>, PersistenceError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)?;
        let row = connection
            .query_row(
                "SELECT schema_version, snapshot_json
                 FROM simulation_snapshots WHERE repository_id = ?1",
                params![self.repository_id.as_ref()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(schema_version, snapshot_json)| decode_snapshot(schema_version, &snapshot_json))
            .transpose()
    }

    pub fn save(&self, snapshot: &SimulationSnapshot) -> Result<(), PersistenceError> {
        validate_snapshot(snapshot)?;
        let encoded = serde_json::to_string(snapshot).map_err(PersistenceError::Encode)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE simulation_snapshots
             SET schema_version = ?2, snapshot_json = ?3
             WHERE repository_id = ?1",
            params![
                self.repository_id.as_ref(),
                i64::from(snapshot.schema_version),
                encoded,
            ],
        )?;
        if updated == 0 {
            transaction.execute(
                "INSERT INTO simulation_snapshots
                 (repository_id, schema_version, snapshot_json) VALUES (?1, ?2, ?3)",
                params![
                    self.repository_id.as_ref(),
                    i64::from(snapshot.schema_version),
                    encoded,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn decode_snapshot(
    schema_version: i64,
    snapshot_json: &str,
) -> Result<SimulationSnapshot, PersistenceError> {
    let schema_version = u16::try_from(schema_version).map_err(|_| {
        PersistenceError::CorruptSnapshot("schema version is out of range".to_owned())
    })?;
    if schema_version != SQLITE_SCHEMA_VERSION {
        return Err(PersistenceError::UnsupportedSchemaVersion(schema_version));
    }
    let snapshot: SimulationSnapshot = serde_json::from_str(snapshot_json)
        .map_err(|error| PersistenceError::CorruptSnapshot(error.to_string()))?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot(snapshot: &SimulationSnapshot) -> Result<(), PersistenceError> {
    PetSimulation::from_snapshot(
        snapshot.clone(),
        SystemClock,
        DefaultNeedProgressionStrategy,
    )
    .map(|_| ())
    .map_err(|error| match error {
        SnapshotRestoreError::UnsupportedSchemaVersion(version) => {
            PersistenceError::UnsupportedSchemaVersion(version)
        }
        error => PersistenceError::InvalidSnapshot(error),
    })
}

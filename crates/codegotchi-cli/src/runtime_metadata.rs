use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::protocol::{RUNTIME_METADATA_SCHEMA_VERSION, RuntimeMetadataV1};
use thiserror::Error;

const MAX_METADATA_BYTES: u64 = 16 * 1024;

#[derive(Debug, Error)]
pub enum RuntimeMetadataError {
    #[error("could not open runtime metadata: {0}")]
    Open(#[source] std::io::Error),
    #[error("could not read runtime metadata: {0}")]
    Read(#[source] std::io::Error),
    #[error("could not write runtime metadata: {0}")]
    Write(#[source] std::io::Error),
    #[error("invalid runtime metadata JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("unsupported runtime metadata schema version {0}")]
    UnsupportedSchema(u16),
}

pub fn write_metadata(
    path: &Path,
    metadata: &RuntimeMetadataV1,
) -> Result<(), RuntimeMetadataError> {
    let bytes = serde_json::to_vec(metadata).map_err(RuntimeMetadataError::Json)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(RuntimeMetadataError::Open)?;
    file.write_all(&bytes)
        .map_err(RuntimeMetadataError::Write)?;
    file.sync_all().map_err(RuntimeMetadataError::Write)
}

pub fn read_metadata(path: &Path) -> Result<RuntimeMetadataV1, RuntimeMetadataError> {
    let file = File::open(path).map_err(RuntimeMetadataError::Open)?;
    let mut limited = Vec::new();
    file.take(MAX_METADATA_BYTES.saturating_add(1))
        .read_to_end(&mut limited)
        .map_err(RuntimeMetadataError::Read)?;
    if limited.len() as u64 > MAX_METADATA_BYTES {
        return Err(RuntimeMetadataError::Read(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "runtime metadata exceeds the size limit",
        )));
    }
    let metadata: RuntimeMetadataV1 =
        serde_json::from_slice(&limited).map_err(RuntimeMetadataError::Json)?;
    if metadata.schema_version != RUNTIME_METADATA_SCHEMA_VERSION {
        return Err(RuntimeMetadataError::UnsupportedSchema(
            metadata.schema_version,
        ));
    }
    Ok(metadata)
}

pub fn remove_metadata(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

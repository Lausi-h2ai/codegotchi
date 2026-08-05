use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
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
    write_metadata_with_hooks(
        path,
        &bytes,
        |file, bytes| file.write_all(bytes),
        |file| file.sync_all(),
    )
}

fn write_metadata_with_hooks<WriteAll, SyncAll>(
    path: &Path,
    bytes: &[u8],
    write_all: WriteAll,
    sync_all: SyncAll,
) -> Result<(), RuntimeMetadataError>
where
    WriteAll: FnOnce(&mut File, &[u8]) -> io::Result<()>,
    SyncAll: FnOnce(&File) -> io::Result<()>,
{
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(RuntimeMetadataError::Open)?;
    if let Err(error) = write_all(&mut file, bytes) {
        let _ = fs::remove_file(path);
        return Err(RuntimeMetadataError::Write(error));
    }
    if let Err(error) = sync_all(&file) {
        let _ = fs::remove_file(path);
        return Err(RuntimeMetadataError::Write(error));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use super::{RuntimeMetadataV1, write_metadata_with_hooks};
    use uuid::Uuid;

    fn test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codegotchi-runtime-metadata-{label}-{}-{}.json",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    fn test_bytes() -> Vec<u8> {
        serde_json::to_vec(&RuntimeMetadataV1::new(
            Uuid::new_v4(),
            ".",
            "http://127.0.0.1:1",
            "token",
            std::process::id(),
        ))
        .unwrap()
    }

    #[test]
    fn failed_write_removes_the_newly_created_metadata_file() {
        let path = test_path("write-failure");
        let result = write_metadata_with_hooks(
            &path,
            &test_bytes(),
            |_file, _bytes| Err(io::Error::other("injected write failure")),
            |_file| Ok(()),
        );
        assert!(result.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn failed_sync_removes_the_newly_created_metadata_file() {
        let path = test_path("sync-failure");
        let result =
            write_metadata_with_hooks(&path, &test_bytes(), std::io::Write::write_all, |_file| {
                Err(io::Error::other("injected sync failure"))
            });
        assert!(result.is_err());
        assert!(!path.exists());
    }
}

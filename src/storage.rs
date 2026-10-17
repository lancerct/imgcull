//! Shared durable creation of immutable files; policy belongs to callers.
use anyhow::{Result, ensure};
use serde::de::DeserializeOwned;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Read bounded regular-file control JSON. The caller owns its schema and size
/// policy. A bounded read also limits files that grow after the metadata check.
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= limit,
        "invalid/oversized control JSON file"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "control JSON grew beyond limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

/// Create a new file, flush its bytes, and refuse to overwrite anything.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// OS-lock owner. Explicit unlock on drop also releases an inherited open-file
/// description during a concurrent Unix fork/exec window.
pub(crate) struct FileLock(File);

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Hold a nonblocking OS lock; process exit releases it without stale-lock cleanup.
pub(crate) fn lock_file(path: &Path) -> Result<FileLock> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => anyhow::ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "invalid lock file"
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .map_err(|e| anyhow::anyhow!("resource is locked: {e}"))?;
    Ok(FileLock(file))
}

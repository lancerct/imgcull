//! Shared durable creation of immutable files; policy belongs to callers.
use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Create a new file, flush its bytes, and refuse to overwrite anything.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Hold a nonblocking OS lock; process exit releases it without stale-lock cleanup.
pub(crate) fn lock_file(path: &Path) -> Result<File> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink(), "invalid lock file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    file.try_lock().map_err(|e| anyhow::anyhow!("resource is locked: {e}"))?;
    Ok(file)
}

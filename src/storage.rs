//! Shared durable creation of immutable files; policy belongs to callers.
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Create a new file, flush its bytes, and refuse to overwrite anything.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

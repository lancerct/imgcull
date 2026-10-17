//! Recoverable metadata replacement. No photo pixels are modified.
use crate::integrity::digest;
use crate::storage::write_new as snapshot;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    target: PathBuf,
    before: Option<String>,
    after: String,
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) => ensure!(
            !m.file_type().is_symlink(),
            "refusing symlink: {}",
            path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
fn locked_target(path: &Path) -> Result<(PathBuf, File)> {
    ensure!(
        crate::discovery::has_extension(path, &["xmp"]),
        "metadata transaction target must be an XMP sidecar"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let target = parent.join(path.file_name().context("missing filename")?);
    reject_link(&target)?;
    let locks = state_directory(&target)?.join("locks");
    reject_link(&locks)?;
    fs::create_dir_all(&locks)?;
    let lock_path = locks
        .join(target.file_name().context("missing filename")?)
        .with_extension("xmp.lock");
    reject_link(&lock_path)?;
    let lock = crate::storage::lock_file(&lock_path)?;
    Ok((target, lock))
}
fn atomic_replace(path: &Path, content: &[u8], must_be_absent: bool) -> Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().context("missing parent")?)?;
    tmp.write_all(content)?;
    if let Ok(meta) = fs::metadata(path) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }
    tmp.as_file().sync_all()?;
    if must_be_absent {
        tmp.persist_noclobber(path).map_err(|e| e.error)?;
    } else {
        tmp.persist(path).map_err(|e| e.error)?;
    }
    Ok(())
}
fn state_directory(target: &Path) -> Result<PathBuf> {
    let root = target.parent().context("missing parent")?.join(".imgcull");
    reject_link(&root)?;
    fs::create_dir_all(&root)?;
    Ok(root)
}
fn transaction_root(target: &Path) -> Result<PathBuf> {
    let root = state_directory(target)?.join("transactions");
    reject_link(&root)?;
    fs::create_dir_all(&root)?;
    Ok(root)
}
/// A stale read, unreadable source, failed backup or failed validation prevents replacement.
pub fn write(path: &Path, expected: Option<&[u8]>, content: &[u8]) -> Result<()> {
    let (target, _lock) = locked_target(path)?;
    let current = read_optional(&target)?;
    ensure!(
        current.as_deref() == expected,
        "XMP changed since read; refusing overwrite: {}",
        target.display()
    );
    if current.as_deref() == Some(content) {
        return Ok(());
    }
    let transaction = tempfile::Builder::new()
        .prefix("change-")
        .tempdir_in(transaction_root(&target)?)?
        .keep();
    if let Some(bytes) = &current {
        snapshot(&transaction.join("before.xmp"), bytes)?;
    }
    snapshot(&transaction.join("after.xmp"), content)?;
    let record = Record {
        version: 1,
        target: target.clone(),
        before: current.as_deref().map(digest),
        after: digest(content),
    };
    snapshot(
        &transaction.join("transaction.json"),
        &serde_json::to_vec_pretty(&record)?,
    )?;
    // Non-cooperating editors cannot share our lock. Keep the final check adjacent to replace.
    ensure!(
        read_optional(&target)? == current,
        "XMP changed while preparing transaction; refusing overwrite"
    );
    atomic_replace(&target, content, current.is_none())?;
    snapshot(&transaction.join("applied"), b"applied\n")?;
    Ok(())
}
/// Roll back only if the file still equals this transaction's output.
pub fn rollback(record_path: &Path) -> Result<()> {
    ensure!(
        record_path
            .file_name()
            .is_some_and(|n| n == "transaction.json"),
        "expected transaction.json"
    );
    let record_path = record_path.canonicalize()?;
    let dir = record_path.parent().context("missing transaction folder")?;
    let record: Record = serde_json::from_slice(&fs::read(&record_path)?)?;
    ensure!(record.version == 1, "unsupported transaction version");
    let (target, _lock) = locked_target(&record.target)?;
    ensure!(
        dir.parent() == Some(transaction_root(&target)?.as_path()),
        "transaction is outside target folder"
    );
    let current = read_optional(&target)?.context("target missing; refusing rollback")?;
    ensure!(
        digest(&current) == record.after,
        "XMP edited after this transaction; refusing rollback"
    );
    let after = fs::read(dir.join("after.xmp"))?;
    ensure!(digest(&after) == record.after, "corrupt transaction output");
    if let Some(hash) = record.before {
        let before = fs::read(dir.join("before.xmp"))?;
        ensure!(digest(&before) == hash, "corrupt transaction backup");
        ensure!(
            read_optional(&target)?.as_deref() == Some(current.as_slice()),
            "target changed during rollback"
        );
        atomic_replace(&target, &before, false)?;
    } else {
        ensure!(
            read_optional(&target)?.as_deref() == Some(current.as_slice()),
            "target changed during rollback"
        );
        fs::remove_file(&target)?;
    }
    snapshot(&dir.join("rolled-back"), b"rolled back\n")?;
    Ok(())
}

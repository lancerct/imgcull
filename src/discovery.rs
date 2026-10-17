//! File discovery for supported image formats.
//!
//! Walks directories and filters files by supported extensions (JPEG and common RAW formats).

use std::fs;
use std::path::{Path, PathBuf};

/// One shared mapping from an image path to its XMP target.
pub fn sidecar_for_image(path: &Path) -> PathBuf {
    path.with_extension("xmp")
}

/// Canonical photo key, shared by RAW/JPEG scheduling and evidence manifests.
/// The sidecar itself need not exist. Windows keys are case-insensitive.
pub fn photo_key(path: &Path) -> anyhow::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    let target = sidecar_for_image(&canonical);
    #[cfg(windows)]
    let target = PathBuf::from(
        target
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-Unicode path"))?
            .to_lowercase(),
    );
    Ok(target)
}

/// One writer per sidecar. RAW is the identity; a same-stem JPEG is its review source.
/// Multiple RAWs with the same stem are ambiguous and abort before any writes.
pub fn deduplicate_images(images: Vec<PathBuf>) -> anyhow::Result<Vec<PathBuf>> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut groups: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    for path in images {
        anyhow::ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "refusing linked image"
        );
        let canonical = path.canonicalize()?;
        let target = photo_key(&canonical)?;
        groups.entry(target).or_default().insert(canonical);
    }
    let mut result = Vec::new();
    for (_, paths) in groups {
        let raws: Vec<_> = paths
            .iter()
            .filter(|p| !has_extension(p, &["jpg", "jpeg"]))
            .collect();
        anyhow::ensure!(
            raws.len() <= 1,
            "multiple RAW files share one XMP: {:?}",
            paths
        );
        anyhow::ensure!(
            paths.len() - raws.len() <= 1,
            "multiple JPEG files share one XMP: {:?}",
            paths
        );
        let selected = raws
            .first()
            .map(|p| (*p).clone())
            .unwrap_or_else(|| paths.first().unwrap().clone());
        result.push(selected);
    }
    result.sort();
    Ok(result)
}

/// Prefer a full JPEG companion for later native-resolution review.
pub fn review_source(identity: &Path) -> PathBuf {
    if has_extension(identity, &["jpg", "jpeg"]) {
        return identity.to_path_buf();
    }
    if let Some(parent) = identity.parent()
        && let Ok(entries) = fs::read_dir(parent)
    {
        let mut candidates: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_ok_and(|t| t.is_file())
                    && same_stem(&e.path(), identity)
                    && has_extension(&e.path(), &["jpg", "jpeg"])
            })
            .map(|e| e.path())
            .collect();
        candidates.sort();
        if let Some(path) = candidates.into_iter().next() {
            return path;
        }
    }
    identity.to_path_buf()
}

/// Enumerate all supported physical companions, even when only one was selected.
/// Reject ambiguous RAW identities and linked companions rather than freezing an
/// incomplete asset set for a later metadata plan.
pub fn photo_assets(image: &Path) -> anyhow::Result<Vec<PathBuf>> {
    anyhow::ensure!(is_supported(image), "unsupported photo identity");
    anyhow::ensure!(
        !fs::symlink_metadata(image)?.file_type().is_symlink(),
        "refusing linked image"
    );
    let image = image.canonicalize()?;
    let mut assets = Vec::new();
    for entry in fs::read_dir(image.parent().unwrap())? {
        let entry = entry?;
        let path = entry.path();
        if same_stem(&path, &image) && is_supported(&path) {
            anyhow::ensure!(entry.file_type()?.is_file(), "non-regular photo companion");
            assets.push(path.canonicalize()?);
        }
    }
    assets.sort();
    assets.dedup();
    anyhow::ensure!(
        assets.contains(&image),
        "identity disappeared during discovery"
    );
    deduplicate_images(assets.clone())?;
    Ok(assets)
}

/// Supported image file extensions (lowercase).
///
/// Single source of truth: [`crate::preprocessing::preprocess_image`] treats
/// `jpg`/`jpeg` as JPEG and every other entry here as a rawler-handled RAW
/// format.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", // JPEG
    "arw", "cr2", "cr3", "dng", "erf", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf",
    "rw2", "sr2", "srw", // RAW formats handled via rawler
];

/// Returns `true` when `path`'s extension matches any of `extensions`
/// case-insensitively.  Returns `false` when the path has no extension or
/// the extension is not valid UTF-8.
pub fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.iter().any(|&s| s.eq_ignore_ascii_case(ext)))
}

fn same_stem(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.file_stem()
            .and_then(|v| v.to_str())
            .map(str::to_lowercase)
            == b.file_stem()
                .and_then(|v| v.to_str())
                .map(str::to_lowercase)
    }
    #[cfg(not(windows))]
    {
        a.file_stem() == b.file_stem()
    }
}

/// Returns `true` if the file at `path` has a supported image extension (case-insensitive).
pub fn is_supported(path: &Path) -> bool {
    has_extension(path, SUPPORTED_EXTENSIONS)
}

/// Discover image files from the given paths.
///
/// Directories are scanned recursively for supported files. Individual file paths are
/// checked directly against the supported extensions. Unsupported or unreadable paths
/// produce a warning via `tracing::warn`. Results are sorted lexicographically.
pub fn discover_images(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut results = Vec::new();

    for path in paths {
        if path.is_dir() {
            scan_dir(path, &mut results);
        } else if path.is_file() {
            if is_supported(path) {
                results.push(path.clone());
            } else {
                tracing::warn!("skipping unsupported file: {}", path.display());
            }
        } else {
            tracing::warn!("skipping unreadable path: {}", path.display());
        }
    }

    results.sort();
    results.dedup();
    results
}

/// Recursively scan a directory, collecting supported image files.
fn scan_dir(dir: &Path, results: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("cannot read directory {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("error reading directory entry: {}", e);
                continue;
            }
        };

        let path = entry.path();
        // `DirEntry::file_type` comes straight from the readdir entry on most
        // platforms, avoiding a stat syscall per file. Never recurse through links.
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => {
                if !entry.file_name().to_string_lossy().starts_with('.') {
                    scan_dir(&path, results);
                }
            }
            Ok(ft) if ft.is_file() => {
                if is_supported(&path) {
                    results.push(path);
                }
            }
            _ => tracing::warn!("skipping symlink or unreadable entry: {}", path.display()),
        }
    }
}

//! Verify LR-generated trial renditions using the shared full decoder and hashes.
//! Export validation is not a visual verdict or permission to apply metadata.
use crate::{imaging::NativePhoto, integrity};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Strict plugin export manifest. Paths and personal metadata stay local.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Contract version.
    pub schema: u32,
    /// Known producer version, not a cryptographic signature.
    pub exporter: String,
    /// Catalog identity supplied by Lightroom; this verifier never opens it.
    pub catalog: PathBuf,
    /// Actual Lightroom version used for the export.
    pub lightroom_version: String,
    /// One entry per selected LR photo or virtual copy; no physical-file deduplication here.
    pub images: Vec<Rendition>,
}

/// A current-effect rendition and the LR state reported during generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rendition {
    /// LR persistent photo identity, distinct for virtual copies.
    pub photo_uuid: String,
    /// Catalog-local identity retained as a string.
    pub local_identifier: String,
    /// Virtual copies must not silently share a future physical-XMP decision.
    pub is_virtual_copy: bool,
    /// Actual physical file Lightroom rendered.
    pub source: PathBuf,
    /// Complete source bytes, checked before and after plugin rendering.
    pub source_sha256: String,
    /// Canonical reported develop-settings digest; experimental SDK is not a catalog lock.
    pub develop_sha256: String,
    /// LR-reported edit time, not a guarantee no later edit happened.
    pub last_edit_time: f64,
    /// LR-reported edit counter, not an absolute edit sequence.
    pub edit_count: u64,
    /// LR-reported current cropped width, before optional orientation swap.
    pub cropped_width: u32,
    /// LR-reported current cropped height.
    pub cropped_height: u32,
    /// Controlled relative filename in the export root.
    pub file: String,
    /// Full rendition fingerprint.
    pub sha256: String,
}

/// Compact verification response; detailed provenance remains in the manifest.
#[derive(Debug, Serialize)]
pub struct Summary {
    /// Fingerprint to bind a later review to this exact manifest.
    pub manifest_sha256: String,
    /// Number of complete, fully decoded current-effect images.
    pub images: usize,
    /// Virtual copies requiring explicit later binding, not sidecar deduplication.
    pub virtual_copies: usize,
    /// Files being valid does not prove the current live catalog is unchanged.
    pub live_catalog_verified: bool,
    /// No review job or focus/semantic review is authorized by this command.
    pub can_apply: bool,
    /// Always false for material preparation and verification.
    pub contains_visual_verdict: bool,
}

fn regular(path: &Path, limit: u64) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() > 0 && meta.len() <= limit,
        "invalid/oversized LR export file: {}",
        path.display()
    );
    Ok(())
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096
}

fn dimensions(width: u32, height: u32, image: &Rendition) -> bool {
    let near = |a: u32, b: u32| a.abs_diff(b) <= 1;
    (near(width, image.cropped_width) && near(height, image.cropped_height))
        || (near(width, image.cropped_height) && near(height, image.cropped_width))
}

/// Validate every file, source revision and full decoded rendition dimensions.
/// A cropped current rendition may legitimately be smaller than the original
/// 45MP technical evidence. This never lowers the review job's source floor.
/// This checks saved artifacts, not live LR state, semantic quality or ICC fidelity.
pub fn verify(directory: &Path) -> Result<Summary> {
    let meta = fs::symlink_metadata(directory)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "invalid LR export directory"
    );
    let root = directory.canonicalize()?;
    let path = root.join("manifest.json");
    regular(&path, 2 * 1024 * 1024)?;
    let bytes = fs::read(&path)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema == 1 && manifest.exporter == "imgcull-lr-v1",
        "unsupported LR export contract"
    );
    ensure!(
        manifest.catalog.is_absolute() && text(&manifest.lightroom_version),
        "missing catalog/version identity"
    );
    ensure!(
        (1..=10).contains(&manifest.images.len()),
        "trial LR export must contain 1..=10 photos"
    );
    let mut ids = BTreeSet::new();
    for (index, image) in manifest.images.iter().enumerate() {
        ensure!(
            text(&image.photo_uuid)
                && text(&image.local_identifier)
                && ids.insert(&image.photo_uuid),
            "missing/duplicate LR identity"
        );
        ensure!(
            image.source.is_absolute()
                && hash(&image.source_sha256)
                && hash(&image.develop_sha256)
                && hash(&image.sha256),
            "invalid LR source/fingerprint"
        );
        ensure!(
            image.last_edit_time.is_finite()
                && image.last_edit_time >= 0.0
                && image.cropped_width > 0
                && image.cropped_height > 0,
            "invalid LR edit state/dimensions"
        );
        ensure!(
            image.file == format!("render-{:04}.jpg", index + 1),
            "unsafe/unexpected rendition filename"
        );
        let file = root.join(&image.file);
        regular(&file, 512 * 1024 * 1024)?;
        regular(&image.source, 512 * 1024 * 1024)?;
        ensure!(
            integrity::file_digest(&image.source)? == image.source_sha256,
            "LR source changed after export"
        );
        let photo = NativePhoto::load(&file, 0)?;
        ensure!(
            photo.info.source_sha256 == image.sha256,
            "LR rendition changed"
        );
        ensure!(
            dimensions(photo.info.width, photo.info.height, image),
            "LR output does not match full current crop dimensions"
        );
        ensure!(
            integrity::file_digest(&image.source)? == image.source_sha256,
            "LR source changed during verification"
        );
    }
    ensure!(
        fs::read(&path)? == bytes,
        "LR manifest changed during verification"
    );
    Ok(Summary {
        manifest_sha256: integrity::digest(&bytes),
        images: manifest.images.len(),
        virtual_copies: manifest.images.iter().filter(|i| i.is_virtual_copy).count(),
        live_catalog_verified: false,
        can_apply: false,
        contains_visual_verdict: false,
    })
}

//! Immutable review evidence; never writes metadata or makes culling decisions.
use crate::imaging::{NativePhoto, RENDERER, Region, RenderInfo, encode};
use crate::integrity::{digest, file_digest};
use crate::storage::write_new;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Compact CLI response; detailed evidence stays on disk instead of in every prompt.
#[derive(Debug, Serialize)]
pub struct EvidenceSummary {
    /// Reusable evidence identifier.
    pub evidence_id: String,
    /// Manifest to load only when detailed provenance is needed.
    pub manifest: PathBuf,
    /// Full normalized source width.
    pub width: u32,
    /// Full normalized source height.
    pub height: u32,
    /// Full raster or full RAW development.
    pub decoding: String,
    /// Number of native detail assets.
    pub detail_count: usize,
    /// Focus assets available for a later reviewer.
    pub focus_ids: Vec<String>,
    /// Preparing/validating files never constitutes a visual verdict.
    pub contains_visual_verdict: bool,
}

/// Why a native crop was requested; overview is generated separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailRole {
    /// Subject detail relevant to focus/blur.
    Focus,
    /// Pose, expression, object label or other contextual detail.
    Context,
    /// Frame edge intrusion or overlapping background.
    Edge,
}

/// Explicit native-pixel request. An empty request does not certify focus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailRequest {
    /// Stable name used by later review records.
    pub id: String,
    /// Purpose of the crop.
    pub role: DetailRole,
    /// Coordinates after EXIF orientation normalization.
    pub region: Region,
}

/// A stored view and its integrity/provenance information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Overview or caller-supplied detail identifier.
    pub id: String,
    /// Plain generated filename inside the bundle.
    pub file: String,
    /// SHA-256 of encoded bytes.
    pub sha256: String,
    /// Actual payload width.
    pub width: u32,
    /// Actual payload height.
    pub height: u32,
    /// None for overview; Some for an unscaled PNG crop.
    pub detail: Option<DetailRequest>,
}

/// Versioned contract shared by review, resume and future decision validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Schema version.
    pub schema: u32,
    /// Canonical manifest checksum, with this field empty during hashing.
    pub evidence_id: String,
    /// Full source decoding provenance.
    pub render: RenderInfo,
    /// Required pixel floor supplied by the caller; never met by upscaling.
    pub min_pixels: u64,
    /// Overview followed by requested native crops.
    pub views: Vec<View>,
}

impl Evidence {
    /// Compact status for both prepare and verify; no image bytes or full history.
    pub fn summary(&self, directory: &Path) -> EvidenceSummary {
        EvidenceSummary {
            evidence_id: self.evidence_id.clone(),
            manifest: directory.join("evidence.json"),
            width: self.render.width,
            height: self.render.height,
            decoding: self.render.decoding.clone(),
            detail_count: self.views.len().saturating_sub(1),
            focus_ids: self.focus_ids().into_iter().map(str::to_string).collect(),
            contains_visual_verdict: false,
        }
    }
    fn checksum(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.evidence_id.clear();
        Ok(digest(&serde_json::to_vec(&copy)?))
    }

    /// List focus assets available for inspection. Existence is not proof a model
    /// saw them or selected the correct subject; later review records must say so.
    pub fn focus_ids(&self) -> Vec<&str> {
        self.views
            .iter()
            .filter(|v| {
                v.detail
                    .as_ref()
                    .is_some_and(|d| d.role == DetailRole::Focus)
            })
            .map(|v| v.id.as_str())
            .collect()
    }
}

fn validate_requests(requests: &[DetailRequest], width: u32, height: u32) -> Result<()> {
    ensure!(requests.len() <= 64, "at most 64 explicit crops per bundle");
    let mut ids = BTreeSet::new();
    for request in requests {
        ensure!(
            !request.id.is_empty()
                && request.id.len() <= 64
                && request
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "detail id must contain 1..=64 ASCII letters, digits, '-' or '_'"
        );
        ensure!(
            request.id != "overview" && ids.insert(&request.id),
            "duplicate/reserved detail id"
        );
        request.region.validate(width, height)?;
    }
    Ok(())
}

/// Decode once, derive all views, and publish a complete immutable bundle.
/// No provider credentials, model requests, XMP writes or screen control.
pub fn prepare(
    identity: &Path,
    output: &Path,
    requests: &[DetailRequest],
    min_pixels: u64,
) -> Result<Evidence> {
    ensure!(
        !output.try_exists()?,
        "evidence output already exists; verify/reuse it or choose a new output"
    );
    let photo = NativePhoto::load(identity, min_pixels)?;
    validate_requests(requests, photo.info.width, photo.info.height)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".imgcull-evidence-")
        .tempdir_in(parent)?;
    let overview = photo.overview(2048)?;
    let bytes = encode(&overview, image::ImageFormat::Jpeg)?;
    write_new(&staging.path().join("overview.jpg"), &bytes)?;
    let mut views = vec![View {
        id: "overview".into(),
        file: "overview.jpg".into(),
        sha256: digest(&bytes),
        width: overview.width(),
        height: overview.height(),
        detail: None,
    }];
    for (index, request) in requests.iter().enumerate() {
        let image = photo.detail(request.region)?;
        let bytes = encode(&image, image::ImageFormat::Png)?;
        let name = format!("detail-{index:04}.png");
        write_new(&staging.path().join(&name), &bytes)?;
        views.push(View {
            id: request.id.clone(),
            file: name,
            sha256: digest(&bytes),
            width: image.width(),
            height: image.height(),
            detail: Some(request.clone()),
        });
    }
    let mut evidence = Evidence {
        schema: 1,
        evidence_id: String::new(),
        render: photo.info,
        min_pixels,
        views,
    };
    evidence.evidence_id = evidence.checksum()?;
    write_new(
        &staging.path().join("evidence.json"),
        &serde_json::to_vec_pretty(&evidence)?,
    )?;
    ensure!(
        file_digest(&evidence.render.source)? == evidence.render.source_sha256,
        "source changed before publishing evidence"
    );
    ensure!(
        !output.try_exists()?,
        "evidence output appeared during preparation"
    );
    fs::rename(staging.path(), output).context("publishing evidence bundle")?;
    Ok(evidence)
}

/// Verify exact source bytes and every generated view before reusing evidence.
/// A checksum detects accidental changes; it is not a signature against a hostile
/// party that can rewrite the manifest and all hashes.
pub fn verify(directory: &Path) -> Result<Evidence> {
    let directory = directory.canonicalize()?;
    let manifest = directory.join("evidence.json");
    ensure!(
        fs::symlink_metadata(&manifest)?.is_file(),
        "manifest must be a regular file"
    );
    ensure!(
        fs::metadata(&manifest)?.len() <= 1024 * 1024,
        "manifest exceeds 1 MiB"
    );
    let evidence: Evidence = serde_json::from_slice(&fs::read(manifest)?)?;
    ensure!(
        evidence.schema == 1 && evidence.render.renderer == RENDERER,
        "unsupported evidence version"
    );
    ensure!(
        evidence.evidence_id == evidence.checksum()?,
        "manifest checksum mismatch"
    );
    ensure!(
        file_digest(&evidence.render.source)? == evidence.render.source_sha256,
        "evidence source changed"
    );
    let mut ids = BTreeSet::new();
    ensure!(
        (1..=65).contains(&evidence.views.len()),
        "evidence must contain one overview and at most 64 details"
    );
    let mut details = Vec::new();
    for (index, view) in evidence.views.iter().enumerate() {
        ensure!(ids.insert(&view.id), "duplicate view id");
        let expected = if index == 0 {
            "overview.jpg".into()
        } else {
            format!("detail-{:04}.png", index - 1)
        };
        ensure!(view.file == expected, "invalid view filename/order");
        let path: PathBuf = directory.join(&view.file);
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "view must be a regular file"
        );
        ensure!(
            file_digest(&path)? == view.sha256,
            "view checksum mismatch: {}",
            view.id
        );
        let dimensions = image::ImageReader::open(&path)?.into_dimensions()?;
        ensure!(
            dimensions == (view.width, view.height),
            "view dimensions mismatch"
        );
        if index == 0 {
            ensure!(
                view.id == "overview"
                    && view.detail.is_none()
                    && view.width <= 2048
                    && view.height <= 2048,
                "invalid overview"
            );
        } else {
            let detail = view.detail.as_ref().context("missing native region")?;
            ensure!(
                detail.id == view.id && (detail.region.width, detail.region.height) == dimensions,
                "native region was resized or misidentified"
            );
            details.push(detail.clone());
        }
    }
    validate_requests(&details, evidence.render.width, evidence.render.height)?;
    ensure!(
        u64::from(evidence.render.width) * u64::from(evidence.render.height) >= evidence.min_pixels,
        "native pixel floor not met"
    );
    Ok(evidence)
}

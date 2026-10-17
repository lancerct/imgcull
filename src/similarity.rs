//! Local similarity proposals over verified evidence. No image is discarded,
//! ranked as keeper or excluded from semantic/native review by this module.
use crate::{evidence, imaging::NativePhoto};
use anyhow::{Result, ensure};
use image::imageops::FilterType;
use serde::Serialize;
use std::{collections::BTreeSet, path::PathBuf};

/// A pair to compare, never a transitive same-moment/sequence assertion.
#[derive(Debug, Serialize)]
pub struct Candidate {
    /// First immutable evidence bundle ID.
    pub left: String,
    /// Second immutable evidence bundle ID.
    pub right: String,
    /// Difference-hash distance on overviews; not optical sharpness.
    pub distance: u32,
}

/// Scheduling aid. Pair bounds keep results small; a reviewer chooses actual
/// semantic groups and can retain unlimited meaningful variations.
#[derive(Debug, Serialize)]
pub struct Suggestions {
    /// Candidate descriptor version for reproducibility.
    pub algorithm: String,
    /// Distance threshold used, not a deletion threshold.
    pub threshold: u32,
    /// Number of verified logical photos compared.
    pub photos: usize,
    /// Candidate pairs sorted by distance and bundle IDs.
    pub pairs: Vec<Candidate>,
    /// Native/semantic/sequence checks still required for every decision.
    pub contains_visual_verdict: bool,
    /// Similarity cannot authorize metadata changes.
    pub can_apply: bool,
}

/// Compare a bounded caller-selected window of 2..=64 bundles. The caller
/// should use capture/session context; global visual similarity is not a burst.
/// Reuses evidence verification and the single decoder; no RAW preview fallback.
pub fn suggest(directories: &[PathBuf], threshold: u32) -> Result<Suggestions> {
    ensure!(
        (2..=64).contains(&directories.len()) && threshold <= 12,
        "similarity requires 2..=64 bundles and distance <=12"
    );
    let mut identities = BTreeSet::new();
    let mut descriptors = Vec::new();
    for directory in directories {
        let bundle = evidence::verify(directory)?;
        ensure!(
            identities.insert(bundle.render.identity.clone()),
            "duplicate logical photo in similarity window"
        );
        let view = bundle
            .views
            .iter()
            .find(|v| v.id == "overview")
            .ok_or_else(|| anyhow::anyhow!("missing overview"))?;
        let photo = NativePhoto::load(&directory.join(&view.file), 0)?;
        ensure!(
            photo.info.source_sha256 == view.sha256,
            "overview changed during similarity read"
        );
        let gray = photo
            .overview(2048)?
            .resize_exact(9, 8, FilterType::Triangle)
            .to_luma8();
        let mut hash = 0_u64;
        for y in 0..8 {
            for x in 0..8 {
                hash =
                    (hash << 1) | u64::from(gray.get_pixel(x, y)[0] > gray.get_pixel(x + 1, y)[0]);
            }
        }
        // The ID is already checksummed by evidence; do not invent photo IDs.
        descriptors.push((bundle.evidence_id, hash));
    }
    descriptors.sort_by(|a, b| a.0.cmp(&b.0));
    let mut pairs = Vec::new();
    for (i, (left, a)) in descriptors.iter().enumerate() {
        for (right, b) in descriptors.iter().skip(i + 1) {
            let distance = (a ^ b).count_ones();
            if distance <= threshold {
                pairs.push(Candidate {
                    left: left.clone(),
                    right: right.clone(),
                    distance,
                });
            }
        }
    }
    pairs.sort_by(|a, b| (a.distance, &a.left, &a.right).cmp(&(b.distance, &b.left, &b.right)));
    ensure!(
        pairs.len() <= 256,
        "too many similar pairs; use a smaller comparison window"
    );
    // All outputs are scheduling hints, including distance zero.
    Ok(Suggestions {
        algorithm: "overview-dhash-v1".into(),
        threshold,
        photos: descriptors.len(),
        pairs,
        contains_visual_verdict: false,
        can_apply: false,
    })
}

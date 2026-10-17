//! Overview adapter for legacy model calls; native decoding lives in imaging.
use crate::imaging::{NativePhoto, encode};
use anyhow::Result;
use base64::{Engine, prelude::BASE64_STANDARD};
use std::path::Path;

/// An overview payload. This does not certify that native details were reviewed.
pub struct PreprocessedImage {
    /// Base64 JPEG with orientation normalized and source EXIF removed.
    pub base64: String,
    /// Whether the overview is smaller than the fully decoded image.
    pub was_resized: bool,
}

/// Fully decode once, then derive an overview through the shared renderer.
/// Personal focus decisions additionally require native detail evidence.
pub fn preprocess_image(path: &Path) -> Result<PreprocessedImage> {
    let photo = NativePhoto::load(path, 0)?;
    let overview = photo.overview(2048)?;
    Ok(PreprocessedImage {
        base64: BASE64_STANDARD.encode(encode(&overview, image::ImageFormat::Jpeg)?),
        was_resized: photo.info.width > 2048 || photo.info.height > 2048,
    })
}

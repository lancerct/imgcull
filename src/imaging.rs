//! The single full-resolution decoder and coordinate system for all review views.
use std::io::Cursor;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result, ensure};
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, metadata::Orientation};
use serde::{Deserialize, Serialize};
use crate::{discovery, integrity};

/// Maximum decoded source area accepted by this renderer.
pub const MAX_PIXELS: u64 = 100_000_000;
/// Maximum native detail edge; details are never resized.
pub const DETAIL_EDGE: u32 = 1024;
/// Renderer version is part of evidence identity.
pub const RENDERER: &str = "imgcull-native-v1";

/// A rectangle in orientation-normalized source pixels, not overview pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    /// Left coordinate.
    pub x: u32,
    /// Top coordinate.
    pub y: u32,
    /// Native width.
    pub width: u32,
    /// Native height.
    pub height: u32,
}

impl Region {
    /// Reject empty, oversized, overflowing and out-of-bounds crops.
    pub fn validate(&self, width: u32, height: u32) -> Result<()> {
        ensure!(self.width > 0 && self.height > 0, "empty detail region");
        ensure!(self.width <= DETAIL_EDGE && self.height <= DETAIL_EDGE, "split detail regions into at most {DETAIL_EDGE}px edges");
        ensure!(self.x.checked_add(self.width).is_some_and(|x| x <= width)
            && self.y.checked_add(self.height).is_some_and(|y| y <= height), "detail region outside native image");
        Ok(())
    }
}

/// Provenance of the exact image decoded for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderInfo {
    /// Renderer contract version.
    pub renderer: String,
    /// Canonical shared metadata target; the XMP itself need not exist.
    pub identity: PathBuf,
    /// Actual decoded source; can be a JPEG companion.
    pub source: PathBuf,
    /// SHA-256 of that source, not the overview.
    pub source_sha256: String,
    /// Raster or full RAW development, never an embedded RAW preview.
    pub decoding: String,
    /// Encoded dimensions before EXIF rotation.
    pub encoded_width: u32,
    /// Encoded height before EXIF rotation.
    pub encoded_height: u32,
    /// EXIF orientation applied exactly once.
    pub orientation: u8,
    /// Width of the normalized native image.
    pub width: u32,
    /// Height of the normalized native image.
    pub height: u32,
}

/// A fully decoded photo kept in memory while all requested views are derived.
pub struct NativePhoto {
    /// Hashes, dimensions and decoder provenance.
    pub info: RenderInfo,
    pixels: DynamicImage,
}

impl NativePhoto {
    /// Prefer the existing JPEG companion; fall back to full RAW if it is too small
    /// or unreadable. Never silently upscale a small image to satisfy a pixel floor.
    pub fn load(identity: &Path, min_pixels: u64) -> Result<Self> {
        ensure!(min_pixels <= MAX_PIXELS, "requested pixel floor exceeds renderer limit");
        let identity = identity.canonicalize()?;
        let preferred = discovery::review_source(&identity);
        match Self::decode(&identity, &preferred, min_pixels) {
            Ok(photo) => Ok(photo),
            Err(error) if preferred != identity => Self::decode(&identity, &identity, min_pixels)
                .with_context(|| format!("JPEG companion could not meet review requirements: {error:#}; full RAW fallback failed")),
            Err(error) => Err(error),
        }
    }

    fn decode(identity: &Path, source: &Path, min_pixels: u64) -> Result<Self> {
        ensure!(std::fs::metadata(source)?.len() <= 512 * 1024 * 1024, "source exceeds 512 MiB input limit");
        let raster = discovery::has_extension(source, &["jpg", "jpeg"]);
        let (mut pixels, orientation, hash, decoding) = if raster {
            let bytes = std::fs::read(source)?;
            let hash = integrity::digest(&bytes);
            let reader = ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
            let mut decoder = reader.into_decoder()?;
            let (w, h) = decoder.dimensions();
            validate_area(w, h, min_pixels)?;
            let orientation = decoder.orientation()?;
            let pixels = DynamicImage::from_decoder(decoder).context("full raster decode failed")?;
            (pixels, orientation, hash, "full-raster")
        } else {
            ensure!(discovery::is_supported(source), "unsupported native source");
            let hash = integrity::file_digest(source)?;
            let metadata = rawler::analyze::analyze_metadata(source)?;
            let Some(rawler::analyze::AnalyzerData::Metadata(metadata)) = metadata.data else {
                anyhow::bail!("RAW metadata unavailable");
            };
            let raw_area = metadata.raw_params.raw_width as u64 * metadata.raw_params.raw_height as u64;
            ensure!(raw_area <= MAX_PIXELS, "RAW exceeds renderer pixel limit");
            let value = metadata.raw_metadata.exif.orientation.unwrap_or(1);
            let orientation = u8::try_from(value).ok().and_then(Orientation::from_exif)
                .context("unsupported RAW orientation")?;
            let pixels = rawler::analyze::raw_to_srgb(source, &rawler::decoders::RawDecodeParams::default())
                .context("full RAW development failed; embedded previews are not a fallback")?;
            (pixels, orientation, hash, "full-raw-srgb")
        };
        let (encoded_width, encoded_height) = pixels.dimensions();
        validate_area(encoded_width, encoded_height, min_pixels)?;
        pixels.apply_orientation(orientation);
        ensure!(integrity::file_digest(source)? == hash, "source changed during rendering");
        let (width, height) = pixels.dimensions();
        Ok(Self {
            info: RenderInfo { renderer: RENDERER.into(), identity: discovery::photo_key(identity)?,
                source: source.to_path_buf(), source_sha256: hash, decoding: decoding.into(),
                encoded_width, encoded_height, orientation: orientation.to_exif(), width, height },
            pixels,
        })
    }

    /// Downscaled composition view. It is never sufficient focus evidence.
    pub fn overview(&self, max_edge: u32) -> Result<DynamicImage> {
        ensure!(max_edge > 0 && max_edge <= 2048, "overview edge must be 1..=2048");
        if self.info.width <= max_edge && self.info.height <= max_edge {
            Ok(self.pixels.clone())
        } else {
            Ok(self.pixels.resize(max_edge, max_edge, image::imageops::FilterType::CatmullRom))
        }
    }

    /// Lossless, unscaled crop from the complete orientation-normalized pixels.
    pub fn detail(&self, region: Region) -> Result<DynamicImage> {
        region.validate(self.info.width, self.info.height)?;
        Ok(self.pixels.crop_imm(region.x, region.y, region.width, region.height))
    }
}

/// Encode a derived view; shared by the provider adapter and evidence exporter.
pub fn encode(image: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, format)?;
    Ok(buffer.into_inner())
}

fn validate_area(width: u32, height: u32, min_pixels: u64) -> Result<()> {
    let area = u64::from(width) * u64::from(height);
    ensure!(area > 0 && area <= MAX_PIXELS, "source exceeds renderer pixel limit");
    ensure!(area >= min_pixels, "source has {area} pixels; at least {min_pixels} native pixels required");
    Ok(())
}

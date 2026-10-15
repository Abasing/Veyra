//! Golden-image fixture mechanics shared by the integration tests.
//!
//! Fixture policy:
//!
//! - A fixture is a pair of files under `tests/golden/`:
//!   `phase1-diagnostic-v<scene>-<W>x<H>.png` (human-reviewable image) and
//!   the same stem with `.sha256` (one line: the canonical pixel hash, 64
//!   lowercase hex characters + newline).
//! - Verification is **pixel-level**: the fresh render is compared
//!   byte-for-byte against the *decoded* fixture PNG, in addition to the
//!   canonical-hash comparison. A hash equality alone would not prove the
//!   fixture PNG itself is right; this does.
//! - Regeneration (`write_fixture`) is an explicit act: the golden test
//!   only writes fixtures when `VEYRA_UPDATE_GOLDEN=1` is set, prints a
//!   loud notice when it does, and never writes during ordinary runs.
//!   Regenerated fixtures are reviewed like code (diff the PNG, check the
//!   scene version bump).
//! - The `VEYRA_GOLDEN_DIR` environment variable redirects the fixture
//!   directory (used by tests that exercise regeneration against a temp
//!   copy without touching the reviewed files).

use std::fs;
use std::path::{Path, PathBuf};

use veyra_gfx::fixture::DIAGNOSTIC_SCENE_VERSION;
use veyra_gfx::{PixelBuffer, RasterSurface as _};

use crate::config::SceneKind;
use crate::error::ScreenshotError;
use crate::hashing::sha256_hex;
use crate::screenshot;

/// Environment variable overriding the fixture directory.
pub const GOLDEN_DIR_ENV: &str = "VEYRA_GOLDEN_DIR";

/// Environment variable enabling fixture regeneration.
pub const UPDATE_ENV: &str = "VEYRA_UPDATE_GOLDEN";

/// The golden viewport: the fixed Phase 1 CI viewport (roadmap AC 1).
pub const GOLDEN_WIDTH: u32 = 800;
/// The golden viewport height.
pub const GOLDEN_HEIGHT: u32 = 600;

/// The directory containing golden fixtures for the current invocation.
pub fn golden_dir() -> PathBuf {
    if let Ok(dir) = std::env::var(GOLDEN_DIR_ENV)
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// The stem used for both fixture files.
pub fn fixture_stem(width: u32, height: u32) -> String {
    format!("phase1-diagnostic-v{DIAGNOSTIC_SCENE_VERSION}-{width}x{height}")
}

/// Path of the PNG half of the fixture.
pub fn fixture_png_path(width: u32, height: u32) -> PathBuf {
    golden_dir().join(format!("{}.png", fixture_stem(width, height)))
}

/// Path of the hash half of the fixture.
pub fn fixture_hash_path(width: u32, height: u32) -> PathBuf {
    golden_dir().join(format!("{}.sha256", fixture_stem(width, height)))
}

/// Renders the golden scene at `(width, height)` and writes both fixture
/// files into `dir`. Explicit, reviewable regeneration.
///
/// # Errors
/// Render/encode errors from the screenshot pipeline, and I/O errors from
/// writing the fixture files.
pub fn write_fixture_into(dir: &Path, width: u32, height: u32) -> Result<String, ScreenshotError> {
    let raster = screenshot::render_to_buffer(width, height, SceneKind::Diagnostic)?;
    let hash = sha256_hex(&raster.buffer().canonical_bytes());
    let png = screenshot::encode_png(raster.buffer())?;

    fs::create_dir_all(dir).map_err(|source| ScreenshotError::OutputWrite {
        path: dir.to_path_buf(),
        source,
    })?;
    let png_path = dir.join(format!("{}.png", fixture_stem(width, height)));
    let hash_path = dir.join(format!("{}.sha256", fixture_stem(width, height)));
    fs::write(&png_path, &png).map_err(|source| ScreenshotError::OutputWrite {
        path: png_path.clone(),
        source,
    })?;
    fs::write(&hash_path, format!("{hash}\n")).map_err(|source| ScreenshotError::OutputWrite {
        path: hash_path.clone(),
        source,
    })?;
    Ok(hash)
}

/// The result of a golden verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenReport {
    /// Canonical hash of the fresh render.
    pub rendered_hash: String,
    /// Canonical hash recorded in the fixture.
    pub fixture_hash: String,
    /// Path of the fixture PNG that was compared.
    pub fixture_path: PathBuf,
}

/// Verifies the fresh render against the reviewed fixture: hash equality
/// **and** decoded-PNG pixel equality.
///
/// # Errors
/// - [`ScreenshotError::Render`] when rendering fails.
/// - [`ScreenshotError::OutputWrite`] when fixture files are missing or
///   unreadable.
/// - [`ScreenshotError::HashMismatch`] when the canonical hashes differ.
/// - [`ScreenshotError::PngEncode`] (repurposed decode context) when the
///   fixture PNG cannot be decoded.
/// - A pixel mismatch is a [`ScreenshotError::HashMismatch`] variant with
///   a diagnostic message naming the first differing pixel.
pub fn verify_against_fixture(width: u32, height: u32) -> Result<GoldenReport, ScreenshotError> {
    let raster = screenshot::render_to_buffer(width, height, SceneKind::Diagnostic)?;
    let rendered_hash = sha256_hex(&raster.buffer().canonical_bytes());

    let png_path = fixture_png_path(width, height);
    let hash_path = fixture_hash_path(width, height);

    let fixture_hash_text =
        fs::read_to_string(&hash_path).map_err(|source| ScreenshotError::OutputWrite {
            path: hash_path.clone(),
            source,
        })?;
    let fixture_hash = fixture_hash_text
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if rendered_hash != fixture_hash {
        return Err(ScreenshotError::HashMismatch {
            expected: fixture_hash,
            actual: rendered_hash,
            width,
            height,
        });
    }

    // Pixel-level proof: the fixture PNG decodes to exactly the render.
    let fixture_bytes = fs::read(&png_path).map_err(|source| ScreenshotError::OutputWrite {
        path: png_path.clone(),
        source,
    })?;
    let fixture_pixels = decode_png_pixels(&fixture_bytes, width, height)?;
    compare_pixels(&fixture_pixels, raster.buffer(), width, height)?;

    Ok(GoldenReport {
        rendered_hash,
        fixture_hash,
        fixture_path: png_path,
    })
}

/// Decodes an RGBA8 PNG into a flat pixel vector (used for fixture
/// comparison; the harness itself only encodes).
pub fn decode_png_pixels(
    bytes: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ScreenshotError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|source| ScreenshotError::PngDecode { source })?;
    let buffer_size = reader
        .output_buffer_size()
        .unwrap_or(width as usize * height as usize * veyra_gfx::BYTES_PER_PIXEL);
    let mut out = vec![0u8; buffer_size];
    reader
        .next_frame(&mut out)
        .map_err(|source| ScreenshotError::PngDecode { source })?;
    out.truncate(width as usize * height as usize * veyra_gfx::BYTES_PER_PIXEL);
    Ok(out)
}

/// Byte-compares decoded fixture pixels against the rendered buffer,
/// reporting the first differing pixel with coordinates.
fn compare_pixels(
    fixture: &[u8],
    rendered: &PixelBuffer,
    width: u32,
    height: u32,
) -> Result<(), ScreenshotError> {
    for y in 0..height {
        let row = rendered.row(y).ok_or(ScreenshotError::Render {
            source: veyra_gfx::GfxError::InvalidRect {
                reason: "row index out of range during comparison",
            },
        })?;
        let offset = y as usize * width as usize * veyra_gfx::BYTES_PER_PIXEL;
        if let Some(diff) = find_diff(&fixture[offset..offset + row.len()], row) {
            let x = (offset + diff) / veyra_gfx::BYTES_PER_PIXEL;
            return Err(ScreenshotError::HashMismatch {
                expected: "pixels of the reviewed fixture PNG".to_string(),
                actual: format!(
                    "fresh render (first difference at x={}, y={y}, byte offset {diff})",
                    x % width as usize
                ),
                width,
                height,
            });
        }
    }
    Ok(())
}

fn find_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b.iter()).position(|(x, y)| x != y)
}

/// Renders at the golden viewport and asserts (via the returned report)
/// that regeneration would produce byte-identical fixture files. Used by
/// the drift test — it never writes.
///
/// # Errors
/// As [`verify_against_fixture`], plus
/// [`ScreenshotError::OutputWrite`] when the on-disk bytes differ from a
/// fresh encode.
pub fn fixture_is_current(width: u32, height: u32) -> Result<GoldenReport, ScreenshotError> {
    let report = verify_against_fixture(width, height)?;
    let raster = screenshot::render_to_buffer(width, height, SceneKind::Diagnostic)?;
    let fresh_png = screenshot::encode_png(raster.buffer())?;
    let on_disk =
        fs::read(&report.fixture_path).map_err(|source| ScreenshotError::OutputWrite {
            path: report.fixture_path.clone(),
            source,
        })?;
    if fresh_png != on_disk {
        return Err(ScreenshotError::HashMismatch {
            expected: format!("fixture bytes at {}", report.fixture_path.display()),
            actual: "freshly encoded PNG bytes differ (encoder or scene drift)".to_string(),
            width,
            height,
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn stem_encodes_scene_version_and_viewport() {
        assert_eq!(
            fixture_stem(GOLDEN_WIDTH, GOLDEN_HEIGHT),
            format!("phase1-diagnostic-v{DIAGNOSTIC_SCENE_VERSION}-800x600")
        );
    }

    #[test]
    fn regeneration_into_temp_dir_is_deterministic() {
        let dir = std::env::temp_dir().join(format!("veyra-golden-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let hash_a = write_fixture_into(&dir, 64, 64).unwrap();
        let hash_b = write_fixture_into(&dir, 64, 64).unwrap();
        assert_eq!(hash_a, hash_b);
        assert!(dir.join(format!("{}.png", fixture_stem(64, 64))).is_file());
        assert!(
            dir.join(format!("{}.sha256", fixture_stem(64, 64)))
                .is_file()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

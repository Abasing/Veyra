//! The screenshot harness: deterministic scene render → PNG file +
//! canonical pixel hash, runnable headless (no window, no display).
//!
//! Pipeline and guarantees:
//!
//! 1. A [`CpuRaster`] is allocated at the requested viewport (validated:
//!    zero/oversized/overflowing dimensions are errors, never panics).
//! 2. The diagnostic scene is rendered on the CPU path (integer-only;
//!    see `veyra_gfx::fixture`).
//! 3. The canonical pixel hash is computed over the buffer's canonical
//!    bytes (see [`crate::hashing`] for the exact definition).
//! 4. The buffer is encoded as PNG (RGBA8) and written **atomically**:
//!    encode to an in-memory buffer, write to a sibling temp file, then
//!    rename. A failed write never leaves a partial PNG at the output
//!    path, and the temp file is cleaned up on failure.
//!
//! The stdout report (produced by [`format_report`]) is a stable,
//! machine-parseable `key=value` contract used by CI and scripts:
//!
//! ```text
//! veyra-screenshot v1
//! file=/tmp/veyra-shot.png
//! width=800
//! height=600
//! stride=3200
//! scene=diagnostic-v1
//! pixels-hash=sha256:<64 lowercase hex chars>
//! ```

use std::io::Write as _;
use std::path::{Path, PathBuf};

use veyra_gfx::fixture::{DIAGNOSTIC_SCENE_VERSION, render_diagnostic_scene};
use veyra_gfx::{CpuRaster, RasterSurface};

use crate::config::SceneKind;
use crate::error::ScreenshotError;
use crate::hashing::sha256_hex;

/// Renders the configured scene onto a surface.
///
/// Phase 1 has exactly one scene; the match is the extension point where
/// later phases add scenes (kept here so the shell never calls fixture
/// code ad hoc).
///
/// # Errors
/// Propagates [`ScreenshotError::Render`] from the scene renderer.
pub fn render_scene(
    surface: &mut dyn RasterSurface,
    scene: SceneKind,
) -> Result<(), ScreenshotError> {
    match scene {
        SceneKind::Diagnostic => {
            render_diagnostic_scene(surface).map_err(|source| ScreenshotError::Render { source })
        }
    }
}

/// Renders the configured scene into a fresh CPU buffer.
///
/// Used by the screenshot path and the golden-image tests.
///
/// # Errors
/// [`ScreenshotError::Render`] for invalid dimensions or scene failure.
pub fn render_to_buffer(
    width: u32,
    height: u32,
    scene: SceneKind,
) -> Result<CpuRaster, ScreenshotError> {
    let mut raster =
        CpuRaster::new(width, height).map_err(|source| ScreenshotError::Render { source })?;
    render_scene(&mut raster, scene)?;
    Ok(raster)
}

/// Encodes a pixel buffer as an RGBA8 PNG, deterministically.
///
/// The encoder uses default (deterministic) compression/filter settings:
/// identical input pixels produce identical PNG bytes for a given `png`
/// crate version. Golden identity is nevertheless carried by the
/// canonical *pixel* hash, not the PNG bytes.
///
/// # Errors
/// [`ScreenshotError::PngEncode`] when the encoder fails (header or data
/// stage).
pub fn encode_png(buffer: &veyra_gfx::PixelBuffer) -> Result<Vec<u8>, ScreenshotError> {
    let mut encoded: Vec<u8> = Vec::with_capacity(buffer.data().len() / 2 + 1024);
    {
        let mut encoder = png::Encoder::new(&mut encoded, buffer.width(), buffer.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|source| ScreenshotError::PngEncode { source })?;
        writer
            .write_image_data(buffer.data())
            .map_err(|source| ScreenshotError::PngEncode { source })?;
        writer
            .finish()
            .map_err(|source| ScreenshotError::PngEncode { source })?;
    }
    Ok(encoded)
}

/// A completed screenshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotOutcome {
    /// Path the PNG was written to (after the atomic rename).
    pub path: PathBuf,
    /// Rendered width in pixels.
    pub width: u32,
    /// Rendered height in pixels.
    pub height: u32,
    /// Row stride in bytes.
    pub stride: usize,
    /// Canonical pixel hash (lowercase hex, SHA-256).
    pub pixels_hash: String,
    /// Encoded PNG size in bytes.
    pub png_bytes: usize,
}

/// Parameters for [`capture`].
#[derive(Debug, Clone)]
pub struct ScreenshotRequest {
    /// Viewport width in pixels.
    pub width: u32,
    /// Viewport height in pixels.
    pub height: u32,
    /// Scene to render.
    pub scene: SceneKind,
    /// Output PNG path (written atomically).
    pub output: PathBuf,
    /// When set, the rendered canonical hash must match (lowercase or
    /// uppercase hex) or [`ScreenshotError::HashMismatch`] is returned
    /// *after* the file is written (the image is still useful for
    /// debugging a mismatch).
    pub expect_hash: Option<String>,
}

/// Captures the diagnostic scene: render → hash → encode → atomic write
/// → optional expectation check.
///
/// # Errors
/// Render/allocation errors ([`ScreenshotError::Render`]), PNG encoding
/// ([`ScreenshotError::PngEncode`]), output problems
/// ([`ScreenshotError::OutputWrite`] — including a missing parent
/// directory, reported before anything is written), and expectation
/// mismatch ([`ScreenshotError::HashMismatch`]).
pub fn capture(request: &ScreenshotRequest) -> Result<ScreenshotOutcome, ScreenshotError> {
    let raster = render_to_buffer(request.width, request.height, request.scene)?;
    let buffer = raster.buffer();
    let canonical = buffer.canonical_bytes();
    let pixels_hash = sha256_hex(&canonical);

    let encoded = encode_png(buffer)?;
    write_atomically(&request.output, &encoded)?;

    let outcome = ScreenshotOutcome {
        path: request.output.clone(),
        width: buffer.width(),
        height: buffer.height(),
        stride: buffer.stride(),
        pixels_hash,
        png_bytes: encoded.len(),
    };

    if let Some(expected) = &request.expect_hash {
        match crate::hashing::parse_sha256_hex(expected) {
            Some(expected_bytes) => {
                let actual_bytes =
                    crate::hashing::parse_sha256_hex(&outcome.pixels_hash).unwrap_or([0u8; 32]);
                if expected_bytes != actual_bytes {
                    return Err(ScreenshotError::HashMismatch {
                        expected: expected.to_ascii_lowercase(),
                        actual: outcome.pixels_hash.clone(),
                        width: outcome.width,
                        height: outcome.height,
                    });
                }
            }
            None => {
                return Err(ScreenshotError::InvalidHashSpec {
                    value: expected.clone(),
                });
            }
        }
    }

    Ok(outcome)
}

/// Writes `bytes` to `path` via a sibling temp file + rename.
///
/// The temp file name includes the process id so parallel captures to the
/// same directory cannot collide. On any failure the temp file is removed
/// (best effort) and the target path is never left partially written.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ScreenshotError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(ScreenshotError::OutputWrite {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("parent directory does not exist: {}", parent.display()),
            ),
        });
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "veyra-screenshot.png".to_string());
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();

    match write_result {
        Ok(()) => Ok(()),
        Err(source) => {
            // Best-effort cleanup: never leave a temp file behind.
            let _ = std::fs::remove_file(&tmp_path);
            Err(ScreenshotError::OutputWrite {
                path: path.to_path_buf(),
                source,
            })
        }
    }
}

/// Formats the machine-readable stdout report (stable `key=value` lines —
/// see the module docs for the contract).
pub fn format_report(outcome: &ScreenshotOutcome, scene: SceneKind) -> String {
    format!(
        "veyra-screenshot v1\nfile={}\nwidth={}\nheight={}\nstride={}\nscene={}-v{}\n\
         pixels-hash=sha256:{}\n",
        outcome.path.display(),
        outcome.width,
        outcome.height,
        outcome.stride,
        scene.as_str(),
        DIAGNOSTIC_SCENE_VERSION,
        outcome.pixels_hash
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn encode_png_produces_valid_dimensions() {
        let raster = render_to_buffer(64, 64, SceneKind::Diagnostic).expect("valid");
        let encoded = encode_png(raster.buffer()).expect("encode");
        let decoder = png::Decoder::new(std::io::Cursor::new(&encoded));
        let reader = decoder.read_info().expect("valid png");
        let info = reader.info();
        assert_eq!((info.width, info.height), (64, 64));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
    }

    #[test]
    fn encoding_is_deterministic() {
        let raster = render_to_buffer(64, 64, SceneKind::Diagnostic).expect("valid");
        let a = encode_png(raster.buffer()).expect("encode a");
        let b = encode_png(raster.buffer()).expect("encode b");
        assert_eq!(a, b, "identical pixels must encode to identical bytes");
    }

    #[test]
    fn capture_writes_file_and_reports_hash() {
        let dir = std::env::temp_dir().join(format!("veyra-test-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let output = dir.join("capture.png");
        let request = ScreenshotRequest {
            width: 800,
            height: 600,
            scene: SceneKind::Diagnostic,
            output: output.clone(),
            expect_hash: None,
        };
        let outcome = capture(&request).expect("capture");
        assert!(output.is_file());
        assert_eq!(outcome.width, 800);
        assert_eq!(outcome.height, 600);
        assert_eq!(outcome.stride, 3200);
        assert_eq!(outcome.pixels_hash.len(), 64);
        assert!(outcome.png_bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capture_is_repeatable() {
        let dir = std::env::temp_dir().join(format!("veyra-test-repeat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let first = ScreenshotRequest {
            width: 320,
            height: 240,
            scene: SceneKind::Diagnostic,
            output: dir.join("a.png"),
            expect_hash: None,
        };
        let second = ScreenshotRequest {
            output: dir.join("b.png"),
            ..first.clone()
        };
        let a = capture(&first).expect("capture a");
        let b = capture(&second).expect("capture b");
        assert_eq!(a.pixels_hash, b.pixels_hash);
        assert_eq!(a.png_bytes, b.png_bytes);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capture_expect_hash_accepts_and_rejects() {
        let dir = std::env::temp_dir().join(format!("veyra-test-expect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let output = dir.join("expect.png");
        let probe = ScreenshotRequest {
            width: 100,
            height: 100,
            scene: SceneKind::Diagnostic,
            output: output.clone(),
            expect_hash: None,
        };
        let outcome = capture(&probe).expect("probe capture");
        let good = outcome.pixels_hash.clone();

        let matching = ScreenshotRequest {
            width: 100,
            height: 100,
            scene: SceneKind::Diagnostic,
            output: output.clone(),
            expect_hash: Some(good.to_uppercase()),
        };
        assert!(capture(&matching).is_ok(), "uppercase hex must be accepted");

        let wrong = format!("{}00", &good[..62]);
        let mismatching = ScreenshotRequest {
            width: 100,
            height: 100,
            scene: SceneKind::Diagnostic,
            output: output.clone(),
            expect_hash: Some(wrong),
        };
        let err = capture(&mismatching).expect_err("must mismatch");
        assert!(matches!(err, ScreenshotError::HashMismatch { .. }));
        // The file is still written (useful for debugging mismatches).
        assert!(output.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capture_rejects_missing_parent_directory() {
        let missing = std::env::temp_dir()
            .join(format!("veyra-missing-{}", std::process::id()))
            .join("no-such-dir")
            .join("out.png");
        let request = ScreenshotRequest {
            width: 64,
            height: 64,
            scene: SceneKind::Diagnostic,
            output: missing.clone(),
            expect_hash: None,
        };
        let err = capture(&request).expect_err("missing parent");
        assert!(matches!(err, ScreenshotError::OutputWrite { .. }));
        assert!(err.to_string().contains("parent directory does not exist"));
        assert!(!missing.exists());
    }

    #[test]
    fn failed_write_leaves_no_partial_output() {
        // Write to an existing *directory* path: File::create fails, and no
        // temp file may survive at the sibling location.
        let dir = std::env::temp_dir().join(format!("veyra-test-occupied-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let target = dir.join("occupied");
        std::fs::create_dir_all(&target).expect("create occupied dir");
        let request = ScreenshotRequest {
            width: 64,
            height: 64,
            scene: SceneKind::Diagnostic,
            output: target.clone(),
            expect_hash: None,
        };
        assert!(capture(&request).is_err());
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_format_matches_contract() {
        let outcome = ScreenshotOutcome {
            path: PathBuf::from("/tmp/x.png"),
            width: 800,
            height: 600,
            stride: 3200,
            pixels_hash: "ab".repeat(32),
            png_bytes: 1234,
        };
        let report = format_report(&outcome, SceneKind::Diagnostic);
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines[0], "veyra-screenshot v1");
        assert_eq!(lines[1], "file=/tmp/x.png");
        assert_eq!(lines[2], "width=800");
        assert_eq!(lines[3], "height=600");
        assert_eq!(lines[4], "stride=3200");
        assert_eq!(lines[5], "scene=diagnostic-v1");
        assert_eq!(lines[6], format!("pixels-hash=sha256:{}", "ab".repeat(32)));
    }
}

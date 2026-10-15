//! `veyra-gfx` — Veyra's device abstraction: pixel buffers and the CPU
//! raster surface.
//!
//! Phase 1 scope (roadmap): the [`PixelBuffer`] format contract, the
//! [`RasterSurface`] trait, the [`CpuRaster`] software backend, and the
//! deterministic [`fixture::render_diagnostic_scene`] test scene used by
//! the screenshot harness and golden-image tests. GPU acceleration arrives
//! in Phase 7 behind the same trait (ADR-0005); external graphics crates
//! are isolated inside this crate and never leak past the trait boundary
//! (`docs/architecture/dependency-graph.md` §5).
//!
//! The crate is dependency-free in Phase 1: drawing is integer-only math
//! over owned, zeroed memory, which is what makes the deterministic
//! rendering guarantee enforceable rather than aspirational.
//!
//! [`fixture::render_diagnostic_scene`]: fixture::render_diagnostic_scene

#![forbid(unsafe_code)]

pub mod buffer;
pub mod error;
pub mod fixture;
pub mod raster;
pub mod types;

pub use buffer::{BYTES_PER_PIXEL, MAX_DIMENSION, PixelBuffer};
pub use error::GfxError;
pub use raster::{CpuRaster, RasterSurface};
pub use types::{Rect, Rgba8};

#[cfg(test)]
mod tests {
    // Scoped lint exception, documented per engineering standards §2: the
    // denied `unwrap`/`expect`/`panic` lints target *untrusted input*
    // handling in production paths. Test fixtures are provably internal,
    // and tests fail through the standard assert/panic machinery. This
    // allow is confined to this module; the workspace lint configuration
    // is unchanged and all non-test code keeps the full deny set.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::fixture::{DIAGNOSTIC_SCENE_VERSION, MIN_SCENE_DIMENSION, render_diagnostic_scene};

    /// Test helper: unwraps a setup result with a clear message.
    fn ok<T>(result: Result<T, GfxError>) -> T {
        result.unwrap_or_else(|err| panic!("unexpected gfx error: {err}"))
    }

    fn red() -> Rgba8 {
        Rgba8::opaque(0xFF, 0x00, 0x00)
    }

    #[test]
    fn new_rejects_zero_dimensions() {
        assert_eq!(
            PixelBuffer::new(0, 10),
            Err(GfxError::ZeroDimension { name: "width" })
        );
        assert_eq!(
            PixelBuffer::new(10, 0),
            Err(GfxError::ZeroDimension { name: "height" })
        );
    }

    #[test]
    fn new_rejects_dimensions_over_limit() {
        let over = MAX_DIMENSION + 1;
        assert_eq!(
            PixelBuffer::new(over, 10),
            Err(GfxError::DimensionExceedsLimit {
                name: "width",
                value: over,
                limit: MAX_DIMENSION
            })
        );
        assert_eq!(
            PixelBuffer::new(10, over),
            Err(GfxError::DimensionExceedsLimit {
                name: "height",
                value: over,
                limit: MAX_DIMENSION
            })
        );
    }

    #[test]
    fn new_zeroes_the_buffer() {
        let buf = ok(PixelBuffer::new(4, 4));
        // Zeroing is part of the determinism contract: hashes must not
        // depend on prior heap contents.
        assert!(buf.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn stride_must_cover_row() {
        assert_eq!(
            PixelBuffer::new_with_stride(4, 4, 15),
            Err(GfxError::StrideTooSmall {
                stride: 15,
                required: 16
            })
        );
        let buf = ok(PixelBuffer::new_with_stride(4, 4, 20));
        assert_eq!(buf.stride(), 20);
        assert_eq!(buf.data().len(), 80);
    }

    #[test]
    fn from_parts_validates_length() {
        let good = vec![0u8; 4 * 4 * 4];
        assert!(ok(PixelBuffer::from_parts(4, 4, 16, good)).data().len() == 64);

        let short = vec![0u8; 4 * 4 * 4 - 1];
        assert_eq!(
            PixelBuffer::from_parts(4, 4, 16, short),
            Err(GfxError::BufferLengthMismatch {
                expected: 64,
                actual: 63
            })
        );

        let long = vec![0u8; 4 * 4 * 4 + 1];
        assert_eq!(
            PixelBuffer::from_parts(4, 4, 16, long),
            Err(GfxError::BufferLengthMismatch {
                expected: 64,
                actual: 65
            })
        );
    }

    #[test]
    fn get_set_pixel_round_trip_and_bounds() {
        let mut buf = ok(PixelBuffer::new(3, 3));
        ok(buf.set_pixel(2, 1, red()));
        assert_eq!(buf.get_pixel(2, 1), Ok(red()));
        assert_eq!(
            buf.get_pixel(3, 1),
            Err(GfxError::InvalidRect {
                reason: "pixel coordinates are outside the buffer"
            })
        );
        assert_eq!(
            buf.set_pixel(0, 3, red()),
            Err(GfxError::InvalidRect {
                reason: "pixel coordinates are outside the buffer"
            })
        );
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut buf = ok(PixelBuffer::new(8, 8));
        let rect = ok(Rect::new(6, 6, 100, 100));
        ok(buf.fill_rect(rect, red()));
        // Only the 2x2 region inside bounds is red.
        assert_eq!(buf.get_pixel(6, 6), Ok(red()));
        assert_eq!(buf.get_pixel(7, 7), Ok(red()));
        assert_eq!(buf.get_pixel(5, 5), Ok(Rgba8::default()));
        assert!(buf.get_pixel(8, 8).is_err());
    }

    #[test]
    fn fill_rect_entirely_outside_is_a_noop() {
        let mut buf = ok(PixelBuffer::new(4, 4));
        let rect = ok(Rect::new(10, 10, 4, 4));
        assert_eq!(buf.fill_rect(rect, red()), Ok(()));
        assert!(buf.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn rect_new_rejects_zero_and_overflow() {
        assert!(Rect::new(0, 0, 0, 4).is_err());
        assert!(Rect::new(0, 0, 4, 0).is_err());
        assert!(Rect::new(u32::MAX - 1, 0, 4, 4).is_err());
        assert!(Rect::new(0, u32::MAX - 1, 4, 4).is_err());
        assert!(Rect::new(1, 1, 4, 4).is_ok());
    }

    #[test]
    fn intersect_handles_partial_and_empty() {
        let rect = ok(Rect::new(2, 2, 10, 10));
        let clipped = rect.intersect(8, 8).expect("partial overlap must clip");
        assert_eq!(
            (clipped.x, clipped.y, clipped.width, clipped.height),
            (2, 2, 6, 6)
        );
        assert_eq!(rect.intersect(1, 8), None);
        assert_eq!(rect.intersect(8, 1), None);
    }

    #[test]
    fn padding_bytes_do_not_affect_canonical_bytes() {
        let mut plain = ok(PixelBuffer::new(3, 2));
        let mut padded = ok(PixelBuffer::new_with_stride(3, 2, 16));
        // Paint both buffers with the same pixels.
        ok(plain.fill_rect(ok(Rect::new(0, 0, 3, 2)), red()));
        ok(padded.fill_rect(ok(Rect::new(0, 0, 3, 2)), red()));
        // Canonical bytes must be identical even though the raw allocations
        // differ (the padded buffer carries extra stride bytes).
        assert_eq!(plain.canonical_bytes(), padded.canonical_bytes());
        assert_eq!(plain.canonical_bytes().len(), 13 + 8 + 2 * 3 * 4);
        // And the padded buffer's padding stayed untouched/zero.
        assert_eq!(padded.data().len(), 16 * 2);
        assert!(padded.row(1).map(|r| r.len()).unwrap_or(0) == 12);
    }

    #[test]
    fn canonical_bytes_have_the_documented_header() {
        let buf = ok(PixelBuffer::new(2, 3));
        let bytes = buf.canonical_bytes();
        assert_eq!(&bytes[..13], b"VEYRA-CANON1\n");
        assert_eq!(&bytes[13..17], &2u32.to_le_bytes());
        assert_eq!(&bytes[17..21], &3u32.to_le_bytes());
        assert_eq!(bytes.len(), 13 + 8 + 3 * 2 * 4);
    }

    #[test]
    fn cpu_raster_implements_surface() {
        let mut raster = ok(CpuRaster::new(32, 32));
        assert_eq!(raster.dimensions(), (32, 32));
        raster.clear(red());
        assert_eq!(raster.buffer().get_pixel(0, 0), Ok(red()));
        let rect = ok(Rect::new(0, 0, 4, 4));
        assert_eq!(raster.fill_rect(rect, Rgba8::default()), Ok(()));
        assert_eq!(raster.buffer().get_pixel(0, 0), Ok(Rgba8::default()));
    }

    #[test]
    fn scene_is_deterministic_across_runs() {
        let mut a = ok(CpuRaster::new(320, 240));
        let mut b = ok(CpuRaster::new(320, 240));
        ok(render_diagnostic_scene(&mut a));
        ok(render_diagnostic_scene(&mut b));
        assert_eq!(a.buffer().data(), b.buffer().data());
    }

    #[test]
    fn scene_rejects_viewports_below_minimum() {
        let mut tiny = ok(CpuRaster::new(MIN_SCENE_DIMENSION - 1, 240));
        assert!(render_diagnostic_scene(&mut tiny).is_err());
        let mut ok_surface = ok(CpuRaster::new(MIN_SCENE_DIMENSION, MIN_SCENE_DIMENSION));
        assert_eq!(render_diagnostic_scene(&mut ok_surface), Ok(()));
    }

    #[test]
    fn scene_draws_expected_landmark_pixels() {
        let mut raster = ok(CpuRaster::new(320, 240));
        ok(render_diagnostic_scene(&mut raster));
        let buf = raster.buffer();
        // The exact center is inside the white core of the target.
        assert_eq!(buf.get_pixel(160, 120), Ok(Rgba8::opaque(0xFF, 0xFF, 0xFF)));
        // Top-left corner marker is red (frame is 3 px thick at 320 width).
        assert_eq!(buf.get_pixel(3, 3), Ok(Rgba8::opaque(0xE0, 0x30, 0x30)));
        // Border frame pixel on the top edge, away from markers.
        assert_eq!(buf.get_pixel(160, 0), Ok(Rgba8::opaque(0xE8, 0xE8, 0xE8)));
        // A bar pixel: first bar (white) inside the top strip.
        assert_eq!(buf.get_pixel(20, 41), Ok(Rgba8::opaque(0xFF, 0xFF, 0xFF)));
    }

    #[test]
    fn scene_version_is_stable_within_this_build() {
        // The golden fixtures embed this version in their filenames; the
        // value must only change with a deliberate fixture regeneration.
        assert_eq!(DIAGNOSTIC_SCENE_VERSION, 1);
    }
}

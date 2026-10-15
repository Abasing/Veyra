//! The `veyra-gfx` surface abstraction and its Phase 1 CPU backend.
//!
//! ADR-0005 fixes the shape: `veyra-gfx` stays thin — buffer/surface
//! abstractions only, no scene graph (display lists are `veyra-paint`'s
//! job, Phase 3). [`RasterSurface`] is the interface the application shell
//! programs against; the Phase 7 GPU backend will implement the same
//! contract behind `wgpu`, so the shell does not change when the backend
//! does. The CPU backend ([`CpuRaster`]) is the determinism anchor: golden
//! images and CI hashes always come from this path, never from GPU output.

use crate::buffer::PixelBuffer;
use crate::error::GfxError;
use crate::types::{Rect, Rgba8};

/// A drawable 2D surface that produces pixels Veyra can read back.
///
/// This is the Phase 1 contract, deliberately minimal: clear, fill
/// rectangles, and access the resulting pixel buffer. It exists so the
/// application shell depends on the interface rather than the CPU
/// implementation (ADR-0005 decision 3).
///
/// # Determinism contract
///
/// Implementations used by the test/screenshot harness must render
/// identically for identical calls on any machine: no GPU involvement, no
/// uninitialized memory, no environment-dependent behavior. The CPU
/// backend satisfies this by construction (integer-only drawing into a
/// zeroed buffer).
///
/// # Errors
///
/// Recoverable failures (invalid rects, allocation refusal) are returned
/// as [`GfxError`]; the surface is never left partially drawn *before* the
/// failing operation — a failed operation may leave earlier operations of
/// the same frame applied, which callers treat as a frame to discard.
pub trait RasterSurface {
    /// The surface's `(width, height)` in pixels.
    fn dimensions(&self) -> (u32, u32);

    /// Read access to the current frame's pixels.
    fn buffer(&self) -> &PixelBuffer;

    /// Fills every pixel with `color`.
    fn clear(&mut self, color: Rgba8);

    /// Fills `rect` with `color`, clipping to the surface bounds.
    ///
    /// # Errors
    /// [`GfxError::InvalidRect`] for a malformed rectangle.
    fn fill_rect(&mut self, rect: Rect, color: Rgba8) -> Result<(), GfxError>;
}

/// The CPU raster backend: draws into an owned [`PixelBuffer`] with plain
/// integer arithmetic. No GPU, no threads, no environment variance.
#[derive(Debug, Clone)]
pub struct CpuRaster {
    buffer: PixelBuffer,
}

impl CpuRaster {
    /// Creates a CPU surface with no row padding.
    ///
    /// # Errors
    /// Dimension/allocation errors from [`PixelBuffer::new`].
    pub fn new(width: u32, height: u32) -> Result<Self, GfxError> {
        Ok(Self {
            buffer: PixelBuffer::new(width, height)?,
        })
    }

    /// Creates a CPU surface with an explicit row stride (bytes).
    ///
    /// # Errors
    /// Dimension/stride/allocation errors from [`PixelBuffer::new_with_stride`].
    pub fn with_stride(width: u32, height: u32, stride: usize) -> Result<Self, GfxError> {
        Ok(Self {
            buffer: PixelBuffer::new_with_stride(width, height, stride)?,
        })
    }
}

impl RasterSurface for CpuRaster {
    fn dimensions(&self) -> (u32, u32) {
        (self.buffer.width(), self.buffer.height())
    }

    fn buffer(&self) -> &PixelBuffer {
        &self.buffer
    }

    fn clear(&mut self, color: Rgba8) {
        self.buffer.clear(color);
    }

    fn fill_rect(&mut self, rect: Rect, color: Rgba8) -> Result<(), GfxError> {
        self.buffer.fill_rect(rect, color)
    }
}

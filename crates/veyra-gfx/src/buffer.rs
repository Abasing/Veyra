//! The CPU pixel buffer: Veyra's Phase 1 image representation.
//!
//! Format contract (stable — the screenshot harness, golden fixtures, and
//! CI hash checks all depend on it):
//!
//! - **Channels:** RGBA, 8 bits each, straight alpha, byte order R, G, B, A.
//! - **Stride:** `stride` is the row pitch **in bytes** and must be at
//!   least `width * 4`. Rows may carry padding; padding bytes are excluded
//!   from the canonical representation.
//! - **Length:** `data.len() == stride * height` exactly for owned buffers
//!   and [`PixelBuffer::from_parts`].
//! - **Ownership:** [`PixelBuffer`] owns its storage (`Vec<u8>`, zeroed on
//!   construction — never uninitialized memory, so hashes cannot depend on
//!   prior heap contents). No `unsafe` is used anywhere in the crate.
//! - **Canonical form:** see [`PixelBuffer::canonical_bytes`]. Hash the
//!   canonical form (never the raw allocation, which includes padding, and
//!   never the encoded PNG, which carries incidental metadata).

use crate::error::GfxError;
use crate::types::{Rect, Rgba8};

/// Bytes per pixel in the Phase 1 RGBA8 format.
pub const BYTES_PER_PIXEL: usize = 4;

/// Inclusive upper bound for any single buffer dimension.
///
/// Phase 1 is a test/diagnostics rig, not a content pipeline: the limit
/// (16,384 = 2^14) is a sanity guard against absurd allocations from bad
/// configuration, not a product limit. Revisit when real content phases
/// need larger viewports (and when allocation-failure handling is exercised
/// on real hardware).
pub const MAX_DIMENSION: u32 = 16_384;

fn validate_dimensions(width: u32, height: u32) -> Result<(), GfxError> {
    if width == 0 {
        return Err(GfxError::ZeroDimension { name: "width" });
    }
    if height == 0 {
        return Err(GfxError::ZeroDimension { name: "height" });
    }
    if width > MAX_DIMENSION {
        return Err(GfxError::DimensionExceedsLimit {
            name: "width",
            value: width,
            limit: MAX_DIMENSION,
        });
    }
    if height > MAX_DIMENSION {
        return Err(GfxError::DimensionExceedsLimit {
            name: "height",
            value: height,
            limit: MAX_DIMENSION,
        });
    }
    Ok(())
}

fn required_len(stride: usize, height: u32, what: &'static str) -> Result<usize, GfxError> {
    let stride_u64 = stride as u64;
    let height_u64 = height as u64;
    stride_u64
        .checked_mul(height_u64)
        .filter(|len| *len <= usize::MAX as u64)
        .map(|len| len as usize)
        .ok_or(GfxError::Overflow { operation: what })
}

/// An owned RGBA8 pixel buffer with explicit stride and validated invariants.
///
/// Construction validates dimensions ([`MAX_DIMENSION`]), stride, and
/// allocation; drawing clips to bounds; serialization excludes padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelBuffer {
    width: u32,
    height: u32,
    stride: usize,
    data: Vec<u8>,
}

impl PixelBuffer {
    /// Allocates a zeroed buffer with no row padding
    /// (`stride == width * BYTES_PER_PIXEL`).
    ///
    /// Zeroing (rather than leaving memory uninitialized) is deliberate:
    /// the deterministic-rendering guarantees in
    /// `docs/architecture/rendering-pipeline.md` require that output cannot
    /// depend on prior heap contents.
    ///
    /// # Errors
    /// [`GfxError::ZeroDimension`], [`GfxError::DimensionExceedsLimit`],
    /// [`GfxError::Overflow`], or [`GfxError::AllocationFailed`] (large but
    /// valid requests are recoverable errors, not aborts).
    pub fn new(width: u32, height: u32) -> Result<Self, GfxError> {
        validate_dimensions(width, height)?;
        let stride = Self::min_stride(width)?;
        Self::new_with_stride(width, height, stride)
    }

    /// Allocates a zeroed buffer with an explicit row stride (in bytes).
    ///
    /// # Errors
    /// As [`PixelBuffer::new`], plus [`GfxError::StrideTooSmall`] when
    /// `stride < width * 4`.
    pub fn new_with_stride(width: u32, height: u32, stride: usize) -> Result<Self, GfxError> {
        validate_dimensions(width, height)?;
        let required = Self::min_stride(width)?;
        if stride < required {
            return Err(GfxError::StrideTooSmall { stride, required });
        }
        let len = required_len(stride, height, "stride * height")?;
        let mut data = Vec::new();
        data.try_reserve_exact(len)
            .map_err(|_| GfxError::AllocationFailed { bytes: len })?;
        data.resize(len, 0);
        Ok(Self {
            width,
            height,
            stride,
            data,
        })
    }

    /// Adopts caller-provided storage after full invariant validation.
    ///
    /// Useful for buffers that arrive from outside the crate; ownership of
    /// `data` moves into the returned buffer.
    ///
    /// # Errors
    /// Dimension/stride errors as above, or
    /// [`GfxError::BufferLengthMismatch`] when `data.len() != stride * height`.
    pub fn from_parts(
        width: u32,
        height: u32,
        stride: usize,
        data: Vec<u8>,
    ) -> Result<Self, GfxError> {
        validate_dimensions(width, height)?;
        let required = Self::min_stride(width)?;
        if stride < required {
            return Err(GfxError::StrideTooSmall { stride, required });
        }
        let expected = required_len(stride, height, "stride * height")?;
        if data.len() != expected {
            return Err(GfxError::BufferLengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            stride,
            data,
        })
    }

    /// The minimum valid stride for `width` (`width * 4`), with overflow
    /// checked.
    pub fn min_stride(width: u32) -> Result<usize, GfxError> {
        (width as usize)
            .checked_mul(BYTES_PER_PIXEL)
            .ok_or(GfxError::Overflow {
                operation: "width * bytes-per-pixel",
            })
    }

    /// Buffer width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Buffer height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Row stride in bytes (`>= width * 4`).
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// The full backing storage (including any row padding).
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable access to the full backing storage.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// The bytes of row `y`, excluding padding.
    ///
    /// Returns `None` for `y >= height`.
    pub fn row(&self, y: u32) -> Option<&[u8]> {
        let y = y as usize;
        let used = self.used_row_len();
        let start = y.checked_mul(self.stride)?;
        let end = start.checked_add(used)?;
        self.data.get(start..end)
    }

    /// Mutable bytes of row `y`, excluding padding.
    ///
    /// Returns `None` for `y >= height`.
    pub fn row_mut(&mut self, y: u32) -> Option<&mut [u8]> {
        let y = y as usize;
        let used = self.used_row_len();
        let start = y.checked_mul(self.stride)?;
        let end = start.checked_add(used)?;
        self.data.get_mut(start..end)
    }

    fn used_row_len(&self) -> usize {
        self.width as usize * BYTES_PER_PIXEL
    }

    /// Reads the pixel at `(x, y)`.
    ///
    /// # Errors
    /// [`GfxError::InvalidRect`] when the coordinates are outside the
    /// buffer (the error type is shared with rect validation; the reason
    /// text distinguishes the case).
    pub fn get_pixel(&self, x: u32, y: u32) -> Result<Rgba8, GfxError> {
        if x >= self.width || y >= self.height {
            return Err(GfxError::InvalidRect {
                reason: "pixel coordinates are outside the buffer",
            });
        }
        let row = self.row(y).ok_or(GfxError::InvalidRect {
            reason: "pixel coordinates are outside the buffer",
        })?;
        let base = x as usize * BYTES_PER_PIXEL;
        Ok(Rgba8 {
            r: row[base],
            g: row[base + 1],
            b: row[base + 2],
            a: row[base + 3],
        })
    }

    /// Writes the pixel at `(x, y)`.
    ///
    /// # Errors
    /// [`GfxError::InvalidRect`] when the coordinates are outside the buffer.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba8) -> Result<(), GfxError> {
        if x >= self.width || y >= self.height {
            return Err(GfxError::InvalidRect {
                reason: "pixel coordinates are outside the buffer",
            });
        }
        let row = self.row_mut(y).ok_or(GfxError::InvalidRect {
            reason: "pixel coordinates are outside the buffer",
        })?;
        let base = x as usize * BYTES_PER_PIXEL;
        row[base] = color.r;
        row[base + 1] = color.g;
        row[base + 2] = color.b;
        row[base + 3] = color.a;
        Ok(())
    }

    /// Fills every pixel with `color` (padding bytes are untouched).
    pub fn clear(&mut self, color: Rgba8) {
        for y in 0..self.height {
            if let Some(row) = self.row_mut(y) {
                for px in row.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
                    px[0] = color.r;
                    px[1] = color.g;
                    px[2] = color.b;
                    px[3] = color.a;
                }
            }
        }
    }

    /// Fills `rect` with `color`, clipping to the buffer bounds.
    ///
    /// Clipping (rather than an error) is the raster contract: scene code
    /// may size rectangles from layout math that can exceed the viewport,
    /// and the visible result must still be well-defined. Rectangles that
    /// fall entirely outside produce no effect and no error.
    ///
    /// # Errors
    /// [`GfxError::InvalidRect`] if `rect` was constructed invalidly
    /// (zero size or overflowing edges) — [`Rect::new`] guarantees
    /// validity, so this only fires for hand-built rects.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba8) -> Result<(), GfxError> {
        let Some(clip) = rect.intersect(self.width, self.height) else {
            return Ok(());
        };
        let used = self.used_row_len();
        let x0 = clip.x as usize * BYTES_PER_PIXEL;
        let x1 = clip.right() as usize * BYTES_PER_PIXEL;
        debug_assert!(
            x1 <= used,
            "clipped rectangle exceeds the used row; \
            intersect() is wrong or the buffer geometry is inconsistent"
        );
        for y in clip.y..clip.bottom() {
            let row = match self.row_mut(y) {
                Some(row) if row.len() >= x1 => &mut row[x0..x1],
                _ => continue,
            };
            for px in row.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
                px[0] = color.r;
                px[1] = color.g;
                px[2] = color.b;
                px[3] = color.a;
            }
        }
        Ok(())
    }

    /// Serializes the buffer in its **canonical representation**.
    ///
    /// Layout (documented so any tool can reproduce the hash):
    ///
    /// ```text
    /// "VEYRA-CANON1\n"           (13-byte ASCII magic + version)
    /// width  as u32 little-endian
    /// height as u32 little-endian
    /// then, for each row in order: the width*4 used bytes (no padding)
    /// ```
    ///
    /// Padding and allocator state are deliberately excluded: two buffers
    /// with identical images hash identically regardless of stride. The
    /// hash over these bytes (SHA-256 in the shell harness) is the image
    /// identity used by golden fixtures and CI.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let used = self.used_row_len();
        let mut out = Vec::with_capacity(13 + 8 + used * self.height as usize);
        out.extend_from_slice(b"VEYRA-CANON1\n");
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        for y in 0..self.height {
            if let Some(row) = self.row(y) {
                out.extend_from_slice(row);
            }
        }
        out
    }
}

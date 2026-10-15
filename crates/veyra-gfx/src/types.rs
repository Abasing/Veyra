//! Colors and rectangles used by the raster surface.

use crate::error::GfxError;

/// An 8-bit-per-channel RGBA color, straight (non-premultiplied) alpha.
///
/// Veyra's Phase 1 pixel format is RGBA8 with a byte order of R, G, B, A in
/// memory (little-endian `u32` value `0xAABBGGRR`). The diagnostic scene is
/// fully opaque; alpha is carried in the format for forward compatibility
/// with the Phase 3+ paint pipeline but never blended in Phase 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba8 {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (255 = fully opaque).
    pub a: u8,
}

impl Rgba8 {
    /// Creates a color from channel values.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a fully opaque color.
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Packs into the presentation format used by the shell's window
    /// backend: a `u32` laid out as `0x00RRGGBB` (high byte must be zero;
    /// the shell's presenter converts RGBA8 bytes into this form — the
    /// alpha byte is dropped because the Phase 1 window surface is opaque).
    pub const fn to_rgb0_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

/// An axis-aligned rectangle in pixel coordinates.
///
/// Coordinates are unsigned: the raster surface's origin is the top-left
/// pixel, `x` grows right and `y` grows down. [`Rect::new`] rejects
/// zero-sized rectangles and coordinate combinations that would overflow
/// `u32`; drawing APIs clip rectangles to the buffer bounds instead of
/// erroring, so a [`Rect`] only has to be internally consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge (inclusive), in pixels.
    pub x: u32,
    /// Top edge (inclusive), in pixels.
    pub y: u32,
    /// Width in pixels (>= 1).
    pub width: u32,
    /// Height in pixels (>= 1).
    pub height: u32,
}

impl Rect {
    /// Validates and creates a rectangle.
    ///
    /// # Errors
    /// Returns [`GfxError::InvalidRect`] for a zero width/height or for
    /// `x + width` / `y + height` overflow.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, GfxError> {
        if width == 0 || height == 0 {
            return Err(GfxError::InvalidRect {
                reason: "width and height must be at least 1",
            });
        }
        x.checked_add(width).ok_or(GfxError::InvalidRect {
            reason: "x + width overflows u32",
        })?;
        y.checked_add(height).ok_or(GfxError::InvalidRect {
            reason: "y + height overflows u32",
        })?;
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// Returns the exclusive right edge (`x + width`).
    pub const fn right(&self) -> u32 {
        self.x + self.width
    }

    /// Returns the exclusive bottom edge (`y + height`).
    pub const fn bottom(&self) -> u32 {
        self.y + self.height
    }

    /// Clips this rectangle to the given bounds, returning `None` when the
    /// result would be empty. Integer math only; cannot fail for in-range
    /// inputs because both rectangles are validated to be non-overflowing.
    pub fn intersect(&self, bounds_width: u32, bounds_height: u32) -> Option<Rect> {
        let left = self.x.min(bounds_width);
        let top = self.y.min(bounds_height);
        let right = self.right().min(bounds_width);
        let bottom = self.bottom().min(bounds_height);
        if right <= left || bottom <= top {
            return None;
        }
        Some(Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

//! Geometry: logical-axis layout coordinates and their physical mapping.
//!
//! Invariant R3 (rendering-pipeline.md): layout is written in **logical
//! axes** (inline = the reading direction, block = the line-stacking
//! direction); physical coordinates appear only at the boundary — when a
//! laid-out box becomes a fragment with a page-coordinate rectangle. The
//! mapping lives here, so adding vertical writing modes later (Phase 7)
//! touches this module and the fragment emitter, not the layout
//! algorithms.
//!
//! Phase 3 implements the `horizontal-tb` writing mode only (the working
//! property set has no `writing-mode`/`direction` properties yet); the
//! types below are the seam the later modes plug into. All arithmetic is
//! f32 with add/subtract/multiply/divide/min/max only — no transcendentals,
//! so results are bit-identical across platforms for identical inputs
//! (the determinism contract the golden dumps rely on).

/// The writing mode a layout run operates in (CSS Writing Modes 4; the
/// physical mapping of the logical axes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    /// `horizontal-tb`: inline axis = x (left → right for the current
    /// working set; `direction` is a recorded deferral), block axis = y
    /// (top → bottom).
    #[default]
    HorizontalTb,
}

impl WritingMode {
    /// Whether the inline axis is horizontal in this mode.
    #[must_use]
    pub fn inline_is_horizontal(self) -> bool {
        match self {
            WritingMode::HorizontalTb => true,
        }
    }
}

/// A size in the logical axes (px).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LogicalSize {
    /// Extent along the inline axis (line width in `horizontal-tb`).
    pub inline: f32,
    /// Extent along the block axis (line height in `horizontal-tb`).
    pub block: f32,
}

/// An offset in the logical axes, measured from the content-box start
/// corner (px).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LogicalOffset {
    /// Offset along the inline axis.
    pub inline: f32,
    /// Offset along the block axis.
    pub block: f32,
}

impl LogicalOffset {
    /// Adds two offsets (composing relative positions).
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        LogicalOffset {
            inline: self.inline + other.inline,
            block: self.block + other.block,
        }
    }
}

/// A physical axis-aligned rectangle in page coordinates (px, f32).
///
/// `x`/`y` are the top-left corner; the coordinate system matches the
/// raster surface (origin top-left, x grows right, y grows down) so the
/// paint batch consumes fragments without a flip.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PhysicalRect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width (>= 0).
    pub width: f32,
    /// Height (>= 0).
    pub height: f32,
}

impl PhysicalRect {
    /// Builds a rectangle (negative sizes clamp to zero — a laid-out box
    /// can legally resolve to an empty rect; layout never errors on it).
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        PhysicalRect {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    /// The exclusive right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The exclusive bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Translates by `(dx, dy)` (page-coordinate composition when a
    /// subtree is placed inside its parent).
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        PhysicalRect::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}

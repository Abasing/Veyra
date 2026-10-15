//! The deterministic diagnostic scene — Veyra's Phase 1 test pattern.
//!
//! This is **test infrastructure, not web-content rendering**. The scene
//! exercises the CPU raster path with shapes that catch the classic
//! failure modes of a new raster stack:
//!
//! - a flat background (catches uninitialized/zeroed-buffer confusion),
//! - a border frame (catches off-by-one at the edges),
//! - hard color bars (catches channel-order swaps, R/B flips),
//! - a gray ramp (catches quantization drift),
//! - a checkerboard (catches row-stride errors — padding corruption shows
//!   as jagged edges),
//! - corner markers in distinct colors (catches horizontal/vertical
//!   mirroring and rotation),
//! - a concentric center target (catches coordinate-origin errors).
//!
//! Everything is integer arithmetic on validated rectangles: identical
//! calls produce identical bytes on every platform and run. The scene
//! version is part of the golden-fixture filename; any pixel-visible
//! change requires bumping [`DIAGNOSTIC_SCENE_VERSION`] and regenerating
//! fixtures through the documented review procedure (see
//! `crates/veyra-shell/tests/golden.rs`).
//!
//! This module will be superseded by the display-list pipeline
//! (`veyra-paint`, Phase 3) per `docs/architecture/rendering-pipeline.md`;
//! until then it is the rendering truth the golden harness checks.

use crate::error::GfxError;
use crate::raster::RasterSurface;
use crate::types::{Rect, Rgba8};

/// Version of the diagnostic scene. Bump on any pixel-visible change.
pub const DIAGNOSTIC_SCENE_VERSION: u32 = 1;

/// The smallest viewport the scene is designed for. Below this, layout
/// integers could degenerate (bars narrower than a pixel); the harness
/// rejects smaller viewports instead of rendering something misleading.
pub const MIN_SCENE_DIMENSION: u32 = 64;

/// Renders the diagnostic scene, version [`DIAGNOSTIC_SCENE_VERSION`],
/// filling the whole surface.
///
/// The layout scales with the surface dimensions using integer division;
/// remainders are absorbed by the background so no rectangle is placed
/// outside the bounds. All drawing clips to the surface.
///
/// # Errors
/// Returns [`GfxError`] from the surface's draw calls (malformed rect).
/// Returns [`GfxError::DimensionExceedsLimit`]-style errors indirectly via
/// construction — callers create surfaces through [`crate::CpuRaster::new`],
/// which validates dimensions up front; this function additionally rejects
/// viewports below [`MIN_SCENE_DIMENSION`].
pub fn render_diagnostic_scene(surface: &mut dyn RasterSurface) -> Result<(), GfxError> {
    let (width, height) = surface.dimensions();
    if width < MIN_SCENE_DIMENSION || height < MIN_SCENE_DIMENSION {
        return Err(GfxError::InvalidRect {
            reason: "viewport is below the diagnostic scene minimum",
        });
    }

    // 1. Background: dark slate.
    surface.clear(Rgba8::opaque(0x20, 0x20, 0x28));

    // 2. Border frame: light gray, thickness scales but stays >= 2 px.
    let border = (width / 100).clamp(2, 16);
    let frame = Rgba8::opaque(0xE8, 0xE8, 0xE8);
    surface.fill_rect(Rect::new(0, 0, width, border)?, frame)?;
    surface.fill_rect(Rect::new(0, height - border, width, border)?, frame)?;
    surface.fill_rect(Rect::new(0, 0, border, height)?, frame)?;
    surface.fill_rect(Rect::new(width - border, 0, border, height)?, frame)?;

    let inner_w = width - 2 * border;

    // 3. Color bars: eight hard bars (white, yellow, cyan, green, magenta,
    //    red, blue, black). Bar width is integer-divided; the last bar
    //    absorbs the remainder so the strip exactly fills the inner width.
    const BAR_COLORS: [Rgba8; 8] = [
        Rgba8::opaque(0xFF, 0xFF, 0xFF),
        Rgba8::opaque(0xFF, 0xFF, 0x00),
        Rgba8::opaque(0x00, 0xFF, 0xFF),
        Rgba8::opaque(0x00, 0xFF, 0x00),
        Rgba8::opaque(0xFF, 0x00, 0xFF),
        Rgba8::opaque(0xFF, 0x00, 0x00),
        Rgba8::opaque(0x00, 0x00, 0xFF),
        Rgba8::opaque(0x00, 0x00, 0x00),
    ];
    let bar_h = height / 6;
    let bar_w = inner_w / 8;
    for (i, color) in BAR_COLORS.iter().enumerate() {
        let i = i as u32;
        let is_last = i == 7;
        let x = border + i * bar_w;
        let w = if is_last { inner_w - 7 * bar_w } else { bar_w };
        surface.fill_rect(Rect::new(x, border, w, bar_h)?, *color)?;
    }

    // 4. Gray ramp: 16 steps from black to white, pure integer division.
    let ramp_y = border + bar_h;
    let ramp_h = height / 8;
    let step_w = inner_w / 16;
    for i in 0..16u32 {
        let value = (i * 255 / 15) as u8;
        let is_last = i == 15;
        let x = border + i * step_w;
        let w = if is_last {
            inner_w - 15 * step_w
        } else {
            step_w
        };
        surface.fill_rect(
            Rect::new(x, ramp_y, w, ramp_h)?,
            Rgba8::opaque(value, value, value),
        )?;
    }

    // 5. Checkerboard: two grays, cell size derived from height.
    let check_y = ramp_y + ramp_h;
    let check_h = height / 3;
    let cell = (height / 32).clamp(4, 64);
    let cols = inner_w / cell;
    let rows = check_h / cell;
    let gray_a = Rgba8::opaque(0xB0, 0xB0, 0xB0);
    let gray_b = Rgba8::opaque(0x58, 0x58, 0x58);
    for r in 0..rows {
        for c in 0..cols {
            let color = if (r + c) % 2 == 0 { gray_a } else { gray_b };
            let x = border + c * cell;
            let y = check_y + r * cell;
            surface.fill_rect(Rect::new(x, y, cell, cell)?, color)?;
        }
    }

    // 6. Corner markers: distinct colors — any mirroring/rotation of the
    //    output is immediately visible in a golden diff. Drawn over the
    //    frame corners (top-left red, top-right green, bottom-left blue,
    //    bottom-right yellow).
    let marker = (height / 24).clamp(4, 24);
    let ml = border; // left x
    let mr = width - border - marker; // right x
    let mt = border; // top y
    let mb = height - border - marker; // bottom y
    surface.fill_rect(
        Rect::new(ml, mt, marker, marker)?,
        Rgba8::opaque(0xE0, 0x30, 0x30),
    )?;
    surface.fill_rect(
        Rect::new(mr, mt, marker, marker)?,
        Rgba8::opaque(0x30, 0xC0, 0x40),
    )?;
    surface.fill_rect(
        Rect::new(ml, mb, marker, marker)?,
        Rgba8::opaque(0x30, 0x60, 0xE0),
    )?;
    surface.fill_rect(
        Rect::new(mr, mb, marker, marker)?,
        Rgba8::opaque(0xE0, 0xC0, 0x30),
    )?;

    // 7. Concentric center target: nested opaque rectangles red > green >
    //    blue > white center block, centered by integer math.
    let cx = width / 2;
    let cy = height / 2;
    let target = height / 5;
    let rings: [(u32, Rgba8); 3] = [
        (target, Rgba8::opaque(0xE0, 0x20, 0x20)),
        (target * 2 / 3, Rgba8::opaque(0x20, 0xC0, 0x40)),
        (target / 3, Rgba8::opaque(0x30, 0x60, 0xE0)),
    ];
    for (size, color) in rings {
        surface.fill_rect(Rect::new(cx - size / 2, cy - size / 2, size, size)?, color)?;
    }
    let center = (height / 48).clamp(2, 8);
    surface.fill_rect(
        Rect::new(cx - center / 2, cy - center / 2, center, center)?,
        Rgba8::opaque(0xFF, 0xFF, 0xFF),
    )?;

    Ok(())
}

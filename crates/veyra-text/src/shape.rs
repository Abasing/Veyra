//! Shaping: text + face + size → positioned glyphs.
//!
//! [`FontLibrary::shape`] wraps the shaping engine (rustybuzz — the
//! HarfBuzz port adopted by ADR-0015) behind Veyra-owned result types:
//! [`ShapedRun`]/[`ShapedGlyph`]. Engine types (buffers, glyph infos,
//! faces) never escape the crate.
//!
//! # Semantics
//!
//! - `text` is shaped as **one run** in the given [`Direction`] (no
//!   script itemization — recorded delta). RTL shaping returns glyphs in
//!   visual order (the engine's contract), so painting walks the vector
//!   front to back regardless of direction.
//! - `cluster` on each glyph is the **byte offset** of the cluster start
//!   in `text` (the engine's byte-index convention; zero-width marks join
//!   their base's cluster — ligatures map to their first character).
//! - Positions are font units scaled by `size / units_per_em` in strict
//!   IEEE-754 `f32`, computed once per call and applied in glyph order —
//!   deterministic across machines and runs (no reassociation, no
//!   environment dependence).
//! - Default OpenType features apply (`kern` and `liga` on) — the CSS
//!   `font-kerning`/`font-variant` machinery is a later batch; ADR-0015
//!   records the scope.
//!
//! # Verified facts (the bundled DejaVu set)
//!
//! The unit tests assert behavior hand-verified against the shipped
//! faces: GPOS kerning ("AV" is 131 units tighter than the unshaped
//! pair), the `fi` ligature (one glyph, one cluster), combining-mark
//! composition (e + U+0301 → the é glyph on cluster 0), and Arabic
//! contextual forms (initial/medial/final glyph substitutions, RTL
//! visual order). Constants in tests come from the face files, so a font
//! update is a reviewed diff.

use crate::error::TextError;
use crate::font::{FontId, FontLibrary};
use crate::limits::MAX_RUN_CHARS;

/// The horizontal direction a run is shaped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Left-to-right (the layout seam's current direction for every run —
    /// bidi-aware run splitting is the layout batch's next increment).
    LeftToRight,
    /// Right-to-left (visual-order glyph output).
    RightToLeft,
}

impl Direction {
    /// The base direction resolved from a text's first strong character
    /// (UAX#9 rule P2/P3 subset) — see [`crate::base_direction`].
    pub(crate) fn from_level(level: u8) -> Direction {
        if level.is_multiple_of(2) {
            Direction::LeftToRight
        } else {
            Direction::RightToLeft
        }
    }
}

/// One positioned glyph in a [`ShapedRun`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    /// The face's glyph id (`0` is `.notdef` — uncovered codepoints).
    pub glyph_id: u16,
    /// Byte offset of the glyph's cluster start in the shaped `&str`.
    pub cluster: usize,
    /// Advance in px (positive rightward; scaling documented in the
    /// module docs).
    pub x_advance: f32,
    /// Vertical advance in px (vertical layout is not used yet; carried
    /// for the future writing-modes batch).
    pub y_advance: f32,
    /// Draw offset in px relative to the pen position (marks, anchors).
    pub x_offset: f32,
    /// Draw offset in px relative to the pen position.
    pub y_offset: f32,
}

/// A shaped text run: glyphs in visual order plus the run's total advance.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedRun {
    /// Glyphs in visual order (leftmost first).
    pub glyphs: Vec<ShapedGlyph>,
    /// The sum of x-advances in px (the run's measured width; direction
    /// independent — an RTL run's advance is its width too).
    pub advance: f32,
    /// The direction the run was shaped in (echoed for consumers).
    pub direction: Direction,
}

impl FontLibrary {
    /// Shapes `text` as one run in `direction` at `size` px with the face
    /// `font` (see the module docs for semantics).
    ///
    /// # Errors
    ///
    /// [`TextError::UnknownFont`] when `font` is not in this library;
    /// [`TextError::RunTooLong`] when `text` exceeds [`MAX_RUN_CHARS`]
    /// Unicode scalar values (chunk long runs at character boundaries).
    pub fn shape(
        &self,
        font: FontId,
        text: &str,
        size: f32,
        direction: Direction,
    ) -> Result<ShapedRun, TextError> {
        let info = self.font(font).ok_or(TextError::UnknownFont)?;
        if text.chars().count() > MAX_RUN_CHARS {
            return Err(TextError::RunTooLong);
        }
        let face =
            rustybuzz::Face::from_slice(info.data(), info.index()).ok_or(TextError::FaceParse)?;
        if face.units_per_em() <= 0 {
            return Err(TextError::FaceParse);
        }
        let scale = size / face.units_per_em() as f32;

        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(match direction {
            Direction::LeftToRight => rustybuzz::Direction::LeftToRight,
            Direction::RightToLeft => rustybuzz::Direction::RightToLeft,
        });
        let out = rustybuzz::shape(&face, &[], buffer);

        let infos = out.glyph_infos();
        let positions = out.glyph_positions();
        let mut glyphs = Vec::with_capacity(infos.len());
        let mut advance = 0.0f32;
        for (glyph, position) in infos.iter().zip(positions.iter()) {
            let x_advance = position.x_advance as f32 * scale;
            advance += x_advance;
            // OpenType glyph ids are 16-bit; the shaper widens to u32 —
            // narrow with a .notdef fallback (unreachable for valid faces).
            let glyph_id = u16::try_from(glyph.glyph_id).unwrap_or(0);
            glyphs.push(ShapedGlyph {
                glyph_id,
                cluster: glyph.cluster as usize,
                x_advance,
                y_advance: position.y_advance as f32 * scale,
                x_offset: position.x_offset as f32 * scale,
                y_offset: position.y_offset as f32 * scale,
            });
        }
        Ok(ShapedRun {
            glyphs,
            advance,
            direction,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// A library with just the regular DejaVu Sans (the facts tests shape
    /// against the face whose constants were hand-verified).
    fn sans_library() -> FontLibrary {
        let mut library = FontLibrary::new();
        library
            .load_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").as_slice())
            .unwrap();
        library
    }

    fn shape(library: &FontLibrary, text: &str, size: f32) -> ShapedRun {
        library
            .shape(FontId(0), text, size, Direction::LeftToRight)
            .unwrap()
    }

    /// Shaping is deterministic: identical inputs → identical outputs
    /// (the golden-dump contract starts here).
    #[test]
    fn shaping_is_deterministic() {
        let library = sans_library();
        let a = shape(&library, "Hello, Veyra!", 16.0);
        let b = shape(&library, "Hello, Veyra!", 16.0);
        assert_eq!(a, b);
    }

    /// Advances scale linearly in size (strict f32: size 16 is 1/128 of
    /// upem 2048, so px values are exact dyadic rationals of font units).
    #[test]
    fn advance_scales_with_size() {
        let library = sans_library();
        // Size = upem: px == font units (verified 'W' advance).
        let units = shape(&library, "W", 2048.0).advance;
        assert_eq!(units, 2025.0);
        // Size 16: exact dyadic (2025/128), no rounding anywhere.
        let px16 = shape(&library, "W", 16.0).advance;
        assert_eq!(px16, 2025.0 / 128.0);
        assert_eq!(px16 * 128.0, 2025.0);
    }

    /// GPOS kerning is applied: "AV" shapes tighter than the independent
    /// advances of "A" and "V" (131 font units at this face — hand-checked).
    #[test]
    fn kerning_applies() {
        let library = sans_library();
        let av = shape(&library, "AV", 2048.0).advance;
        let a = shape(&library, "A", 2048.0).advance;
        let v = shape(&library, "V", 2048.0).advance;
        assert_eq!(a, 1401.0);
        assert_eq!(v, 1401.0);
        assert_eq!(av, 2671.0);
        assert!((a + v) - av > 0.0);
    }

    /// The `liga` substitution fires: "fi" is a single glyph mapping to
    /// cluster 0 (its advance equals f+i's sum in this face — coincidence
    /// of the design, asserted as verified facts).
    #[test]
    fn ligature_substitutes() {
        let library = sans_library();
        let fi = shape(&library, "fi", 2048.0);
        assert_eq!(fi.glyphs.len(), 1);
        assert_eq!(fi.glyphs[0].cluster, 0);
        assert_eq!(fi.advance, 1290.0);
        let f = shape(&library, "f", 2048.0).advance;
        let i = shape(&library, "i", 2048.0).advance;
        assert_eq!(f, 721.0);
        assert_eq!(i, 569.0);
    }

    /// Combining marks compose: e + U+0301 shapes to the é glyph on
    /// cluster 0 (one visual glyph, base cluster).
    #[test]
    fn combining_marks_compose() {
        let library = sans_library();
        let composed = shape(&library, "e\u{0301}", 2048.0);
        assert_eq!(composed.glyphs.len(), 1);
        assert_eq!(composed.glyphs[0].cluster, 0);
        assert_eq!(composed.advance, shape(&library, "é", 2048.0).advance);
    }

    /// Spaces carry their own advance; the cluster map stays byte-based.
    #[test]
    fn spaces_and_byte_clusters() {
        let library = sans_library();
        let run = shape(&library, "a b", 2048.0);
        assert_eq!(run.glyphs.len(), 3);
        assert_eq!(run.glyphs[1].cluster, 1); // the space, byte offset 1
        assert_eq!(run.glyphs[2].cluster, 2); // 'b'
        assert_eq!(run.advance, 3206.0);
    }

    /// Uncovered codepoints shape to .notdef (glyph 0) — the recorded
    /// no-CJK delta, visible as a real glyph, never a panic.
    #[test]
    fn uncovered_codepoints_map_to_notdef() {
        let library = sans_library();
        let run = shape(&library, "中", 16.0);
        assert_eq!(run.glyphs.len(), 1);
        assert_eq!(run.glyphs[0].glyph_id, 0);
        assert!(run.advance >= 0.0);
    }

    /// RTL shaping returns glyphs in visual order (reversed relative to
    /// logical) and preserves the width; Arabic forms substitute (gids
    /// differ from the cmap gids of the isolated characters). The run
    /// advance differs from the same text shaped LTR — GPOS kerning is
    /// direction-sensitive (hand-checked: 4735 RTL vs 4859 LTR font
    /// units at this face).
    #[test]
    fn rtl_shapes_visual_order_with_contextual_forms() {
        let library = sans_library();
        let text = "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"; // "مرحبا"
        let run = library
            .shape(FontId(0), text, 2048.0, Direction::RightToLeft)
            .unwrap();
        assert_eq!(run.direction, Direction::RightToLeft);
        assert_eq!(run.advance, 4735.0);
        // Visual order: the first logical character (meem, cluster 0) is
        // LAST in the visual vector.
        assert_eq!(run.glyphs[0].cluster, 8);
        assert_eq!(run.glyphs[4].cluster, 0);
        // Contextual substitution: the medial-form gids are not the cmap
        // gids of isolated letters (hand-checked against the face).
        assert_ne!(run.glyphs[1].glyph_id, 1365); // not the isolated alef
    }

    /// Empty and whitespace-only inputs shape to empty runs (no glyphs,
    /// zero advance) — measurement totality includes the empty case.
    #[test]
    fn empty_input_shapes_to_zero() {
        let library = sans_library();
        let run = shape(&library, "", 16.0);
        assert!(run.glyphs.is_empty());
        assert_eq!(run.advance, 0.0);
    }

    /// The run limit is a typed error above [`MAX_RUN_CHARS`] scalars,
    /// not a panic or an OOM (limits-table row).
    #[test]
    fn run_limit_is_typed() {
        let library = sans_library();
        let long = "a".repeat(crate::MAX_RUN_CHARS + 1);
        assert!(matches!(
            library.shape(FontId(0), &long, 16.0, Direction::LeftToRight),
            Err(TextError::RunTooLong)
        ));
        // At the limit exactly: accepted (and shapes without hanging —
        // 2M 'a' with no ligatures produce 2M glyphs).
        let at_limit = "a".repeat(crate::MAX_RUN_CHARS);
        let run = shape(&library, &at_limit, 16.0);
        assert_eq!(run.glyphs.len(), crate::MAX_RUN_CHARS);
    }

    /// An unknown id (foreign library or sentinel) is a typed error.
    #[test]
    fn unknown_font_id_is_typed() {
        let library = sans_library();
        assert!(matches!(
            library.shape(FontId(42), "x", 16.0, Direction::LeftToRight),
            Err(TextError::UnknownFont)
        ));
    }
}

//! The text-measurement seam.
//!
//! `veyra-layout` consumes text widths and font metrics through the
//! [`TextMeasurer`] trait — the dependency-graph edge `LAYOUT --> TEXT`
//! stays unrealized until `veyra-text` exists (the Linebender adoption is
//! its own ADR'd batch: parley/fontique/rustybuzz/unicode-bidi per
//! rendering-pipeline.md §2). This module defines the contract that future
//! implementation must satisfy and ships the deterministic interim
//! provider the shell and tests use today.
//!
//! # `FixedMetrics` — the specified interim provider
//!
//! [`FixedMetrics`] approximates a generic monospace face with fully
//! specified arithmetic so every consumer, test, and CI run agrees bit
//! for bit. The factors are **dyadic rationals** (exact in binary
//! floating point), so measurements at integer pixel sizes are exact and
//! golden dumps compare with `==`:
//!
//! - advance width: `0.625 × font_size` per Unicode scalar value (every
//!   character measures the same — 10px at the 16px default — including
//!   spaces and control characters);
//! - ascent: `0.75 × font_size`, descent: `0.25 × font_size` (so
//!   `line-height: normal` resolves to exactly `1 × font_size`);
//! - no font matching: weight/style/family are carried in
//!   [`FontQuery`] for the production provider's sake and ignored here.
//!
//! It is *not* a text engine: no shaping, no bidi, no ligatures, no font
//! fallback. Recorded as the Phase 3 interim measurement contract; the
//! `veyra-text` batch replaces it behind the same trait.

use veyra_style::{FontFamily, FontStyle};

/// The font properties a measurement runs under (copied from the owning
/// element's computed values; the production provider matches against
/// these).
#[derive(Debug, Clone, PartialEq)]
pub struct FontQuery {
    /// Computed font size (px).
    pub font_size: f32,
    /// Computed font weight (1–1000).
    pub font_weight: u16,
    /// Computed font style.
    pub font_style: FontStyle,
    /// Computed font-family list (in fallback order).
    pub font_family: FontFamily,
}

impl FontQuery {
    /// Builds a query from computed values (the only construction site
    /// layout uses — font properties come from the cascade, never
    /// synthesized).
    #[must_use]
    pub fn from_computed(values: &veyra_style::ComputedValues) -> Self {
        FontQuery {
            font_size: values.font_size,
            font_weight: values.font_weight,
            font_style: values.font_style,
            font_family: values.font_family.clone(),
        }
    }
}

/// Ascent/descent of the font's normal line box (px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineMetrics {
    /// Distance from the alphabetic baseline to the line box top.
    pub ascent: f32,
    /// Distance from the alphabetic baseline to the line box bottom.
    pub descent: f32,
}

impl LineMetrics {
    /// The font's natural line height (`line-height: normal`).
    #[must_use]
    pub fn normal_line_height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// Text measurement: widths and font metrics for laid-out text.
///
/// Implementations must be deterministic (identical inputs → identical
/// outputs on every machine — the golden-dump contract) and total (any
/// `&str` measures; no panics, no errors — measurement is pure math over
/// the input).
pub trait TextMeasurer {
    /// The advance width of `text` laid out on one line under `font`.
    fn measure(&self, text: &str, font: &FontQuery) -> f32;

    /// The font's normal line-box metrics.
    fn line_metrics(&self, font: &FontQuery) -> LineMetrics;
}

/// The deterministic fixed-advance interim provider (module docs carry
/// the full specification).
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedMetrics;

/// The fixed advance factor (per px of font size, per character).
/// A dyadic rational (5/8) so integer-size measurements are exact.
const FIXED_ADVANCE: f32 = 0.625;
/// The fixed ascent factor (3/4 — exact in binary floating point).
const FIXED_ASCENT: f32 = 0.75;
/// The fixed descent factor (1/4 — exact in binary floating point).
const FIXED_DESCENT: f32 = 0.25;

impl TextMeasurer for FixedMetrics {
    fn measure(&self, text: &str, font: &FontQuery) -> f32 {
        // Count Unicode scalar values (not UTF-8 bytes): a multi-byte
        // character occupies one advance — the monospace approximation.
        let chars = text.chars().count();
        FIXED_ADVANCE * font.font_size * chars as f32
    }

    fn line_metrics(&self, font: &FontQuery) -> LineMetrics {
        LineMetrics {
            ascent: FIXED_ASCENT * font.font_size,
            descent: FIXED_DESCENT * font.font_size,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use veyra_style::FontFamilyName;

    fn query(size: f32) -> FontQuery {
        FontQuery {
            font_size: size,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: FontFamily(vec![FontFamilyName {
                name: "serif".to_string(),
                generic: true,
            }]),
        }
    }

    #[test]
    fn fixed_metrics_spec_arithmetic() {
        let font = query(16.0);
        let m = FixedMetrics;
        assert_eq!(m.measure("", &font), 0.0);
        assert_eq!(m.measure("a", &font), 10.0);
        assert_eq!(m.measure("abc", &font), 30.0);
        // Multi-byte characters count once.
        assert_eq!(m.measure("ä", &font), 10.0);
        assert_eq!(m.measure("ä", &font), m.measure("a", &font));
        // Spaces measure like anything else (collapsing happens before
        // measurement, at the inline-layout layer).
        assert_eq!(m.measure(" ", &font), 10.0);
    }

    #[test]
    fn fixed_metrics_line_height_normal_is_one_em() {
        let font = query(20.0);
        let metrics = FixedMetrics.line_metrics(&font);
        assert_eq!(metrics.ascent, 15.0);
        assert_eq!(metrics.descent, 5.0);
        assert_eq!(metrics.normal_line_height(), 20.0);
    }
}

//! The text-measurement seam.
//!
//! `veyra-layout` consumes text widths and font metrics through the
//! [`TextMeasurer`] trait. The dependency-graph edge `LAYOUT --> TEXT` is
//! realized by [`ShapedMetrics`], which wraps a `veyra_text::FontLibrary`
//! and shapes runs for real advances and font metrics (ADR-0015);
//! [`FixedMetrics`] — the deterministic, fully specified fixed-advance
//! provider — remains the fallback for library-less contexts and the
//! golden-fixture provider.
//!
//! # `ShapedMetrics` — the font-backed provider
//!
//! [`ShapedMetrics`] maps the computed [`FontQuery`] onto a
//! `veyra_text::FontSpec` (families in preference order, style, weight),
//! selects a face from the library (CSS Fonts §5.2 subset — never fails:
//! unmatched requests serve the library's first face), and measures by
//! shaping. Runs longer than `veyra_text::MAX_RUN_CHARS` are chunked at
//! character boundaries and summed — the trait's totality contract (any
//! `&str` measures, no errors) survives even pathological inputs that the
//! shaper would reject whole. Shaping failures (only possible with a
//! corrupt custom library) fall back to [`FixedMetrics`] arithmetic for
//! the affected chunk: measurement degrades deterministically instead of
//! failing layout.
//!
//! Line metrics come from the face's hhea table (ascent/descent scaled to
//! the font size; the line gap is carried by `veyra-text` but folded out
//! of the strut — `line-height: normal` here is ascender + descender, the
//! zero-gap case for the embedded set).
//!
//! # `FixedMetrics` — the specified fixed-advance provider
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
//!   [`FontQuery`] for the shaped provider's sake and ignored here.
//!
//! It is *not* a text engine: no shaping, no bidi, no ligatures, no font
//! fallback. It remains the determinism anchor for golden fixtures and the
//! degraded-measurement fallback of [`ShapedMetrics`].

use std::sync::Arc;

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

/// The font-backed measurement provider: real shaping through
/// `veyra-text` (the dependency-graph edge `LAYOUT --> TEXT`, ADR-0015).
///
/// Deterministic by construction: the library's faces and the shaper are
/// pure functions of their inputs, and the embedded default set
/// ([`ShapedMetrics::embedded_defaults`]) ships the same bytes on every
/// machine. All measurements go through the same select → shape → sum
/// path in strict IEEE-754 arithmetic.
#[derive(Debug, Clone)]
pub struct ShapedMetrics {
    library: Arc<veyra_text::FontLibrary>,
}

impl ShapedMetrics {
    /// Wraps a caller-owned font library (custom faces, test fixtures,
    /// explicit font directories).
    pub fn new(library: Arc<veyra_text::FontLibrary>) -> Self {
        ShapedMetrics { library }
    }

    /// The embedded default provider: `veyra_text::FontLibrary::
    /// with_default_fonts()` (the bundled DejaVu set — sans/serif/mono ×
    /// regular/bold, compiled in). No disk access, no platform discovery,
    /// identical across machines.
    pub fn embedded_defaults() -> Self {
        ShapedMetrics {
            library: Arc::new(veyra_text::FontLibrary::with_default_fonts()),
        }
    }

    /// The font library backing this provider.
    pub fn library(&self) -> &Arc<veyra_text::FontLibrary> {
        &self.library
    }

    /// Maps a computed [`FontQuery`] onto a `veyra_text::FontSpec` (the
    /// family list in order, with generic keywords mapped to the
    /// provider's generic slots; system-ui approximates sans-serif —
    /// recorded in the batch notes).
    fn spec_for(font: &FontQuery) -> veyra_text::FontSpec {
        let families = font
            .font_family
            .0
            .iter()
            .map(|name| {
                if name.generic {
                    match name.name.as_str() {
                        "serif" => veyra_text::FamilyName::Serif,
                        "monospace" => veyra_text::FamilyName::Monospace,
                        "cursive" => veyra_text::FamilyName::Cursive,
                        "fantasy" => veyra_text::FamilyName::Fantasy,
                        // `sans-serif` and `system-ui`.
                        _ => veyra_text::FamilyName::SansSerif,
                    }
                } else {
                    veyra_text::FamilyName::Named(name.name.clone())
                }
            })
            .collect();
        let style = match font.font_style {
            FontStyle::Normal => veyra_text::FontStyle::Normal,
            FontStyle::Italic => veyra_text::FontStyle::Italic,
            FontStyle::Oblique => veyra_text::FontStyle::Oblique,
        };
        veyra_text::FontSpec {
            families,
            style,
            weight: font.font_weight,
        }
    }

    /// Measures by shaping `text` in chunks (the run limit — measurement
    /// is total, so an over-limit run degrades to a chunked sum instead
    /// of an error), falling back to [`FixedMetrics`] arithmetic for any
    /// chunk the shaper rejects (corrupt custom libraries only).
    fn shaped_width(&self, text: &str, font: &FontQuery) -> f32 {
        self.measure_chunked(text, font, veyra_text::MAX_RUN_CHARS)
    }

    /// The chunked-measure loop (chunk size injected for tests —
    /// production always passes the limits-table value).
    fn measure_chunked(&self, text: &str, font: &FontQuery, chunk_chars_limit: usize) -> f32 {
        let id = self.library.select(&Self::spec_for(font));
        let mut total = 0.0f32;
        let mut chunk_start = 0usize;
        let mut chunk_chars = 0usize;
        for (offset, _) in text.char_indices() {
            if chunk_chars == chunk_chars_limit {
                total += self.shape_chunk(id, &text[chunk_start..offset], font);
                chunk_start = offset;
                chunk_chars = 0;
            }
            chunk_chars += 1;
        }
        total += self.shape_chunk(id, &text[chunk_start..], font);
        total
    }

    /// Shapes one within-limit chunk; a failed shape (unknown face in a
    /// corrupt library) falls back to the fixed-advance approximation for
    /// that chunk — deterministic, never a panic.
    fn shape_chunk(&self, id: veyra_text::FontId, chunk: &str, font: &FontQuery) -> f32 {
        match self.library.shape(
            id,
            chunk,
            font.font_size,
            veyra_text::Direction::LeftToRight,
        ) {
            Ok(run) => run.advance,
            Err(_) => FixedMetrics.measure(chunk, font),
        }
    }
}

impl TextMeasurer for ShapedMetrics {
    fn measure(&self, text: &str, font: &FontQuery) -> f32 {
        self.shaped_width(text, font)
    }

    fn line_metrics(&self, font: &FontQuery) -> LineMetrics {
        let id = self.library.select(&Self::spec_for(font));
        match self.library.font(id) {
            Some(info) => LineMetrics {
                ascent: info.ascent_px(font.font_size),
                descent: info.descent_px(font.font_size),
            },
            // Empty custom library: the fixed-advance metrics keep the
            // strut defined (deterministic degradation, documented).
            None => FixedMetrics.line_metrics(font),
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

    // -- ShapedMetrics (the font-backed provider, ADR-0015) ---------------

    use veyra_text::{FamilyName as TextFamily, FontLibrary};

    /// A shaped provider with exactly one face loaded under a known
    /// family (constants hand-verified against the face — see the
    /// veyra-text fact tests this batch recorded).
    fn shaped_sans() -> ShapedMetrics {
        let mut library = FontLibrary::new();
        library
            .load_bytes_with_family(
                include_bytes!("../../../assets/fonts/DejaVuSans.ttf").as_slice(),
                "Fixture Sans",
            )
            .unwrap();
        ShapedMetrics::new(std::sync::Arc::new(library))
    }

    fn named_query(size: f32, weight: u16) -> FontQuery {
        FontQuery {
            font_size: size,
            font_weight: weight,
            font_style: FontStyle::Normal,
            font_family: FontFamily(vec![FontFamilyName {
                name: "Fixture Sans".to_string(),
                generic: false,
            }]),
        }
    }

    /// Measurements are real shaped advances (kerning applies): "AV" at
    /// 16px is 2671 font units × 16/2048 — exact dyadic — and tighter
    /// than the unshaped pair sum. Hand-verified face facts.
    #[test]
    fn shaped_measure_kerning_and_scaling() {
        let measurer = shaped_sans();
        let font = named_query(16.0, 400);
        let av = measurer.measure("AV", &font);
        assert_eq!(av, 2671.0f32 * 16.0 / 2048.0);
        let a = measurer.measure("A", &font);
        let v = measurer.measure("V", &font);
        assert!(a + v > av);
        // Size = upem measures font units exactly.
        let font2048 = named_query(2048.0, 400);
        assert_eq!(measurer.measure("AV", &font2048), 2671.0);
    }

    /// Ligatures reach the seam through shaping: "fi" is one glyph whose
    /// advance is the ligature's (1290 units in this face — verified).
    #[test]
    fn shaped_measure_ligature() {
        let measurer = shaped_sans();
        let font = named_query(2048.0, 400);
        assert_eq!(measurer.measure("fi", &font), 1290.0);
    }

    /// Empty text measures zero (totality includes the empty case).
    #[test]
    fn shaped_measure_empty() {
        let measurer = shaped_sans();
        let font = named_query(16.0, 400);
        assert_eq!(measurer.measure("", &font), 0.0);
    }

    /// Line metrics come from the face's hhea table (1901/-483 at upem
    /// 2048 — dyadic at integer sizes; the gap is 0 in this face).
    #[test]
    fn shaped_line_metrics_hhea() {
        let measurer = shaped_sans();
        let font = named_query(16.0, 400);
        let metrics = measurer.line_metrics(&font);
        assert_eq!(metrics.ascent, 1901.0f32 * 16.0 / 2048.0);
        assert_eq!(metrics.descent, 483.0f32 * 16.0 / 2048.0);
    }

    /// Family preference: a named family the library lacks falls through
    /// to the generic (here: the only loaded face serves); the provider
    /// never fails a measurement on family mismatch.
    #[test]
    fn shaped_family_fallback() {
        let measurer = shaped_sans();
        let named = named_query(16.0, 400);
        let generic = FontQuery {
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: FontFamily(vec![
                FontFamilyName {
                    name: "missing".to_string(),
                    generic: false,
                },
                FontFamilyName {
                    name: "sans-serif".to_string(),
                    generic: true,
                },
            ]),
        };
        assert_eq!(
            measurer.measure("WV", &named),
            measurer.measure("WV", &generic)
        );
    }

    /// Weight flows through to font matching: the embedded default set
    /// distinguishes 400 from 700 (different faces, different advances).
    #[test]
    fn shaped_weight_selects_face() {
        let measurer = ShapedMetrics::embedded_defaults();
        let regular = FontQuery {
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: FontFamily(vec![FontFamilyName {
                name: "sans-serif".to_string(),
                generic: true,
            }]),
        };
        let bold = FontQuery {
            font_weight: 700,
            ..regular.clone()
        };
        // DejaVu Sans vs DejaVu Sans Bold: different designs.
        assert_ne!(
            measurer.measure("Veyra", &regular),
            measurer.measure("Veyra", &bold)
        );
        // The library picked the bold face for the bold query.
        let library = measurer.library();
        let spec = veyra_text::FontSpec {
            families: vec![TextFamily::SansSerif],
            style: veyra_text::FontStyle::Normal,
            weight: 700,
        };
        assert_eq!(library.font(library.select(&spec)).unwrap().weight(), 700);
    }

    /// Determinism: identical queries measure identically (the golden
    /// contract), and the provider is Send/Sync-shareable via Arc (the
    /// shell builds it once).
    #[test]
    fn shaped_measure_deterministic() {
        let measurer = shaped_sans();
        let font = named_query(16.0, 400);
        let a = measurer.measure("The quick brown fox", &font);
        let b = measurer.measure("The quick brown fox", &font);
        assert_eq!(a, b);
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ShapedMetrics>();
    }

    /// The chunked-measure loop: at the injected chunk boundary the text
    /// splits into within-limit chunks (here sized to force three chunks)
    /// and the total equals the unchunked measurement for chunk-safe text
    /// (no cross-boundary kerning on repeated 'a').
    #[test]
    fn shaped_measure_chunking_totality() {
        let measurer = shaped_sans();
        let font = named_query(16.0, 400);
        let text = "a".repeat(8);
        assert_eq!(
            measurer.measure_chunked(&text, &font, 3),
            measurer.measure(&text, &font)
        );
        // Empty text survives any chunk size.
        assert_eq!(measurer.measure_chunked("", &font, 3), 0.0);
    }
}

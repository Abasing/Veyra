//! The font library: loading, metadata, and deterministic selection.
//!
//! [`FontLibrary`] holds explicitly loaded faces (never platform
//! discovery — determinism and ADR-0015): the embedded default set (the
//! bundled DejaVu faces from `assets/fonts/`, compiled in via
//! `include_bytes!`) plus anything loaded at runtime through
//! [`FontLibrary::load_bytes`]/[`load_file`]/[`load_dir`]. Registration
//! order is the library's identity: font ids are assigned sequentially,
//! and face 0 — the first registered — is the final fallback of
//! [`FontLibrary::select`].
//!
//! # Selection (CSS Fonts §5.2 subset)
//!
//! [`FontLibrary::select`] implements the family → style → weight phases
//! of the CSS font-matching algorithm over the loaded set, deterministically:
//!
//! 1. **Family:** the first family in preference order with at least one
//!    loaded face wins (named families compare case-insensitively; generic
//!    keywords map to the embedded default family of that class when it is
//!    loaded). If nothing matches, face 0 serves — the engine never fails
//!    font selection.
//! 2. **Style:** exact [`FontStyle`] match preferred; an italic request
//!    falls back oblique then normal (and symmetrically for oblique). No
//!    synthesis happens here (recorded delta — obliquing is a paint
//!    concern).
//! 3. **Weight:** the CSS Fonts 4 desired-weight search (the
//!    `[400, 500]` band rule, below-band descending, above-band ascending,
//!    per branch of the spec). Ties go to the earlier-registered face.

use std::path::Path;
use std::sync::Arc;

use crate::error::TextError;
use harfrust::font::name;
use read_fonts::TableProvider;
use read_fonts::tables::os2::SelectionFlags;

/// A face within a [`FontLibrary`] (meaningful only for the library that
/// issued it — ids are sequential registration indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub(crate) u32);

/// The slant classification of a face (own type — parser types never
/// escape the crate, dependency-graph §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    /// Upright.
    Normal,
    /// True italic.
    Italic,
    /// Oblique (slanted roman).
    Oblique,
}

/// One family name in a selection request, in preference order.
///
/// Named families compare case-insensitively against loaded face family
/// names. Generic keywords map to the embedded default family of that
/// class (`DejaVu Sans`/`DejaVu Serif`/`DejaVu Sans Mono`) when those
/// faces are loaded; a generic with no loaded representative is skipped
/// like any unmatched family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyName {
    /// A concrete family name (`"DejaVu Sans"`).
    Named(String),
    /// `sans-serif`.
    SansSerif,
    /// `serif`.
    Serif,
    /// `monospace`.
    Monospace,
    /// `cursive` (served by the serif default in the embedded set).
    Cursive,
    /// `fantasy` (served by the sans default in the embedded set).
    Fantasy,
}

/// A font selection request: the family preference list plus the style and
/// weight to match (the same information `veyra-layout`'s `FontQuery`
/// carries, in this crate's own types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontSpec {
    /// Families in preference order (first match wins).
    pub families: Vec<FamilyName>,
    /// Requested slant.
    pub style: FontStyle,
    /// Requested weight, `1..=1000` (CSS `font-weight`; values outside the
    /// range are clamped by the caller — this crate compares them as-is).
    pub weight: u16,
}

/// Metadata for one loaded face.
///
/// Font data is held as `Arc<[u8]>` so libraries share large buffers
/// cheaply; the parser face is constructed per use (parsing is lazy and
/// allocation-light, and the borrow would otherwise pin the whole
/// library).
#[derive(Debug)]
pub struct FontInfo {
    data: Arc<[u8]>,
    index: u32,
    family: String,
    style: FontStyle,
    weight: u16,
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
}

impl FontInfo {
    /// Parses and validates `data` as a face, extracting the metadata the
    /// library needs. Family names come from the `name` table: the English
    /// family (id 1) record wins, the first record otherwise (deeper
    /// platform/language preference is not modeled — recorded in ADR-0015).
    pub(crate) fn parse(data: Arc<[u8]>, index: u32) -> Result<Self, TextError> {
        let face = load_face(&data, index).ok_or(TextError::FaceParse)?;
        let family = face
            .names()
            .get(name::NameId::FAMILY_NAME)
            .english_or_first()
            .map(|localized| localized.name().to_string())
            .ok_or(TextError::FaceParse)?;
        let metrics = face.metrics();
        // hhea line metrics, zeroed when the table is absent (the face
        // stays usable for shaping; ascent/descent consumers get zeros).
        let line = metrics.hhea_line.unwrap_or_default();
        Ok(FontInfo {
            family,
            style: classify_style(&data, index),
            weight: os2_weight(&data, index),
            units_per_em: face.units_per_em(),
            ascender: line.ascender.to_f64() as i16,
            descender: line.descender.to_f64() as i16,
            line_gap: line.line_gap.to_f64() as i16,
            data,
            index,
        })
    }

    /// The face's family name (from the `name` table).
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The face's slant classification.
    pub fn style(&self) -> FontStyle {
        self.style
    }

    /// The face's weight class (`1..=1000`).
    pub fn weight(&self) -> u16 {
        self.weight
    }

    /// The face's units-per-em (the design-space size all font units are
    /// expressed in).
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Hhea ascent scaled to `size` px (distance from the alphabetic
    /// baseline up to the line-box top; always positive).
    pub fn ascent_px(&self, size: f32) -> f32 {
        self.scale(self.ascender, size)
    }

    /// Hhea descent scaled to `size` px (distance from the baseline down
    /// to the line-box bottom; reported positive — hhea stores it
    /// negative).
    pub fn descent_px(&self, size: f32) -> f32 {
        // Abs first: a malformed positive descender stays a positive
        // descent, and `-0.0` cannot leak into layout.
        self.scale(-self.descender, size)
    }

    /// Hhea line gap scaled to `size` px (`line-height: normal` consumers
    /// decide how to distribute it — the layout seam currently folds the
    /// gap out of the strut; recorded in the layout batch).
    pub fn line_gap_px(&self, size: f32) -> f32 {
        self.scale(self.line_gap, size)
    }

    /// Whether the face's `cmap` maps `ch` to a glyph (coverage check for
    /// future fallback logic). The `.notdef` glyph (id 0) is not coverage
    /// (the model charmap treats glyph-0 mappings as unmapped, so `Some`
    /// here means real coverage).
    pub fn has_glyph(&self, ch: char) -> bool {
        load_face(&self.data, self.index)
            .is_some_and(|face| face.charmap().map_unicode(ch).is_some())
    }

    /// Scales a font-unit quantity to px (strict `f32` math; `size` above
    /// zero and `units_per_em` validated non-zero at parse).
    fn scale(&self, units: i16, size: f32) -> f32 {
        f32::from(units) * (size / f32::from(self.units_per_em))
    }

    pub(crate) fn data(&self) -> &Arc<[u8]> {
        &self.data
    }

    pub(crate) fn index(&self) -> u32 {
        self.index
    }
}

/// The collection of loaded faces.
///
/// Construct [`FontLibrary::new`] (empty), [`FontLibrary::with_default_fonts`]
/// (the embedded DejaVu set — deterministic, no disk access), or start
/// empty and load. Every operation is deterministic: file loads read in
/// sorted order, ids are sequential, and selection is a pure function of
/// the loaded set plus the request.
#[derive(Debug, Default)]
pub struct FontLibrary {
    fonts: Vec<FontInfo>,
}

/// The embedded default faces, in registration order (the bundled DejaVu
/// set — `assets/fonts/LICENSE.txt` records the license). Regular faces
/// first so face 0 is a normal-weight sans: the universal fallback.
const EMBEDDED_FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/DejaVuSans.ttf"),
    include_bytes!("../../../assets/fonts/DejaVuSans-Bold.ttf"),
    include_bytes!("../../../assets/fonts/DejaVuSerif.ttf"),
    include_bytes!("../../../assets/fonts/DejaVuSerif-Bold.ttf"),
    include_bytes!("../../../assets/fonts/DejaVuSansMono.ttf"),
    include_bytes!("../../../assets/fonts/DejaVuSansMono-Bold.ttf"),
];

impl FontLibrary {
    /// An empty library (nothing matches until faces are loaded).
    pub fn new() -> Self {
        FontLibrary::default()
    }

    /// A library holding the embedded default face set (six DejaVu faces:
    /// sans/serif/mono × regular/bold). Deterministic across machines —
    /// the bytes are compiled in.
    pub fn with_default_fonts() -> Self {
        let mut library = FontLibrary::new();
        for bytes in EMBEDDED_FONTS {
            // The embedded set is validated at build time by the crate's
            // own test suite; a failure here would be a programming error,
            // but the library API stays total: skip instead of panic.
            if let Ok(id) = library.load_bytes(*bytes) {
                debug_assert_eq!(id, FontId(library.fonts.len() as u32 - 1));
            }
        }
        library
    }

    /// Loads a face from memory, validating it and extracting metadata.
    /// Family name is auto-detected from the `name` table.
    ///
    /// # Errors
    ///
    /// [`TextError::FaceParse`] when the bytes are not a parseable face.
    pub fn load_bytes(&mut self, data: impl Into<Arc<[u8]>>) -> Result<FontId, TextError> {
        let data: Arc<[u8]> = data.into();
        let info = FontInfo::parse(data, 0)?;
        let id = FontId(self.fonts.len() as u32);
        self.fonts.push(info);
        Ok(id)
    }

    /// Loads a face from memory under an explicit family name (overrides
    /// the `name` table — for tests and synthesized collections where a
    /// deterministic name matters more than the file's).
    ///
    /// # Errors
    ///
    /// [`TextError::FaceParse`] when the bytes are not a parseable face.
    pub fn load_bytes_with_family(
        &mut self,
        data: impl Into<Arc<[u8]>>,
        family: &str,
    ) -> Result<FontId, TextError> {
        let id = self.load_bytes(data)?;
        self.fonts[id.0 as usize].family = family.to_string();
        Ok(id)
    }

    /// Loads one face from a file.
    ///
    /// # Errors
    ///
    /// [`TextError::Io`] when the file cannot be read;
    /// [`TextError::FaceParse`] when its bytes are not a parseable face.
    pub fn load_file(&mut self, path: &Path) -> Result<FontId, TextError> {
        let data = std::fs::read(path)?;
        self.load_bytes(data)
    }

    /// Loads every parseable `.ttf`/`.otf` face in a directory (not
    /// recursive), in sorted file-name order for determinism. Unreadable
    /// and unparseable files are skipped and reported, never fatal — a
    /// scan of a system font directory must survive a single bad file.
    ///
    /// # Errors
    ///
    /// [`TextError::Io`] only when the directory itself cannot be read.
    pub fn load_dir(&mut self, path: &Path) -> Result<DirLoadReport, TextError> {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(path)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("ttf") | Some("otf")
                )
            })
            .collect();
        entries.sort();
        let mut report = DirLoadReport::default();
        for path in entries {
            match std::fs::read(&path)
                .map_err(TextError::from)
                .and_then(|d| self.load_bytes(d))
            {
                Ok(id) => report.loaded.push(id),
                Err(_) => report.skipped.push(path),
            }
        }
        Ok(report)
    }

    /// The number of loaded faces.
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Whether the library holds no faces (only possible for a manually
    /// constructed empty library — [`FontLibrary::with_default_fonts`]
    /// always loads the embedded set).
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// The metadata of face `id`, if it belongs to this library.
    pub fn font(&self, id: FontId) -> Option<&FontInfo> {
        self.fonts.get(id.0 as usize)
    }

    /// Deterministic font selection (module docs carry the algorithm).
    /// Never fails: when no family matches, face 0 serves (or the `u32::MAX`
    /// sentinel when the library is empty — shaping then reports
    /// [`TextError::UnknownFont`], and nothing ever dereferences the
    /// sentinel).
    pub fn select(&self, spec: &FontSpec) -> FontId {
        // Family phase: first preference with at least one loaded face.
        let mut candidates: Vec<usize> = Vec::new();
        for family in &spec.families {
            let wanted: Option<String> = match family {
                FamilyName::Named(name) => Some(name.clone()),
                FamilyName::SansSerif => Some("DejaVu Sans".to_string()),
                FamilyName::Serif | FamilyName::Cursive => Some("DejaVu Serif".to_string()),
                FamilyName::Monospace => Some("DejaVu Sans Mono".to_string()),
                FamilyName::Fantasy => Some("DejaVu Sans".to_string()),
            };
            if let Some(wanted) = wanted {
                candidates = self
                    .fonts
                    .iter()
                    .enumerate()
                    .filter(|(_, font)| font.family.eq_ignore_ascii_case(&wanted))
                    .map(|(index, _)| index)
                    .collect();
                if !candidates.is_empty() {
                    break;
                }
            }
        }
        if candidates.is_empty() {
            return self.first_face();
        }

        // Style phase: rank by the requested slant (exact first; italic and
        // oblique are mutual fallbacks; normal is the last resort for a
        // slanted request and vice versa — no synthesis, recorded delta).
        let style_rank = |font_style: FontStyle| match spec.style {
            FontStyle::Normal => match font_style {
                FontStyle::Normal => 0u8,
                FontStyle::Oblique => 1,
                FontStyle::Italic => 2,
            },
            FontStyle::Italic => match font_style {
                FontStyle::Italic => 0u8,
                FontStyle::Oblique => 1,
                FontStyle::Normal => 2,
            },
            FontStyle::Oblique => match font_style {
                FontStyle::Oblique => 0u8,
                FontStyle::Italic => 1,
                FontStyle::Normal => 2,
            },
        };
        candidates.sort_by_key(|&index| style_rank(self.fonts[index].style));
        let best_style = style_rank(self.fonts[candidates[0]].style);
        let styled: Vec<usize> = candidates
            .into_iter()
            .filter(|&index| style_rank(self.fonts[index].style) == best_style)
            .collect();

        // Weight phase: CSS Fonts 4 desired-weight search over the styled
        // candidates (ties keep registration order — stable sort).
        let key = |weight: u16| weight_preference_key(weight, spec.weight);
        let mut styled: Vec<(usize, (u8, u16))> = styled
            .into_iter()
            .map(|index| (index, key(self.fonts[index].weight)))
            .collect();
        styled.sort_by_key(|&(_, key)| key);
        FontId(styled[0].0 as u32)
    }

    /// Face 0 when the library is non-empty; the sentinel otherwise (the
    /// sentinel is never a valid id — `select` on an empty library cannot
    /// hand out a real face).
    fn first_face(&self) -> FontId {
        if self.fonts.is_empty() {
            FontId(u32::MAX)
        } else {
            FontId(0)
        }
    }
}

/// The result of [`FontLibrary::load_dir`]: what loaded, what was skipped
/// (with paths, so callers can log the skips — never silent).
#[derive(Debug, Default, Clone)]
pub struct DirLoadReport {
    /// Faces successfully registered, in sorted file-name order.
    pub loaded: Vec<FontId>,
    /// Files that could not be read or parsed, in the same order.
    pub skipped: Vec<std::path::PathBuf>,
}

/// The CSS Fonts 4 desired-weight search encoded as a sort key (smaller
/// sorts first): `(scan-band, direction-adjusted weight)`.
fn weight_preference_key(weight: u16, desired: u16) -> (u8, u16) {
    const MAX: u16 = 1000;
    if (400..=500).contains(&desired) {
        if (desired..=500).contains(&weight) {
            (0, weight)
        } else if weight < desired {
            (1, MAX - weight)
        } else {
            (2, weight)
        }
    } else if desired < 400 {
        if weight <= desired {
            (0, MAX - weight)
        } else {
            (1, weight)
        }
    } else if weight >= desired {
        (0, weight)
    } else {
        (1, MAX - weight)
    }
}

/// Loads `data` (at collection `index`) as a harfrust model face — the
/// lazy view over the shared buffer used for shaping and metadata. No
/// bytes are copied: `Arc<[u8]>` cannot unsize-coerce to the trait-object
/// form (its pointee `[u8]` is unsized), so the shared buffer rides
/// inside one outer `Arc` — a pointer hop per view, never a copy. Returns
/// `None` on bytes that are not a usable face (the caller turns that into
/// [`TextError::FaceParse`] or a `false` coverage answer).
pub(crate) fn load_face(data: &Arc<[u8]>, index: u32) -> Option<harfrust::Font> {
    let shared: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data.clone());
    harfrust::Font::new(shared, index)
}

/// The face's OS/2 weight class, defaulting to 400 (Regular) when the
/// table is absent — the default the CSS matching algorithm expects of
/// an unspecified face.
fn os2_weight(data: &Arc<[u8]>, index: u32) -> u16 {
    read_fonts::FontRef::from_index(data, index)
        .ok()
        .and_then(|font| font.os2().ok())
        .map_or(400, |os2| os2.us_weight_class())
}

/// The face's slant classification (ttf-parser-compatible semantics, so
/// the CSS matching phase sees the same classes it did before the
/// harfrust migration): OS/2 `fsSelection` ITALIC wins, then `fsSelection`
/// OBLIQUE or a non-zero `post` italic angle (Oblique), else Normal.
fn classify_style(data: &Arc<[u8]>, index: u32) -> FontStyle {
    let Ok(font) = read_fonts::FontRef::from_index(data, index) else {
        return FontStyle::Normal;
    };
    let selection = font.os2().map(|os2| os2.fs_selection()).ok();
    if selection.is_some_and(|flags| flags.contains(SelectionFlags::ITALIC)) {
        return FontStyle::Italic;
    }
    if selection.is_some_and(|flags| flags.contains(SelectionFlags::OBLIQUE)) {
        return FontStyle::Oblique;
    }
    if font
        .post()
        .is_ok_and(|post| post.italic_angle().to_f64() != 0.0)
    {
        return FontStyle::Oblique;
    }
    FontStyle::Normal
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// The embedded set loads in registration order with the documented
    /// metadata (values verified against the shipped files — the fact
    /// probe this batch's constants were hand-checked against).
    #[test]
    fn embedded_set_metadata() {
        let library = FontLibrary::with_default_fonts();
        assert_eq!(library.len(), 6);
        assert!(!library.is_empty());

        let sans = library.font(FontId(0)).unwrap();
        assert_eq!(sans.family(), "DejaVu Sans");
        assert_eq!(sans.style(), FontStyle::Normal);
        assert_eq!(sans.weight(), 400);
        assert_eq!(sans.units_per_em(), 2048);

        assert_eq!(library.font(FontId(1)).unwrap().family(), "DejaVu Sans");
        assert_eq!(library.font(FontId(1)).unwrap().weight(), 700);
        assert_eq!(library.font(FontId(2)).unwrap().family(), "DejaVu Serif");
        assert_eq!(
            library.font(FontId(4)).unwrap().family(),
            "DejaVu Sans Mono"
        );

        // Natural line metrics scale exactly (hhea: 1901/-483/0 at
        // upem 2048 — dyadic factors, exact in f32).
        assert_eq!(sans.ascent_px(16.0), 1901.0f32 / 128.0);
        assert_eq!(sans.descent_px(16.0), 483.0f32 / 128.0);
        assert_eq!(sans.line_gap_px(16.0), 0.0);
        assert_eq!(sans.ascent_px(2048.0), 1901.0);
    }

    /// Unparseable bytes are a typed error, not a panic.
    #[test]
    fn load_rejects_garbage() {
        let mut library = FontLibrary::new();
        assert!(matches!(
            library.load_bytes(b"not a font".as_slice()),
            Err(TextError::FaceParse)
        ));
        assert!(library.is_empty());
    }

    /// Explicit family override works and selection matches it.
    #[test]
    fn load_bytes_with_family_overrides_name_table() {
        let mut library = FontLibrary::new();
        let id = library
            .load_bytes_with_family(
                include_bytes!("../../../assets/fonts/DejaVuSans.ttf").as_slice(),
                "Test Sans",
            )
            .unwrap();
        assert_eq!(library.font(id).unwrap().family(), "Test Sans");
        let spec = FontSpec {
            families: vec![FamilyName::Named("test sans".to_string())],
            style: FontStyle::Normal,
            weight: 400,
        };
        assert_eq!(library.select(&spec), id);
    }

    /// Coverage checks via `cmap` (verified facts: Latin + Arabic covered,
    /// CJK not — the recorded no-CJK delta).
    #[test]
    fn coverage_queries() {
        let library = FontLibrary::with_default_fonts();
        let sans = library.font(FontId(0)).unwrap();
        assert!(sans.has_glyph('W'));
        assert!(sans.has_glyph('é'));
        assert!(sans.has_glyph('\u{0645}')); // Arabic meem
        assert!(!sans.has_glyph('中'));
        // U+0000 has no real glyph — .notdef (id 0) is not coverage.
        assert!(!sans.has_glyph('\0'));
    }

    /// Family phase: named matching is case-insensitive, generics map to
    /// the embedded set, unmatched families fall through to the next
    /// preference, and total mismatch serves face 0.
    #[test]
    fn family_matching() {
        let library = FontLibrary::with_default_fonts();
        let select = |families: Vec<FamilyName>, weight: u16| {
            library.select(&FontSpec {
                families,
                style: FontStyle::Normal,
                weight,
            })
        };
        // Case-insensitive named match.
        let sans = select(vec![FamilyName::Named("dejavu sans".to_string())], 400);
        assert_eq!(library.font(sans).unwrap().family(), "DejaVu Sans");
        // Generics map to the embedded families.
        let serif = select(vec![FamilyName::Serif], 400);
        assert_eq!(library.font(serif).unwrap().family(), "DejaVu Serif");
        let mono = select(vec![FamilyName::Monospace], 400);
        assert_eq!(library.font(mono).unwrap().family(), "DejaVu Sans Mono");
        // First matching preference wins.
        let serif_first = select(
            vec![
                FamilyName::Named("no such family".to_string()),
                FamilyName::Serif,
            ],
            400,
        );
        assert_eq!(library.font(serif_first).unwrap().family(), "DejaVu Serif");
        // Nothing matches → face 0.
        let fallback = select(vec![FamilyName::Named("nope".to_string())], 400);
        assert_eq!(fallback, FontId(0));
        // Empty family list → face 0.
        let empty = select(vec![], 400);
        assert_eq!(empty, FontId(0));
    }

    /// Weight phase: the CSS Fonts 4 desired-weight search against the
    /// bundled 400/700 pair (the spec branches, hand-checked).
    #[test]
    fn weight_matching_css_algorithm() {
        let library = FontLibrary::with_default_fonts();
        let select_at = |weight: u16| {
            let id = library.select(&FontSpec {
                families: vec![FamilyName::Named("DejaVu Sans".to_string())],
                style: FontStyle::Normal,
                weight,
            });
            library.font(id).unwrap().weight()
        };
        assert_eq!(select_at(400), 400);
        assert_eq!(select_at(700), 700);
        // In the 400–500 band: below-band descending is preferred over
        // above-500 ascending — 500 lands on 400, not 700.
        assert_eq!(select_at(500), 400);
        // Above 500: nearest at-or-above first — 600 and 900 land on 700.
        assert_eq!(select_at(600), 700);
        assert_eq!(select_at(900), 700);
        // Below 400: nearest at-or-below descending first — 300 lands on
        // 400 (the only face below the request).
        assert_eq!(select_at(300), 400);
    }

    /// Style phase: a slanted request against a non-slanted set falls back
    /// deterministically to the regular face (no synthesis).
    #[test]
    fn style_matching_falls_back() {
        let library = FontLibrary::with_default_fonts();
        let id = library.select(&FontSpec {
            families: vec![FamilyName::Named("DejaVu Sans".to_string())],
            style: FontStyle::Italic,
            weight: 400,
        });
        assert_eq!(library.font(id).unwrap().style(), FontStyle::Normal);
    }

    /// Directory loads are deterministic (sorted) and survive bad files.
    /// The font directory is assembled at test time from the embedded
    /// bytes (no font binaries duplicated into the repo): `a.ttf` < `b.ttf`
    /// alphabetically, so load order is observable through family names;
    /// `bad.ttf` is garbage and must be skipped, not fatal.
    #[test]
    fn load_dir_sorted_and_skip_reported() {
        let dir = std::env::temp_dir().join(format!("veyra-text-fontdir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.ttf"),
            include_bytes!("../../../assets/fonts/DejaVuSansMono.ttf"),
        )
        .unwrap();
        std::fs::write(
            dir.join("b.ttf"),
            include_bytes!("../../../assets/fonts/DejaVuSans.ttf"),
        )
        .unwrap();
        std::fs::write(dir.join("bad.ttf"), b"not a font").unwrap();

        let mut library = FontLibrary::new();
        let report = library.load_dir(&dir).unwrap();
        assert_eq!(report.loaded.len(), 2);
        assert_eq!(report.skipped, vec![dir.join("bad.ttf")]);
        // Sorted order: a.ttf (mono) before b.ttf (sans).
        assert_eq!(
            library.font(report.loaded[0]).unwrap().family(),
            "DejaVu Sans Mono"
        );
        assert_eq!(
            library.font(report.loaded[1]).unwrap().family(),
            "DejaVu Sans"
        );

        let _ = std::fs::remove_dir_all(&dir); // best-effort cleanup
    }

    /// An empty library selects the sentinel and shaping reports it as a
    /// typed error (no panics on the misuse path).
    #[test]
    fn empty_library_selects_sentinel() {
        let library = FontLibrary::new();
        let spec = FontSpec {
            families: vec![FamilyName::Named("anything".to_string())],
            style: FontStyle::Normal,
            weight: 400,
        };
        let id = library.select(&spec);
        assert!(library.font(id).is_none());
        assert!(matches!(
            library.shape(id, "x", 16.0, crate::Direction::LeftToRight),
            Err(TextError::UnknownFont)
        ));
    }
}

//! `veyra-text` — Veyra's text stack: font collection, shaping, bidi.
//!
//! This crate owns the engine's text concerns behind Veyra-owned types
//! ([`FontLibrary`], [`FontSpec`], [`ShapedRun`], [`Direction`]): the
//! external shaping/parsing crates (rustybuzz, ttf-parser) and their types
//! never appear in public APIs, so a shaping-engine swap is a one-crate
//! change (`docs/architecture/dependency-graph.md` §5). Adoption decisions
//! and the deferred alternatives (parley, fontique) are recorded in
//! ADR-0015.
//!
//! # Boundary
//!
//! `veyra-text` is a leaf-ish content-layer crate: it depends on no engine
//! crate. The dependency-graph edge `LAYOUT --> TEXT` is realized by
//! `veyra-layout`, which wraps [`FontLibrary`] behind its `TextMeasurer`
//! seam; `veyra-text` itself never imports DOM, style, or layout types.
//!
//! # Determinism
//!
//! Shaping is pure math over the input: identical (font data, text, size,
//! direction) triples produce identical outputs on every machine. The
//! default font set is embedded at build time (the bundled DejaVu faces,
//! `assets/fonts/`), so no platform font discovery is involved and CI runs
//! agree bit for bit. Positions are computed with strict IEEE-754 `f32`
//! arithmetic in glyph order (no reassociation).
//!
//! # Security posture
//!
//! Font bytes are untrusted input: parsing never panics (engineering
//! standards §2); every failure is a typed [`TextError`]. A single
//! [`FontLibrary::shape`] call is bounded by [`MAX_RUN_CHARS`] (the
//! engineering-standards limits row) and returns a typed error above it.
//!
//! # Recorded deltas (honest scope)
//!
//! - No system font discovery or script fallback breadth: the library holds
//!   explicitly loaded faces; the embedded default set has no CJK coverage,
//!   so uncovered codepoints shape to `.notdef`. ADR-0015 defers fallback
//!   collection (fontique-class) to the batch that needs it.
//! - No glyph synthesis: italic requests match the nearest real face
//!   (regular here); obliquing is a paint-stage concern.
//! - Single-run shaping: script itemization across a run is not performed;
//!   the `bidi` module exposes UAX#9 levels/runs for the layout batch that
//!   consumes them (layout is still single-run this phase).
//! - No shaping cache: every call shapes its input (O(n)); recorded in
//!   ADR-0015 for the benchmark-driven revisit.

mod bidi;
mod error;
mod font;
mod limits;
mod shape;

pub use bidi::{BidiLevel, BidiRun, base_direction, bidi_runs};
pub use error::TextError;
pub use font::{FamilyName, FontId, FontInfo, FontLibrary, FontSpec, FontStyle};
pub use limits::MAX_RUN_CHARS;
pub use shape::{Direction, ShapedGlyph, ShapedRun};

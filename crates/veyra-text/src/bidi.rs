//! UAX#9 bidirectional analysis (levels and visual runs).
//!
//! The Unicode Bidirectional Algorithm decides which parts of a text run
//! flow right-to-left and how logical order maps to visual order. This
//! module wraps the `unicode-bidi` implementation (adopted by ADR-0015)
//! behind this crate's own types ([`BidiLevel`], [`BidiRun`]) — the
//! engine never imports the wrapper crate's types.
//!
//! # Scope (ADR-0015)
//!
//! The analysis covers a text's **first paragraph** (the layout seam is
//! single-paragraph today). [`bidi_runs`] returns the paragraph's runs in
//! **visual order** with their embedding levels — the input to the
//! bidi-aware line layout batch (per the rendering-pipeline doc, layout
//! is still single-run this phase; consuming these runs is its next
//! increment, which is why the analysis ships now instead of at Phase 7).

use std::ops::Range;

use crate::shape::Direction;

/// An embedding level (`0..=125`; even = LTR, odd = RTL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BidiLevel(pub u8);

impl BidiLevel {
    /// Whether the level flows right-to-left (odd levels).
    pub fn is_rtl(self) -> bool {
        self.0 % 2 == 1
    }

    /// The run direction implied by the level.
    pub fn direction(self) -> Direction {
        Direction::from_level(self.0)
    }
}

/// One directional run: a byte range of the input plus its level. Runs
/// are returned in **visual order** (leftmost first) by [`bidi_runs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidiRun {
    /// Byte range of the run in the input text.
    pub range: Range<usize>,
    /// The run's embedding level.
    pub level: BidiLevel,
}

/// The paragraph's base direction resolved from its first strong
/// directional character (UAX#9 rules P2/P3; default LTR when the text
/// has none — the spec's auto-default).
pub fn base_direction(text: &str) -> Direction {
    let info = unicode_bidi::BidiInfo::new(text, None);
    match info.paragraphs.first() {
        Some(paragraph) => Direction::from_level(paragraph.level.number()),
        None => Direction::LeftToRight,
    }
}

/// Splits the text's first paragraph into directional runs in visual
/// order (leftmost first), each with its embedding level. Empty input
/// yields no runs.
///
/// The caller decides what to do with them: the current layout seam
/// measures one run at a time; the bidi-aware layout batch will walk
/// these runs in this order and place them accordingly.
pub fn bidi_runs(text: &str, base: Direction) -> Vec<BidiRun> {
    let default_level = match base {
        Direction::LeftToRight => unicode_bidi::Level::ltr(),
        Direction::RightToLeft => unicode_bidi::Level::rtl(),
    };
    let info = unicode_bidi::BidiInfo::new(text, Some(default_level));
    let Some(paragraph) = info.paragraphs.first() else {
        return Vec::new();
    };
    let line = paragraph.range.clone();
    // `visual_runs` returns the line's per-byte levels (line-relative
    // indexing) plus the L2-reordered runs (absolute offsets): a run's
    // level is the level at its start byte. Pairing runs with the level
    // vector by position would be wrong — the vector is not run-parallel.
    let (levels, runs) = info.visual_runs(paragraph, line.clone());
    runs.into_iter()
        .map(|range| {
            let level = levels[range.start - line.start];
            BidiRun {
                range,
                level: BidiLevel(level.number()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// Pure-LTR text: one run at level 0, spanning the whole input.
    #[test]
    fn ltr_text_is_one_run() {
        let text = "Hello, Veyra!";
        assert_eq!(base_direction(text), Direction::LeftToRight);
        let runs = bidi_runs(text, Direction::LeftToRight);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].range, 0..text.len());
        assert_eq!(runs[0].level, BidiLevel(0));
        assert!(!runs[0].level.is_rtl());
    }

    /// Pure-RTL text: one run at level 1 (odd — RTL). Visual order of a
    /// single RTL run is the whole range; the base direction the caller
    /// passes is honored (explicit RTL keeps the paragraph RTL).
    #[test]
    fn rtl_text_is_one_odd_run() {
        let text = "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"; // "مرحبا"
        assert_eq!(base_direction(text), Direction::RightToLeft);
        let runs = bidi_runs(text, Direction::RightToLeft);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].range, 0..text.len());
        assert_eq!(runs[0].level, BidiLevel(1));
        assert!(runs[0].level.is_rtl());
    }

    /// Mixed text: an LTR base with an embedded RTL segment yields
    /// multiple runs; the embedded run is at an odd level, and the ranges
    /// partition the input byte-for-byte.
    #[test]
    fn mixed_text_partitions_input() {
        let text = "ab \u{0645}\u{0631} cd"; // "ab مرح cd"
        let runs = bidi_runs(text, Direction::LeftToRight);
        assert!(
            runs.len() >= 2,
            "expected an embedded RTL run, got {runs:?}"
        );
        // The RTL segment carries an odd level.
        assert!(runs.iter().any(|run| run.level.is_rtl()));
        // Runs in visual order tile the input exactly, in byte ranges.
        let mut cursor = 0usize;
        for run in &runs {
            assert_eq!(run.range.start, cursor, "visual order must tile: {runs:?}");
            cursor = run.range.end;
        }
        assert_eq!(cursor, text.len());
    }

    /// Numbers inside RTL text keep a separate (even) level per UAX#9 —
    /// European numbers in an RTL paragraph run LTR.
    #[test]
    fn numbers_in_rtl_stay_ltr_leveled() {
        let text = "\u{0645}123\u{0645}"; // meem, 123, meem
        let runs = bidi_runs(text, Direction::RightToLeft);
        assert!(runs.iter().any(|run| !run.level.is_rtl()));
    }

    /// Empty input: no runs, base direction defaults to LTR.
    #[test]
    fn empty_text_has_no_runs() {
        assert_eq!(base_direction(""), Direction::LeftToRight);
        assert!(bidi_runs("", Direction::LeftToRight).is_empty());
    }
}

//! Resource limits for the paint stage (engineering standards §6).
//!
//! The display list is O(fragments) with a per-command constant, so the
//! cap bounds worst-case memory linearly and keeps a pathological frame
//! from allocating without bound. The default matches the layout budget's
//! shape: the layout row caps fragments at 200,000, and one fragment
//! emits at most five commands (one background + four border sides as a
//! single border command, plus text runs) — 1,000,000 commands covers
//! every frame layout can legally produce, with headroom for the
//! command-per-side expansion the raster batch may introduce.

/// The default maximum number of commands in one display list
/// (`1,000,000`; the engineering-standards limits row). Exhaustion is a
/// typed [`PaintError::LimitExceeded`](crate::PaintError::LimitExceeded).
pub const MAX_COMMANDS: usize = 1_000_000;

/// Paint-stage limits (configuration, defaulted conservatively — the
/// limits table is normative and every limit has tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintLimits {
    /// The maximum number of commands one display list may contain.
    pub max_commands: usize,
}

impl PaintLimits {
    /// The default limits (`max_commands` = [`MAX_COMMANDS`]).
    pub fn new() -> Self {
        PaintLimits {
            max_commands: MAX_COMMANDS,
        }
    }
}

impl Default for PaintLimits {
    fn default() -> Self {
        PaintLimits::new()
    }
}

//! `veyra-paint` — Veyra's paint stage: fragments → display list.
//!
//! The display list is the rendering pipeline's paint-stage output
//! (rendering-pipeline.md §1): a flat, ordered sequence of paint
//! commands built by walking the layout fragment tree. Rasterization
//! consumes the list, never the tree — that separation is what makes
//! the CPU and GPU paths (and the screenshot harness) consume the same
//! structure (ADR-0003 decision 1).
//!
//! # Paint order (invariant R2, Phase 3 subset)
//!
//! Commands are emitted in **fragment-tree order**: each box paints its
//! background, then its borders, then its descendants. For the in-flow,
//! non-positioned trees Phase 3 layout produces this is the CSS 2.1
//! Appendix E order (context background → descendants' backgrounds →
//! inline content), because the fragment tree already encodes it:
//! block-in-inline hoisting keeps every box's children at one level, and
//! no stacking contexts, positioned elements, or floats exist yet (all
//! three are recorded layout deltas — they arrive with the
//! positioned/stacking batch, at which point this walk grows the
//! Appendix E steps that consume the stacking-context tree).
//!
//! # Determinism
//!
//! The list is a pure function of the fragment tree: same input → same
//! commands, byte-identical dumps (`DisplayList::to_dump_string`).
//! Transparent backgrounds (alpha 0) and zero-width border sides emit
//! nothing — a skipped command is deterministic, not an optimization
//! with observable variance.
//!
//! # Security posture
//!
//! Construction is bounded ([`PaintLimits::max_commands`], the
//! engineering-standards limits row): exhaustion is a typed
//! [`PaintError::LimitExceeded`], never unbounded allocation (the
//! command count is O(fragments) but the constant is per-command — the
//! cap keeps worst-case memory linear and stated).

mod display_list;
mod error;
mod limits;

pub use display_list::{DisplayCommand, DisplayList, build_display_list};
pub use error::PaintError;
pub use limits::{MAX_COMMANDS, PaintLimits};

//! Layout resource limits (engineering standards §6: the limits table is
//! normative; each limit has an owning crate, tests, and a conservative
//! default).
//!
//! The box/fragment budget bounds memory amplification: a document within
//! the DOM limits (2,000,000 nodes, 512 depth) must not be able to make
//! layout allocate unboundedly. The default matches the row this batch adds
//! to the engineering-standards table; hosts may tighten (never widen) it
//! through [`crate::LayoutLimits`].

use crate::error::LayoutError;

/// The default box/fragment budget (engineering-standards limits table,
/// `veyra-layout` row).
pub const MAX_BOXES: usize = 200_000;

/// Layout resource limits.
///
/// Counted against: box-tree boxes, inline items, and fragments — every
/// allocation layout performs per document. The counter is shared so a
/// shape that manufactures many small allocations (e.g. a deeply nested
/// inline soup) hits the same budget as a wide block fan-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutLimits {
    /// Maximum boxes + inline items + fragments per layout run.
    pub max_boxes: usize,
}

impl Default for LayoutLimits {
    fn default() -> Self {
        LayoutLimits {
            max_boxes: MAX_BOXES,
        }
    }
}

impl LayoutLimits {
    /// The default limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The running allocation budget for one layout run.
///
/// Every persistent intermediate the engine creates charges one unit:
/// box-tree boxes, inline items, inline tokens, and fragments. Charging
/// the intermediates (not just the outputs) bounds the peak memory a
/// single document can drive — a pathological text node cannot allocate
/// millions of transient tokens on the way to a small fragment count.
pub(crate) struct Budget {
    spent: usize,
    max: usize,
}

impl Budget {
    /// Starts a budget from the configured limits.
    pub(crate) fn new(limits: LayoutLimits) -> Self {
        Budget {
            spent: 0,
            max: limits.max_boxes,
        }
    }

    /// Charges one allocation.
    ///
    /// # Errors
    /// [`LayoutError::LimitExceeded`] when the budget is exhausted.
    pub(crate) fn charge(&mut self) -> Result<(), LayoutError> {
        self.spent += 1;
        if self.spent > self.max {
            return Err(LayoutError::LimitExceeded {
                limit: "boxes",
                max: self.max,
            });
        }
        Ok(())
    }
}

//! Intrinsic sizing (css-sizing §5: min-content/max-content as layout
//! *modes*, not a pass) and the shrink-to-fit width resolution atomic
//! inline-level boxes use.
//!
//! - **min-content**: the narrowest the box's content can be without
//!   overflow — the widest unbreakable unit (word or atomic box) across
//!   the content, with soft (space) breaks all taken.
//! - **max-content**: the widest the content can be without any soft
//!   break; forced breaks (`<br>`) segment the measurement and the widest
//!   segment wins.
//! - Intrinsic contexts resolve percentages to zero (css-sizing §5.2:
//!   percentages resolve against zero for intrinsic contributions);
//!   `auto` margins contribute zero. The box's own margins, borders, and
//!   paddings are *excluded* from these functions (they measure content
//!   space) — callers add [`intrinsic_extras`] for outer contributions.
//!
//! Recursion depth is bounded by the DOM depth limit (the same bound the
//! rest of layout runs under).

use crate::block::{ContainingBlock, Placed};
use crate::box_tree::{Box, BoxContent};
use crate::error::LayoutError;
use crate::inline::tokenize;
use crate::limits::Budget;
use crate::text::TextMeasurer;

/// The box's horizontal extras (margin + border + padding) with
/// percentages resolved against zero — the outer contribution a nested
/// box adds to its parent's intrinsic widths.
pub(crate) fn intrinsic_extras(b: &Box) -> f32 {
    let margin = |value: &veyra_style::MarginSizing| match value {
        veyra_style::MarginSizing::Auto => 0.0,
        veyra_style::MarginSizing::LengthPercent(v) => resolve_zero(v),
    };
    let padding = |value: &veyra_style::LengthPercent| resolve_zero(value);
    let borders = crate::box_tree::used_border_widths(&b.style);
    margin(&b.style.margin.left)
        + margin(&b.style.margin.right)
        + borders.left
        + borders.right
        + padding(&b.style.padding.left)
        + padding(&b.style.padding.right)
}

/// Percentages resolve to zero in intrinsic contexts; lengths are px.
fn resolve_zero(value: &veyra_style::LengthPercent) -> f32 {
    match value {
        veyra_style::LengthPercent::Px(px) => *px,
        _ => 0.0,
    }
}

/// The min-content width of a box's content (px, content space).
pub(crate) fn min_content_width(
    measurer: &dyn TextMeasurer,
    b: &Box,
    budget: &mut Budget,
) -> Result<f32, LayoutError> {
    match &b.content {
        BoxContent::Empty => Ok(0.0),
        BoxContent::Inlines(items) => {
            let tokens = tokenize(measurer, items, budget)?;
            let mut min = 0.0f32;
            for token in tokens {
                match token {
                    crate::inline::PreToken::Word { width, .. } => min = min.max(width),
                    crate::inline::PreToken::Atomic(child) => {
                        let inner = min_content_width(measurer, child, budget)?;
                        min = min.max(inner + intrinsic_extras(child));
                    }
                    // Spaces break; they never widen min-content.
                    _ => {}
                }
            }
            Ok(min)
        }
        BoxContent::Blocks(children) => {
            let mut min = 0.0f32;
            for child in children {
                let contribution = match child.style.width {
                    veyra_style::Sizing::Auto => min_content_width(measurer, child, budget)?,
                    veyra_style::Sizing::LengthPercent(v) => resolve_zero(&v),
                };
                min = min.max(contribution + intrinsic_extras(child));
            }
            Ok(min)
        }
    }
}

/// The max-content width of a box's content (px, content space).
pub(crate) fn max_content_width(
    measurer: &dyn TextMeasurer,
    b: &Box,
    budget: &mut Budget,
) -> Result<f32, LayoutError> {
    match &b.content {
        BoxContent::Empty => Ok(0.0),
        BoxContent::Inlines(items) => {
            let tokens = tokenize(measurer, items, budget)?;
            let mut max = 0.0f32;
            let mut segment = 0.0f32;
            let mut pending_space = 0.0f32;
            for token in tokens {
                match token {
                    crate::inline::PreToken::Word { width, .. } => {
                        segment += pending_space + width;
                        pending_space = 0.0;
                    }
                    crate::inline::PreToken::Space { width } => {
                        pending_space += width;
                    }
                    crate::inline::PreToken::Atomic(child) => {
                        let inner = max_content_width(measurer, child, budget)?;
                        segment += pending_space + inner + intrinsic_extras(child);
                        pending_space = 0.0;
                    }
                    crate::inline::PreToken::Break => {
                        // Forced break: close the segment (a trailing
                        // space before the break never counted).
                        max = max.max(segment);
                        segment = 0.0;
                        pending_space = 0.0;
                    }
                }
            }
            Ok(max.max(segment))
        }
        BoxContent::Blocks(children) => {
            let mut max = 0.0f32;
            for child in children {
                let contribution = match child.style.width {
                    veyra_style::Sizing::Auto => max_content_width(measurer, child, budget)?,
                    veyra_style::Sizing::LengthPercent(v) => resolve_zero(&v),
                };
                max = max.max(contribution + intrinsic_extras(child));
            }
            Ok(max)
        }
    }
}

/// The shrink-to-fit width (CSS 2.1 §10.3.5):
/// `min(max(min-content, available), max-content)`.
fn shrink_to_fit(min: f32, max: f32, available: f32) -> f32 {
    min.max(available.min(max))
}

/// Lays an atomic inline-level box out for placement on a line:
/// resolves its outer spacings against the line's containing block,
/// derives the shrink-to-fit content width, and runs its internal layout
/// as a block-formatting-context root.
pub(crate) fn layout_atomic_on_line(
    measurer: &dyn TextMeasurer,
    b: &Box,
    containing_inline: f32,
    block_definite: Option<f32>,
    budget: &mut Budget,
) -> Result<std::boxed::Box<Placed>, LayoutError> {
    let borders = crate::box_tree::used_border_widths(&b.style);
    let paddings = crate::box_tree::used_paddings(&b.style, containing_inline);
    let margin = |value: &veyra_style::MarginSizing| match value {
        veyra_style::MarginSizing::Auto => 0.0,
        veyra_style::MarginSizing::LengthPercent(v) => v.resolve_against(containing_inline),
    };
    let margin_left = margin(&b.style.margin.left);
    let margin_right = margin(&b.style.margin.right);

    let available = containing_inline
        - margin_left
        - margin_right
        - borders.left
        - borders.right
        - paddings.left
        - paddings.right;
    let min = min_content_width(measurer, b, budget)?;
    let max = max_content_width(measurer, b, budget)?;
    let content_width = shrink_to_fit(min, max, available.max(0.0));

    let cb = ContainingBlock {
        inline: content_width,
        block_definite,
    };
    crate::block::layout_block_box(measurer, b, &cb, true, budget)
}

//! The block formatting context (CSS 2.1 §9.4.1, §10.3.3, §10.6.2/§10.6.3,
//! §8.3.1 margin collapsing): width resolution, collapsed-margin flow,
//! height resolution, and fragment construction.
//!
//! # Margin collapsing model
//!
//! Collapsing margins are tracked as a *(max positive, min negative)*
//! pair — the CSS 2.1 §8.3.1 combination rule ("maximum of the positive
//! values, minimum of the negative values") is associative only in this
//! pair form, and nesting collapses unions the participating margins.
//! The flow keeps three sets per container:
//!
//! - **top_open** — margins that escaped upward past the container's top
//!   edge (first child's top margin when the container is not separated,
//!   plus any self-collapsing siblings ahead of it);
//! - **pending** — margins between the last placed content and the next
//!   box (previous sibling's bottom margin, plus self-collapsing runs);
//! - **bottom_open** — margins that escape below the content (last
//!   child's bottom margin when the container is not separated, trailing
//!   self-collapsing runs).
//!
//! Containers that establish a block formatting context (the root,
//! inline-blocks) do not collapse with their children: both separation
//! flags are forced on for the collapse computation.
//!
//! # The Phase 3 subset (each deviation has a recorded home)
//!
//! - No floats/positioning/table boxes yet (not in the working property
//!   set); every block-level child is in normal flow.
//! - `clear`ance does not exist; the collapse rules above assume its
//!   absence.
//! - Percentage heights resolve only against a *definite* containing
//!   block height (the ICB, or a box with a specified height); otherwise
//!   they behave as `auto` (CSS 2.1 §10.5).
//! - Over-constrained width resolution is resolved in `ltr` (the
//!   `direction` property is not in the working set): an over-constrained
//!   `margin-right` absorbs the difference.
//! - Overflow is always visible; content may extend past a specified
//!   height.

use veyra_style::{LengthPercent, MarginSizing, Sizing};

use crate::box_tree::{Box, BoxContent, used_border_widths, used_paddings};
use crate::error::LayoutError;
use crate::fragment::{BoxFragment, Fragment};
use crate::geometry::PhysicalRect;
use crate::inline::{InlineFlow, layout_inline_flow};
use crate::limits::Budget;
use crate::text::TextMeasurer;

/// A collapsed-margin set: the spec's "maximum positive + minimum
/// negative" carried as components so merges stay associative.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct CollapsedMargin {
    max_positive: f32,
    min_negative: f32,
}

impl CollapsedMargin {
    /// A set holding one margin value.
    pub(crate) fn single(value: f32) -> Self {
        CollapsedMargin {
            max_positive: value.max(0.0),
            min_negative: value.min(0.0),
        }
    }

    /// Merges another set (union of participating margins).
    pub(crate) fn merge(self, other: Self) -> Self {
        CollapsedMargin {
            max_positive: self.max_positive.max(other.max_positive),
            min_negative: self.min_negative.min(other.min_negative),
        }
    }

    /// The collapsed margin value.
    pub(crate) fn value(self) -> f32 {
        self.max_positive + self.min_negative
    }
}

/// The containing block a box is laid out in: its content-box inline
/// size, and its definite block size when one exists (percentage heights
/// resolve only against a definite height).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContainingBlock {
    /// Content-box width (px).
    pub inline: f32,
    /// Content-box height when definite (px).
    pub block_definite: Option<f32>,
}

/// A laid-out box, before the parent places it.
pub(crate) struct Placed {
    /// The complete fragment (rects relative to the box's own border-box
    /// origin; the parent translates it into page coordinates).
    pub fragment: Fragment,
    /// Used margin-left (px; `auto` resolves per width resolution).
    pub margin_left: f32,
    /// Used margin-top (px; `auto` is 0 in normal flow).
    pub margin_top: f32,
    /// Border-box width.
    pub border_box_width: f32,
    /// Border-box height.
    pub border_box_height: f32,
    /// The top margin this box presents to sibling/parent collapsing.
    pub collapsed_top: CollapsedMargin,
    /// The bottom margin this box presents to collapsing.
    pub collapsed_bottom: CollapsedMargin,
    /// Whether the box's own margins collapse through it (it occupies
    /// no vertical space).
    pub collapses_through: bool,
    /// The baseline of the box's last line box, from the border-box top
    /// (inline-block baseline rule; `None` without line boxes).
    pub baseline_from_border_top: Option<f32>,
}

/// Lays out one block-level (or atomic inline-level) box in `cb`.
///
/// `is_bfc_root` marks boxes that establish a block formatting context
/// (the root element, inline-blocks): their children's margins do not
/// collapse with their own. The result is boxed to keep the recursion's
/// stack frames small — the fragment tree itself lives on the heap, so
/// the full DOM depth cap (512) cannot overflow even a 2 MiB stack.
pub(crate) fn layout_block_box(
    measurer: &dyn TextMeasurer,
    b: &Box,
    cb: &ContainingBlock,
    is_bfc_root: bool,
    budget: &mut Budget,
) -> Result<std::boxed::Box<Placed>, LayoutError> {
    let borders = used_border_widths(&b.style);
    let paddings = used_paddings(&b.style, cb.inline);
    let auto_margin = |value: &MarginSizing| match value {
        MarginSizing::Auto => 0.0,
        MarginSizing::LengthPercent(v) => v.resolve_against(cb.inline),
    };
    let margin_top = auto_margin(&b.style.margin.top);
    let margin_bottom = auto_margin(&b.style.margin.bottom);
    let specified_ml = auto_margin(&b.style.margin.left);
    let specified_mr = auto_margin(&b.style.margin.right);

    // -- Width resolution (CSS 2.1 §10.3.3) ------------------------------
    let content_space =
        (cb.inline - borders.left - borders.right - paddings.left - paddings.right).max(0.0);
    let (used_width, margin_left, _margin_right) = match b.style.width {
        Sizing::Auto => {
            // Width auto: every auto margin is 0; the box fills the
            // remaining space.
            (
                (content_space - specified_ml - specified_mr).max(0.0),
                specified_ml,
                specified_mr,
            )
        }
        Sizing::LengthPercent(v) => {
            let width = v.resolve_against(cb.inline).max(0.0);
            match (&b.style.margin.left, &b.style.margin.right) {
                (MarginSizing::Auto, MarginSizing::Auto) => {
                    // Both auto: equal split, clamped at zero (an
                    // over-wide box overflows, it is not pulled left).
                    let split = ((content_space - width) / 2.0).max(0.0);
                    (width, split, split)
                }
                (MarginSizing::Auto, _) => {
                    // Solve for the auto left margin.
                    (width, content_space - width - specified_mr, specified_mr)
                }
                (_, MarginSizing::Auto) => {
                    // Solve for the auto right margin.
                    (width, specified_ml, content_space - width - specified_ml)
                }
                (_, _) => {
                    // Over-constrained (ltr): margin-right absorbs.
                    (width, specified_ml, content_space - width - specified_ml)
                }
            }
        }
    };
    let content_width = used_width;

    // -- Definite-height resolution (CSS 2.1 §10.5) ----------------------
    let specified_height: Option<f32> = match &b.style.height {
        Sizing::Auto => None,
        Sizing::LengthPercent(LengthPercent::Percent(p)) => {
            cb.block_definite.map(|h| h * p / 100.0)
        }
        Sizing::LengthPercent(other) => Some(other.resolve_against(b.style.font_size)),
    };

    // -- Children ---------------------------------------------------------
    let child_cb = ContainingBlock {
        inline: content_width,
        block_definite: specified_height,
    };
    let container_top_separated = is_bfc_root || b.top_separated(cb.inline);
    let container_bottom_separated = is_bfc_root || b.bottom_separated(cb.inline);

    let mut children: Vec<Fragment> = Vec::new();
    let mut flow = BfcFlow::new(container_top_separated, container_bottom_separated);
    let mut inline_flow: Option<InlineFlow> = None;

    match &b.content {
        BoxContent::Blocks(block_children) => {
            for child in block_children {
                let placed = layout_block_box(measurer, child, &child_cb, false, budget)?;
                flow.absorb(*placed, &mut children);
            }
            flow.finish();
        }
        BoxContent::Inlines(items) => {
            let mut result = layout_inline_flow(
                measurer,
                items,
                content_width,
                &b.style,
                specified_height,
                budget,
            )?;
            // Inline fragments are content-relative; rebase them onto the
            // border-box origin.
            let dx = borders.left + paddings.left;
            let dy = borders.top + paddings.top;
            for fragment in &mut result.fragments {
                fragment.translate(dx, dy);
            }
            children.append(&mut result.fragments);
            flow.content_height = result.content_height;
            flow.has_line_boxes = result.has_line_boxes;
            inline_flow = Some(result);
        }
        BoxContent::Empty => {}
    }

    // -- Height (CSS 2.1 §10.6.2/§10.6.3) --------------------------------
    let content_height = specified_height.unwrap_or(flow.content_height);

    // -- Fragment ---------------------------------------------------------
    let border_box_width =
        borders.left + paddings.left + content_width + paddings.right + borders.right;
    let border_box_height =
        borders.top + paddings.top + content_height + paddings.bottom + borders.bottom;
    budget.charge()?;
    let fragment = Fragment::Box(BoxFragment {
        node: b.node,
        name: b.name.clone(),
        anonymous: b.anonymous,
        border: PhysicalRect::new(0.0, 0.0, border_box_width, border_box_height),
        padding: PhysicalRect::new(
            borders.left,
            borders.top,
            (border_box_width - borders.left - borders.right).max(0.0),
            (border_box_height - borders.top - borders.bottom).max(0.0),
        ),
        content: PhysicalRect::new(
            borders.left + paddings.left,
            borders.top + paddings.top,
            content_width,
            content_height,
        ),
        children,
        background: b.style.background_color,
        border_colors: b.style.border_color,
    });

    // -- Collapse metadata ------------------------------------------------
    let own_top = CollapsedMargin::single(margin_top);
    let own_bottom = CollapsedMargin::single(margin_bottom);
    let collapsed_top = if container_top_separated {
        own_top
    } else {
        own_top.merge(flow.top_open)
    };
    let collapsed_bottom = if container_bottom_separated {
        own_bottom
    } else {
        own_bottom.merge(flow.bottom_open)
    };
    let collapses_through = !container_top_separated
        && !container_bottom_separated
        && content_height == 0.0
        && !flow.has_line_boxes
        && !flow.placed_any;

    // Inline-block baseline: last line box's baseline from the border-box
    // top; none without line boxes.
    let baseline_from_border_top = if b.kind == crate::box_tree::BoxKind::InlineBlock {
        inline_flow.as_ref().and_then(|flow| {
            flow.last_baseline
                .map(|baseline| borders.top + paddings.top + baseline)
        })
    } else {
        None
    };

    Ok(std::boxed::Box::new(Placed {
        fragment,
        margin_left,
        margin_top,
        border_box_width,
        border_box_height,
        collapsed_top,
        collapsed_bottom,
        collapses_through,
        baseline_from_border_top,
    }))
}
/// The in-flow vertical state of one block container's children (the
/// collapsed-margin flow described in the module docs).
struct BfcFlow {
    container_top_separated: bool,
    container_bottom_separated: bool,
    top_open: CollapsedMargin,
    bottom_open: CollapsedMargin,
    pending: CollapsedMargin,
    y: f32,
    placed_any: bool,
    has_line_boxes: bool,
    content_height: f32,
}

impl BfcFlow {
    fn new(container_top_separated: bool, container_bottom_separated: bool) -> Self {
        BfcFlow {
            container_top_separated,
            container_bottom_separated,
            top_open: CollapsedMargin::default(),
            bottom_open: CollapsedMargin::default(),
            pending: CollapsedMargin::default(),
            y: 0.0,
            placed_any: false,
            has_line_boxes: false,
            content_height: 0.0,
        }
    }

    /// Places one laid-out child into the flow.
    fn absorb(&mut self, placed: Placed, children: &mut Vec<Fragment>) {
        if placed.collapses_through {
            // Self-collapsing box: no vertical space; its margins join
            // whichever opening they adjoin (upward before any placed
            // content in an unseparated container, the inter-box gap
            // otherwise).
            if !self.placed_any && !self.container_top_separated {
                self.top_open = self
                    .top_open
                    .merge(placed.collapsed_top)
                    .merge(placed.collapsed_bottom);
            } else {
                self.pending = self
                    .pending
                    .merge(placed.collapsed_top)
                    .merge(placed.collapsed_bottom);
            }
            return;
        }

        let gap = if !self.placed_any && !self.container_top_separated {
            // Parent-child top collapse: the child's top margin (plus any
            // leading self-collapsing run) escapes above the container.
            self.top_open = self
                .top_open
                .merge(self.pending)
                .merge(placed.collapsed_top);
            self.pending = CollapsedMargin::default();
            0.0
        } else {
            self.pending.merge(placed.collapsed_top).value()
        };
        self.pending = CollapsedMargin::default();

        let border_top_y = self.y + gap;
        let mut fragment = placed.fragment;
        // Horizontal placement: the child's used margin-left offsets its
        // border box within our content box.
        fragment.translate(placed.margin_left, border_top_y);
        children.push(fragment);
        self.y = border_top_y + placed.border_box_height;
        self.pending = placed.collapsed_bottom;
        self.placed_any = true;
    }

    /// Closes the flow: resolves the content height and the bottom
    /// opening.
    fn finish(&mut self) {
        if self.placed_any {
            if self.container_bottom_separated {
                // The last child's bottom margin stays inside.
                self.content_height = self.y + self.pending.value();
            } else {
                // It escapes below the content edge.
                self.content_height = self.y;
                self.bottom_open = self.pending;
            }
        } else {
            // No placed content (all children self-collapsed, or none):
            // the pending set is the container's own content height when
            // the bottom edge holds it, and escapes when it does not.
            if self.container_bottom_separated {
                self.content_height = self.pending.value();
            } else {
                self.bottom_open = self.pending;
                if !self.container_top_separated {
                    // The same collapsed set escapes both ways: the box
                    // collapses through entirely.
                    self.top_open = self.top_open.merge(self.pending);
                }
            }
        }
    }
}

//! Box-tree construction: a DOM tree + computed styles → the layout
//! structure CSS 2.1 §9 describes (display filtering, anonymous block
//! boxes, inline content).
//!
//! The builder is a faithful structural pass — it decides *what boxes
//! exist*; the formatting contexts (block.rs/inline.rs) decide *where they
//! go*. Construction never panics: every tree shape the DOM limits allow
//! (512 depth, 2,000,000 nodes) maps to boxes or is filtered, and the
//! [`LayoutLimits`] budget bounds the total allocation.
//!
//! # The Phase 3 subset (each deviation has a recorded home)
//!
//! - `display: none` subtrees are skipped (UA sheet hides `head`/`script`
//!   and friends, so they produce no boxes).
//! - `block` → block-level boxes; `inline-block` → atomic inline-level
//!   boxes (their own BFC); `inline` → transparent inline context: the
//!   element produces no box of its own this batch (inline box decoration
//!   — borders/padding/margins on non-atomic inline elements — is the
//!   recorded deferral; the paint batch's inline fragments supersede).
//!   Its children continue in the nearest block-level ancestor's inline
//!   flow, carrying the inline element's computed style (inheritance is
//!   preserved for text measurement).
//! - A block-level element inside an inline run is **hoisted** to the
//!   nearest block-level box's child list, splitting the run into
//!   anonymous block boxes (CSS 2.1 §9.2.1.1 semantics, reached by the
//!   flattening walk rather than a separate fixup pass).
//! - Inline runs grouped *between* block-level children become anonymous
//!   block boxes (display forced to `block`, margins/paddings/borders
//!   zeroed per spec, inheritable properties from the owning element).
//!   A run that contains only collapsible whitespace produces no box at
//!   all (it cannot generate a line box under `white-space: normal`).
//! - `<br>` (HTML namespace) is a forced line break inside inline content;
//!   every other element is classified purely by computed display.

use veyra_dom::{NodeData, NodeId, Tree};
use veyra_style::{ComputedStyleMap, ComputedValues, Display, LengthPercent, MarginSizing, Sides};

use crate::error::LayoutError;
use crate::limits::Budget;

/// An inline-formatting item: a piece of the inline flow in document
/// order (text with its owning style context, an atomic inline-level box,
/// or a forced break).
#[derive(Debug, Clone)]
pub(crate) enum InlineItem {
    /// A text node's content under its parent element's computed style.
    Text {
        /// The text node (fragment provenance).
        node: NodeId,
        /// The text (raw; whitespace collapsing is inline layout's job).
        text: String,
        /// The innermost element's computed values the text inherits
        /// from (font group, color, alignment).
        style: ComputedValues,
    },
    /// An atomic inline-level box (`display: inline-block`).
    Atomic(Box),
    /// `<br>`: a forced line break.
    ForcedBreak,
}

/// What a box's content is formatted as.
#[derive(Debug, Clone)]
pub(crate) enum BoxContent {
    /// No content at all.
    Empty,
    /// Only inline content (the box runs an inline formatting context).
    Inlines(Vec<InlineItem>),
    /// Only block-level children (the box runs a block formatting
    /// context over them).
    Blocks(Vec<Box>),
}

/// The kind of box: how it participates in its parent's formatting
/// context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoxKind {
    /// Block-level: stacked in the parent's block flow.
    BlockLevel,
    /// Atomic inline-level: participates in inline flow, establishes its
    /// own internal formatting context (BFC semantics for its children).
    InlineBlock,
}

/// One box: an element (or an anonymous block) with computed style and
/// content.
#[derive(Debug, Clone)]
pub(crate) struct Box {
    /// The owning element (`None` for anonymous boxes).
    pub node: Option<NodeId>,
    /// The element's local name (empty for anonymous boxes; kept for the
    /// deterministic dump without re-walking the DOM).
    pub name: String,
    /// Whether this is an anonymous block box.
    pub anonymous: bool,
    /// Computed values (cloned at build; anonymous boxes carry the
    /// owning element's inheritables with display/margin/padding/border
    /// forced per spec).
    pub style: ComputedValues,
    /// How the box participates in its parent.
    pub kind: BoxKind,
    /// The content to format.
    pub content: BoxContent,
}

impl Box {
    /// Whether the box's top edge can collapse margins with its first
    /// in-flow child (CSS 2.1 §8.3.1: no top border/padding separates
    /// them; a `none`/`hidden` border style has zero used width).
    pub(crate) fn top_separated(&self, containing_inline: f32) -> bool {
        let border = used_border_widths(&self.style);
        border.top > 0.0 || used_paddings(&self.style, containing_inline).top > 0.0
    }

    /// Whether the box's bottom edge can collapse margins with its last
    /// in-flow child.
    pub(crate) fn bottom_separated(&self, containing_inline: f32) -> bool {
        let border = used_border_widths(&self.style);
        border.bottom > 0.0 || used_paddings(&self.style, containing_inline).bottom > 0.0
    }
}

/// The used border widths (px): a `none`/`hidden` style forces the used
/// width to zero per CSS 2.1 §8.5.4, whatever `border-width` parsed to.
#[must_use]
pub(crate) fn used_border_widths(style: &ComputedValues) -> Sides<f32> {
    let side = |width: f32, style: veyra_style::BorderStyle| match style {
        veyra_style::BorderStyle::None | veyra_style::BorderStyle::Hidden => 0.0,
        _ => width,
    };
    Sides {
        top: side(style.border_width.top, style.border_style.top),
        right: side(style.border_width.right, style.border_style.right),
        bottom: side(style.border_width.bottom, style.border_style.bottom),
        left: side(style.border_width.left, style.border_style.left),
    }
}

/// The used paddings (px): percentages resolve against the containing
/// block's content width (all four sides, per CSS 2.1 §8.4).
#[must_use]
pub(crate) fn used_paddings(style: &ComputedValues, containing_inline: f32) -> Sides<f32> {
    let resolve = |value: &LengthPercent| value.resolve_against(containing_inline).max(0.0);
    Sides {
        top: resolve(&style.padding.top),
        right: resolve(&style.padding.right),
        bottom: resolve(&style.padding.bottom),
        left: resolve(&style.padding.left),
    }
}

/// The built box tree: the root element's box laid out in the initial
/// containing block.
pub(crate) struct BuiltBoxTree {
    /// The root box (`<html>`, or a synthetic box for degenerate
    /// documents without an element).
    pub root: Box,
}

/// One child slot during collection: either an accumulated inline run or
/// a completed block-level box.
enum ChildSlot {
    Inline(Vec<InlineItem>),
    /// Std-boxed: `box_tree::Box` carries a full `ComputedValues`
    /// (~280 bytes); the slot enum stays pointer-sized for the hot
    /// collector paths.
    Block(std::boxed::Box<Box>),
}

/// The shared build state: DOM/style references and the allocation
/// budget.
pub(crate) struct BuildContext<'a> {
    pub tree: &'a Tree,
    pub styles: &'a ComputedStyleMap,
    pub budget: &'a mut Budget,
}

impl<'a> BuildContext<'a> {
    /// Charges one allocation against the budget.
    pub(crate) fn charge(&mut self) -> Result<(), LayoutError> {
        self.budget.charge()
    }
}

/// Builds the box tree for a document.
///
/// # Errors
///
/// [`LayoutError::LimitExceeded`] when the box budget is exhausted.
pub(crate) fn build(
    tree: &Tree,
    styles: &ComputedStyleMap,
    budget: &mut Budget,
) -> Result<BuiltBoxTree, LayoutError> {
    let mut ctx = BuildContext {
        tree,
        styles,
        budget,
    };

    // The root: the document's element child (`<html>` per the HTML
    // tree-construction spec; the defensive fallback covers degenerate
    // trees so an empty document lays out as blank rather than erroring).
    let document = tree.document().ok_or(LayoutError::LimitExceeded {
        limit: "boxes",
        max: 0,
    })?;
    let root_element = tree
        .children(document)
        .unwrap_or(&[])
        .iter()
        .copied()
        .find(|child| matches!(tree.data(*child), Some(NodeData::Element { .. })));

    let Some(root_element) = root_element else {
        return Ok(BuiltBoxTree {
            root: synthetic_root(&mut ctx, document)?,
        });
    };

    // The explicit-stack construction machine. Frames never recurse:
    // a 512-deep DOM cannot overflow the stack regardless of thread
    // stack size (the engineering-standards traversal discipline).
    //
    // - `Frame::BoxFrame` — a block-level/inline-block element under
    //   construction; owns a collector for its children. On completion
    //   its [`Box`] is routed to the parent recorded at push time (as a
    //   block slot, or as an atomic inline item).
    // - `Frame::Inline` — an inline element being flattened: it owns no
    //   collector; its children continue in the nearest enclosing box
    //   frame, with the inline element's computed style as the text
    //   style context.
    enum Frame {
        BoxFrame {
            element: NodeId,
            name: String,
            style: ComputedValues,
            kind: BoxKind,
            /// Stack index of the receiving parent box frame (`None`
            /// for the root).
            parent: Option<usize>,
            /// The parent receives this box as an atomic inline item
            /// (inline-block) rather than a block slot.
            as_atomic: bool,
            /// Child cursor: (parent node, next child index).
            cursor: (NodeId, usize),
            collector: ChildCollector,
        },
        Inline {
            style: ComputedValues,
            cursor: (NodeId, usize),
        },
    }

    impl Frame {
        /// Whether the frame's children are exhausted.
        fn exhausted(&self, tree: &Tree) -> bool {
            let (parent, index) = match self {
                Frame::BoxFrame { cursor, .. } | Frame::Inline { cursor, .. } => *cursor,
            };
            index >= tree.children(parent).unwrap_or(&[]).len()
        }

        /// The stack index of the nearest enclosing box frame.
        fn nearest_box_index(stack: &[Frame]) -> Option<usize> {
            stack
                .iter()
                .rposition(|frame| matches!(frame, Frame::BoxFrame { .. }))
        }
    }

    let root_name = match ctx.tree.data(root_element) {
        Some(NodeData::Element { name, .. }) => name.local.clone(),
        _ => String::new(),
    };
    let root_style = ctx
        .styles
        .get(root_element)
        .cloned()
        .unwrap_or_else(ComputedValues::initial);
    let root_kind = match root_style.display {
        Display::InlineBlock => BoxKind::InlineBlock,
        _ => BoxKind::BlockLevel,
    };
    ctx.charge()?;
    let mut stack: Vec<Frame> = vec![Frame::BoxFrame {
        element: root_element,
        name: root_name,
        style: root_style,
        kind: root_kind,
        parent: None,
        as_atomic: false,
        cursor: (root_element, 0),
        collector: ChildCollector::default(),
    }];

    let mut completed: Option<Box> = None;
    while completed.is_none() {
        // Pop every exhausted frame (inline frames vanish; box frames
        // finalize and route their Box to the parent recorded at push
        // time).
        while let Some(top) = stack.last() {
            if !top.exhausted(ctx.tree) {
                break;
            }
            let Some(frame) = stack.pop() else { break };
            let Frame::BoxFrame {
                element,
                name,
                style,
                kind,
                parent,
                as_atomic,
                collector,
                ..
            } = frame
            else {
                // Inline frames leave nothing behind.
                continue;
            };
            let mut collector = collector;
            collector.flush_group();
            let content = merge_slots(&mut ctx, &mut collector.slots, &style)?;
            let box_ = Box {
                node: Some(element),
                name,
                anonymous: false,
                style,
                kind,
                content,
            };
            match parent {
                None => {
                    completed = Some(box_);
                    break;
                }
                Some(index) => {
                    let Some(owner) = stack.get_mut(index) else {
                        continue;
                    };
                    let Frame::BoxFrame { collector, .. } = owner else {
                        continue;
                    };
                    if as_atomic {
                        ctx.charge()?;
                        collector.group.push(InlineItem::Atomic(box_));
                    } else {
                        collector
                            .slots
                            .push(ChildSlot::Block(std::boxed::Box::new(box_)));
                    }
                }
            }
        }
        if completed.is_some() || stack.is_empty() {
            break;
        }

        // Advance the top frame by one child.
        let next = {
            let top = stack.last();
            let (parent_node, index) = match top {
                Some(Frame::BoxFrame { cursor, .. } | Frame::Inline { cursor, .. }) => *cursor,
                None => break,
            };
            let children = ctx.tree.children(parent_node).unwrap_or(&[]);
            children.get(index).copied()
        };
        let Some(child) = next else {
            // Exhausted (raced with the pop loop) — loop again to pop.
            continue;
        };
        // Advance the cursor.
        match stack.last_mut() {
            Some(Frame::BoxFrame { cursor, .. } | Frame::Inline { cursor, .. }) => cursor.1 += 1,
            None => break,
        }

        match ctx.tree.data(child) {
            Some(NodeData::Text { data }) => {
                if data.is_empty() {
                    continue;
                }
                // The text inherits from the innermost frame's style.
                let style = match stack.last() {
                    Some(Frame::Inline { style, .. }) => style.clone(),
                    Some(Frame::BoxFrame { style, .. }) => style.clone(),
                    None => continue,
                };
                let Some(index) = Frame::nearest_box_index(&stack) else {
                    continue;
                };
                let Some(Frame::BoxFrame { collector, .. }) = stack.get_mut(index) else {
                    continue;
                };
                ctx.charge()?;
                collector.group.push(InlineItem::Text {
                    node: child,
                    text: data.clone(),
                    style,
                });
            }
            Some(NodeData::Element { name, .. }) => {
                let Some(style) = ctx.styles.get(child) else {
                    continue;
                };
                match style.display {
                    Display::None => continue,
                    Display::Inline => {
                        if name.namespace == veyra_dom::Namespace::Html && name.local == "br" {
                            let Some(index) = Frame::nearest_box_index(&stack) else {
                                continue;
                            };
                            let Some(Frame::BoxFrame { collector, .. }) = stack.get_mut(index)
                            else {
                                continue;
                            };
                            ctx.charge()?;
                            collector.group.push(InlineItem::ForcedBreak);
                            continue;
                        }
                        // Flatten: the inline element's children continue
                        // in this context under its computed style.
                        stack.push(Frame::Inline {
                            style: style.clone(),
                            cursor: (child, 0),
                        });
                    }
                    display => {
                        // Block-level or atomic inline-level: a new box
                        // frame, routed to the nearest enclosing box.
                        let Some(index) = Frame::nearest_box_index(&stack) else {
                            continue;
                        };
                        if display == Display::Block {
                            // The inline run before a block closes.
                            let Some(Frame::BoxFrame { collector, .. }) = stack.get_mut(index)
                            else {
                                continue;
                            };
                            collector.flush_group();
                        }
                        let box_name = name.local.clone();
                        let box_style = style.clone();
                        let box_kind = match display {
                            Display::InlineBlock => BoxKind::InlineBlock,
                            _ => BoxKind::BlockLevel,
                        };
                        ctx.charge()?;
                        stack.push(Frame::BoxFrame {
                            element: child,
                            name: box_name,
                            style: box_style,
                            kind: box_kind,
                            parent: Some(index),
                            as_atomic: display == Display::InlineBlock,
                            cursor: (child, 0),
                            collector: ChildCollector::default(),
                        });
                    }
                }
            }
            // Comments, doctypes, fragments: no boxes.
            _ => continue,
        }
    }

    let root = match completed {
        Some(box_) => box_,
        // Defensive: the machine must always complete for a well-formed
        // stack; degrade to a blank synthetic root rather than panic.
        None => synthetic_root(&mut ctx, document)?,
    };
    Ok(BuiltBoxTree { root })
}

/// The synthetic root box for degenerate documents without an element
/// child (blank page; the layout engine never panics on such input).
fn synthetic_root(ctx: &mut BuildContext<'_>, document: NodeId) -> Result<Box, LayoutError> {
    ctx.charge()?;
    let mut style = ComputedValues::initial();
    style.display = Display::Block;
    Ok(Box {
        node: Some(document),
        name: "#document".to_string(),
        anonymous: true,
        style,
        kind: BoxKind::BlockLevel,
        content: BoxContent::Empty,
    })
}

/// The per-element child collector: accumulates inline runs and block
/// boxes in document order.
#[derive(Default)]
struct ChildCollector {
    slots: Vec<ChildSlot>,
    group: Vec<InlineItem>,
}

impl ChildCollector {
    /// Emits the pending inline run as one slot (empty runs emit
    /// nothing).
    fn flush_group(&mut self) {
        if !self.group.is_empty() {
            let group = std::mem::take(&mut self.group);
            self.slots.push(ChildSlot::Inline(group));
        }
    }
}

/// Merges the collected slots into the box's content: all-inline content
/// stays inline; mixed content wraps each inline run in an anonymous
/// block (whitespace-only runs vanish — they cannot produce a line box).
///
/// # Errors
///
/// [`LayoutError::LimitExceeded`] when the box budget is exhausted.
fn merge_slots(
    ctx: &mut BuildContext<'_>,
    slots: &mut Vec<ChildSlot>,
    element_style: &ComputedValues,
) -> Result<BoxContent, LayoutError> {
    let has_blocks = slots.iter().any(|slot| matches!(slot, ChildSlot::Block(_)));
    if !has_blocks {
        // All inline: zero or one group exists (a block boundary is the
        // only thing that can split groups).
        return Ok(match slots.pop() {
            Some(ChildSlot::Inline(items)) => BoxContent::Inlines(items),
            _ => BoxContent::Empty,
        });
    }

    let mut blocks = Vec::new();
    for slot in slots.drain(..) {
        match slot {
            ChildSlot::Block(box_) => blocks.push(*box_),
            ChildSlot::Inline(items) => {
                if group_is_whitespace_only(&items) {
                    continue;
                }
                ctx.charge()?;
                let style = anonymous_block_style(element_style);
                blocks.push(Box {
                    node: None,
                    name: String::new(),
                    anonymous: true,
                    style,
                    kind: BoxKind::BlockLevel,
                    content: BoxContent::Inlines(items),
                });
            }
        }
    }
    if blocks.is_empty() {
        Ok(BoxContent::Empty)
    } else {
        Ok(BoxContent::Blocks(blocks))
    }
}

/// Whether an inline run contains only collapsible whitespace (CSS Text
/// §3: the v1 working set is always `white-space: normal`, so such a run
/// collapses to nothing and cannot generate a line box).
fn group_is_whitespace_only(items: &[InlineItem]) -> bool {
    items.iter().all(|item| match item {
        InlineItem::Text { text, .. } => text
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')),
        // Atomics and forced breaks are content.
        _ => false,
    })
}

/// The computed style of an anonymous block: inheritables from the owning
/// element, `display: block`, and zeroed box spacing (CSS 2.1 §9.2.1.1:
/// anonymous boxes are not addressable, so their margins/paddings/borders
/// are the initial zeros regardless of what the owning element has).
fn anonymous_block_style(parent: &ComputedValues) -> ComputedValues {
    let mut style = parent.clone();
    style.display = Display::Block;
    style.margin = Sides {
        top: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
        right: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
        bottom: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
        left: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
    };
    style.padding = zero_sides();
    style.border_style = Sides {
        top: veyra_style::BorderStyle::None,
        right: veyra_style::BorderStyle::None,
        bottom: veyra_style::BorderStyle::None,
        left: veyra_style::BorderStyle::None,
    };
    style.border_width = Sides {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
    style.width = veyra_style::Sizing::Auto;
    style.height = veyra_style::Sizing::Auto;
    style
}

/// `Sides` of zero lengths (padding/border-width helper).
pub(crate) fn zero_sides() -> Sides<LengthPercent> {
    Sides {
        top: LengthPercent::Px(0.0),
        right: LengthPercent::Px(0.0),
        bottom: LengthPercent::Px(0.0),
        left: LengthPercent::Px(0.0),
    }
}

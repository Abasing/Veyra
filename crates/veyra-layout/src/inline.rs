//! The inline formatting context (CSS 2.1 §9.4.2/§10.8, CSS Text 3 §3
//! whitespace subset): whitespace collapsing, line breaking at break
//! opportunities, line boxes with the strut, `text-align`, and
//! `line-height` leading distribution.
//!
//! # The Phase 3 subset (each deviation has a recorded home)
//!
//! - **Whitespace**: `white-space: normal` semantics only (the working
//!   property set has no `white-space` yet): runs of space/tab/newline
//!   collapse to one space; collapsible spaces at line-box edges vanish
//!   (start-of-line, after a forced break, and trailing at end-of-line).
//! - **Line breaking**: opportunities at collapsible spaces only. UAX#14
//!   (CSS Text full line breaking) is the Phase 7 work item per
//!   rendering-pipeline.md §2; CJK therefore does not break mid-run, and
//!   a word wider than the line overflows rather than breaking.
//! - **Alignment**: `start`/`left`/`end`/`right`/`center`; `justify`
//!   behaves as `start` (recorded until the CSS Text batch).
//! - **Vertical alignment**: every inline box is baseline-aligned
//!   (CSS 2.1 §10.8.1 with half-leading distribution); the
//!   `vertical-align` property is not in the working set. Atomic
//!   inline-level boxes align by their last line box's baseline, or by
//!   their bottom border edge when they have no line boxes (CSS 2.1
//!   §10.8.1 inline-block baseline rule); an atomic's contribution to the
//!   line metrics is its baseline-to-margin-box extent (vertical margins
//!   ignored — the recorded simplification).
//! - **Inline box decoration**: non-atomic inline elements contribute no
//!   box-decoration space this batch (see box_tree.rs); their text
//!   inherits their style, which is what the fragments carry.
//! - **Bidi**: single logical run per line in document order
//!   (unicode-bidi is the veyra-text batch's work item).

use veyra_dom::NodeId;
use veyra_style::{ComputedValues, LineHeight, TextAlign};

use crate::box_tree::{Box, InlineItem};
use crate::error::LayoutError;
use crate::fragment::{Fragment, TextFragment};
use crate::geometry::PhysicalRect;
use crate::limits::Budget;
use crate::text::{FontQuery, TextMeasurer};

/// Whether a character is collapsible whitespace under
/// `white-space: normal` (CSS Text 3 §3: spaces, tabs, newlines, carriage
/// returns, form feeds).
fn is_collapsible_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

/// A pre-layout inline token (shared by line building and intrinsic
/// sizing).
pub(crate) enum PreToken<'a> {
    /// A run of non-whitespace text with its owning style context.
    Word {
        /// The source text node.
        node: NodeId,
        /// The word's text.
        text: String,
        /// The style context the word inherits from.
        style: &'a ComputedValues,
        /// The word's measured width (px).
        width: f32,
    },
    /// One collapsed space (advance only; never a fragment).
    Space {
        /// The space's measured width (px).
        width: f32,
    },
    /// An atomic inline-level box (laid out on demand).
    Atomic(&'a Box),
    /// `<br>`: a forced line break.
    Break,
}

/// The collapsible-whitespace scanner state (crossing item boundaries —
/// collapsing is a property of the whole inline formatting context, not
/// of one text node).
struct Scanner {
    at_line_start: bool,
    prev_was_space: bool,
}

impl Scanner {
    /// Scanner state at the start of an inline formatting context.
    fn new() -> Self {
        Scanner {
            at_line_start: true,
            prev_was_space: false,
        }
    }
}

/// Tokenizes the inline items of one inline formatting context in
/// document order, applying CSS Text §3 whitespace collapsing across item
/// boundaries (single space between items, none at content start or after
/// a forced break).
pub(crate) fn tokenize<'a>(
    measurer: &dyn TextMeasurer,
    items: &'a [InlineItem],
    budget: &mut Budget,
) -> Result<Vec<PreToken<'a>>, LayoutError> {
    let mut tokens = Vec::new();
    let mut scanner = Scanner::new();

    for item in items {
        match item {
            InlineItem::Text { node, text, style } => {
                let query = FontQuery::from_computed(style);
                let mut word = String::new();
                for c in text.chars() {
                    if is_collapsible_whitespace(c) {
                        if !word.is_empty() {
                            let word = std::mem::take(&mut word);
                            emit_word(measurer, &mut tokens, budget, *node, word, style, &query)?;
                            // Content seen on this line.
                            scanner.at_line_start = false;
                            scanner.prev_was_space = false;
                        }
                        // A space only survives when it separates content
                        // on the same line (not at line start, not after
                        // another collapsed space).
                        if !scanner.at_line_start && !scanner.prev_was_space {
                            budget.charge()?;
                            tokens.push(PreToken::Space {
                                width: measurer.measure(" ", &query),
                            });
                            scanner.prev_was_space = true;
                        }
                    } else {
                        word.push(c);
                    }
                }
                if !word.is_empty() {
                    emit_word(measurer, &mut tokens, budget, *node, word, style, &query)?;
                    scanner.at_line_start = false;
                    scanner.prev_was_space = false;
                }
            }
            InlineItem::Atomic(box_) => {
                budget.charge()?;
                tokens.push(PreToken::Atomic(box_));
                scanner.at_line_start = false;
                scanner.prev_was_space = false;
            }
            InlineItem::ForcedBreak => {
                budget.charge()?;
                tokens.push(PreToken::Break);
                scanner.at_line_start = true;
                scanner.prev_was_space = false;
            }
        }
    }
    Ok(tokens)
}

/// Emits a word token with its measured width (charges the budget).
fn emit_word<'a>(
    measurer: &dyn TextMeasurer,
    tokens: &mut Vec<PreToken<'a>>,
    budget: &mut Budget,
    node: NodeId,
    text: String,
    style: &'a ComputedValues,
    query: &FontQuery,
) -> Result<(), LayoutError> {
    budget.charge()?;
    let width = measurer.measure(&text, query);
    tokens.push(PreToken::Word {
        node,
        text,
        style,
        width,
    });
    Ok(())
}

/// One positioned inline item on a line (x relative to the line's
/// content start, before text-align offsetting).
struct LineItem {
    /// The item's x within the line (advance cursor at placement).
    x: f32,
    /// The item's advance (word width, or atomic margin + border box).
    advance: f32,
    kind: LineItemKind,
}

enum LineItemKind {
    /// A word: text, node provenance, and its font metrics (for the
    /// fragment's content-area rect).
    Word {
        node: NodeId,
        text: String,
        font_ascent: f32,
        font_descent: f32,
    },
    /// An atomic inline-level box with its baseline (from its border-box
    /// top), laid out.
    Atomic(Fragment, Option<f32>),
}

/// One line box under construction.
struct LineBuilder {
    items: Vec<LineItem>,
    /// The advance cursor (includes pending spaces).
    x: f32,
    /// The width excluding trailing spaces (the aligned extent).
    committed: f32,
    /// Line ascent above the baseline (strut and item max, half-leading
    /// included).
    ascent: f32,
    /// Line descent below the baseline.
    descent: f32,
}

impl LineBuilder {
    /// A fresh line box: only the strut contributes metrics.
    fn empty(strut_ascent: f32, strut_descent: f32) -> Self {
        LineBuilder {
            items: Vec::new(),
            x: 0.0,
            committed: 0.0,
            ascent: strut_ascent,
            descent: strut_descent,
        }
    }

    fn has_content(&self) -> bool {
        !self.items.is_empty()
    }

    /// Grows the line metrics for one inline box's style (CSS 2.1
    /// §10.8.1: leading = line-height − (A + D), split half above/below).
    fn grow_metrics(&mut self, style: &ComputedValues, measurer: &dyn TextMeasurer) {
        let query = FontQuery::from_computed(style);
        let metrics = measurer.line_metrics(&query);
        let line_height = match style.line_height {
            LineHeight::Normal => metrics.normal_line_height(),
            LineHeight::Number(n) => n * style.font_size,
            LineHeight::Length(px) => px,
        };
        let leading = line_height - (metrics.ascent + metrics.descent);
        self.ascent = self.ascent.max(metrics.ascent + leading / 2.0);
        self.descent = self.descent.max(metrics.descent + leading / 2.0);
    }

    /// Grows the line metrics for an atomic inline box (its baseline-to-
    /// border-box extents; vertical margins ignored — recorded).
    fn grow_atomic_metrics(&mut self, border_height: f32, baseline: Option<f32>) {
        match baseline {
            Some(baseline) => {
                self.ascent = self.ascent.max(baseline);
                self.descent = self.descent.max(border_height - baseline);
            }
            // No line box inside: the bottom border edge sits on the
            // baseline (CSS 2.1 §10.8.1).
            None => {
                self.ascent = self.ascent.max(border_height);
            }
        }
    }
}

/// The inline formatting result: fragments positioned relative to the
/// owning box's *content* origin, plus the metrics the block layer needs.
pub(crate) struct InlineFlow {
    /// The line fragments (text runs and atomic box fragments).
    pub fragments: Vec<Fragment>,
    /// Total content height (the stack of line boxes).
    pub content_height: f32,
    /// Whether any line box was produced (drives margin-collapse
    /// throughness).
    pub has_line_boxes: bool,
    /// The last line's baseline y from the content top.
    pub last_baseline: Option<f32>,
}

/// Lays out one inline formatting context.
pub(crate) fn layout_inline_flow(
    measurer: &dyn TextMeasurer,
    items: &[InlineItem],
    content_width: f32,
    strut_style: &ComputedValues,
    block_definite: Option<f32>,
    budget: &mut Budget,
) -> Result<InlineFlow, LayoutError> {
    let tokens = tokenize(measurer, items, budget)?;

    // The strut: the block's own font/line-height starts every line box's
    // metric max (CSS 2.1 §10.8.1).
    let strut_query = FontQuery::from_computed(strut_style);
    let strut_metrics = measurer.line_metrics(&strut_query);
    let strut_line_height = match strut_style.line_height {
        LineHeight::Normal => strut_metrics.normal_line_height(),
        LineHeight::Number(n) => n * strut_style.font_size,
        LineHeight::Length(px) => px,
    };
    let strut_leading = strut_line_height - (strut_metrics.ascent + strut_metrics.descent);
    let strut_ascent = strut_metrics.ascent + strut_leading / 2.0;
    let strut_descent = strut_metrics.descent + strut_leading / 2.0;

    let mut fragments: Vec<Fragment> = Vec::new();
    let mut line = LineBuilder::empty(strut_ascent, strut_descent);
    let mut y = 0.0;
    let mut last_baseline: Option<f32> = None;
    let mut line_boxes = 0usize;

    // Flushes the current line box: emits its fragments (aligned per the
    // strut's text-align) and advances y. `force` emits a strut-only line
    // when the line has no content (an empty line between two forced
    // breaks).
    macro_rules! flush_line {
        ($force:expr) => {
            if $force || line.has_content() {
                let line_width = line.committed;
                let offset = align_offset(strut_style.text_align, content_width, line_width);
                let baseline_y = y + line.ascent;
                last_baseline = Some(baseline_y);
                line_boxes += 1;

                for item in std::mem::take(&mut line.items) {
                    let x = offset + item.x;
                    match item.kind {
                        LineItemKind::Word {
                            node,
                            text,
                            font_ascent,
                            font_descent,
                        } => {
                            budget.charge()?;
                            fragments.push(Fragment::Text(TextFragment {
                                node,
                                text,
                                // The font content area (ascent..descent),
                                // not the leading-adjusted line box.
                                rect: PhysicalRect::new(
                                    x,
                                    baseline_y - font_ascent,
                                    item.advance,
                                    font_ascent + font_descent,
                                ),
                            }));
                        }
                        LineItemKind::Atomic(mut fragment, baseline) => {
                            // Baseline alignment: last line's baseline, or
                            // the bottom border edge without one.
                            let height = match &fragment {
                                Fragment::Box(box_) => box_.border.height,
                                Fragment::Text(text) => text.rect.height,
                            };
                            let dy = match baseline {
                                Some(baseline) => baseline_y - baseline,
                                None => baseline_y - height,
                            };
                            fragment.translate(x, dy);
                            fragments.push(fragment);
                        }
                    }
                }
                y += line.ascent + line.descent;
                line = LineBuilder::empty(strut_ascent, strut_descent);
            }
        };
    }

    for token in tokens {
        match token {
            PreToken::Word {
                node,
                text,
                style,
                width,
            } => {
                // Break before the word when it does not fit on a
                // non-empty line (words never split — the overflow case
                // is a recorded deviation). Metrics grow only for the
                // line the word actually lands on.
                if line.has_content() && line.x + width > content_width {
                    flush_line!(false);
                }
                line.grow_metrics(style, measurer);
                let query = FontQuery::from_computed(style);
                let metrics = measurer.line_metrics(&query);
                line.items.push(LineItem {
                    x: line.x,
                    advance: width,
                    kind: LineItemKind::Word {
                        node,
                        text,
                        font_ascent: metrics.ascent,
                        font_descent: metrics.descent,
                    },
                });
                line.x += width;
                line.committed = line.x;
            }
            PreToken::Space { width } => {
                // Advance only: a space becomes real (committed) when
                // content follows on the same line, else it is trailing
                // and vanishes at the line edge.
                line.x += width;
            }
            PreToken::Atomic(box_) => {
                // Shrink-to-fit width (CSS 2.1 §10.3.9), then lay the
                // atomic's content out at that width.
                let placed = crate::intrinsic::layout_atomic_on_line(
                    measurer,
                    box_,
                    content_width,
                    block_definite,
                    budget,
                )?;
                let advance = placed.margin_left + placed.border_box_width;
                if line.has_content() && line.x + advance > content_width {
                    flush_line!(false);
                }
                line.grow_atomic_metrics(placed.border_box_height, placed.baseline_from_border_top);
                line.items.push(LineItem {
                    x: line.x + placed.margin_left,
                    advance,
                    kind: LineItemKind::Atomic(placed.fragment, placed.baseline_from_border_top),
                });
                line.x += advance;
                line.committed = line.x;
            }
            PreToken::Break => {
                // A forced break always ends the line box (a break after
                // a break yields the strut-only empty line).
                flush_line!(true);
            }
        }
    }
    // A trailing forced break does not produce an empty last line box;
    // unwrapped content flushes once.
    if line.has_content() {
        flush_line!(false);
    }
    let _ = &line; // the macro's post-flush reset is intentionally dead here

    Ok(InlineFlow {
        fragments,
        content_height: y,
        has_line_boxes: line_boxes > 0,
        last_baseline,
    })
}

/// The text-align offset for a line (CSS 2.1 §16.2; `justify` behaves as
/// `start` — the recorded deviation).
fn align_offset(align: TextAlign, content_width: f32, line_width: f32) -> f32 {
    match align {
        TextAlign::Start | TextAlign::Left | TextAlign::Justify => 0.0,
        TextAlign::End | TextAlign::Right => content_width - line_width,
        TextAlign::Center => (content_width - line_width) / 2.0,
    }
}

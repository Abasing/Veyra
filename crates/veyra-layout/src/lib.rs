//! `veyra-layout` — Veyra's layout engine: the box tree, the block and
//! inline formatting contexts, intrinsic sizing, and the fragment tree.
//!
//! Batch 4 (roadmap Phase 3): computed styles in, positioned fragments
//! out. [`layout_document`] consumes a `veyra-dom` tree with its
//! [`veyra_style::ComputedStyleMap`] and a viewport, and produces the
//! [`LayoutResult`] fragment tree the paint batch renders and the
//! deterministic [`LayoutResult::to_dump_string`] report prints. ADR-0003
//! fixes the architecture (custom block+inline implementation, display
//! lists later); rendering-pipeline.md §3 fixes the strategy (intrinsic
//! sizing as a layout *mode*, R3 logical axes); this crate implements the
//! documented subset with every deviation recorded in the module docs.
//!
//! # What is implemented (the Phase 3 batch-4 scope)
//!
//! - **Box tree** (`box_tree`): display filtering (`none` subtrees
//!   vanish), block/inline/inline-block classification, anonymous block
//!   boxes for mixed content (CSS 2.1 §9.2.1.1, block-in-inline hoisting
//!   included), `<br>` as a forced break.
//! - **Block formatting** (`block`): CSS 2.1 §10.3.3 width resolution
//!   (auto fill, auto-margin centering, over-constrained ltr rule),
//!   §8.3.1 margin collapsing (siblings, parent/child, through-collapsing
//!   boxes, negative margins — associative pair-form merges), §10.6
//!   heights with definite-percentage resolution.
//! - **Inline formatting** (`inline`): CSS Text §3 whitespace
//!   collapsing, line breaking at spaces, strut-correct line boxes
//!   (§10.8.1 half-leading), `text-align`, inline-block shrink-to-fit
//!   placement and baseline alignment.
//! - **Intrinsic sizing** (`intrinsic`): min-content/max-content as
//!   layout modes (css-sizing §5), feeding shrink-to-fit.
//! - **Fragments** (the `fragment` module): absolute page-coordinate rects for
//!   paint; the dump format is deterministic byte-for-byte.
//!
//! # The measurement seam
//!
//! Text measurement goes through [`TextMeasurer`]. The production
//! provider is `veyra-text` (Linebender stack, its own ADR'd batch — the
//! dependency-graph `LAYOUT --> TEXT` edge); today
//! [`FixedMetrics`] provides the fully specified deterministic
//! approximation (module docs carry the exact arithmetic) the shell and
//! tests use.
//!
//! # Security posture
//!
//! - Layout never panics on any DOM/style combination (engineering
//!   standards §2); the DOM's own limits (node count, 512 depth) bound
//!   recursion, and traversals stay iterative or depth-bounded.
//! - The limits table's layout row ([`LayoutLimits`], default
//!   [`MAX_BOXES`]) bounds boxes + inline items + tokens + fragments per
//!   run — unbounded allocation is a memory-amplification vector
//!   (engineering standards §6); exhaustion is a typed
//!   [`LayoutError::LimitExceeded`], never a crash.
//!
//! # Examples
//!
//! ```
//! use veyra_layout::{layout_document, FixedMetrics, LayoutLimits};
//! use veyra_style::{compute_styles, StyleSet, Stylesheet, Limits as CssLimits};
//! use veyra_url::Url;
//!
//! // A small document: parse, style, lay out at 800x600.
//! let html = b"<html><body><p>Hello layout</p></body></html>";
//! let tree = veyra_html::parse_document(
//!     html,
//!     Url::parse("https://example.org/").expect("url").origin(),
//! )
//! .expect("parse");
//! let styles = compute_styles(&tree, &StyleSet::new());
//! let result = layout_document(
//!     &tree,
//!     &styles,
//!     (800, 600),
//!     &FixedMetrics,
//!     LayoutLimits::default(),
//! )
//! .expect("layout");
//!
//! // The report is deterministic (fixed-metrics arithmetic).
//! let dump = result.to_dump_string();
//! assert!(dump.starts_with("layout v1 viewport=800x600\n"));
//! assert!(dump.contains("box html"));
//! assert!(dump.contains("box body"));
//! assert!(dump.contains("text \"Hello\""));
//! ```
//!
#![forbid(unsafe_code)]

mod block;
mod box_tree;
mod error;
mod fragment;
mod geometry;
mod inline;
mod intrinsic;
mod limits;
mod text;

use veyra_dom::Tree;
use veyra_style::ComputedStyleMap;

pub use error::LayoutError;
pub use fragment::{BoxFragment, Fragment, LayoutResult, TextFragment};
pub use geometry::{LogicalOffset, LogicalSize, PhysicalRect, WritingMode};
pub use limits::{LayoutLimits, MAX_BOXES};
pub use text::{FixedMetrics, FontQuery, LineMetrics, TextMeasurer};

/// The budget shared across box-tree construction and layout (the
/// limits-table row bounds the *whole run*, both phases together).
use limits::Budget;

/// Lays out a document: DOM + computed styles + viewport → fragments.
///
/// The four steps run in one call: box-tree construction (display
/// filtering, anonymous blocks), block formatting (widths, margin
/// collapsing, heights), inline formatting (whitespace, line breaking,
/// alignment), and fragment assembly in page coordinates. The root
/// fragment is positioned inside the initial containing block by the
/// root's used margins.
///
/// # Errors
///
/// [`LayoutError::LimitExceeded`] when the run exceeds
/// [`LayoutLimits::max_boxes`] (the engineering-standards limits row).
/// No partial fragments escape a failed run.
pub fn layout_document(
    tree: &Tree,
    styles: &ComputedStyleMap,
    viewport: (u32, u32),
    measurer: &dyn TextMeasurer,
    limits: LayoutLimits,
) -> Result<LayoutResult, LayoutError> {
    let mut budget = Budget::new(limits);
    let built = box_tree::build(tree, styles, &mut budget)?;

    // The initial containing block: the viewport. Its height is definite
    // (percentage heights resolve against it at the root).
    let icb = block::ContainingBlock {
        inline: viewport.0 as f32,
        block_definite: Some(viewport.1 as f32),
    };
    let placed = block::layout_block_box(measurer, &built.root, &icb, true, &mut budget)?;

    // The root takes its own used margins inside the ICB (the root's
    // margins never collapse with anything).
    let (dx, dy) = (placed.margin_left, placed.margin_top);
    let mut root = placed.fragment;
    root.translate(dx, dy);

    let fragment_count = count_fragments(&root);
    Ok(LayoutResult {
        viewport,
        root,
        fragment_count,
    })
}

/// Counts fragments in a tree (iterative — the walk must not recurse on
/// a bounded-depth but wide fragment tree).
fn count_fragments(root: &Fragment) -> usize {
    let mut count = 0usize;
    let mut stack = vec![root];
    while let Some(fragment) = stack.pop() {
        count += 1;
        if let Fragment::Box(box_) = fragment {
            for child in &box_.children {
                stack.push(child);
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use veyra_dom::NodeData;
    use veyra_style::{StyleSet, compute_styles};
    use veyra_url::Url;

    /// Builds a styled document tree from HTML text (the UA sheet is the
    /// only stylesheet — inline `style` attributes exercise the cascade
    /// directly).
    fn document(html: &str) -> (Tree, ComputedStyleMap) {
        let url = Url::parse("https://example.org/").unwrap();
        let tree = veyra_html::parse_document(html.as_bytes(), url.origin()).unwrap();
        let styles = compute_styles(&tree, &StyleSet::new());
        (tree, styles)
    }

    fn layout(html: &str) -> LayoutResult {
        let (tree, styles) = document(html);
        layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap()
    }

    fn root_box(result: &LayoutResult) -> &BoxFragment {
        match &result.root {
            Fragment::Box(box_) => box_,
            Fragment::Text(_) => panic!("root is not a box"),
        }
    }

    fn find_box<'a>(fragment: &'a Fragment, name: &str) -> Option<&'a BoxFragment> {
        match fragment {
            Fragment::Box(box_) => {
                if box_.name == name {
                    return Some(box_);
                }
                box_.children.iter().find_map(|child| find_box(child, name))
            }
            Fragment::Text(_) => None,
        }
    }

    fn find_texts(fragment: &Fragment) -> Vec<&TextFragment> {
        let mut out = Vec::new();
        let mut stack = vec![fragment];
        while let Some(current) = stack.pop() {
            match current {
                Fragment::Box(box_) => {
                    for child in &box_.children {
                        stack.push(child);
                    }
                }
                Fragment::Text(text) => out.push(text),
            }
        }
        out.reverse();
        out
    }

    #[test]
    fn body_margin_collapses_with_first_paragraph() {
        // UA sheet: body { margin: 8px }, p { margin: 1em 0 }. body is
        // not separated, so p's 16px top margin collapses with body's
        // 8px: the collapsed 16 escapes above body, which sits at 16.
        let result = layout("<html><body><p>hi</p></body></html>");
        let body = find_box(&result.root, "body").unwrap();
        assert_eq!((body.border.x, body.border.y), (8.0, 16.0));
        assert_eq!(body.border.width, 784.0);
    }

    #[test]
    fn paragraph_margins_collapse_with_siblings() {
        // Two p's (1em margin each at 16px font): the gap between them
        // collapses to max(16, 16) = 16, not 32; the first p's top margin
        // collapses out of body (which sits at 16 from the collapse with
        // body's own 8px margin).
        let result = layout("<html><body><p>a</p><p>b</p></body></html>");
        let body = find_box(&result.root, "body").unwrap();
        let paragraphs: Vec<&BoxFragment> = body
            .children
            .iter()
            .filter_map(|f| match f {
                Fragment::Box(b) if b.name == "p" => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(paragraphs.len(), 2);
        // First p sits at body's content top (its margin escaped).
        assert_eq!(paragraphs[0].border.y, 16.0);
        // Second p: first p's bottom edge + collapsed gap 16.
        let first_height = paragraphs[0].border.height;
        assert_eq!(paragraphs[1].border.y, 16.0 + first_height + 16.0);
    }

    #[test]
    fn text_runs_collapse_whitespace_and_align_start() {
        let result = layout("<html><body><p>  Hello   world  </p></body></html>");
        let texts = find_texts(&result.root);
        // Leading/trailing whitespace vanished; the middle run stayed.
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].text, "Hello");
        assert_eq!(texts[1].text, "world");
        // FixedMetrics: 10px per char at 16px font. "Hello" = 50px; one
        // collapsed space (10px) between the words; p content starts at
        // body's 8px margin.
        assert_eq!(texts[0].rect.width, 50.0);
        assert_eq!(texts[0].rect.x, 8.0);
        assert_eq!(texts[1].rect.x, 68.0);
    }

    #[test]
    fn line_breaking_wraps_at_spaces() {
        // Body content 60px wide: "aaaa" (40px) + space (10px) leaves
        // 10px — "bbbb" breaks; same for the next line. One word per
        // line, three lines, 16px line height (normal = 1em at 16px).
        let result =
            layout("<html><body style=\"width: 60px\"><p>aaaa bbbb cccc</p></body></html>");
        let texts = find_texts(&result.root);
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[1].rect.y, texts[0].rect.y + 16.0);
        assert_eq!(texts[2].rect.y, texts[1].rect.y + 16.0);
    }

    #[test]
    fn display_none_produces_nothing() {
        let result = layout(
            "<html><body><p style=\"display: none\">hidden</p><div>shown</div></body></html>",
        );
        let dump = result.to_dump_string();
        assert!(
            !dump.contains("hidden"),
            "display:none subtree vanished: {dump}"
        );
        assert!(dump.contains("shown"));
    }

    #[test]
    fn anonymous_block_splits_mixed_content() {
        // text <p>block</p> text → anonymous block + p + anonymous block.
        let result = layout("<html><body>before<p>block</p>after</body></html>");
        let body = find_box(&result.root, "body").unwrap();
        let child_names: Vec<&str> = body
            .children
            .iter()
            .map(|f| match f {
                Fragment::Box(b) => {
                    if b.anonymous {
                        "(anonymous)"
                    } else {
                        &b.name
                    }
                }
                Fragment::Text(_) => "(text)",
            })
            .collect();
        assert_eq!(child_names, vec!["(anonymous)", "p", "(anonymous)"]);
    }

    #[test]
    fn forced_break_creates_strut_only_lines() {
        let result = layout("<html><body><p>a<br><br>b</p></body></html>");
        let texts = find_texts(&result.root);
        assert_eq!(texts.len(), 2);
        // Lines: "a" (content top 16), strut-only line, "b" (16 + 32).
        // The p sits at 16 (its top margin collapsed out of body).
        assert_eq!(texts[0].rect.y, 16.0);
        assert_eq!(texts[1].rect.y, 16.0 + 32.0);
    }

    #[test]
    fn text_align_center_offsets_the_line() {
        let result = layout("<html><body style=\"text-align: center\"><p>ab</p></body></html>");
        let texts = find_texts(&result.root);
        // Word width 2 × 10 = 20; body content 784 → offset (784 −
        // 20)/2 = 382; body content starts at 8 (its margin) → x = 390.
        assert_eq!(texts[0].rect.x, 390.0);
    }

    #[test]
    fn inline_block_shrinks_to_fit_and_carries_baseline() {
        let result = layout(
            "<html><body><p>before <span style=\"display: inline-block\">x y</span> after</p></body></html>",
        );
        let inline_block = find_box(&result.root, "span").unwrap();
        // Shrink-to-fit: max-content = "x y" = 3 chars × 10px = 30 (no
        // extras) < 784 available → used width 30.
        assert_eq!(inline_block.border.width, 30.0);
        // One line inside: content height 16 → border box height 16.
        assert_eq!(inline_block.border.height, 16.0);
    }

    #[test]
    fn deep_tree_does_not_overflow_the_stack() {
        // The full DOM depth cap (512) of nested divs: layout must
        // complete without stack recursion blowing the test thread's
        // 2 MiB stack (the box-tree build is an explicit-stack machine;
        // the layout pass returns boxed fragments to keep frames small).
        let mut html = String::from("<html><body>");
        for _ in 0..500 {
            html.push_str("<div>");
        }
        html.push_str("leaf");
        for _ in 0..500 {
            html.push_str("</div>");
        }
        html.push_str("</body></html>");
        let (tree, styles) = document(&html);
        let result = layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap();
        assert!(
            result.fragment_count > 500,
            "fragment count {}",
            result.fragment_count
        );
        assert!(result.to_dump_string().contains("leaf"));
    }

    #[test]
    fn empty_document_lays_out_blank() {
        let url = Url::parse("https://example.org/").unwrap();
        let tree = Tree::new_document(url.origin()).unwrap();
        let styles = compute_styles(&tree, &StyleSet::new());
        let result = layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap();
        let root = root_box(&result);
        assert_eq!(root.border.width, 800.0);
        assert_eq!(root.border.height, 0.0);
    }

    #[test]
    fn fragment_count_matches_the_dump_lines() {
        let (tree, styles) = document("<html><body><p>one two</p><div>x</div></body></html>");
        let result = layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap();
        let dump = result.to_dump_string();
        let dump_fragments = dump.lines().count() - 1; // minus the header
        assert_eq!(result.fragment_count, dump_fragments);
        // Every element with boxes appears: html, body, anonymous? (no),
        // p, div, plus two text runs.
        let _ = find_texts(&result.root);
    }

    #[test]
    fn dump_is_byte_identical_across_runs() {
        let (tree, styles) =
            document("<html><body><h1>Title</h1><p>Some  text here</p></body></html>");
        let first = layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap()
        .to_dump_string();
        let second = layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap()
        .to_dump_string();
        assert_eq!(first, second);
        // Text fragments are per-word runs (one unbreakable unit each).
        assert!(first.contains("text \"Title\""));
        assert!(first.contains("text \"Some\""));
        assert!(first.contains("text \"text\""));
        assert!(first.contains("text \"here\""));
    }

    #[test]
    fn head_elements_produce_no_boxes() {
        let result = layout(
            "<html><head><title>t</title><style>p { color: red }</style></head><body><p>x</p></body></html>",
        );
        let dump = result.to_dump_string();
        assert!(!dump.contains("head"));
        assert!(!dump.contains("title"));
        assert!(dump.contains("p"));
    }

    // The tree walker needs NodeData for one assertion; keep the import
    // scoped to this test.
    #[test]
    fn text_fragments_carry_their_node() {
        let (tree, styles) = document("<html><body><p>node</p></body></html>");
        let result = layout_document(
            &tree,
            &styles,
            (800, 600),
            &FixedMetrics,
            LayoutLimits::default(),
        )
        .unwrap();
        let texts = find_texts(&result.root);
        assert_eq!(texts.len(), 1);
        let text_node = texts[0].node;
        match tree.data(text_node) {
            Some(NodeData::Text { data }) => assert_eq!(data, "node"),
            other => panic!("expected text node, got {other:?}"),
        }
    }
}

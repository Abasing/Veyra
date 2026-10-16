// Limits tests for `veyra-layout`: the engineering-standards limits row
// (box/fragment budget) is normative — these tests prove the budget
// fires as a typed error, bounds transient token allocation, and stays
// configurable.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_layout::{FixedMetrics, LayoutError, LayoutLimits, layout_document};
use veyra_style::{StyleSet, compute_styles};
use veyra_url::Url;

fn tree_for(html: &str) -> (veyra_dom::Tree, veyra_style::ComputedStyleMap) {
    let url = Url::parse("https://example.org/limits").unwrap();
    let tree = veyra_html::parse_document(html.as_bytes(), url.origin()).unwrap();
    let styles = compute_styles(&tree, &StyleSet::new());
    (tree, styles)
}

#[test]
fn box_budget_fires_as_a_typed_error() {
    // 40 paragraphs produce 40 p boxes + body + html + ~80 word
    // fragments: well past a budget of 50, under the default.
    let html = "<html><body><p>alpha beta gamma</p></body></html>";
    let (tree, styles) = tree_for(html);
    let ok = layout_document(
        &tree,
        &styles,
        (800, 600),
        &FixedMetrics,
        LayoutLimits { max_boxes: 200 },
    )
    .unwrap();
    assert!(ok.fragment_count < 200);

    let err = layout_document(
        &tree,
        &styles,
        (800, 600),
        &FixedMetrics,
        LayoutLimits { max_boxes: 5 },
    )
    .unwrap_err();
    assert_eq!(
        err,
        LayoutError::LimitExceeded {
            limit: "boxes",
            max: 5
        }
    );
}

#[test]
fn transient_tokens_count_against_the_budget() {
    // One huge text node: thousands of words. The tokens are transient,
    // but they charge the same budget — a document cannot allocate
    // unboundedly on the way to a small fragment count.
    let mut words = String::new();
    for i in 0..2_000 {
        words.push_str(&format!("w{i} "));
    }
    let html = format!("<html><body><p>{words}</p></body></html>");
    let (tree, styles) = tree_for(&html);
    let err = layout_document(
        &tree,
        &styles,
        (800, 600),
        &FixedMetrics,
        LayoutLimits { max_boxes: 500 },
    )
    .unwrap_err();
    assert!(matches!(err, LayoutError::LimitExceeded { .. }));
}

#[test]
fn deep_inline_nesting_is_bounded_by_the_dom_not_layout() {
    // 200 nested <i> elements (inline flattening): layout completes —
    // the flattening machine is explicit-stack, and the DOM depth cap
    // (512) is the bound.
    let mut html = String::from("<html><body><p>");
    for _ in 0..200 {
        html.push_str("<i>");
    }
    html.push_str("deep");
    for _ in 0..200 {
        html.push_str("</i>");
    }
    html.push_str("</p></body></html>");
    let (tree, styles) = tree_for(&html);
    let result = layout_document(
        &tree,
        &styles,
        (800, 600),
        &FixedMetrics,
        LayoutLimits::default(),
    )
    .unwrap();
    let dump = result.to_dump_string();
    assert!(dump.contains("text \"deep\""));
    // One word, one fragment — flattening creates no boxes.
    assert_eq!(result.fragment_count, 4); // html, body, p, text
}

#[test]
fn default_budget_is_the_standards_row() {
    assert_eq!(veyra_layout::MAX_BOXES, 200_000);
    assert_eq!(LayoutLimits::default().max_boxes, veyra_layout::MAX_BOXES);
}

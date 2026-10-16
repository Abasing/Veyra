// End-to-end paint scenarios for the display-list batch: parse → style
// → layout → display list → dump, with full golden dump strings pinned
// byte for byte (FixedMetrics makes the geometry exact; the colors come
// from the author sheets in each fixture). These are the acceptance
// fixtures the matrix row's "display-list snapshot tests" point at; the
// shell's `--paint` report is the same format.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_layout::{FixedMetrics, LayoutLimits, layout_document};
use veyra_paint::{PaintLimits, build_display_list};
use veyra_style::{Limits as CssLimits, StyleSet, Stylesheet, compute_styles};
use veyra_url::Url;

fn paint_with_author_css(html: &str, css: &str) -> String {
    let url = Url::parse("https://example.org/doc").unwrap();
    let tree = veyra_html::parse_document(html.as_bytes(), url.origin()).unwrap();
    let mut styles = StyleSet::new();
    if !css.is_empty() {
        let sheet = Stylesheet::parse(css, &CssLimits::default()).unwrap();
        styles.append_author(sheet, None);
    }
    let computed = compute_styles(&tree, &styles);
    let result = layout_document(
        &tree,
        &computed,
        (800, 600),
        &FixedMetrics,
        LayoutLimits::default(),
    )
    .unwrap();
    build_display_list(&result, PaintLimits::default())
        .unwrap()
        .to_dump_string()
}

fn paint(html: &str) -> String {
    paint_with_author_css(html, "")
}

#[test]
fn golden_uncolored_document_is_text_only() {
    // No author colors: the UA sheet paints no backgrounds or borders,
    // so the display list is exactly the text runs (tree order).
    let dump = paint("<!DOCTYPE html><html><body><p>Hello veyra</p></body></html>");
    let expected = "\
paint v1 viewport=800x600
text \"Hello\" [8.00,16.00,50.00x16.00] #000000FF
text \"veyra\" [68.00,16.00,50.00x16.00] #000000FF
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_colored_background_and_border() {
    // body background paints first (root-subtree order), the p's border
    // strips hug its border box, and its own background (none) emits
    // nothing.
    let dump = paint_with_author_css(
        "<html><body><p>one</p><p>two</p></body></html>",
        "body { background: #102030 } p { border: 2px solid red; }",
    );
    let expected = "\
paint v1 viewport=800x600
rect [8.00,16.00,784.00x56.00] #102030FF
border [8.00,16.00,784.00x20.00] top=2.00 #FF0000FF right=2.00 #FF0000FF bottom=2.00 #FF0000FF left=2.00 #FF0000FF
text \"one\" [10.00,18.00,30.00x16.00] #000000FF
border [8.00,52.00,784.00x20.00] top=2.00 #FF0000FF right=2.00 #FF0000FF bottom=2.00 #FF0000FF left=2.00 #FF0000FF
text \"two\" [10.00,54.00,30.00x16.00] #000000FF
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_text_color_and_inheritance() {
    // The computed text color flows from the cascade into the display
    // list (color on body inherits to the p).
    let dump = paint_with_author_css(
        "<html><body><p>styled words</p></body></html>",
        "body { color: #336699 }",
    );
    let expected = "\
paint v1 viewport=800x600
text \"styled\" [8.00,16.00,60.00x16.00] #336699FF
text \"words\" [78.00,16.00,50.00x16.00] #336699FF
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_deep_nesting_paint_order() {
    // Three nested colored boxes: outer background, middle background,
    // inner background, then the text inside the innermost — each box's
    // own paint strictly precedes its descendants'.
    let dump = paint_with_author_css(
        "<html><body><div><div><div>deep</div></div></div></body></html>",
        "div { background: lime } body > div { background: navy } html > body > div > div { background: maroon }",
    );
    let expected = "\
paint v1 viewport=800x600
rect [8.00,8.00,784.00x16.00] #000080FF
rect [8.00,8.00,784.00x16.00] #800000FF
rect [8.00,8.00,784.00x16.00] #00FF00FF
text \"deep\" [8.00,8.00,40.00x16.00] #000000FF
";
    assert_eq!(dump, expected);
}

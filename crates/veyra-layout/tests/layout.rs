// End-to-end layout scenarios for `veyra-layout` batch 4: parse → style
// → layout → dump, with full golden dump strings pinned byte for byte
// (the FixedMetrics contract makes them exact). These are the acceptance
// fixtures the matrix row's "deterministic fragment-tree dumps" point at;
// the shell's `--layout` report is the same format.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_layout::{FixedMetrics, LayoutLimits, layout_document};
use veyra_style::{StyleSet, compute_styles};
use veyra_url::Url;

fn layout(html: &str) -> String {
    let url = Url::parse("https://example.org/doc").unwrap();
    let tree = veyra_html::parse_document(html.as_bytes(), url.origin()).unwrap();
    let styles = compute_styles(&tree, &StyleSet::new());
    layout_document(
        &tree,
        &styles,
        (800, 600),
        &FixedMetrics,
        LayoutLimits::default(),
    )
    .unwrap()
    .to_dump_string()
}

#[test]
fn golden_simple_document() {
    let dump = layout("<!DOCTYPE html><html><body><p>Hello veyra</p></body></html>");
    let expected = "\
layout v1 viewport=800x600
box html content=[0.00,0.00 800.00x48.00] padding=[0.00,0.00 800.00x48.00] border=[0.00,0.00 800.00x48.00]
  box body content=[8.00,16.00 784.00x16.00] padding=[8.00,16.00 784.00x16.00] border=[8.00,16.00 784.00x16.00]
    box p content=[8.00,16.00 784.00x16.00] padding=[8.00,16.00 784.00x16.00] border=[8.00,16.00 784.00x16.00]
      text \"Hello\" [8.00,16.00 50.00x16.00]
      text \"veyra\" [68.00,16.00 50.00x16.00]
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_nested_blocks_with_collapsing_margins() {
    // Two sections; the UA sheet gives div { display: block } with zero
    // margins; the paragraphs inside collapse their 16px margins with
    // each other and out of the divs (no borders/padding anywhere).
    let dump =
        layout("<html><body><div><p>one</p><p>two</p></div><div><p>three</p></div></body></html>");
    // Everything collapses: p margins (16px) collapse with siblings,
    // through the margin-less divs, and with body's 8px margin (max
    // wins) — so body content starts at 16 and every box below sits
    // flush with 16px gaps between paragraphs.
    let expected = "\
layout v1 viewport=800x600
box html content=[0.00,0.00 800.00x112.00] padding=[0.00,0.00 800.00x112.00] border=[0.00,0.00 800.00x112.00]
  box body content=[8.00,16.00 784.00x80.00] padding=[8.00,16.00 784.00x80.00] border=[8.00,16.00 784.00x80.00]
    box div content=[8.00,16.00 784.00x48.00] padding=[8.00,16.00 784.00x48.00] border=[8.00,16.00 784.00x48.00]
      box p content=[8.00,16.00 784.00x16.00] padding=[8.00,16.00 784.00x16.00] border=[8.00,16.00 784.00x16.00]
        text \"one\" [8.00,16.00 30.00x16.00]
      box p content=[8.00,48.00 784.00x16.00] padding=[8.00,48.00 784.00x16.00] border=[8.00,48.00 784.00x16.00]
        text \"two\" [8.00,48.00 30.00x16.00]
    box div content=[8.00,80.00 784.00x16.00] padding=[8.00,80.00 784.00x16.00] border=[8.00,80.00 784.00x16.00]
      box p content=[8.00,80.00 784.00x16.00] padding=[8.00,80.00 784.00x16.00] border=[8.00,80.00 784.00x16.00]
        text \"three\" [8.00,80.00 50.00x16.00]
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_wrapped_line_with_border_and_padding() {
    // A bordered, padded paragraph narrower than the viewport: the box
    // decorations show up as distinct extents; text wraps inside the
    // content box.
    let dump = layout(
        "<html><body><p style=\"width: 200px; border: 2px solid black; padding: 10px\">alpha beta</p></body></html>",
    );
    // A bordered, padded paragraph: the UA rule `p { margin: 1em 0 }`
    // sets the *horizontal* margins to 0 (not auto), so the box starts
    // at body's content left; the decorations show as distinct extents
    // (content ⊂ padding ⊂ border). p's 16px top margin collapses out
    // of body (border top at 16); border box = 224x40.
    let expected = "\
layout v1 viewport=800x600
box html content=[0.00,0.00 800.00x72.00] padding=[0.00,0.00 800.00x72.00] border=[0.00,0.00 800.00x72.00]
  box body content=[8.00,16.00 784.00x40.00] padding=[8.00,16.00 784.00x40.00] border=[8.00,16.00 784.00x40.00]
    box p content=[20.00,28.00 200.00x16.00] padding=[10.00,18.00 220.00x36.00] border=[8.00,16.00 224.00x40.00]
      text \"alpha\" [20.00,28.00 50.00x16.00]
      text \"beta\" [80.00,28.00 40.00x16.00]
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_centered_text_and_forced_breaks() {
    let dump = layout("<html><body style=\"text-align: center\"><p>ab<br>cd</p></body></html>");
    // Two centered lines: word width 20 → offset (784 − 20)/2 = 382;
    // p content starts at x=8 → 390. Line 1 y=16, line 2 y=32; the
    // escaped p/body margins give html a height of 16+32+16.
    let expected = "\
layout v1 viewport=800x600
box html content=[0.00,0.00 800.00x64.00] padding=[0.00,0.00 800.00x64.00] border=[0.00,0.00 800.00x64.00]
  box body content=[8.00,16.00 784.00x32.00] padding=[8.00,16.00 784.00x32.00] border=[8.00,16.00 784.00x32.00]
    box p content=[8.00,16.00 784.00x32.00] padding=[8.00,16.00 784.00x32.00] border=[8.00,16.00 784.00x32.00]
      text \"ab\" [390.00,16.00 20.00x16.00]
      text \"cd\" [390.00,32.00 20.00x16.00]
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_inline_block_atomic() {
    let dump = layout(
        "<html><body><p>a <span style=\"display: inline-block; width: 50px; height: 30px\">b</span> c</p></body></html>",
    );
    // The atomic span: specified 50x30 wins over shrink-to-fit; the
    // inline-block's last line baseline (12px from its border top) sits
    // on the line baseline, and its descent (30 − 12 = 18) grows the
    // line box to 30px. "a" advances 10, a 10px space, then the span at
    // x = 8 + 20 = 28.
    let expected = "\
layout v1 viewport=800x600
box html content=[0.00,0.00 800.00x62.00] padding=[0.00,0.00 800.00x62.00] border=[0.00,0.00 800.00x62.00]
  box body content=[8.00,16.00 784.00x30.00] padding=[8.00,16.00 784.00x30.00] border=[8.00,16.00 784.00x30.00]
    box p content=[8.00,16.00 784.00x30.00] padding=[8.00,16.00 784.00x30.00] border=[8.00,16.00 784.00x30.00]
      text \"a\" [8.00,16.00 10.00x16.00]
      box span content=[28.00,16.00 50.00x30.00] padding=[28.00,16.00 50.00x30.00] border=[28.00,16.00 50.00x30.00]
        text \"b\" [28.00,16.00 10.00x16.00]
      text \"c\" [88.00,16.00 10.00x16.00]
";
    assert_eq!(dump, expected);
}

#[test]
fn golden_whitespace_between_blocks_vanishes() {
    // Inter-block whitespace and the whitespace-only head-adjacent runs
    // produce no anonymous boxes.
    let dump = layout("<html><body>\n  <p>one</p>\n  <p>two</p>\n</body></html>");
    let anonymous = dump.matches("(anonymous)").count();
    assert_eq!(anonymous, 0, "whitespace runs vanish: {dump}");
    assert!(dump.contains("text \"one\""));
    assert!(dump.contains("text \"two\""));
}

#[test]
fn headless_document_lays_out() {
    // No <html>/<body> wrappers the parser would add anyway; the
    // degenerate tree (empty document) is the blank-page path.
    let url = Url::parse("https://example.org/doc").unwrap();
    let tree = veyra_dom::Tree::new_document(url.origin()).unwrap();
    let styles = compute_styles(&tree, &StyleSet::new());
    let result = layout_document(
        &tree,
        &styles,
        (800, 600),
        &FixedMetrics,
        LayoutLimits::default(),
    )
    .unwrap();
    assert_eq!(
        result.to_dump_string(),
        "layout v1 viewport=800x600\nbox (anonymous #document) content=[0.00,0.00 800.00x0.00] padding=[0.00,0.00 800.00x0.00] border=[0.00,0.00 800.00x0.00]\n"
    );
}

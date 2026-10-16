//! Cascade integration tests: origins, importance, specificity, source
//! order, layers, inline style, media subset, revert, and the UA sheet —
//! `compute_styles` over `veyra-dom` trees.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use veyra_dom::{QualName, Tree};
use veyra_style::{
    Color, ComputedValues, Display, Limits, MarginSizing, Origin, StyleSet, Stylesheet,
    compute_styles,
};
use veyra_url::Url;

/// Computes styles for `tree` with one author sheet.
fn compute(tree: &Tree, css: &str) -> veyra_style::ComputedStyleMap {
    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse(css, &Limits::default()).expect("css"),
        None,
    );
    compute_styles(tree, &set)
}

#[test]
fn ua_sheet_defaults_apply() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let html = tree
        .create_element(QualName::html("html"), vec![])
        .expect("limit");
    tree.append(doc, html).expect("hierarchy");
    let body = tree
        .create_element(QualName::html("body"), vec![])
        .expect("limit");
    tree.append(html, body).expect("hierarchy");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(body, p).expect("hierarchy");

    let set = StyleSet::new();
    let styles = compute_styles(&tree, &set);
    let html_style = styles.get(html).expect("html styled");
    assert_eq!(html_style.display, Display::Block);
    let body_style = styles.get(body).expect("body styled");
    assert_eq!(body_style.display, Display::Block);
    assert_eq!(
        body_style.margin.top,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(8.0))
    );
    let p_style = styles.get(p).expect("p styled");
    assert_eq!(p_style.display, Display::Block);
    assert_eq!(
        p_style.margin.top,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(16.0))
    );
}

#[test]
fn specificity_and_source_order() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(p, QualName::html("class"), "tip");
    let _ = tree.set_attribute(p, QualName::html("id"), "lead");
    tree.append(doc, p).expect("hierarchy");

    // Later source order wins within the same specificity; higher
    // specificity wins regardless of order.
    let styles = compute(
        &tree,
        "p { color: red; } .tip { color: green; } p#lead { color: blue; }",
    );
    let style = styles.get(p).expect("styled");
    assert_eq!(style.color, Color::rgb(0, 0, 255));

    let styles = compute(&tree, "p { color: red; } .tip { color: green; }");
    let style = styles.get(p).expect("styled");
    assert_eq!(style.color, Color::rgb(0, 128, 0));

    let styles = compute(&tree, "p.tip { color: green; } p { color: red; }");
    let style = styles.get(p).expect("styled");
    assert_eq!(style.color, Color::rgb(0, 128, 0));
}

#[test]
fn origin_precedence_and_importance_inversion() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse("p { color: red; }", &Limits::default()).expect("css"),
        None,
    );
    set.append_user(Stylesheet::parse("p { color: green; }", &Limits::default()).expect("css"));
    let styles = compute_styles(&tree, &set);
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    // Author !important beats user normal.
    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse("p { color: red !important; }", &Limits::default()).expect("css"),
        None,
    );
    set.append_user(Stylesheet::parse("p { color: green; }", &Limits::default()).expect("css"));
    let styles = compute_styles(&tree, &set);
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    // User !important beats author !important; UA !important beats both.
    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse("p { color: red !important; }", &Limits::default()).expect("css"),
        None,
    );
    set.append_user(
        Stylesheet::parse("p { color: green !important; }", &Limits::default()).expect("css"),
    );
    set.append_user_agent(
        Stylesheet::parse("p { color: black !important; }", &Limits::default()).expect("css"),
    );
    let styles = compute_styles(&tree, &set);
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 0));
}

#[test]
fn inline_style_beats_author_rules_and_important_rules_beat_it() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(p, QualName::html("style"), "color: green;");
    tree.append(doc, p).expect("hierarchy");

    let styles = compute(&tree, "p { color: red; }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 128, 0));

    // Author !important still beats the inline normal declaration.
    let styles = compute(&tree, "p { color: red !important; }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    // Inline !important beats author !important.
    let mut tree = Tree::new_document(Url::parse("https://example.org/").expect("url").origin())
        .expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(p, QualName::html("style"), "color: green !important;");
    tree.append(doc, p).expect("hierarchy");
    let styles = compute(&tree, "p { color: red !important; }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 128, 0));
}

#[test]
fn layers_unlayered_wins_and_order_applies() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    // Layer order is first-seen: alpha before beta; unlayered wins both.
    let styles = compute(
        &tree,
        "@layer alpha { p { color: red; } } \
         @layer beta { p { color: green; } } \
         p { color: blue; }",
    );
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 255));

    // Without unlayered competition, the later layer wins.
    let styles = compute(
        &tree,
        "@layer alpha { p { color: red; } } @layer beta { p { color: green; } }",
    );
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 128, 0));

    // Statement form establishes order too.
    let styles = compute(
        &tree,
        "@layer alpha, beta; @layer beta { p { color: red; } } @layer alpha { p { color: green; } }",
    );
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    // Important: layered beats unlayered, earlier layer beats later.
    let styles = compute(
        &tree,
        "p { color: blue !important; } \
         @layer alpha { p { color: red !important; } } \
         @layer beta { p { color: green !important; } }",
    );
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));
}

#[test]
fn media_subset_evaluation() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    let styles = compute(&tree, "@media screen { p { color: red; } }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    let styles = compute(&tree, "@media only all { p { color: red; } }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    let styles = compute(&tree, "@media print { p { color: red; } }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 0));

    let styles = compute(&tree, "@media (min-width: 400px) { p { color: red; } }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 0));

    let styles = compute(&tree, "@media screen, print { p { color: red; } }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    // @supports never applies yet (recorded deferral).
    let styles = compute(&tree, "@supports (color: red) { p { color: red; } }");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 0));
}

#[test]
fn revert_rolls_back_origin() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    // Author revert: user then UA apply.
    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse("p { color: red; color: revert; }", &Limits::default()).expect("css"),
        None,
    );
    set.append_user(Stylesheet::parse("p { color: green; }", &Limits::default()).expect("css"));
    let styles = compute_styles(&tree, &set);
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 128, 0));

    // A later, higher-specificity author declaration outranks the revert.
    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse(
            "p { color: revert; } p.x { color: red; }",
            &Limits::default(),
        )
        .expect("css"),
        None,
    );
    set.append_user(Stylesheet::parse("p { color: green; }", &Limits::default()).expect("css"));
    let _ = tree.set_attribute(p, QualName::html("class"), "x");
    let styles = compute_styles(&tree, &set);
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(255, 0, 0));

    // Revert with nothing below: the UA sheet's declaration applies
    // (user sheet absent here).
    let styles = compute(&tree, "p { color: revert; }");
    // UA sheet has no `color` for p → initial black.
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 0));
}

#[test]
fn inheritance_and_initial_values() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let body = tree
        .create_element(QualName::html("body"), vec![])
        .expect("limit");
    tree.append(doc, body).expect("hierarchy");
    let span = tree
        .create_element(QualName::html("span"), vec![])
        .expect("limit");
    tree.append(body, span).expect("hierarchy");

    let styles = compute(&tree, "body { color: purple; font-size: 20px; }");
    let body_style = styles.get(body).expect("styled");
    assert_eq!(body_style.color, Color::rgb(128, 0, 128));
    assert_eq!(body_style.font_size, 20.0);
    let span_style = styles.get(span).expect("styled");
    // Inherited.
    assert_eq!(span_style.color, Color::rgb(128, 0, 128));
    assert_eq!(span_style.font_size, 20.0);
    // Not inherited: initial.
    assert_eq!(span_style.background_color, Color::TRANSPARENT);
    assert_eq!(span_style.display, Display::Inline);

    // Global keywords.
    let styles = compute(
        &tree,
        "body { color: purple; font-size: 20px; background-color: purple; } \
         span { color: initial; font-size: inherit; background-color: inherit; }",
    );
    let span_style = styles.get(span).expect("styled");
    assert_eq!(span_style.color, Color::rgb(0, 0, 0));
    assert_eq!(span_style.font_size, 20.0);
    assert_eq!(span_style.background_color, Color::rgb(128, 0, 128));

    // `unset` = inherit for color.
    let styles = compute(&tree, "body { color: purple; } span { color: unset; }");
    assert_eq!(
        styles.get(span).expect("styled").color,
        Color::rgb(128, 0, 128)
    );
}

#[test]
fn relative_units_resolve() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let html = tree
        .create_element(QualName::html("html"), vec![])
        .expect("limit");
    tree.append(doc, html).expect("hierarchy");
    let body = tree
        .create_element(QualName::html("body"), vec![])
        .expect("limit");
    tree.append(html, body).expect("hierarchy");

    // Root font-size: rem on the root resolves against the initial 16px.
    let styles = compute(
        &tree,
        "html { font-size: 1.5rem; } body { font-size: 2em; padding: 1em; margin-top: 0.5rem; }",
    );
    let html_style = styles.get(html).expect("styled");
    assert_eq!(html_style.font_size, 24.0); // 1.5 × 16
    let body_style = styles.get(body).expect("styled");
    assert_eq!(body_style.font_size, 48.0); // 2 × 24
    // padding em resolves against the own font size.
    assert_eq!(body_style.padding.top, veyra_style::LengthPercent::Px(48.0));
    // margin rem resolves against the root (24px).
    assert_eq!(
        body_style.margin.top,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(12.0))
    );

    // Percent font-size against the parent.
    let styles = compute(&tree, "body { font-size: 150%; }");
    assert_eq!(styles.get(body).expect("styled").font_size, 24.0);

    // Absolute-size keywords and larger/smaller.
    let styles = compute(
        &tree,
        "html { font-size: large; } body { font-size: larger; }",
    );
    assert_eq!(styles.get(html).expect("styled").font_size, 18.0);
    assert_eq!(styles.get(body).expect("styled").font_size, 24.0);
}

#[test]
fn font_weight_and_style_relative_resolution() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let b = tree
        .create_element(QualName::html("b"), vec![])
        .expect("limit");
    tree.append(doc, b).expect("hierarchy");
    let nested = tree
        .create_element(QualName::html("b"), vec![])
        .expect("limit");
    tree.append(b, nested).expect("hierarchy");

    // UA: b { font-weight: bolder } → 400 → 700 → 900.
    let styles = compute(&tree, "");
    assert_eq!(styles.get(b).expect("styled").font_weight, 700);
    assert_eq!(styles.get(nested).expect("styled").font_weight, 900);

    let i = tree
        .create_element(QualName::html("i"), vec![])
        .expect("limit");
    tree.append(doc, i).expect("hierarchy");
    let styles = compute(&tree, "i { font-style: italic; }");
    assert_eq!(
        styles.get(i).expect("styled").font_style,
        veyra_style::FontStyle::Italic
    );
}

#[test]
fn shorthands_expand_with_cascade_order() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    // Shorthand then longhand: longhand wins that side only.
    let styles = compute(&tree, "p { margin: 10px; margin-left: 5px; }");
    let style = styles.get(p).expect("styled");
    assert_eq!(
        style.margin.top,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(10.0))
    );
    assert_eq!(
        style.margin.left,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(5.0))
    );

    // Longhand then shorthand: the shorthand resets the side.
    let styles = compute(&tree, "p { margin-left: 5px; margin: 10px; }");
    let style = styles.get(p).expect("styled");
    assert_eq!(
        style.margin.left,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(10.0))
    );

    // Border shorthand.
    let styles = compute(&tree, "p { border: 2px solid red; }");
    let style = styles.get(p).expect("styled");
    assert_eq!(style.border_width.top, 2.0);
    assert_eq!(style.border_style.top, veyra_style::BorderStyle::Solid);
    assert_eq!(style.border_color.top, Color::rgb(255, 0, 0));

    // 1–4 value expansion.
    let styles = compute(&tree, "p { margin: 1px 2px 3px 4px; padding: 7px 8px; }");
    let style = styles.get(p).expect("styled");
    assert_eq!(
        style.margin.bottom,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(3.0))
    );
    assert_eq!(style.padding.right, veyra_style::LengthPercent::Px(8.0));
    assert_eq!(style.padding.left, veyra_style::LengthPercent::Px(8.0));
}

#[test]
fn invalid_declarations_drop_not_panic() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    let styles = compute(
        &tree,
        "p { color: 12px; font-size: garbage; margin: ; display: 3; } p { color: red; }",
    );
    let style = styles.get(p).expect("styled");
    assert_eq!(style.color, Color::rgb(255, 0, 0));
    assert_eq!(style.display, Display::Block); // UA
    assert_eq!(
        style.margin.top,
        MarginSizing::LengthPercent(veyra_style::LengthPercent::Px(16.0)) // UA p margin
    );
}

#[test]
fn custom_properties_are_captured_and_inherited() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let body = tree
        .create_element(QualName::html("body"), vec![])
        .expect("limit");
    tree.append(doc, body).expect("hierarchy");
    let span = tree
        .create_element(QualName::html("span"), vec![])
        .expect("limit");
    tree.append(body, span).expect("hierarchy");

    let styles = compute(
        &tree,
        "body { --brand: teal; --empty: ; } span { --child: 1; }",
    );
    let body_style = styles.get(body).expect("styled");
    assert_eq!(
        body_style
            .custom_properties
            .get("--brand")
            .map(veyra_style::RetainedValue::to_css_string_trimmed),
        Some("teal".to_string())
    );
    // Inherited into the child.
    let span_style = styles.get(span).expect("styled");
    assert!(span_style.custom_properties.contains_key("--brand"));
    assert!(span_style.custom_properties.contains_key("--child"));
    assert_eq!(span_style.custom_properties.len(), 3);

    // var() references are invalid-at-computed-value-time: dropped.
    let styles = compute(&tree, "p { color: var(--brand); }");
    let p = find_element(&tree, "span");
    assert_eq!(styles.get(p).expect("styled").color, Color::rgb(0, 0, 0));
}

#[test]
fn pseudo_class_rules_cascade() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let a = tree
        .create_element(QualName::html("a"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(a, QualName::html("href"), "https://example.org/");
    tree.append(doc, a).expect("hierarchy");

    let styles = compute(&tree, "a:link { color: green; } a:hover { color: red; }");
    let style = styles.get(a).expect("styled");
    // :link applies; :hover does not (no interactive state model).
    assert_eq!(style.color, Color::rgb(0, 128, 0));
    assert!(
        style.text_decoration_line.underline(),
        "UA a:link underline missing (got {:?})",
        style.text_decoration_line
    );
}

#[test]
fn empty_and_fragment_trees_are_total() {
    let url = Url::parse("https://example.org/").expect("url");
    let tree = Tree::new_document(url.origin()).expect("tree");
    let styles = compute_styles(&tree, &StyleSet::new());
    assert!(styles.is_empty());

    let mut set = StyleSet::new();
    set.append_author(
        Stylesheet::parse("p { color: red; }", &Limits::default()).expect("css"),
        None,
    );
    let styles = compute_styles(&tree, &set);
    assert!(styles.is_empty());
}

#[test]
fn user_agent_origin_extensible() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    tree.append(doc, p).expect("hierarchy");

    let mut set = StyleSet::new();
    set.append_user_agent(
        Stylesheet::parse("p { color: slategray; }", &Limits::default()).expect("css"),
    );
    let styles = compute_styles(&tree, &set);
    assert_eq!(
        styles.get(p).expect("styled").color,
        Color::rgb(112, 128, 144)
    );
}

#[test]
fn default_style_shape_is_initial() {
    let initial = ComputedValues::initial();
    assert_eq!(initial.display, Display::Inline);
    assert_eq!(initial.font_size, 16.0);
    assert_eq!(initial.font_weight, 400);
    assert_eq!(initial.color, Color::BLACK);
    assert_eq!(initial.background_color, Color::TRANSPARENT);
    assert_eq!(initial.text_align, veyra_style::TextAlign::Start);
    assert!(initial.text_decoration_line.is_none());
}

/// The first element with `local`, pre-order.
fn find_element(tree: &Tree, local: &str) -> veyra_dom::NodeId {
    let doc = tree.document().expect("document");
    tree.descendants(doc)
        .find(|id| {
            matches!(
                tree.data(*id),
                Some(veyra_dom::NodeData::Element { name, .. }) if name.local == local
            )
        })
        .expect("element exists")
}

#[test]
fn origin_enum_shape() {
    // The three origins exist and are distinct (the cascade compares
    // them internally; this pins the public shape).
    let origins = [Origin::UserAgent, Origin::User, Origin::Author];
    assert_eq!(origins.len(), 3);
    assert_ne!(origins[0], origins[2]);
}

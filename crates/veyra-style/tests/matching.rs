//! Selector matching integration tests: `VeyraElement` against real
//! `veyra-dom` trees (the `selectors` matcher drives combinators and
//! tree-structural pseudo-classes; these tests pin the adapter).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use veyra_dom::{QualName, QuirksMode, Tree};
use veyra_style::{MatchOptions, SelectorList, VeyraElement};
use veyra_url::Url;

/// Builds a document tree from a small HTML fixture by hand (the DOM
/// shape the HTML parser produces; parsing itself is `veyra-html`'s
/// tested surface).
fn fixture_tree() -> Tree {
    let url = Url::parse("https://example.org/doc").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    let doc = tree.document().expect("document");
    let html = tree
        .create_element(QualName::html("html"), vec![])
        .expect("limit");
    tree.append(doc, html).expect("hierarchy");
    let head = tree
        .create_element(QualName::html("head"), vec![])
        .expect("limit");
    tree.append(html, head).expect("hierarchy");
    let body = tree
        .create_element(QualName::html("body"), vec![])
        .expect("limit");
    tree.append(html, body).expect("hierarchy");

    let div = tree
        .create_element(QualName::html("div"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(div, QualName::html("class"), "content main");
    let _ = tree.set_attribute(div, QualName::html("id"), "main");
    tree.append(body, div).expect("hierarchy");

    let p = tree
        .create_element(QualName::html("p"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(p, QualName::html("lang"), "en-GB");
    tree.append(div, p).expect("hierarchy");
    tree.append_text(p, "hello").expect("hierarchy");

    let span = tree
        .create_element(QualName::html("span"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(span, QualName::html("data-kind"), "widget");
    tree.append(p, span).expect("hierarchy");

    let empty = tree
        .create_element(QualName::html("em"), vec![])
        .expect("limit");
    tree.append(p, empty).expect("hierarchy");

    let link = tree
        .create_element(QualName::html("a"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(link, QualName::html("href"), "https://example.org/next");
    tree.append(body, link).expect("hierarchy");

    let input = tree
        .create_element(QualName::html("input"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(input, QualName::html("type"), "text");
    let _ = tree.set_attribute(input, QualName::html("required"), "");
    tree.append(body, input).expect("hierarchy");

    let svg = tree
        .create_element(
            QualName {
                prefix: None,
                namespace: veyra_dom::Namespace::Svg,
                local: "rect".to_string(),
            },
            vec![],
        )
        .expect("limit");
    tree.append(body, svg).expect("hierarchy");

    tree
}

/// Wraps a node id for matching.
fn element_of<'tree>(tree: &'tree Tree, id: veyra_dom::NodeId) -> VeyraElement<'tree> {
    VeyraElement::new(tree, id).expect("element")
}

/// The first element with `local`, pre-order.
fn find(tree: &Tree, local: &str) -> veyra_dom::NodeId {
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

fn matches(tree: &Tree, id: veyra_dom::NodeId, selector: &str) -> bool {
    let selectors = SelectorList::parse(selector).expect("valid selector");
    element_of(tree, id).matches(&selectors, &MatchOptions::default())
}

#[test]
fn type_class_id_and_universal() {
    let tree = fixture_tree();
    let div = find(&tree, "div");
    let span = find(&tree, "span");

    assert!(matches(&tree, div, "div"));
    assert!(!matches(&tree, div, "span"));
    assert!(matches(&tree, div, "*"));
    assert!(matches(&tree, div, ".content"));
    assert!(matches(&tree, div, ".main.content"));
    assert!(!matches(&tree, div, ".missing"));
    assert!(matches(&tree, div, "#main"));
    assert!(!matches(&tree, div, "#other"));
    // Type selectors are ASCII case-insensitive in HTML documents.
    assert!(matches(&tree, span, "SPAN"));
    // SVG elements match unprefixed type selectors (no default
    // namespace declared: any-namespace semantics).
    assert!(matches(&tree, find(&tree, "rect"), "rect"));
}

#[test]
fn attribute_selectors() {
    let tree = fixture_tree();
    let span = find(&tree, "span");
    let div = find(&tree, "div");

    assert!(matches(&tree, span, "[data-kind]"));
    assert!(matches(&tree, span, "[data-kind=widget]"));
    assert!(matches(&tree, span, "[data-kind~=widget]"));
    assert!(!matches(&tree, span, "[data-kind~=idget]"));
    assert!(matches(&tree, span, "[data-kind^=wid]"));
    assert!(matches(&tree, span, "[data-kind$=get]"));
    assert!(matches(&tree, span, "[data-kind*=idg]"));
    assert!(!matches(&tree, span, "[data-kind=Widget]"));
    assert!(matches(&tree, span, "[data-kind=widget i]"));
    // Attribute names match ASCII case-insensitively in HTML.
    assert!(matches(&tree, span, "[DATA-KIND=widget]"));
    // The class attribute is a whitespace-separated list.
    assert!(matches(&tree, div, "[class~=main]"));
}

#[test]
fn combinators() {
    let tree = fixture_tree();
    let span = find(&tree, "span");
    let p = find(&tree, "p");
    let div = find(&tree, "div");
    let a = find(&tree, "a");

    assert!(matches(&tree, span, "p span"));
    assert!(matches(&tree, span, "div p span"));
    assert!(matches(&tree, span, "body span"));
    assert!(!matches(&tree, span, "a span"));
    assert!(matches(&tree, p, "div > p"));
    assert!(!matches(&tree, p, "body > p"));
    assert!(matches(&tree, a, "div + a"));
    assert!(!matches(&tree, a, "p + a"));
    assert!(matches(&tree, a, "div ~ a"));
    // p is div's *child*, not its sibling: the sibling chain must fail
    // while the ancestor chain holds.
    assert!(!matches(&tree, span, "div ~ p > span"));
    assert!(matches(&tree, span, "body > div p > span"));
    let _ = div;
}

#[test]
fn tree_structural_pseudo_classes() {
    let tree = fixture_tree();
    let html = find(&tree, "html");
    let p = find(&tree, "p");
    let span = find(&tree, "span");
    let empty = find(&tree, "em");
    let a = find(&tree, "a");

    assert!(matches(&tree, html, ":root"));
    assert!(!matches(&tree, p, ":root"));
    assert!(matches(&tree, empty, ":empty"));
    assert!(!matches(&tree, p, ":empty"));
    assert!(matches(&tree, span, ":first-child"));
    assert!(!matches(&tree, empty, ":first-child"));
    assert!(matches(&tree, empty, ":last-child"));
    assert!(matches(&tree, a, "body > :nth-child(2)"));
    assert!(matches(&tree, a, ":nth-child(2n)"));
    assert!(!matches(&tree, a, ":nth-child(odd)"));
    assert!(matches(&tree, p, ":nth-child(1 of p)"));
    // p is div's only element child; span and em are siblings.
    assert!(matches(&tree, p, ":only-child"));
    assert!(!matches(&tree, span, ":only-child"));
}

#[test]
fn logical_pseudo_classes_is_where_not_has() {
    let tree = fixture_tree();
    let span = find(&tree, "span");
    let p = find(&tree, "p");
    let div = find(&tree, "div");

    assert!(matches(&tree, span, ":is(span, b)"));
    assert!(matches(&tree, span, ":where(p > *)"));
    assert!(matches(&tree, p, ":not(div)"));
    assert!(!matches(&tree, p, ":not(div *)"));
    assert!(matches(&tree, div, ":has(span)"));
    assert!(!matches(&tree, div, ":has(a)"));
    assert!(matches(&tree, p, ":has(> span)"));
    // `:has` with a sibling relative selector.
    assert!(matches(&tree, find(&tree, "body"), "body:has(> div > p)"));
}

#[test]
fn language_and_direction_matching() {
    let tree = fixture_tree();
    let p = find(&tree, "p");
    let span = find(&tree, "span");

    // `lang` inherits from the ancestor.
    assert!(matches(&tree, p, ":lang(en)"));
    assert!(matches(&tree, p, ":lang(en-GB)"));
    assert!(matches(&tree, span, ":lang(en)"));
    assert!(!matches(&tree, p, ":lang(fr)"));
    assert!(matches(&tree, p, ":lang(EN)"));
}

#[test]
fn link_and_form_pseudo_classes() {
    let tree = fixture_tree();
    let a = find(&tree, "a");
    let input = find(&tree, "input");
    let div = find(&tree, "div");

    assert!(matches(&tree, a, ":link"));
    assert!(!matches(&tree, div, ":link"));
    // Static approximation: visited never matches.
    assert!(!matches(&tree, a, ":visited"));
    // Interactive state has no model yet.
    assert!(!matches(&tree, a, ":hover"));
    assert!(!matches(&tree, a, ":focus"));

    assert!(matches(&tree, input, ":enabled"));
    assert!(matches(&tree, input, ":required"));
    assert!(!matches(&tree, input, ":optional"));
    assert!(!matches(&tree, input, ":disabled"));
    assert!(matches(&tree, input, ":read-write"));
    assert!(!matches(&tree, input, ":read-only"));
    // A div is read-only per the HTML definition.
    assert!(matches(&tree, div, ":read-only"));
    assert!(!matches(&tree, div, ":read-write"));
}

#[test]
fn target_matching_uses_match_options() {
    let tree = fixture_tree();
    let div = find(&tree, "div");
    let a = find(&tree, "a");

    assert!(!matches(&tree, div, ":target"));
    let selectors = SelectorList::parse(":target").expect("selector");
    let element = element_of(&tree, div);
    assert!(element.matches(
        &selectors,
        &MatchOptions {
            target_id: Some(div)
        }
    ));
    assert!(!element.matches(&selectors, &MatchOptions { target_id: Some(a) }));
}

#[test]
fn pseudo_elements_never_match_real_elements() {
    let tree = fixture_tree();
    let p = find(&tree, "p");
    assert!(!matches(&tree, p, "p::before"));
    assert!(!matches(&tree, p, "::after"));
}

#[test]
fn namespaces_in_selectors() {
    let tree = fixture_tree();
    // Explicit any-namespace and no-namespace forms parse without a
    // namespace map and match per their constraints.
    assert!(matches(&tree, find(&tree, "rect"), "*|rect"));
    // The no-namespace form does not match namespaced elements.
    assert!(!matches(&tree, find(&tree, "rect"), "|rect"));
    assert!(!matches(&tree, find(&tree, "body"), "|body"));
    // Prefixed forms (`svg|rect`) need `@namespace` prefix resolution —
    // the recorded cascade-batch deferral; they are typed parse errors
    // today.
    assert!(SelectorList::parse("svg|rect").is_err());
}

#[test]
fn quirks_mode_makes_class_matching_case_insensitive() {
    let url = Url::parse("https://example.org/").expect("url");
    let mut tree = Tree::new_document(url.origin()).expect("tree");
    tree.set_quirks_mode(QuirksMode::Yes);
    let doc = tree.document().expect("document");
    let div = tree
        .create_element(QualName::html("div"), vec![])
        .expect("limit");
    let _ = tree.set_attribute(div, QualName::html("class"), "Boxed");
    tree.append(doc, div).expect("hierarchy");

    assert!(matches(&tree, div, ".boxed"));
    assert!(matches(&tree, div, ".Boxed"));

    // Standards mode stays case-sensitive.
    let url = Url::parse("https://example.org/").expect("url");
    let mut strict = Tree::new_document(url.origin()).expect("tree");
    strict.set_quirks_mode(QuirksMode::No);
    let doc = strict.document().expect("document");
    let div = strict
        .create_element(QualName::html("div"), vec![])
        .expect("limit");
    let _ = strict.set_attribute(div, QualName::html("class"), "Boxed");
    strict.append(doc, div).expect("hierarchy");
    assert!(!matches(&strict, div, ".boxed"));
    assert!(matches(&strict, div, ".Boxed"));
}

#[test]
fn stale_and_non_element_ids_are_rejected_not_panicking() {
    let tree = fixture_tree();
    let doc = tree.document().expect("document");
    let text = tree
        .descendants(doc)
        .find(|id| matches!(tree.data(*id), Some(veyra_dom::NodeData::Text { .. })))
        .expect("text node");
    // Text nodes and the document are not elements.
    assert!(VeyraElement::new(&tree, text).is_none());
    assert!(VeyraElement::new(&tree, doc).is_none());

    // A destroyed node's id is stale: no element wrapper, no panic.
    let mut tree = fixture_tree();
    let doomed = find(&tree, "span");
    tree.destroy_subtree(doomed).expect("destroy");
    assert!(!tree.is_live(doomed));
    assert!(VeyraElement::new(&tree, doomed).is_none());
}

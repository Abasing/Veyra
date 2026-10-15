// Public-API behavior tests for `veyra-style` batch 1: stylesheet
// parsing, rule model, selector parsing, retained-value serialization,
// and the typed error surface. Conformance corpora have their own
// suites (src/corpus.rs); these pin the API contracts the rest of the
// engine codes against.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_style::{
    CssRule, DeclarationList, KeyframeSelector, LayerRule, Limits, ParseError, PseudoClass,
    SelectorList, Stylesheet,
};

#[test]
fn style_rules_parse_with_selectors_and_declarations() {
    let sheet = Stylesheet::parse(
        "h1, h2 { color: crimson; margin: 0 auto !important }",
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(sheet.len(), 1);
    let CssRule::Style(rule) = &sheet.rules()[0] else {
        panic!("expected style rule");
    };
    assert_eq!(rule.selectors.to_css_string(), "h1, h2");
    assert_eq!(rule.declarations.len(), 2);
    let declarations = rule.declarations.declarations();
    assert_eq!(declarations[0].name, "color");
    assert!(!declarations[0].important);
    assert_eq!(declarations[1].name, "margin");
    assert!(declarations[1].important);
    // Raw value tokens keep boundary whitespace (conformance-faithful
    // canonical form; the corpus pins this)...
    assert_eq!(declarations[1].value.to_css_string(), " 0 auto ");
    // ...while block serialization trims it (engine/CSSOM style).
    assert_eq!(
        rule.declarations.to_css_string(),
        "color: crimson; margin: 0 auto !important;"
    );
}

#[test]
fn style_rule_serialization_round_trips() {
    let css = "a.b > c { color: red; width: calc(1px + 2%) }";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    let serialized = sheet.to_css_string();
    assert_eq!(serialized, "a.b > c { color: red; width: calc(1px + 2%); }");
    // Reparse equality: the serialization is a fixed point.
    let reparsed = Stylesheet::parse(&serialized, &Limits::default()).unwrap();
    assert_eq!(reparsed.to_css_string(), serialized);
}

#[test]
fn at_rules_parse_and_serialize() {
    let css = "@import url(\"theme.css\") screen; @media (width > 100px) { p { color: blue } } \
               @supports (display: grid) { div { display: grid } }";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    assert_eq!(sheet.len(), 3);
    assert!(matches!(&sheet.rules()[0], CssRule::Import { .. }));
    let CssRule::Media { rules, .. } = &sheet.rules()[1] else {
        panic!("expected media rule");
    };
    assert_eq!(rules.len(), 1);
    let CssRule::Supports { rules, .. } = &sheet.rules()[2] else {
        panic!("expected supports rule");
    };
    assert_eq!(rules.len(), 1);
    assert!(
        sheet
            .to_css_string()
            .starts_with("@import \"theme.css\" screen;")
    );
}

#[test]
fn keyframes_parse_positions() {
    let css = "@keyframes slide { from { left: 0 } 50%, 60% { left: 5 } to { left: 10 } }";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    let CssRule::Keyframes { name, keyframes } = &sheet.rules()[0] else {
        panic!("expected keyframes");
    };
    assert_eq!(name, "slide");
    assert_eq!(keyframes.len(), 3);
    assert_eq!(keyframes[0].selectors, vec![KeyframeSelector::From]);
    assert_eq!(
        keyframes[1].selectors,
        vec![
            KeyframeSelector::Percentage(50),
            KeyframeSelector::Percentage(60)
        ]
    );
    assert_eq!(keyframes[2].selectors, vec![KeyframeSelector::To]);
}

#[test]
fn keyframes_out_of_range_percentages_drop_the_keyframe() {
    let css = "@keyframes x { 150% { left: 0 } 0% { top: 1 } }";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    let CssRule::Keyframes { keyframes, .. } = &sheet.rules()[0] else {
        panic!("expected keyframes");
    };
    // 150% is invalid; 0% survives.
    assert_eq!(keyframes.len(), 1);
    assert_eq!(
        keyframes[0].selectors,
        vec![KeyframeSelector::Percentage(0)]
    );
}

#[test]
fn layer_statement_and_block_forms_parse() {
    let css = "@layer base, components.typos; @layer base { p { color: red } } @layer { q {} }";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    let CssRule::Layer(LayerRule::Statement(names)) = &sheet.rules()[0] else {
        panic!("expected layer statement");
    };
    assert_eq!(names, &["base".to_string(), "components.typos".to_string()]);
    let CssRule::Layer(LayerRule::Block { name, rules }) = &sheet.rules()[1] else {
        panic!("expected layer block");
    };
    assert_eq!(name.as_deref(), Some("base"));
    assert_eq!(rules.len(), 1);
    let CssRule::Layer(LayerRule::Block { name, rules }) = &sheet.rules()[2] else {
        panic!("expected anonymous layer block");
    };
    assert_eq!(name.as_deref(), None);
    assert_eq!(rules.len(), 1);
}

#[test]
fn namespace_parses_with_and_without_prefix() {
    let css = "@namespace url(http://www.w3.org/1999/xhtml); @namespace svg url(http://www.w3.org/2000/svg);";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    let CssRule::Namespace { prefix, url } = &sheet.rules()[0] else {
        panic!("expected namespace");
    };
    assert_eq!(prefix.as_deref(), None);
    assert_eq!(url, "http://www.w3.org/1999/xhtml");
    let CssRule::Namespace { prefix, url } = &sheet.rules()[1] else {
        panic!("expected namespaced namespace");
    };
    assert_eq!(prefix.as_deref(), Some("svg"));
    assert_eq!(url, "http://www.w3.org/2000/svg");
}

#[test]
fn unknown_at_rules_are_retained_losslessly() {
    let css = "@unknown-at foo bar { weird: [structure]; } @other;";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    assert_eq!(sheet.len(), 2);
    let CssRule::Unknown {
        name,
        prelude,
        block,
    } = &sheet.rules()[0]
    else {
        panic!("expected unknown rule");
    };
    assert_eq!(name, "unknown-at");
    assert_eq!(prelude.to_css_string_trimmed(), "foo bar");
    assert!(block.is_some());
    // The block round-trips.
    let CssRule::Unknown { block, .. } = &sheet.rules()[0] else {
        panic!();
    };
    let block_text = block.as_ref().unwrap().to_css_string();
    let reparsed = Stylesheet::parse(&format!("@x {{ {block_text} }}"), &Limits::default());
    assert!(reparsed.is_ok());
    let CssRule::Unknown { block: None, .. } = &sheet.rules()[1] else {
        panic!("expected blockless unknown rule");
    };
}

#[test]
fn charset_is_consumed_at_top_and_invalid_elsewhere() {
    // First rule: consumed silently (not a real rule).
    let sheet =
        Stylesheet::parse("@charset \"utf-8\"; p { color: red }", &Limits::default()).unwrap();
    assert_eq!(sheet.len(), 1);
    // Anywhere else: the rule is dropped per error recovery.
    let sheet = Stylesheet::parse("p {} @charset \"utf-8\"; q {}", &Limits::default()).unwrap();
    assert_eq!(sheet.len(), 2);
}

#[test]
fn invalid_selectors_drop_only_their_rule() {
    let css = "p { color: red } a:::broken { color: blue } q { color: green }";
    let sheet = Stylesheet::parse(css, &Limits::default()).unwrap();
    assert_eq!(sheet.len(), 2);
    let CssRule::Style(rule) = &sheet.rules()[0] else {
        panic!();
    };
    assert_eq!(rule.selectors.to_css_string(), "p");
    let CssRule::Style(rule) = &sheet.rules()[1] else {
        panic!();
    };
    assert_eq!(rule.selectors.to_css_string(), "q");
}

#[test]
fn selector_list_parses_and_serializes_canonically() {
    let list = SelectorList::parse("a>b , .c[d~=e]:hover::before").unwrap();
    assert_eq!(list.len(), 2);
    // Attribute values serialize unquoted when they are valid
    // identifiers (the selectors crate's canonical form).
    assert_eq!(list.to_css_string(), "a > b, .c[d~=e]:hover::before");
}

#[test]
fn selector_errors_are_typed() {
    let error = SelectorList::parse("a:hoverx").unwrap_err();
    let ParseError::Selector { kind, .. } = error else {
        panic!("expected selector error, got {error}");
    };
    assert_eq!(
        kind,
        veyra_style::SelectorErrorKind::UnsupportedPseudoClassOrElement
    );
}

#[test]
fn functional_pseudo_classes_parse_arguments() {
    let list = SelectorList::parse(":lang(en, \"zh-Hans\"):dir(ltr)").unwrap();
    assert_eq!(list.len(), 1);
    // The serialization re-emits ranges canonically (identifiers
    // unquoted; the string/ident distinction is not preserved).
    let serialized = list.to_css_string();
    assert!(serialized.contains("lang(en, zh-Hans)"), "{serialized}");
    assert!(serialized.contains("dir(ltr)"), "{serialized}");
}

#[test]
fn pseudo_class_model_is_queryable() {
    assert!(PseudoClass::Hover.is_active_or_hover());
    assert!(PseudoClass::Focus.is_user_action_state());
    assert!(!PseudoClass::Link.is_active_or_hover());
}

#[test]
fn declaration_lists_parse_standalone() {
    let declarations = DeclarationList::parse("color: red; width: 10px !important; : broken");
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations.declarations()[0].value.to_css_string(), " red");
    assert!(declarations.declarations()[1].important);
    assert_eq!(
        declarations.to_css_string(),
        "color: red; width: 10px !important;"
    );
}

#[test]
fn custom_properties_are_retained_verbatim() {
    let declarations = DeclarationList::parse("--main-color: rgb(0 0 0 / var(--alpha))");
    assert_eq!(declarations.len(), 1);
    assert_eq!(
        declarations.declarations()[0].value.to_css_string(),
        " rgb(0 0 0 / var(--alpha))"
    );
    assert_eq!(
        declarations.declarations()[0].value.to_css_string_trimmed(),
        "rgb(0 0 0 / var(--alpha))"
    );
}

#[test]
fn bom_and_encoding_rejections_are_typed() {
    // UTF-8 BOM is consumed.
    let mut bom = b"\xEF\xBB\xBF".to_vec();
    bom.extend_from_slice(b"p { color: red }");
    let sheet = Stylesheet::parse_bytes(&bom, &Limits::default()).unwrap();
    assert_eq!(sheet.len(), 1);
    // UTF-16 BOMs are typed rejections (the charset stack is later work).
    let error = Stylesheet::parse_bytes(b"\xFF\xFE p {}", &Limits::default()).unwrap_err();
    assert_eq!(error, ParseError::NonUtf8);
    let error = Stylesheet::parse_bytes(b"\xFE\xFF p {}", &Limits::default()).unwrap_err();
    assert_eq!(error, ParseError::NonUtf8);
    // Invalid UTF-8 is a typed rejection, not a misparse.
    let error =
        Stylesheet::parse_bytes(b"p { content: \"\xFF\" }", &Limits::default()).unwrap_err();
    assert_eq!(error, ParseError::NonUtf8);
}

#[test]
fn deeply_nested_blocks_do_not_overflow_the_stack() {
    // The parser's internal nesting limit must turn pathological
    // nesting into typed recovery, not a crash.
    let depth = 10_000;
    let mut css = String::with_capacity(depth * 2 + 8);
    css.push_str("a{b:c}");
    for _ in 0..depth {
        css.push_str("@media all{");
    }
    for _ in 0..depth {
        css.push('}');
    }
    // Must return (recovered or parsed) in bounded time without
    // crashing.
    let sheet = Stylesheet::parse(&css, &Limits::default()).unwrap();
    let _ = sheet.to_css_string();
}

#[test]
fn case_insensitivity_of_at_rule_names_and_pseudo_names() {
    let sheet = Stylesheet::parse(
        "@MEDIA screen { p:HOVER { color: red } @SUPPORTS (x: y) { q {} } }",
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(sheet.len(), 1);
    let CssRule::Media { rules, .. } = &sheet.rules()[0] else {
        panic!("expected media");
    };
    assert_eq!(rules.len(), 2);
}

//! Vendored conformance corpus harness (`css-parsing-tests`).
//!
//! Unit tests (not `tests/` integration tests) because the corpus
//! comparison maps through `pub(crate)` internals — retained token
//! fields and the error-marker parse paths — that must stay out of the
//! public API. The JSON mapping mirrors the reference harness shipped
//! with `cssparser` itself (its `tests.rs`), so corpus expectations are
//! compared exactly as upstream compares them:
//!
//! - token/component values map to the corpus's JSON shape
//!   (`["ident", "a"]`, `" "` for whitespace, `["error", "invalid"]`
//!   for dropped items);
//! - numeric comparison is relative-epsilon (`almost_equals`);
//! - `"extra-input"`/`"empty"` expectations normalize to `"invalid"`.
//!
//! Serialization round-trips are canonical-form: Veyra serializes
//! retained values per the CSS Syntax canonical algorithm, and the
//! round-trip test (like cssparser's own `serializer` test) re-parses
//! the serialization and compares *parse results*, not strings.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]

use serde_json::{Value, json};

use crate::declarations::{Declaration, DeclarationListParser, parse_block_contents_with_markers};
use crate::rules::{CssRule, LayerRule};
use crate::tokens::RetainedValue;
use cssparser::{Parser, ToCss, Token, parse_nth, parse_one_declaration, parse_one_rule};

// ---------------------------------------------------------------------------
// JSON comparison helpers (mirrors of the reference harness)
// ---------------------------------------------------------------------------

fn almost_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0));
            (a - b).abs() <= a.abs() * 1e-6
        }
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| almost_equals(a, b))
        }
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}

fn normalize(json: &mut Value) {
    match *json {
        Value::Array(ref mut list) => {
            for item in list.iter_mut() {
                normalize(item)
            }
        }
        Value::String(ref mut s) if *s == "extra-input" || *s == "empty" => {
            *s = "invalid".to_string()
        }
        _ => {}
    }
}

/// The per-corpus delta tables: the only skip mechanism. Each entry is
/// a corpus *input* that the engine does not satisfy, documented with a
/// category in `tests/data/deltas.md`. Mirrors the `veyra-url` runner:
/// a failure without a deltas entry is a test bug; a documented delta
/// that stops reproducing (e.g. after a dependency bump) is a hard
/// failure, so the table shrinks instead of rotting.
const DOCUMENTED_DELTAS: &[(&str, &[&str])] = &[
    // (corpus file stem, inputs)
    //
    // 2026-10-16 corpus run (cssparser 0.38 pin): see
    // tests/data/deltas.md for the categories and counts —
    // declaration/token corpora clean; rule/stylesheet deltas are
    // canonical-layout regeneration and selector-validated preludes.
    (
        "one_rule",
        &[
            " /**/ div > p { color: #aaa;  } /**/ ",
            " /**/ { color: #aaa  ",
            " /* CDO/CDC are not special */ <!-- --> {",
        ],
    ),
    (
        "rule_list",
        &[
            " /**/ div > p { color: #aaa;  } /**/ ",
            " /**/ { color: #aaa  ",
            " /* CDO/CDC are not special */ <!-- --> {",
            "div { color: #aaa; } p{}",
            "div {} -->",
            "{}a",
            "{}@a",
        ],
    ),
    (
        "stylesheet",
        &[
            " /**/ div > p { color: #aaa;  } /**/ ",
            " /**/ { color: #aaa  ",
            " /* CDO/CDC are ignored between rules */ <!-- --> {",
            " <!-- --> a<!---->{",
            "div { color: #aaa; } p{}",
            "div {} -->",
            "{}a",
            "{}@a",
        ],
    ),
];

fn documented_deltas(corpus_stem: &str) -> &'static [&'static str] {
    DOCUMENTED_DELTAS
        .iter()
        .find(|(stem, _)| *stem == corpus_stem)
        .map(|(_, inputs)| *inputs)
        .unwrap_or(&[])
}

/// Runs a corpus with the delta mechanism: every failure must be a
/// documented delta, every documented delta must fail. Returns
/// (passed, delta_count) and prints the failure inventory when
/// `VEYRA_DUMP_STYLE_DELTAS` is set (used to seed the delta tables).
fn run_corpus_with_deltas<F: Fn(&str) -> Value>(corpus_stem: &str, parse: F) -> (usize, usize) {
    let corpus_data = corpus(corpus_stem);
    let dump = std::env::var("VEYRA_DUMP_STYLE_DELTAS").is_ok();
    use std::cell::Cell;
    use std::rc::Rc;

    let passed = Rc::new(Cell::new(0usize));
    let failed: Rc<std::cell::RefCell<Vec<String>>> = Rc::new(std::cell::RefCell::new(Vec::new()));
    {
        let passed = Rc::clone(&passed);
        let failed = Rc::clone(&failed);
        run_raw_json_tests(&corpus_data, |input, expected| {
            let Value::String(input) = input else {
                panic!("Unexpected corpus input shape")
            };
            let results = parse(&input);
            let mut expected = expected;
            normalize(&mut expected);
            if almost_equals(&results, &expected) {
                passed.set(passed.get() + 1);
            } else {
                failed.borrow_mut().push(input);
            }
        });
    }
    let passed = passed.get();
    let failed = Rc::try_unwrap(failed)
        .ok()
        .map(|cell| cell.into_inner())
        .unwrap_or_default();

    let documented = documented_deltas(corpus_stem);
    let undocumented: Vec<_> = failed
        .iter()
        .filter(|i| !documented.contains(&&i[..]))
        .collect();
    let resolved: Vec<_> = documented
        .iter()
        .filter(|d| !failed.contains(&d[..].to_string()))
        .collect();
    if dump {
        for input in &failed {
            println!("DELTA[{corpus_stem}]: {input:?}");
        }
    }
    assert!(
        undocumented.is_empty(),
        "{corpus_stem}: undocumented failures (add to DOCUMENTED_DELTAS + deltas.md): {:?}",
        &undocumented[..undocumented.len().min(5)]
    );
    assert!(
        resolved.is_empty(),
        "{corpus_stem}: documented deltas no longer reproduce (shrink deltas.md): {:?}",
        &resolved[..resolved.len().min(5)]
    );
    (passed, failed.len())
}

fn run_raw_json_tests<F: Fn(Value, Value)>(corpus: &str, run: F) {
    let items = match serde_json::from_str(corpus) {
        Ok(Value::Array(items)) => items,
        other => panic!("Invalid corpus JSON: {other:?}"),
    };
    assert!(items.len() % 2 == 0, "corpus must be input/expected pairs");
    let mut input = None;
    for item in items.into_iter() {
        match (&input, item) {
            (None, json_obj) => input = Some(json_obj),
            (Some(_), expected) => {
                let input = input.take().unwrap_or(Value::Null);
                run(input, expected)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Token/value JSON mapping (mirrors the reference harness)
// ---------------------------------------------------------------------------

/// Maps a retained value to corpus JSON by canonical re-serialization
/// and re-tokenization: the round-trip is the conformance claim (see
/// the module docs).
fn retained_value_to_json(value: &RetainedValue) -> Value {
    let serialized = value.to_css_string();
    let mut parser = Parser::new(&serialized);
    Value::Array(component_values_to_json(&mut parser))
}

/// Token-level mapping identical to the reference harness's
/// `component_values_to_json`.
fn component_values_to_json(input: &mut Parser<'_>) -> Vec<Value> {
    let mut values = Vec::new();
    while let Ok(token) = input.next_including_whitespace() {
        values.push(one_component_value_to_json(token.clone(), input));
    }
    values
}

fn numeric(serialized: String, value: f32, int_value: Option<i32>) -> Vec<Value> {
    vec![
        json!(serialized),
        match int_value {
            Some(i) => json!(i),
            None => json!(value),
        },
        match int_value {
            Some(_) => json!("integer"),
            None => json!("number"),
        },
    ]
}

fn one_component_value_to_json(token: cssparser::Token<'_>, input: &mut Parser<'_>) -> Value {
    use cssparser::Token::*;
    match token {
        Ident(value) => json!(["ident", &*value]),
        AtKeyword(value) => json!(["at-keyword", &*value]),
        Hash(value) => json!(["hash", &*value, "unrestricted"]),
        IDHash(value) => json!(["hash", &*value, "id"]),
        QuotedString(value) => json!(["string", &*value]),
        UnquotedUrl(value) => json!(["url", &*value]),
        Delim('\\') => json!("\\"),
        Delim(value) => json!(value.to_string()),
        Number {
            value,
            int_value,
            has_sign,
        } => {
            let mut v = vec![json!("number")];
            v.extend(numeric(
                Token::Number {
                    value,
                    int_value,
                    has_sign,
                }
                .to_css_string(),
                value,
                int_value,
            ));
            Value::Array(v)
        }
        Percentage {
            unit_value,
            int_value,
            has_sign,
        } => {
            let mut v = vec![json!("percentage")];
            v.extend(numeric(
                Token::Number {
                    value: unit_value * 100.0,
                    int_value,
                    has_sign,
                }
                .to_css_string(),
                unit_value * 100.0,
                int_value,
            ));
            Value::Array(v)
        }
        Dimension {
            value,
            int_value,
            has_sign,
            unit,
        } => {
            let mut v = vec![json!("dimension")];
            v.extend(numeric(
                Token::Number {
                    value,
                    int_value,
                    has_sign,
                }
                .to_css_string(),
                value,
                int_value,
            ));
            v.push(json!(&*unit));
            Value::Array(v)
        }
        WhiteSpace(_) => json!(" "),
        Comment(_) => json!("/**/"),
        Colon => json!(":"),
        Semicolon => json!(";"),
        Comma => json!(","),
        IncludeMatch => json!("~="),
        DashMatch => json!("|="),
        PrefixMatch => json!("^="),
        SuffixMatch => json!("$="),
        SubstringMatch => json!("*="),
        CDO => json!("<!--"),
        CDC => json!("-->"),
        Function(name) => {
            let nested: Result<Value, cssparser::ParseError<()>> =
                input.parse_nested_block(|input| Ok(Value::Array(component_values_to_json(input))));
            let mut v = vec![json!("function"), json!(&*name)];
            // The reference harness *extends* the nested values into the
            // function's array (flat), not a nested sub-array.
            if let Value::Array(items) = nested.unwrap_or(Value::Null) {
                v.extend(items);
            }
            Value::Array(v)
        }
        ParenthesisBlock => {
            let nested: Result<Value, cssparser::ParseError<()>> =
                input.parse_nested_block(|input| Ok(Value::Array(component_values_to_json(input))));
            let mut v = vec![json!("()")];
            if let Value::Array(items) = nested.unwrap_or(Value::Null) {
                v.extend(items);
            }
            Value::Array(v)
        }
        SquareBracketBlock => {
            let nested: Result<Value, cssparser::ParseError<()>> =
                input.parse_nested_block(|input| Ok(Value::Array(component_values_to_json(input))));
            let mut v = vec![json!("[]")];
            if let Value::Array(items) = nested.unwrap_or(Value::Null) {
                v.extend(items);
            }
            Value::Array(v)
        }
        CurlyBracketBlock => {
            let nested: Result<Value, cssparser::ParseError<()>> =
                input.parse_nested_block(|input| Ok(Value::Array(component_values_to_json(input))));
            let mut v = vec![json!("{}")];
            if let Value::Array(items) = nested.unwrap_or(Value::Null) {
                v.extend(items);
            }
            Value::Array(v)
        }
        BadUrl(_) => json!(["error", "bad-url"]),
        BadString(_) => json!(["error", "bad-string"]),
        CloseParenthesis => json!(["error", ")"]),
        CloseSquareBracket => json!(["error", "]"]),
        CloseCurlyBracket => json!(["error", "}"]),
    }
}

// ---------------------------------------------------------------------------
// Rule JSON mapping (Veyra types → corpus shape, canonically)
// ---------------------------------------------------------------------------

fn declaration_to_json(declaration: &Declaration) -> Value {
    json!([
        "declaration",
        declaration.name,
        retained_value_to_json(&declaration.value),
        declaration.important,
    ])
}

/// Maps conformance items (rules + reported declaration shapes) to
/// corpus JSON.
fn conformance_items_to_json(
    items: Vec<Result<crate::rules::conformance::ConformanceItem, ()>>,
) -> Value {
    Value::Array(
        items
            .into_iter()
            .map(|item| match item {
                Ok(crate::rules::conformance::ConformanceItem::Rule(rule)) => rule_to_json(&rule),
                Ok(crate::rules::conformance::ConformanceItem::Declaration(declaration)) => {
                    declaration_to_json(&declaration)
                }
                Err(()) => json!(["error", "invalid"]),
            })
            .collect(),
    )
}

fn rule_to_json(rule: &CssRule) -> Value {
    match rule {
        CssRule::Style(style) => {
            // Prelude: the parsed selector list, canonically
            // re-serialized and re-tokenized. Deltas against the
            // corpus's raw-token prelude are documented in
            // tests/data/deltas.md (combinator/compound canonical
            // spacing).
            json!([
                "qualified rule",
                retained_value_to_json(&reparsed(&style.selectors.to_css_string())),
                retained_value_to_json(&reparsed(&style.declarations.to_css_string())),
            ])
        }
        CssRule::Media { prelude, rules } => json!([
            "at-rule",
            "media",
            retained_value_to_json(prelude),
            rules_to_canonical(rules),
        ]),
        CssRule::Supports { prelude, rules } => json!([
            "at-rule",
            "supports",
            retained_value_to_json(prelude),
            rules_to_canonical(rules),
        ]),
        CssRule::Import {
            url,
            layer_and_media,
        } => {
            let mut prelude = vec![json!(" ")];
            prelude.push(json!(["string", url]));
            let rest = retained_value_to_json(layer_and_media);
            if rest != Value::Array(Vec::new())
                && let Value::Array(items) = rest
            {
                prelude.extend(items);
            }
            json!(["at-rule", "import", Value::Array(prelude), Value::Null])
        }
        CssRule::FontFace(declarations) => json!([
            "at-rule",
            "font-face",
            Value::Array(Vec::new()),
            retained_value_to_json(&reparsed(&declarations.to_css_string())),
        ]),
        CssRule::Keyframes { name, keyframes } => {
            let mut body = Vec::new();
            for keyframe in keyframes {
                let mut selectors = Vec::new();
                for (index, selector) in keyframe.selectors.iter().enumerate() {
                    if index > 0 {
                        selectors.push(json!(","));
                    }
                    selectors.push(match selector {
                        crate::KeyframeSelector::From => json!(["ident", "from"]),
                        crate::KeyframeSelector::To => json!(["ident", "to"]),
                        crate::KeyframeSelector::Percentage(value) => {
                            json!([
                                "percentage",
                                *value as f64 / 100.0,
                                *value as i32,
                                "integer"
                            ])
                        }
                    });
                }
                body.push(json!([
                    "qualified rule",
                    Value::Array(selectors),
                    retained_value_to_json(&reparsed(&keyframe.declarations.to_css_string()))
                ]));
            }
            json!([
                "at-rule",
                "keyframes",
                vec![json!(["ident", name])],
                Value::Array(body)
            ])
        }
        CssRule::Layer(LayerRule::Statement(names)) => {
            let mut prelude = Vec::new();
            for (index, name) in names.iter().enumerate() {
                if index > 0 {
                    prelude.push(json!(","));
                }
                prelude.push(json!(["ident", name]));
            }
            json!(["at-rule", "layer", Value::Array(prelude), Value::Null])
        }
        CssRule::Layer(LayerRule::Block { name, rules }) => {
            let prelude = match name {
                Some(name) => vec![json!(["ident", name])],
                None => Vec::new(),
            };
            json!(["at-rule", "layer", prelude, rules_to_canonical(rules)])
        }
        CssRule::Namespace { prefix, url } => {
            let mut prelude = Vec::new();
            if let Some(prefix) = prefix {
                prelude.push(json!(["ident", prefix]));
                prelude.push(json!(" "));
            }
            prelude.push(json!(["url", url]));
            json!(["at-rule", "namespace", Value::Array(prelude), Value::Null])
        }
        CssRule::Unknown {
            name,
            prelude,
            block,
        } => {
            json!([
                "at-rule",
                name,
                retained_value_to_json(prelude),
                match block {
                    Some(block) => retained_value_to_json(block),
                    None => Value::Null,
                },
            ])
        }
    }
}

/// Maps nested rules to the corpus's raw-token body shape: canonical
/// re-serialization followed by re-tokenization (documented delta
/// category: canonical spacing).
fn rules_to_canonical(rules: &[CssRule]) -> Value {
    let serialized = rules
        .iter()
        .map(|rule| rule.to_css_string())
        .collect::<Vec<_>>()
        .join(" ");
    retained_value_to_json(&reparsed(&serialized))
}

/// Re-parses `css` into a retained value (for canonical round-trips of
/// serialization output through the token mapper).
fn reparsed(css: &str) -> RetainedValue {
    let mut parser = Parser::new(css);
    RetainedValue::parse(&mut parser)
}

// ---------------------------------------------------------------------------
// The corpus tests
// ---------------------------------------------------------------------------

const CORPUS: &str = "tests/data";

fn corpus(name: &str) -> String {
    std::fs::read_to_string(format!("{CORPUS}/{name}.json"))
        .unwrap_or_else(|error| panic!("vendored corpus file {name}.json missing: {error}"))
}

#[test]
fn corpus_component_value_list() {
    let (passed, deltas) = run_corpus_with_deltas("component_value_list", |input| {
        let mut parser = Parser::new(input);
        let retained = RetainedValue::parse(&mut parser);
        // Round-trip: serialize canonically, re-parse, compare parse
        // results (the same claim cssparser's serializer test makes).
        let serialized = retained.to_css_string();
        let mut reparsed = Parser::new(&serialized);
        Value::Array(component_values_to_json(&mut reparsed))
    });
    println!("component_value_list: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_one_component_value() {
    let (passed, deltas) = run_corpus_with_deltas("one_component_value", |input| {
        let mut parser = Parser::new(input);
        match parser.next() {
            Ok(token) => {
                let value = one_component_value_to_json(token.clone(), &mut parser);
                if parser.is_exhausted() {
                    value
                } else {
                    json!(["error", "invalid"])
                }
            }
            Err(_) => json!(["error", "invalid"]),
        }
    });
    println!("one_component_value: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_declaration_list() {
    let (passed, deltas) = run_corpus_with_deltas("declaration_list", |input| {
        let mut parser = Parser::new(input);
        let markers = parse_block_contents_with_markers(&mut parser);
        Value::Array(
            markers
                .into_iter()
                .map(|item| match item {
                    Ok(crate::declarations::BlockItem::Declaration(declaration)) => {
                        declaration_to_json(&declaration)
                    }
                    Ok(crate::declarations::BlockItem::AtRuleShape {
                        name,
                        prelude,
                        block,
                    }) => json!([
                        "at-rule",
                        name,
                        retained_value_to_json(&prelude),
                        match block {
                            Some(block) => retained_value_to_json(&block),
                            None => Value::Null,
                        },
                    ]),
                    Ok(crate::declarations::BlockItem::QualifiedRuleShape { prelude, block }) => {
                        json!([
                            "qualified rule",
                            retained_value_to_json(&prelude),
                            retained_value_to_json(&block),
                        ])
                    }
                    Err(()) => json!(["error", "invalid"]),
                })
                .collect(),
        )
    });
    println!("declaration_list: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_one_declaration() {
    let (passed, deltas) = run_corpus_with_deltas("one_declaration", |input| {
        let mut parser = Parser::new(input);
        match parse_one_declaration(&mut parser, &mut DeclarationListParser) {
            Ok(crate::declarations::BlockItem::Declaration(declaration)) => {
                declaration_to_json(&declaration)
            }
            _ => json!(["error", "invalid"]),
        }
    });
    println!("one_declaration: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_rule_list() {
    let (passed, deltas) = run_corpus_with_deltas("rule_list", |input| {
        let mut parser = Parser::new(input);
        let mut corpus_parser = crate::rules::conformance::CorpusParser {
            rule: crate::rules::RuleParser,
        };
        let items: Vec<_> = cssparser::RuleBodyParser::new(&mut parser, &mut corpus_parser)
            .map(|item| item.map_err(|_| ()))
            .collect();
        conformance_items_to_json(items)
    });
    println!("rule_list: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_stylesheet() {
    let (passed, deltas) = run_corpus_with_deltas("stylesheet", |input| {
        let mut parser = Parser::new(input);
        let mut corpus_parser = crate::rules::conformance::CorpusParser {
            rule: crate::rules::RuleParser,
        };
        let items: Vec<_> = cssparser::StyleSheetParser::new(&mut parser, &mut corpus_parser)
            .map(|item| item.map_err(|_| ()))
            .collect();
        conformance_items_to_json(items)
    });
    println!("stylesheet: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_one_rule() {
    let (passed, deltas) = run_corpus_with_deltas("one_rule", |input| {
        let mut parser = Parser::new(input);
        let mut rule_parser = crate::rules::RuleParser;
        match parse_one_rule(&mut parser, &mut rule_parser) {
            Ok(rule) => rule_to_json(&rule),
            Err(_) => json!(["error", "invalid"]),
        }
    });
    println!("one_rule: {passed} passed, {deltas} documented deltas");
}

#[test]
fn corpus_anb() {
    let (passed, deltas) = run_corpus_with_deltas("An+B", |input| {
        let mut parser = Parser::new(input);
        parser
            .parse_entirely(|input| -> Result<(i32, i32), cssparser::ParseError<()>> {
                parse_nth(input).map_err(|e| cssparser::ParseError::from_basic_kind(e.kind))
            })
            .ok()
            .map(|(a, b)| json!([a, b]))
            .unwrap_or(Value::Null)
    });
    println!("An+B: {passed} passed, {deltas} documented deltas");
}

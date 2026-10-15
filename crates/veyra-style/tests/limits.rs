// Limits tests (Phase 3 batch 1; engineering standards §6: limits are
// configuration, defaulted conservatively, and each limit has an owner
// with tests).
//
// The normative limits-table row owned by veyra-style:
//   stylesheet size 16 MiB
// Enforcement happens before any parsing work (one length check), and
// the rejection is typed — no partial parse, no crash.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_style::{Limits, MAX_STYLESHEET_BYTES, ParseError, Stylesheet};

#[test]
fn oversized_stylesheet_is_rejected_before_parsing() {
    let big = vec![b'a'; MAX_STYLESHEET_BYTES + 1];
    let error = Stylesheet::parse_bytes(&big, &Limits::default()).unwrap_err();
    assert_eq!(
        error,
        ParseError::TooLarge {
            max: MAX_STYLESHEET_BYTES,
            actual: big.len(),
        }
    );
}

#[test]
fn str_parse_enforces_the_same_limit() {
    let big = "a{}".repeat(MAX_STYLESHEET_BYTES / 3 + 1);
    let error = Stylesheet::parse(&big, &Limits::default()).unwrap_err();
    assert!(matches!(error, ParseError::TooLarge { .. }));
}

#[test]
fn limit_is_configurable_downward() {
    let limits = Limits {
        max_stylesheet_bytes: 8,
    };
    let error = Stylesheet::parse("a { color: red }", &limits).unwrap_err();
    assert_eq!(error, ParseError::TooLarge { max: 8, actual: 16 });
}

#[test]
fn stylesheet_at_the_limit_parses() {
    // Exactly at the limit (inclusive boundary): must parse, not reject.
    let mut css = String::with_capacity(MAX_STYLESHEET_BYTES);
    let rule = "a{}\n";
    while css.len() + rule.len() <= MAX_STYLESHEET_BYTES {
        css.push_str(rule);
    }
    css.push_str(&" ".repeat(MAX_STYLESHEET_BYTES - css.len()));
    assert_eq!(css.len(), MAX_STYLESHEET_BYTES);
    let sheet = Stylesheet::parse(&css, &Limits::default()).unwrap();
    assert!(!sheet.is_empty());
}

#[test]
fn limit_display_is_stable() {
    let limits = Limits::default();
    assert_eq!(
        limits.to_string(),
        format!("Limits {{ max_stylesheet_bytes: {MAX_STYLESHEET_BYTES} }}")
    );
}

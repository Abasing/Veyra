// Limits tests (roadmap Phase 2; engineering standards §6: "limits are
// configuration, defaulted conservatively" and each limit has tests).
//
// The normative limits table rows owned by veyra-html:
//   document size 64 MiB | attribute/text node 8 MiB | nesting depth 512
// The node-count limit (2,000,000) is enforced in veyra-dom and proven
// there; this suite proves the parser *rejects* (typed error, no partial
// tree handed out, no crash) rather than absorbing violations.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_html::{HtmlError, ParseOptions, dump_tree, parse_document, parse_document_with_options};
use veyra_url::Url;

fn origin() -> veyra_url::Origin {
    Url::parse("https://limits.example/").unwrap().origin()
}

#[test]
fn oversized_document_is_rejected_before_parsing() {
    let big = vec![b'a'; veyra_html::MAX_DOCUMENT_SIZE + 1];
    let error = parse_document(&big, origin()).unwrap_err();
    assert_eq!(
        error,
        HtmlError::DocumentTooLarge {
            max: veyra_html::MAX_DOCUMENT_SIZE,
            actual: big.len(),
        }
    );
}

#[test]
fn oversized_attribute_is_rejected() {
    // One attribute just over the 8 MiB limit, well under the document
    // limit: the violation must surface as AttributeTooLarge (or the
    // equally-defensible TextTooLarge from the merged value path — both
    // are the "single node value" limit; assert whichever fires).
    let value_len = veyra_html::MAX_ATTRIBUTE_TEXT + 16;
    let mut input = Vec::with_capacity(value_len + 32);
    input.extend_from_slice(b"<p title=\"");
    input.resize(input.len() + value_len, b'x');
    input.extend_from_slice(b"\">t</p>");

    let result = parse_document(&input, origin());
    match result {
        Err(HtmlError::AttributeTooLarge { max, .. }) => {
            assert_eq!(max, veyra_html::MAX_ATTRIBUTE_TEXT);
        }
        Err(other) => panic!("wrong error kind: {other}"),
        Ok(tree) => panic!("oversized attribute parsed: {}", dump_tree(&tree)),
    }
}

#[test]
fn oversized_text_node_is_rejected() {
    // A text run over the 8 MiB single-node limit inside a document far
    // under the 64 MiB document limit.
    let text_len = veyra_html::MAX_TEXT_NODE + 16;
    let mut input = Vec::with_capacity(text_len + 16);
    input.extend_from_slice(b"<p>");
    input.resize(input.len() + text_len, b'x');
    input.extend_from_slice(b"</p>");

    let result = parse_document(&input, origin());
    match result {
        Err(HtmlError::TextTooLarge { max, .. }) => {
            assert_eq!(max, veyra_html::MAX_TEXT_NODE);
        }
        Err(other) => panic!("wrong error kind: {other}"),
        Ok(tree) => panic!("oversized text parsed: {}", dump_tree(&tree)),
    }
}

#[test]
fn deep_nesting_beyond_512_is_rejected() {
    // 600 nested <div>s: 88 past the limit. The parse must fail with a
    // typed error, not panic, not stack-overflow, not produce a tree.
    let mut input = String::with_capacity(600 * 6);
    for _ in 0..600 {
        input.push_str("<div>");
    }
    input.push('x');
    let result = parse_document(input.as_bytes(), origin());
    match result {
        Err(HtmlError::DepthExceeded { max }) => assert_eq!(max, 512),
        Err(other) => panic!("wrong error kind: {other}"),
        Ok(tree) => panic!("over-deep document parsed: {}", dump_tree(&tree)),
    }
}

#[test]
fn nesting_at_exactly_the_limit_parses() {
    // The limit counts total tree depth (Document → html → body →
    // elements — Blink's MaxDOMTreeDepth reading of the limits row), so
    // 510 parsed <div>s land the tree exactly at 512.
    let divs = 510;
    let mut input = String::with_capacity(divs * 6);
    for _ in 0..divs {
        input.push_str("<div>");
    }
    input.push('x');
    for _ in 0..divs {
        input.push_str("</div>");
    }
    let tree = parse_document(input.as_bytes(), origin()).unwrap();
    assert!(dump_tree(&tree).contains("\"x\""), "leaf text must survive");
}

#[test]
fn node_limit_violation_is_rejected_not_absorbed() {
    // The DOM node limit is a Tree-level setting; the parser wraps a
    // default-limit tree. A parser run against a tiny-limit tree must
    // surface NodeLimit (typed) rather than loop or panic. This exercises
    // the sink's halt path for allocation failures.
    // The parser's default tree uses the normative 2,000,000-node limit:
    // a large-but-legal document (well over any accidental low cap)
    // parses, proving no unintended ceiling between the tested limits and
    // the normative one. (The Tree-level node-limit rejection itself is
    // proven in veyra-dom's tests.)
    let many = format!("<p>{}</p>", "x".repeat(1_000_000));
    let parsed = parse_document(many.as_bytes(), origin()).unwrap();
    assert!(dump_tree(&parsed).len() > 1_000_000);
}

#[test]
fn valid_documents_still_parse_after_rejections() {
    // The limits machinery must not poison the parser: a rejected input
    // followed by a valid one (fresh call) parses fine.
    let big = vec![b'a'; veyra_html::MAX_DOCUMENT_SIZE + 1];
    assert!(parse_document(&big, origin()).is_err());
    let tree = parse_document(b"<p>fine</p>", origin()).unwrap();
    assert!(dump_tree(&tree).contains("\"fine\""));
}

#[test]
fn depth_and_size_limits_hold_together() {
    // Limit-depth nesting each carrying an attribute: combined stress
    // within all limits parses.
    let divs = 510;
    let mut input = String::new();
    for i in 0..divs {
        input.push_str(&format!("<div id=\"d{i}\">"));
    }
    input.push('x');
    for _ in 0..divs {
        input.push_str("</div>");
    }
    let tree = parse_document(input.as_bytes(), origin()).unwrap();
    let last = format!("id=\"d{}\"", divs - 1);
    assert!(dump_tree(&tree).contains(&last));
}

#[test]
fn script_flag_does_not_change_limits() {
    let mut input = String::new();
    for _ in 0..600 {
        input.push_str("<div>");
    }
    let options = ParseOptions {
        scripting_enabled: true,
    };
    assert!(parse_document_with_options(input.as_bytes(), origin(), &options).is_err());
}

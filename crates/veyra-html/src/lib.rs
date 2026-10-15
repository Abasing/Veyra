//! `veyra-html` — Veyra's HTML parser: bytes in, [`veyra-dom`] trees out.
//!
//! Wraps the html5ever tree builder (the audit-adopted Phase 2 dependency,
//! ADR-0003/ADR-0013) behind the Veyra node model. The external crate is
//! consumed only here and emits only `veyra-dom` handles
//! (dependency-graph §5 isolation table): a parser replacement stays a
//! one-crate change, and nothing in the engine parses HTML twice.
//!
//! Scope (roadmap Phase 2):
//!
//! - document parsing over UTF-8/UTF-16 inputs (BOM sniffing; full
//!   charset-stack support is a recorded Phase 3+ item),
//! - fragment parsing with a context element (used by `innerHTML` in
//!   Phase 5),
//! - the normative input limits — document size 64 MiB, single
//!   attribute/text node 8 MiB, element nesting depth 512, DOM node count
//!   2,000,000 — enforced with typed rejections, never panics
//!   (engineering standards §6),
//! - the html5lib-format tree dumper used by conformance tests and by the
//!   shell's `--dump-dom`.
//!
//! # Security posture
//!
//! This crate is an untrusted-input leaf (dependency-graph invariant 6):
//! no privileged imports, every failure a typed [`HtmlError`], and the
//! parser never panics on malformed input — the vendored html5lib corpus
//! and the deterministic fuzz smoke (`tests/fuzz_smoke.rs`) hold that
//! line.
//!
//! # Examples
//!
//! ```
//! use veyra_html::parse_document;
//! use veyra_url::{Origin, Url};
//!
//! let url = Url::parse("https://example.org/").expect("url");
//! let tree = parse_document(b"<!DOCTYPE html><p>Hello", url.origin()).expect("parse");
//! assert_eq!(
//!     veyra_html::dump_tree(&tree),
//!     "| <!DOCTYPE html>\n| <html>\n|   <head>\n|   <body>\n|     <p>\n|       \"Hello\"\n"
//! );
//! ```

#![forbid(unsafe_code)]

mod decode;
mod dump;
mod sink;

use html5ever::tendril::TendrilSink;
use veyra_dom::NodeId;
use veyra_url::Origin;

pub use decode::{Bom, Decoded, decode};
pub use dump::{dump_fragment, dump_tree};

/// Normative limit: maximum document size in bytes (limits table,
/// engineering standards §6 — owner: veyra-html).
pub const MAX_DOCUMENT_SIZE: usize = 64 * 1024 * 1024;

/// Normative limit: maximum size of one text node's data or one attribute
/// value in bytes (limits table — owner: veyra-html).
pub const MAX_TEXT_NODE: usize = 8 * 1024 * 1024;

/// Normative limit: maximum size of a single attribute value in bytes
/// (limits table — owner: veyra-html).
pub const MAX_ATTRIBUTE_TEXT: usize = 8 * 1024 * 1024;

/// Normative limit: maximum element nesting depth (limits table — owner:
/// veyra-html; the DOM mutation path enforces the same bound).
pub const MAX_NESTING_DEPTH: usize = 512;

/// Parser options (limits are the crate constants; scripting affects
/// `<noscript>` parsing).
#[derive(Clone, Copy, Debug, Default)]
pub struct ParseOptions {
    /// Is scripting enabled? Affects `<noscript>` contents. Phase 2 has no
    /// script engine; the flag exists because the parsed tree differs, and
    /// the conformance runner exercises both values.
    pub scripting_enabled: bool,
}

/// Typed parse outcomes (limits violations reject the document; nothing
/// here is ever a panic).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HtmlError {
    /// The input exceeded [`MAX_DOCUMENT_SIZE`] before parsing began.
    DocumentTooLarge {
        /// The configured maximum in bytes.
        max: usize,
        /// The actual size in bytes.
        actual: usize,
    },
    /// A single attribute value exceeded [`MAX_ATTRIBUTE_TEXT`].
    AttributeTooLarge {
        /// The configured maximum in bytes.
        max: usize,
        /// The actual size in bytes.
        actual: usize,
    },
    /// A text node's data exceeded [`MAX_TEXT_NODE`].
    TextTooLarge {
        /// The configured maximum in bytes.
        max: usize,
        /// The actual size in bytes.
        actual: usize,
    },
    /// The tree reached the DOM node-count limit during construction.
    NodeLimit {
        /// The configured maximum node count.
        max: usize,
    },
    /// The tree exceeded the nesting-depth limit during construction.
    DepthExceeded {
        /// The configured maximum depth.
        max: usize,
    },
}

impl std::fmt::Display for HtmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentTooLarge { max, actual } => {
                write!(f, "document of {actual} bytes exceeds the {max}-byte limit")
            }
            Self::AttributeTooLarge { max, actual } => {
                write!(
                    f,
                    "attribute of {actual} bytes exceeds the {max}-byte limit"
                )
            }
            Self::TextTooLarge { max, actual } => {
                write!(
                    f,
                    "text node of {actual} bytes exceeds the {max}-byte limit"
                )
            }
            Self::NodeLimit { max } => write!(f, "document exceeded the {max}-node limit"),
            Self::DepthExceeded { max } => {
                write!(f, "document exceeded the nesting-depth limit of {max}")
            }
        }
    }
}

impl std::error::Error for HtmlError {}

/// Parses a complete HTML document into an origin-tagged [`veyra_dom::Tree`].
///
/// The input is decoded per [`decode`] (BOM sniffing, UTF-8/UTF-16), the
/// document-size limit is enforced before parsing, and the tree is built
/// with default limits ([`MAX_TEXT_NODE`], [`MAX_NESTING_DEPTH`]).
///
/// # Errors
///
/// [`HtmlError::DocumentTooLarge`] before parsing; limits violations from
/// the tree builder afterwards. A rejected parse does not return a partial
/// tree.
pub fn parse_document(input: &[u8], origin: Origin) -> Result<veyra_dom::Tree, HtmlError> {
    parse_document_with_options(input, origin, &ParseOptions::default())
}

/// Parses a document with explicit options.
///
/// # Errors
///
/// As [`parse_document`].
pub fn parse_document_with_options(
    input: &[u8],
    origin: Origin,
    options: &ParseOptions,
) -> Result<veyra_dom::Tree, HtmlError> {
    if input.len() > MAX_DOCUMENT_SIZE {
        return Err(HtmlError::DocumentTooLarge {
            max: MAX_DOCUMENT_SIZE,
            actual: input.len(),
        });
    }
    let decoded = decode(input);
    let tree = veyra_dom::Tree::new_document(origin).map_err(|_| HtmlError::NodeLimit {
        max: veyra_dom::DEFAULT_MAX_NODES,
    })?;
    let sink = sink::DomSink::for_document(tree);
    let parser = html5ever::parse_document(sink, opts_for(options));
    // `one()` drives the whole parse and returns the sink's `Output`.
    parser.one(decoded.text)
}

/// Parses an HTML fragment with an existing context element (the
/// `innerHTML` parsing algorithm's shape). Fragment children land under
/// `context` (or its template contents when the context is a template);
/// the caller keeps the `context` id it created and dumps its children.
///
/// # Errors
///
/// As [`parse_document`] (minus the document-size check, which applies to
/// documents).
pub fn parse_fragment(
    input: &str,
    tree: veyra_dom::Tree,
    context: NodeId,
    options: &ParseOptions,
) -> Result<veyra_dom::Tree, HtmlError> {
    let sink = sink::DomSink::for_document(tree);
    let parser = html5ever::driver::parse_fragment_for_element(
        sink,
        opts_for(options),
        context,
        options.scripting_enabled,
        None,
    );
    parser.one(input.to_string())
}

/// Creates a detached element usable as a fragment-parse context.
///
/// # Errors
///
/// [`veyra_dom::DomError`] as from [`veyra_dom::Tree::create_element`].
pub fn context_element(
    tree: &mut veyra_dom::Tree,
    local_name: &str,
) -> Result<NodeId, veyra_dom::DomError> {
    tree.create_element(veyra_dom::QualName::html(local_name), Vec::new())
}

fn opts_for(options: &ParseOptions) -> html5ever::ParseOpts {
    html5ever::ParseOpts {
        tree_builder: html5ever::tree_builder::TreeBuilderOpts {
            scripting_enabled: options.scripting_enabled,
            ..html5ever::tree_builder::TreeBuilderOpts::default()
        },
        ..html5ever::ParseOpts::default()
    }
}

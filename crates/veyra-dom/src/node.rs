//! Node data model: what the arena stores per node.
//!
//! The node kinds cover what HTML parsing produces plus the fragment /
//! document kinds the engine needs (template contents, detached
//! insertion contexts). Namespaces exist because the HTML parser can emit
//! SVG/MathML adjusted names; everything is HTML-namespace for plain
//! documents.

use std::fmt;

/// Element/attribute namespaces the parser can produce (HTML standard
/// §4.2-ish; foreign content adjustments). `Other` exists so the type is
/// closed but future-proof — not constructible through public APIs yet.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// The HTML namespace (the default for parsed elements).
    Html,
    /// The SVG namespace (foreign content).
    Svg,
    /// The MathML namespace (foreign content).
    MathMl,
    /// A namespace outside the three above.
    Other(String),
}

impl Namespace {
    /// The namespace URI string, or `None` for `Namespace::Html`-style
    /// callers that treat "no namespace" as HTML at this stage.
    pub fn uri(&self) -> &str {
        match self {
            Self::Html => "http://www.w3.org/1999/xhtml",
            Self::Svg => "http://www.w3.org/2000/svg",
            Self::MathMl => "http://www.w3.org/1998/Math/MathML",
            Self::Other(uri) => uri,
        }
    }
}

/// A qualified name: optional prefix + local name in a namespace.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QualName {
    /// The prefix (`xlink` in `xlink:href`), when present.
    pub prefix: Option<String>,
    /// The namespace.
    pub namespace: Namespace,
    /// The local name, ASCII-lowercased for HTML-namespace content.
    pub local: String,
}

impl QualName {
    /// Builds an HTML-namespace name without a prefix.
    pub fn html(local: impl Into<String>) -> Self {
        Self {
            prefix: None,
            namespace: Namespace::Html,
            local: local.into(),
        }
    }
}

impl fmt::Display for QualName {
    /// `prefix:local` when a prefix is present, `local` otherwise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.prefix {
            Some(prefix) => write!(f, "{prefix}:{}", self.local),
            None => f.write_str(&self.local),
        }
    }
}

/// An attribute: a qualified name and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    /// The attribute's qualified name.
    pub name: QualName,
    /// The attribute value (may be empty; parser always supplies one).
    pub value: String,
}

/// Quirks mode of a document (CSS decides what it means in Phase 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuirksMode {
    /// Standards mode (`<!DOCTYPE html>` and friends).
    No,
    /// Quirks mode (no/legacy doctype).
    Yes,
    /// Limited-quirks mode.
    Limited,
}

/// The per-node payload stored in the arena.
///
/// Document nodes always carry an origin — construction goes through
/// [`crate::Tree::new_document`], which requires one; that is the
/// compile-time-side enforcement of "origin tag present on every document"
/// (roadmap Phase 2 acceptance criterion 5; dependency-graph invariant 2).
#[derive(Clone, Debug)]
pub enum NodeData {
    /// The document root: origin-tagged, quirks-mode-carrier.
    Document {
        /// The document's origin (parse-derived; never synthesized).
        origin: veyra_url::Origin,
        /// The quirks mode selected by the tree builder.
        quirks: QuirksMode,
    },
    /// A document-type declaration.
    Doctype {
        /// The doctype name (usually `html`).
        name: String,
        /// The public identifier, when present.
        public_id: Option<String>,
        /// The system identifier, when present.
        system_id: Option<String>,
    },
    /// An element with its attributes. `template_contents` points at the
    /// node holding the template's contents (a fragment), per the HTML
    /// template element's separate content document fragment.
    Element {
        /// The element's qualified name.
        name: QualName,
        /// Attributes in insertion order (parser drops duplicates: first
        /// wins, per the tree builder).
        attributes: Vec<Attribute>,
        /// The template content fragment, for `template` elements only.
        template_contents: Option<crate::NodeId>,
    },
    /// Character data (never empty in practice; parser merges runs).
    Text {
        /// The text content.
        data: String,
    },
    /// A comment.
    Comment {
        /// The comment data.
        data: String,
    },
    /// A document fragment: detached insertion context or template
    /// contents container.
    Fragment,
}

impl NodeData {
    /// Short human-readable kind name, used in diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Document { .. } => "document",
            Self::Doctype { .. } => "doctype",
            Self::Element { .. } => "element",
            Self::Text { .. } => "text",
            Self::Comment { .. } => "comment",
            Self::Fragment => "fragment",
        }
    }
}

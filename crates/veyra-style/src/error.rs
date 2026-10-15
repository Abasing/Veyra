//! Typed errors for CSS parsing.
//!
//! Per engineering standards §2, errors are typed at the boundary and
//! every variant is documented; no stringly-typed errors cross crate
//! lines. [`SyntaxKind`] mirrors the upstream tokenizer/parser's
//! `BasicParseErrorKind` and [`SelectorErrorKind`] mirrors the
//! `selectors` crate's `SelectorParseErrorKind` — one-to-one, with the
//! upstream names kept for auditability. The mirrors are exhaustive
//! `match`es over the upstream enums, so a dependency bump that adds a
//! kind *fails the build here* instead of silently collapsing a new
//! failure mode into an existing variant (stronger than a runtime
//! `Unmapped` fallback). Attacker-derived input (identifiers, strings
//! from the stylesheet) never enters error payloads — only parser-kind
//! enumerations and source positions do.

use std::fmt;

/// Where in the input an error was detected.
///
/// Zero-based line and one-based column, matching the CSS Syntax
/// source-location convention and `cssparser::SourceLocation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    /// Zero-based line number.
    pub line: u32,
    /// One-based column number.
    pub column: u32,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A CSS syntax failure kind, one-to-one with the upstream parser's
/// `BasicParseErrorKind` (CSS Syntax 3 error handling plus structural
/// parser errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyntaxKind {
    /// A token that is invalid in this position ended the parse (the
    /// spec's "unexpected token" outcome). The token itself is not
    /// stored, mirroring upstream: it is recoverable from the source.
    UnexpectedToken,
    /// The input ended where more tokens were required.
    EndOfInput,
    /// An at-rule was encountered that is invalid for its position;
    /// the rule is dropped per error recovery.
    AtRuleInvalid,
    /// The body of an at-rule was invalid; the rule is dropped.
    AtRuleBodyInvalid,
    /// A qualified rule was invalid; the rule is dropped.
    QualifiedRuleInvalid,
    /// The block-nesting limit was reached (the parser refuses to
    /// recurse deeper than its internal limit; inputs shaped like
    /// `calc(calc(calc(…)))` cannot exhaust the stack).
    TooManyNestedBlocks,
}

impl From<cssparser::BasicParseErrorKind> for SyntaxKind {
    fn from(kind: cssparser::BasicParseErrorKind) -> Self {
        match kind {
            cssparser::BasicParseErrorKind::UnexpectedToken => SyntaxKind::UnexpectedToken,
            cssparser::BasicParseErrorKind::EndOfInput => SyntaxKind::EndOfInput,
            cssparser::BasicParseErrorKind::AtRuleInvalid => SyntaxKind::AtRuleInvalid,
            cssparser::BasicParseErrorKind::AtRuleBodyInvalid => SyntaxKind::AtRuleBodyInvalid,
            cssparser::BasicParseErrorKind::QualifiedRuleInvalid => {
                SyntaxKind::QualifiedRuleInvalid
            }
            cssparser::BasicParseErrorKind::TooManyNestedBlocks => SyntaxKind::TooManyNestedBlocks,
        }
    }
}

/// A selector parse failure kind, one-to-one with the `selectors`
/// crate's `SelectorParseErrorKind` (Selectors 4 §parsing plus the
/// grammar checks the parser performs). Payloads (attribute tokens,
/// pseudo names) are dropped at the mapping: the kind enumerates the
/// failure class, the source text names the culprit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectorErrorKind {
    /// An attribute selector had no qualified name (`[|x]`-shaped).
    NoQualifiedNameInAttributeSelector,
    /// The selector list was empty.
    EmptySelector,
    /// A combinator or descendant position was followed by nothing
    /// that can complete the complex selector.
    DanglingCombinator,
    /// A compound selector was required but a combinator or end was
    /// found (e.g. inside `:is()`).
    NonCompoundSelector,
    /// A pseudo-element followed `::slotted()` that cannot.
    NonPseudoElementAfterSlotted,
    /// An unsupported pseudo-element followed `::slotted()`.
    InvalidPseudoElementAfterSlotted,
    /// A pseudo-element appeared inside `:where()`, which cannot
    /// contain them.
    InvalidPseudoElementInsideWhere,
    /// The parser reached a state the grammar forbids (internal
    /// invariant of the upstream parser).
    InvalidState,
    /// An attribute selector contained an unexpected token.
    UnexpectedTokenInAttributeSelector,
    /// A pseudo-element position lacked the required second colon.
    PseudoElementExpectedColon,
    /// A functional pseudo-element lacked its argument.
    PseudoElementExpectedIdent,
    /// A pseudo-element position lacked its name.
    NoIdentForPseudo,
    /// A pseudo-class/element this parser does not support was used;
    /// the enclosing rule is dropped per Selectors 4 error handling.
    UnsupportedPseudoClassOrElement,
    /// An identifier was found where the grammar required something
    /// else.
    UnexpectedIdent,
    /// A namespace prefix was used with no mapping, or `@namespace`
    /// syntax was violated.
    ExpectedNamespace,
    /// An attribute selector lacked the `|` of a namespace form.
    ExpectedBarInAttr,
    /// An attribute selector value was malformed.
    BadValueInAttr,
    /// A namespaced attribute name was malformed.
    InvalidQualNameInAttr,
    /// A type selector or attribute name had `|` without a valid
    /// namespace form.
    ExplicitNamespaceUnexpectedToken,
    /// A class selector (`.x`) lacked its identifier.
    ClassNeedsIdent,
}

impl From<selectors::parser::SelectorParseErrorKind> for SelectorErrorKind {
    fn from(kind: selectors::parser::SelectorParseErrorKind) -> Self {
        match kind {
            selectors::parser::SelectorParseErrorKind::NoQualifiedNameInAttributeSelector => {
                SelectorErrorKind::NoQualifiedNameInAttributeSelector
            }
            selectors::parser::SelectorParseErrorKind::EmptySelector => {
                SelectorErrorKind::EmptySelector
            }
            selectors::parser::SelectorParseErrorKind::DanglingCombinator => {
                SelectorErrorKind::DanglingCombinator
            }
            selectors::parser::SelectorParseErrorKind::NonCompoundSelector => {
                SelectorErrorKind::NonCompoundSelector
            }
            selectors::parser::SelectorParseErrorKind::NonPseudoElementAfterSlotted => {
                SelectorErrorKind::NonPseudoElementAfterSlotted
            }
            selectors::parser::SelectorParseErrorKind::InvalidPseudoElementAfterSlotted => {
                SelectorErrorKind::InvalidPseudoElementAfterSlotted
            }
            selectors::parser::SelectorParseErrorKind::InvalidPseudoElementInsideWhere => {
                SelectorErrorKind::InvalidPseudoElementInsideWhere
            }
            selectors::parser::SelectorParseErrorKind::InvalidState => {
                SelectorErrorKind::InvalidState
            }
            selectors::parser::SelectorParseErrorKind::UnexpectedTokenInAttributeSelector => {
                SelectorErrorKind::UnexpectedTokenInAttributeSelector
            }
            selectors::parser::SelectorParseErrorKind::PseudoElementExpectedColon => {
                SelectorErrorKind::PseudoElementExpectedColon
            }
            selectors::parser::SelectorParseErrorKind::PseudoElementExpectedIdent => {
                SelectorErrorKind::PseudoElementExpectedIdent
            }
            selectors::parser::SelectorParseErrorKind::NoIdentForPseudo => {
                SelectorErrorKind::NoIdentForPseudo
            }
            selectors::parser::SelectorParseErrorKind::UnsupportedPseudoClassOrElement => {
                SelectorErrorKind::UnsupportedPseudoClassOrElement
            }
            selectors::parser::SelectorParseErrorKind::UnexpectedIdent => {
                SelectorErrorKind::UnexpectedIdent
            }
            selectors::parser::SelectorParseErrorKind::ExpectedNamespace => {
                SelectorErrorKind::ExpectedNamespace
            }
            selectors::parser::SelectorParseErrorKind::ExpectedBarInAttr => {
                SelectorErrorKind::ExpectedBarInAttr
            }
            selectors::parser::SelectorParseErrorKind::BadValueInAttr => {
                SelectorErrorKind::BadValueInAttr
            }
            selectors::parser::SelectorParseErrorKind::InvalidQualNameInAttr => {
                SelectorErrorKind::InvalidQualNameInAttr
            }
            selectors::parser::SelectorParseErrorKind::ExplicitNamespaceUnexpectedToken => {
                SelectorErrorKind::ExplicitNamespaceUnexpectedToken
            }
            selectors::parser::SelectorParseErrorKind::ClassNeedsIdent => {
                SelectorErrorKind::ClassNeedsIdent
            }
        }
    }
}

/// The single error type of [`Stylesheet::parse`](crate::Stylesheet::parse)
/// and the other Veyra CSS entry points.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The input exceeded the configured stylesheet size limit before
    /// parsing began. Normative default: [`MAX_STYLESHEET_BYTES`](crate::MAX_STYLESHEET_BYTES).
    TooLarge {
        /// The configured maximum input length in bytes.
        max: usize,
        /// The actual input length in bytes.
        actual: usize,
    },
    /// The input is not valid UTF-8. Batch 1 parses UTF-8 only; the
    /// encoding-detection stack (BOM-based UTF-16, `@charset`) is the
    /// recorded charset work item — a misparse would be worse than a
    /// rejection, so non-UTF-8 input is a hard typed error.
    NonUtf8,
    /// A stylesheet-level syntax failure at the given location.
    Syntax {
        /// Where the failure was detected.
        location: SourceLocation,
        /// What kind of failure.
        kind: SyntaxKind,
    },
    /// A selector failed to parse; the enclosing rule is dropped per
    /// Selectors 4 error handling. The reported location is the rule
    /// start.
    Selector {
        /// Where the enclosing rule started.
        location: SourceLocation,
        /// What kind of selector failure.
        kind: SelectorErrorKind,
    },
    /// A parse error reported through the upstream `Custom` channel
    /// with no Veyra mapping (Veyra's own custom errors are mapped
    /// before this point). Not attacker-derived data: the string is
    /// the upstream error's `Debug` rendering, and the variant exists
    /// so unmapped channels fail loudly instead of silently.
    Unmapped {
        /// `Debug` rendering of the unmapped upstream error.
        upstream: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::TooLarge { max, actual } => {
                write!(
                    f,
                    "stylesheet too large: {actual} bytes exceeds the {max} byte limit"
                )
            }
            ParseError::NonUtf8 => write!(f, "stylesheet is not valid UTF-8"),
            ParseError::Syntax { location, kind } => {
                write!(f, "CSS syntax error at {location}: {kind:?}")
            }
            ParseError::Selector { location, kind } => {
                write!(
                    f,
                    "selector parse error at {location} (rule dropped): {kind:?}"
                )
            }
            ParseError::Unmapped { upstream } => {
                write!(f, "unmapped upstream parse error: {upstream}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

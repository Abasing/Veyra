//! Selector parsing: the `selectors` crate's parser driven with Veyra's
//! [`SelectorImpl`], wrapped in a Veyra-owned [`SelectorList`].
//!
//! The wrapper hides the upstream types entirely: callers parse from a
//! CSS fragment and serialize back, and the cascade batch matches
//! against `veyra-dom` elements through this crate. Parse errors map
//! one-to-one onto [`SelectorErrorKind`](crate::SelectorErrorKind).
//!
//! Enabled grammar (Selectors 4 working set for Phase 3): complex
//! selectors with all four combinators, attribute selectors,
//! namespaced forms (parsed; prefix resolution arrives with
//! `@namespace` application), tree-structural pseudo-classes including
//! `:nth-child(an+b of S)`, `:is()`/`:where()`/`:not()`/`:has()`, and
//! the Veyra [`PseudoClass`]/[`PseudoElement`] sets. Deferred: `&`
//! parent selectors (CSS nesting), `::slotted()`/`::part()` (shadow
//! DOM), and relative-selector forms outside `:has()`.

use std::fmt;

use cssparser::{ParseError as CssParseError, Parser, ToCss};
use selectors::parser::{
    ParseRelative, Parser as SelectorParser, SelectorList as UpstreamList, SelectorParseErrorKind,
};

use crate::error::{ParseError, SelectorErrorKind, SourceLocation, SyntaxKind};
use crate::selector_impl::{PseudoClass, PseudoElement, VeyraSelectorImpl};

/// The parser configuration the `selectors` crate drives.
#[derive(Debug, Default)]
struct VeyraSelectorParser;

impl<'i> SelectorParser<'i> for VeyraSelectorParser {
    type Impl = VeyraSelectorImpl;
    type Error = SelectorErrorKind;

    fn parse_non_ts_pseudo_class(
        &self,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoClass, CssParseError<SelectorErrorKind>> {
        parse_pseudo_class_name(&name.to_ascii_lowercase()).ok_or_else(|| {
            CssParseError::custom(SelectorParseErrorKind::UnsupportedPseudoClassOrElement)
        })
    }

    fn parse_non_ts_functional_pseudo_class(
        &self,
        name: cssparser::CowRcStr<'i>,
        input: &mut Parser<'i>,
        _after_part: bool,
    ) -> Result<PseudoClass, CssParseError<SelectorErrorKind>> {
        // The parser is positioned inside the function's block; the
        // argument grammar is Selectors 4's. Pseudo names are
        // ASCII case-insensitive.
        match name.to_ascii_lowercase().as_str() {
            "lang" => {
                let mut ranges = Vec::new();
                loop {
                    let range = input
                        .try_parse(|input| parse_language_range(input))
                        .map_err(|_| {
                            CssParseError::custom(
                                SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
                            )
                        })?;
                    ranges.push(range);
                    match input.next() {
                        Ok(&cssparser::Token::Comma) => continue,
                        Ok(_) => {
                            return Err(CssParseError::custom(
                                SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
                            ));
                        }
                        // End of the argument block.
                        Err(_) => break,
                    }
                }
                Ok(PseudoClass::Lang(ranges))
            }
            "dir" => {
                let direction = input.expect_ident()?.clone();
                Ok(PseudoClass::Dir(Box::from(&*direction)))
            }
            _ => Err(CssParseError::custom(
                SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
            )),
        }
    }

    fn parse_pseudo_element(
        &self,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoElement, CssParseError<SelectorErrorKind>> {
        parse_pseudo_element_name(&name.to_ascii_lowercase()).ok_or_else(|| {
            CssParseError::custom(SelectorParseErrorKind::UnsupportedPseudoClassOrElement)
        })
    }

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_has(&self) -> bool {
        true
    }

    fn parse_nth_child_of(&self) -> bool {
        true
    }
}

/// Parses one `<language-range>`: an identifier or a string
/// (`:lang("en-GB")` is allowed for ranges that are not valid
/// identifiers).
fn parse_language_range<'i>(
    input: &mut Parser<'i>,
) -> Result<Box<str>, cssparser::BasicParseError> {
    let token = input.next()?;
    match token {
        cssparser::Token::Ident(value) => Ok(Box::<str>::from(value.as_ref())),
        cssparser::Token::QuotedString(value) => Ok(Box::<str>::from(value.as_ref())),
        _ => Err(cssparser::BasicParseError::unexpected_token()),
    }
}

/// Maps a non-tree-structural leaf pseudo-class name to the
/// [`PseudoClass`] model. Functional forms are handled in
/// `parse_non_ts_functional_pseudo_class`.
fn parse_pseudo_class_name(name: &str) -> Option<PseudoClass> {
    Some(match name {
        "link" => PseudoClass::Link,
        "visited" => PseudoClass::Visited,
        "active" => PseudoClass::Active,
        "hover" => PseudoClass::Hover,
        "focus" => PseudoClass::Focus,
        "focus-visible" => PseudoClass::FocusVisible,
        "focus-within" => PseudoClass::FocusWithin,
        "enabled" => PseudoClass::Enabled,
        "disabled" => PseudoClass::Disabled,
        "checked" => PseudoClass::Checked,
        "indeterminate" => PseudoClass::Indeterminate,
        "placeholder-shown" => PseudoClass::PlaceholderShown,
        "autofill" => PseudoClass::Autofill,
        "read-only" => PseudoClass::ReadOnly,
        "read-write" => PseudoClass::ReadWrite,
        "required" => PseudoClass::Required,
        "optional" => PseudoClass::Optional,
        "valid" => PseudoClass::Valid,
        "invalid" => PseudoClass::Invalid,
        "user-valid" => PseudoClass::UserValid,
        "user-invalid" => PseudoClass::UserInvalid,
        "target" => PseudoClass::Target,
        _ => return None,
    })
}

/// Maps a pseudo-element name to the [`PseudoElement`] model.
fn parse_pseudo_element_name(name: &str) -> Option<PseudoElement> {
    Some(match name {
        "before" => PseudoElement::Before,
        "after" => PseudoElement::After,
        "first-line" => PseudoElement::FirstLine,
        "first-letter" => PseudoElement::FirstLetter,
        "selection" => PseudoElement::Selection,
        "marker" => PseudoElement::Marker,
        "placeholder" => PseudoElement::Placeholder,
        "backdrop" => PseudoElement::Backdrop,
        _ => return None,
    })
}

/// A parsed selector list (Selectors 4 "complex selector list").
///
/// Cheap to clone (the upstream representation is an arc); the cascade
/// batch matches elements against it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorList {
    upstream: UpstreamList<VeyraSelectorImpl>,
}

impl SelectorList {
    /// Parses a selector list from `text` (a full complex-selector
    /// list, as in a style rule prelude).
    ///
    /// # Errors
    ///
    /// [`ParseError::Selector`] when the grammar is violated or a
    /// pseudo is unsupported; [`ParseError::Syntax`] for tokenizer
    /// failures. The caller (style rule parsing) drops the enclosing
    /// rule per Selectors 4 error handling.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut input = Parser::new(text);
        Self::parse_from(&mut input)
    }

    /// Parses from an in-progress CSS parser (style rule preludes are
    /// consumed this way so the rule body follows).
    pub(crate) fn parse_from(input: &mut Parser<'_>) -> Result<Self, ParseError> {
        let parser = VeyraSelectorParser;
        let location = input.current_source_location();
        UpstreamList::parse(&parser, input, ParseRelative::No)
            .map(|upstream| SelectorList { upstream })
            .map_err(|error| map_selector_error(error, location))
    }

    /// Serializes to canonical CSS (Selectors 4 serialization: single
    /// spaces around combinators, normalized pseudos).
    pub fn to_css_string(&self) -> String {
        let mut out = String::new();
        let _ = self.upstream.to_css(&mut out);
        out
    }

    /// The number of complex selectors in the list.
    pub fn len(&self) -> usize {
        self.upstream.len()
    }

    /// Whether the list is empty (unreachable via parsing — an empty
    /// list is a parse error — but representable in tests).
    pub fn is_empty(&self) -> bool {
        self.upstream.len() == 0
    }
}

impl fmt::Display for SelectorList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css_string())
    }
}

/// Maps an upstream selector parse error onto the Veyra error model,
/// preferring the error's own source location when it is a tokenizer
/// failure and the rule-start location for grammar failures (the
/// upstream grammar errors report positions inside the fragment;
/// Veyra reports the rule start, where the drop decision applies).
fn map_selector_error(
    error: CssParseError<SelectorErrorKind>,
    rule_start: cssparser::SourceLocation,
) -> ParseError {
    let location = SourceLocation {
        line: rule_start.line,
        column: rule_start.column,
    };
    match error.kind {
        cssparser::ParseErrorKind::Basic(basic) => ParseError::Syntax {
            location,
            kind: SyntaxKind::from(basic),
        },
        // The upstream parser converts `SelectorParseErrorKind` through
        // Veyra's `From` impl; `Custom` payloads are already
        // `SelectorErrorKind`. cssparser 0.38's `ParseError` carries no
        // location — the caller's rule-start position is reported.
        cssparser::ParseErrorKind::Custom(kind) => ParseError::Selector { location, kind },
    }
}

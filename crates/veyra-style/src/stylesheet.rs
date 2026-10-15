//! The [`Stylesheet`] type: bytes in, a rule tree out.
//!
//! Entry points: [`Stylesheet::parse`] (UTF-8 text with optional BOM)
//! and [`Stylesheet::parse_bytes`] (the explicit byte form used by the
//! navigation pipeline). Both enforce the [`Limits`] stylesheet size
//! limit before any parsing work. The parse itself is cssparser's
//! stylesheet-level algorithm (top-level `@charset` consumption, CDO/
//! CDC skipping, per-rule error recovery) driven by Veyra's rule
//! adapter.

use crate::error::ParseError;
use crate::limits::Limits;
use crate::rules::CssRule;

/// A parsed stylesheet: the rule list in source order.
///
/// The origin (user-agent/user/author) is a cascade-batch concept and
/// lives with the style set, not on the parsed sheet; parsing is
/// origin-agnostic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stylesheet {
    rules: Vec<CssRule>,
}

/// The UTF-8 byte-order mark.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

impl Stylesheet {
    /// Parses a stylesheet from UTF-8 text.
    ///
    /// Parsing is error-tolerant by design: malformed rules are
    /// dropped per CSS Syntax error recovery and the surviving rules
    /// are returned. The only failures are pre-parse rejections
    /// (limits, encoding).
    ///
    /// # Errors
    ///
    /// [`ParseError::TooLarge`] over the configured byte limit;
    /// [`ParseError::NonUtf8`] for non-UTF-8 input (the charset stack
    /// is a recorded later work item — see the crate docs).
    pub fn parse(css: &str, limits: &Limits) -> Result<Self, ParseError> {
        check_size(css.len(), limits)?;
        // Deeply nested blocks are bounded by cssparser's internal
        // nesting limit (see the crate docs' security posture).
        let mut input = cssparser::Parser::new(css);
        let rules = crate::rules::parse_stylesheet_rules(&mut input);
        Ok(Stylesheet { rules })
    }

    /// Parses a stylesheet from bytes: a UTF-8 BOM is consumed, any
    /// other BOM (UTF-16) is a typed rejection — a misparse would be
    /// worse (see the crate docs' encoding note).
    ///
    /// # Errors
    ///
    /// [`ParseError::TooLarge`], [`ParseError::NonUtf8`].
    pub fn parse_bytes(bytes: &[u8], limits: &Limits) -> Result<Self, ParseError> {
        check_size(bytes.len(), limits)?;
        let without_bom = if bytes.starts_with(UTF8_BOM) {
            &bytes[UTF8_BOM.len()..]
        } else {
            bytes
        };
        if starts_with_utf16_bom(without_bom) {
            return Err(ParseError::NonUtf8);
        }
        let text = std::str::from_utf8(without_bom).map_err(|_| ParseError::NonUtf8)?;
        Self::parse(text, limits)
    }

    /// The rules, in source order.
    pub fn rules(&self) -> &[CssRule] {
        &self.rules
    }

    /// The number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the stylesheet has no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Serializes to canonical CSS, one rule per line-grouped block.
    pub fn to_css_string(&self) -> String {
        let mut out = String::new();
        for (index, rule) in self.rules.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(&rule.to_css_string());
        }
        out
    }
}

impl std::fmt::Display for Stylesheet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_css_string())
    }
}

/// Enforces the stylesheet size limit before parsing.
fn check_size(length: usize, limits: &Limits) -> Result<(), ParseError> {
    if length > limits.max_stylesheet_bytes {
        return Err(ParseError::TooLarge {
            max: limits.max_stylesheet_bytes,
            actual: length,
        });
    }
    Ok(())
}

/// Whether the bytes begin with a UTF-16 byte-order mark (either
/// endianness).
fn starts_with_utf16_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFE, 0xFF]) || bytes.starts_with(&[0xFF, 0xFE])
}

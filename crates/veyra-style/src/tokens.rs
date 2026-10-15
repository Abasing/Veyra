//! Owned, canonically-serializable token trees for values the engine
//! does not interpret yet.
//!
//! Declaration values, at-rule preludes, and unknown at-rule bodies are
//! retained as a [`RetainedValue`] instead of being discarded, because
//! CSS custom properties make "unknown value" a *valid* state and the
//! cascade batch needs the raw tokens. The tree stores each token's
//! canonical serialization (`cssparser`'s `Token::to_css`) plus its
//! [`TokenSerializationType`], which is exactly what the CSS Syntax
//! canonical-serialization algorithm needs: whitespace between tokens
//! is reconstructed from the stored types, and `/**/` separators are
//! inserted where two adjacent tokens would otherwise merge on reparse.
//!
//! Conformance: the round-trip behavior (parse → serialize → parse
//! equality) is pinned by the vendored `css-parsing-tests`
//! `component_value_list.json` corpus (tests/corpus_tokens.rs); the
//! serializer structure mirrors `cssparser`'s own corpus harness.

use std::fmt;

use cssparser::{Parser, ToCss, Token, TokenSerializationType};

/// An owned component-value tree (CSS Syntax "component value list"),
/// serializable back to canonical CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedValue {
    tokens: Vec<RetainedToken>,
}

/// One owned component value: the token's canonical serialization, its
/// serialization type (for whitespace reconstruction), and — for
/// function/block tokens — the nested contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedToken {
    /// The token's own canonical text (`Token::to_css` output). For
    /// block-opening tokens this is the opener only (`"f("`, `"{"`,
    /// `"["`, `"("`); the nested contents and the closer are written
    /// from `block` and the opener kind.
    serialized: String,
    /// The token's serialization type, needed to decide separators.
    serialization_type: TokenSerializationType,
    /// For `Function`, `ParenthesisBlock`, `SquareBracketBlock`, and
    /// `CurlyBracketBlock` tokens: the nested component values.
    block: Option<Box<RetainedValue>>,
}

impl RetainedToken {
    /// Whether this token opens a block, and the block-closing token's
    /// text, per the tokenizer's pairing (`(` → `)`, `[` → `]`,
    /// `{` → `}`).
    ///
    /// Block openers serialize to text ending in the opening bracket
    /// (`"f("`, `"("`), so the suffix identifies the pairing.
    fn closer(&self) -> Option<&'static str> {
        self.block.as_ref()?;
        if self.serialized.ends_with('(') {
            Some(")")
        } else if self.serialized.ends_with('[') {
            Some("]")
        } else if self.serialized.ends_with('{') {
            Some("}")
        } else {
            None
        }
    }
}

impl RetainedValue {
    /// Retains the remaining component values of the current block or
    /// stylesheet, consuming the parser to its end (or the enclosing
    /// block boundary).
    pub(crate) fn parse(input: &mut Parser<'_>) -> RetainedValue {
        let tokens = retain_component_values(input);
        RetainedValue { tokens }
    }

    /// Wraps already-retained tokens (the declaration parser retains
    /// value tokens one at a time to interleave `!important` handling).
    pub(crate) fn from_tokens(tokens: Vec<RetainedToken>) -> RetainedValue {
        RetainedValue { tokens }
    }

    /// Serializes to the canonical CSS form with boundary whitespace
    /// trimmed (the engine/CSSOM style: `name: value` rendering, where
    /// the value's leading/trailing whitespace tokens are insignificant).
    ///
    /// The untrimmed [`to_css_string`](Self::to_css_string) is the
    /// conformance-faithful form pinned by the vendored corpus; this
    /// trimmed form is what rule/declaration serialization emits.
    pub fn to_css_string_trimmed(&self) -> String {
        let mut text = self.to_css_string();
        while text.starts_with(' ') {
            text.remove(0);
        }
        while text.ends_with(' ') {
            text.pop();
        }
        text
    }

    /// Serializes to the canonical CSS form (CSS Syntax 3
    /// serialization): single-space whitespace tokens, `/**/` between
    /// tokens that would otherwise merge, comments dropped.
    pub fn to_css_string(&self) -> String {
        let mut out = String::new();
        self.write_canonical(TokenSerializationType::Nothing, &mut out);
        out
    }

    /// Writes the canonical serialization.
    ///
    /// `incoming` is the serialization type of the token written just
    /// before this tree (the opener's type for nested contents, per
    /// the reference algorithm). The returned type is the opener's
    /// type when a block was written last — matching the reference
    /// algorithm, which does not re-check separators against closers
    /// (a closer can never merge with the following token: the merge
    /// risk exists between *openers* and their neighbors).
    pub(crate) fn write_canonical(
        &self,
        incoming: TokenSerializationType,
        out: &mut String,
    ) -> TokenSerializationType {
        let mut last = incoming;
        for token in &self.tokens {
            if last.needs_separator_when_before(token.serialization_type) {
                out.push_str("/**/");
            }
            out.push_str(&token.serialized);
            last = token.serialization_type;
            if let Some(block) = &token.block {
                block.write_canonical(token.serialization_type, out);
                if let Some(closer) = token.closer() {
                    out.push_str(closer);
                }
            }
        }
        last
    }
}

impl fmt::Display for RetainedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css_string())
    }
}

/// Serializes a CSS identifier with escapes (`serialize_identifier`),
/// for at-rule/animation names re-emitted outside token position.
pub(crate) fn serialize_identifier_css(name: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_identifier(name, &mut out);
    out
}

/// Serializes a CSS string literal with escapes (`serialize_string`).
pub(crate) fn serialize_string_css(value: &str) -> String {
    let mut out = String::new();
    let _ = cssparser::serialize_string(value, &mut out);
    out
}

/// Retains the remaining component values, recursing into function and
/// block tokens. Comments are dropped (canonical form); whitespace
/// tokens are kept as single spaces.
fn retain_component_values(input: &mut Parser<'_>) -> Vec<RetainedToken> {
    let mut tokens = Vec::new();
    while let Ok(token) = input.next_including_whitespace() {
        tokens.push(retain_one_token(token.clone(), input));
    }
    tokens
}

/// Retains one token; block-opening tokens recurse via
/// `Parser::parse_nested_block`.
pub(crate) fn retain_one_token(token: Token<'_>, input: &mut Parser<'_>) -> RetainedToken {
    let serialization_type = token.serialization_type();
    match token {
        Token::Function(_)
        | Token::ParenthesisBlock
        | Token::SquareBracketBlock
        | Token::CurlyBracketBlock => {
            let serialized = token.to_css_string();
            let block: Result<_, cssparser::ParseError<()>> = input.parse_nested_block(|input| {
                Ok(RetainedValue {
                    tokens: retain_component_values(input),
                })
            });
            let block = block.ok().map(Box::new);
            RetainedToken {
                serialized,
                serialization_type,
                block,
            }
        }
        _ => RetainedToken {
            serialized: token.to_css_string(),
            serialization_type,
            block: None,
        },
    }
}

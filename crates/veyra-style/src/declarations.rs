//! Declaration lists: `name: value !important` pairs inside style
//! rule and at-rule bodies.
//!
//! Values are retained ([`RetainedValue`]) — typed property values are
//! the cascade batch's job. The declaration *name* is parsed here
//! (identifiers only, per CSS Syntax), the `!important` flag follows
//! the spec grammar (a `!` followed by the `important` ident, allowed
//! only at the end of the value), and everything else obeys the
//! declaration-list error-recovery rules: a malformed declaration is
//! dropped, its neighbors survive. Conformance is pinned by the
//! vendored `css-parsing-tests` `declaration_list.json` /
//! `one_declaration.json` corpora.

use std::fmt;

use cssparser::{
    AtRuleParser, BasicParseErrorKind, DeclarationParser, Parser, ParserState, QualifiedRuleParser,
    RuleBodyItemParser, RuleBodyParser, parse_important,
};

use crate::tokens::{RetainedValue, retain_one_token};

/// One declaration: a property/custom-property name, its retained
/// value tokens, and the `!important` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The declared name (a property or custom-property identifier).
    pub name: String,
    /// The retained value tokens (canonical-serializable).
    pub value: RetainedValue,
    /// Whether the declaration carried a trailing `!important`.
    pub important: bool,
}

/// One item of a generic declaration-list block: either a parsed
/// declaration or an at-rule/qualified-rule *shape* (CSS Syntax's
/// block-contents algorithm reports the shape; the engine drops
/// non-declaration items from rule bodies — declarations cannot nest
/// rules in batch 1 — while the conformance corpus pins the shapes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockItem {
    /// A parsed declaration.
    Declaration(Declaration),
    /// An at-rule shape: name, retained prelude, optional retained
    /// block.
    AtRuleShape {
        /// The at-rule name (lowercased).
        name: String,
        /// The retained prelude.
        prelude: RetainedValue,
        /// The retained block, if present.
        block: Option<RetainedValue>,
    },
    /// A qualified-rule shape: retained prelude and block.
    QualifiedRuleShape {
        /// The retained prelude.
        prelude: RetainedValue,
        /// The retained block.
        block: RetainedValue,
    },
}

/// A parsed declaration list, in source order (order matters for the
/// cascade's later-wins rule within an origin).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeclarationList {
    declarations: Vec<Declaration>,
}

impl DeclarationList {
    /// The declarations, in source order.
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    /// The number of declarations.
    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// Parses a declaration list from `text` (as in a style rule
    /// block).
    ///
    /// Parsing a declaration list has no top-level failure mode:
    /// malformed declarations are dropped per CSS Syntax error
    /// recovery, so a wholly malformed input parses to an empty list.
    /// Deeply nested blocks are bounded by the parser's internal
    /// nesting limit (block-shaped items simply fail to parse and are
    /// dropped).
    pub fn parse(text: &str) -> Self {
        let mut input = Parser::new(text);
        Self::parse_block_contents(&mut input)
    }

    /// Parses the declaration list of an open block (style rule
    /// bodies).
    pub(crate) fn parse_block_contents(input: &mut Parser<'_>) -> Self {
        DeclarationList {
            declarations: parse_block_contents_with_markers(input)
                .into_iter()
                .filter_map(|item| match item {
                    Ok(BlockItem::Declaration(declaration)) => Some(declaration),
                    // Non-declaration shapes are invalid in rule
                    // bodies (batch 1 has no nesting); dropped per
                    // error recovery.
                    _ => None,
                })
                .collect(),
        }
    }

    /// Serializes to canonical CSS: `name: value;` per declaration,
    /// space-separated (`name: value !important` where flagged), with
    /// a trailing semicolon after the last declaration.
    pub fn to_css_string(&self) -> String {
        let mut out = String::new();
        for (index, declaration) in self.declarations.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            out.push_str(&declaration.name);
            out.push_str(": ");
            out.push_str(&declaration.value.to_css_string_trimmed());
            if declaration.important {
                out.push_str(" !important");
            }
            out.push(';');
        }
        out
    }
}

impl fmt::Display for DeclarationList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css_string())
    }
}

/// Parses an open declaration-list block, keeping dropped-declaration
/// markers for the vendored conformance corpus (see
/// `parse_nested_rule_list_with_markers`).
pub(crate) fn parse_block_contents_with_markers(
    input: &mut Parser<'_>,
) -> Vec<Result<BlockItem, ()>> {
    let mut parser = DeclarationListParser;
    RuleBodyParser::new(input, &mut parser)
        .map(|item| item.map_err(|_| ()))
        .collect()
}

/// The cssparser parser adapter for declaration bodies.
pub(crate) struct DeclarationListParser;

impl<'i> DeclarationParser<'i> for DeclarationListParser {
    type Declaration = BlockItem;
    type Error = ();

    fn parse_value(
        &mut self,
        name: cssparser::CowRcStr<'i>,
        input: &mut Parser<'i>,
        _declaration_start: &ParserState,
    ) -> Result<BlockItem, cssparser::ParseError<()>> {
        let mut value = Vec::new();
        let mut important = false;
        loop {
            let start = input.state();
            let Ok(token) = input.next_including_whitespace() else {
                break;
            };
            let mut token = token.clone();
            // `!important` is only valid at the end of the value; a
            // `!` elsewhere cannot be part of the value grammar. Try
            // the spec form first; a mid-value `!` is retained as the
            // delimiter token it lexes as (the recovery shape the
            // reference corpus pins).
            if token == cssparser::Token::Delim('!') {
                input.reset(&start);
                if parse_important(input).is_ok() && input.is_exhausted() {
                    important = true;
                    break;
                }
                input.reset(&start);
                let Ok(next) = input.next_including_whitespace() else {
                    break;
                };
                token = next.clone();
            }
            value.push(retain_one_token(token, input));
        }
        Ok(BlockItem::Declaration(Declaration {
            name: name.to_string(),
            value: RetainedValue::from_tokens(value),
            important,
        }))
    }
}

impl<'i> AtRuleParser<'i> for DeclarationListParser {
    type Prelude = (String, RetainedValue);
    type AtRule = BlockItem;
    type Error = ();

    fn parse_prelude(
        &mut self,
        name: cssparser::CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, cssparser::ParseError<()>> {
        // Declaration lists contain no valid at-rules, but at-rule
        // *shapes* still delimit at `{}` blocks (cssparser handles the
        // delimiting); the shape is retained for the conformance
        // corpus and dropped by the engine model. `@charset` is never
        // a real rule anywhere (the stylesheet driver consumes it).
        if name.eq_ignore_ascii_case("charset") {
            return Err(cssparser::ParseError::from_basic_kind(
                BasicParseErrorKind::AtRuleInvalid,
            ));
        }
        Ok((name.to_ascii_lowercase(), RetainedValue::parse(input)))
    }

    fn rule_without_block(
        &mut self,
        (name, prelude): Self::Prelude,
        _start: &ParserState,
    ) -> Result<BlockItem, ()> {
        Ok(BlockItem::AtRuleShape {
            name,
            prelude,
            block: None,
        })
    }

    fn parse_block(
        &mut self,
        (name, prelude): Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<BlockItem, cssparser::ParseError<()>> {
        Ok(BlockItem::AtRuleShape {
            name,
            prelude,
            block: Some(RetainedValue::parse(input)),
        })
    }
}

impl<'i> QualifiedRuleParser<'i> for DeclarationListParser {
    type Prelude = RetainedValue;
    type QualifiedRule = BlockItem;
    type Error = ();

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, cssparser::ParseError<()>> {
        // Qualified-rule shapes in declaration contexts are retained
        // (conformance corpus) and dropped by the engine model.
        Ok(RetainedValue::parse(input))
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<BlockItem, cssparser::ParseError<()>> {
        Ok(BlockItem::QualifiedRuleShape {
            prelude,
            block: RetainedValue::parse(input),
        })
    }
}

impl RuleBodyItemParser<'_, BlockItem, ()> for DeclarationListParser {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

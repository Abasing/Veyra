//! The rule model: what a stylesheet contains.
//!
//! [`CssRule`] covers the Phase 3 working set of at-rules — `@media`,
//! `@supports`, `@import`, `@font-face`, `@keyframes`, `@layer`,
//! `@namespace` — plus style rules and a parse-and-retain fallback for
//! unknown at-rules (they must round-trip losslessly; dropping rules
//! the engine does not understand yet is not error recovery, it is
//! data loss). Preludes the engine does not interpret yet are
//! [`RetainedValue`]s.
//!
//! Error recovery is the CSS Syntax algorithm at stylesheet level:
//! invalid rules are dropped, the parser resynchronizes at the next
//! rule boundary, and the surviving rules keep source order. What the
//! corpus pins is that recovery shape (vendored `stylesheet.json` /
//! `rule_list.json` / `one_rule.json`).

use std::fmt;

use cssparser::{
    AtRuleParser, BasicParseErrorKind, ParseError as CssParseError, Parser, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser,
};

use crate::declarations::DeclarationList;
use crate::selectors::SelectorList;
use crate::tokens::{RetainedValue, serialize_identifier_css, serialize_string_css};

/// A style rule: a selector list plus its declaration block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRule {
    /// The parsed selector list (Selectors 4).
    pub selectors: SelectorList,
    /// The declaration block.
    pub declarations: DeclarationList,
}

/// One `@keyframes` step: its selector (positions) and declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyframe {
    /// The keyframe selector (one or more positions).
    pub selectors: Vec<KeyframeSelector>,
    /// The declarations at this position.
    pub declarations: DeclarationList,
}

/// A keyframe position selector: `from` (0%), `to` (100%), or a
/// percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyframeSelector {
    /// `from` — 0%.
    From,
    /// `to` — 100%.
    To,
    /// A percentage position (integral; the cascade batch's matching
    /// layer compares these directly).
    Percentage(u16),
}

/// `@layer` rule: either a statement (`@layer a, b;`) naming layers,
/// or a block (`@layer name { … }`) containing nested rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerRule {
    /// The statement form: layer names declared, no rules.
    Statement(Vec<String>),
    /// The block form: one named (or anonymous) layer containing rules.
    Block {
        /// The layer name, if any (`@layer { … }` is anonymous).
        name: Option<String>,
        /// The nested rules.
        rules: Vec<CssRule>,
    },
}

/// A stylesheet rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssRule {
    /// A qualified rule: selector list + declaration block.
    Style(StyleRule),
    /// `@media` — prelude retained (media-query parsing is its own
    /// batch), body nested rules.
    Media {
        /// The retained media-query prelude.
        prelude: RetainedValue,
        /// The nested rules.
        rules: Vec<CssRule>,
    },
    /// `@supports` — prelude retained (condition parsing is its own
    /// batch), body nested rules.
    Supports {
        /// The retained supports-condition prelude.
        prelude: RetainedValue,
        /// The nested rules.
        rules: Vec<CssRule>,
    },
    /// `@import` — the target URL (string or `url()` token value) and
    /// the retained layer/media prelude. No block.
    Import {
        /// The import target as written.
        url: String,
        /// The retained `layer(...)`/media prelude after the URL.
        layer_and_media: RetainedValue,
    },
    /// `@font-face` — a declaration block.
    FontFace(DeclarationList),
    /// `@keyframes` (and vendor-prefixed aliases, which parse the same
    /// and are distinguishable at serialization only).
    Keyframes {
        /// The animation name.
        name: String,
        /// The keyframe steps, in source order.
        keyframes: Vec<Keyframe>,
    },
    /// `@layer` — statement or block form.
    Layer(LayerRule),
    /// `@namespace` — optional prefix and the namespace URL.
    Namespace {
        /// The prefix, if declared (`@namespace svg url(...)`).
        prefix: Option<String>,
        /// The namespace URL as written.
        url: String,
    },
    /// An at-rule the engine does not model yet: retained losslessly
    /// (prelude + optional block) and re-serialized in canonical form.
    Unknown {
        /// The at-rule name (lowercased; at-rule names are
        /// ASCII case-insensitive).
        name: String,
        /// The retained prelude.
        prelude: RetainedValue,
        /// The retained block, if the rule had one.
        block: Option<RetainedValue>,
    },
}

impl CssRule {
    /// Serializes the rule to canonical CSS.
    pub fn to_css_string(&self) -> String {
        match self {
            CssRule::Style(rule) => {
                let mut out = String::new();
                out.push_str(&rule.selectors.to_css_string());
                if rule.declarations.is_empty() {
                    // Canonical serialization of an empty block has no
                    // inner space (re-parses to zero tokens).
                    out.push_str(" {}");
                } else {
                    out.push_str(" { ");
                    out.push_str(&rule.declarations.to_css_string());
                    out.push_str(" }");
                }
                out
            }
            CssRule::Media { prelude, rules } => {
                let mut out = String::from("@media ");
                out.push_str(&prelude.to_css_string_trimmed());
                out.push_str(" { ");
                out.push_str(&serialize_rule_list(rules));
                out.push_str(" }");
                out
            }
            CssRule::Supports { prelude, rules } => {
                let mut out = String::from("@supports ");
                out.push_str(&prelude.to_css_string_trimmed());
                out.push_str(" { ");
                out.push_str(&serialize_rule_list(rules));
                out.push_str(" }");
                out
            }
            CssRule::Import {
                url,
                layer_and_media,
            } => {
                let mut out = String::from("@import ");
                out.push_str(&serialize_string_css(url));
                let rest = layer_and_media.to_css_string_trimmed();
                if !rest.is_empty() {
                    out.push(' ');
                    out.push_str(&rest);
                }
                out.push(';');
                out
            }
            CssRule::FontFace(declarations) => {
                let mut out = String::from("@font-face { ");
                out.push_str(&declarations.to_css_string());
                out.push_str(" }");
                out
            }
            CssRule::Keyframes { name, keyframes } => {
                let mut out = String::from("@keyframes ");
                out.push_str(&serialize_identifier_css(name));
                out.push_str(" { ");
                for (index, keyframe) in keyframes.iter().enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    for (index, selector) in keyframe.selectors.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        match selector {
                            KeyframeSelector::From => out.push_str("from"),
                            KeyframeSelector::To => out.push_str("to"),
                            KeyframeSelector::Percentage(value) => {
                                out.push_str(&value.to_string());
                                out.push('%');
                            }
                        }
                    }
                    out.push_str(" { ");
                    out.push_str(&keyframe.declarations.to_css_string());
                    out.push_str("; }");
                }
                out.push_str(" }");
                out
            }
            CssRule::Layer(LayerRule::Statement(names)) => {
                let mut out = String::from("@layer ");
                for (index, name) in names.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(name);
                }
                out.push(';');
                out
            }
            CssRule::Layer(LayerRule::Block { name, rules }) => {
                let mut out = String::from("@layer ");
                if let Some(name) = name {
                    out.push_str(name);
                    out.push(' ');
                }
                out.push_str("{ ");
                out.push_str(&serialize_rule_list(rules));
                out.push_str(" }");
                out
            }
            CssRule::Namespace { prefix, url } => {
                let mut out = String::from("@namespace ");
                if let Some(prefix) = prefix {
                    out.push_str(&serialize_identifier_css(prefix));
                    out.push(' ');
                }
                out.push_str(&serialize_string_css(url));
                out.push(';');
                out
            }
            CssRule::Unknown {
                name,
                prelude,
                block,
            } => {
                let mut out = String::from("@");
                out.push_str(name);
                let prelude_text = prelude.to_css_string_trimmed();
                if !prelude_text.is_empty() {
                    out.push(' ');
                    out.push_str(&prelude_text);
                }
                if let Some(block) = block {
                    out.push_str(" { ");
                    out.push_str(&block.to_css_string_trimmed());
                    out.push_str(" }");
                } else {
                    out.push(';');
                }
                out
            }
        }
    }
}

impl fmt::Display for CssRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css_string())
    }
}

/// Serializes a rule list, space-separated.
fn serialize_rule_list(rules: &[CssRule]) -> String {
    let mut out = String::new();
    for (index, rule) in rules.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&rule.to_css_string());
    }
    out
}

/// The cssparser adapter for stylesheet-level and nested rule bodies.
///
/// The same adapter serves both contexts: at stylesheet top level the
/// driver is [`StyleSheetParser`] (which owns `@charset` handling);
/// inside `@media`/`@supports`/`@layer` bodies it is
/// [`RuleBodyParser`] (no `@charset`). Visible to the vendored-corpus
/// harness (`corpus.rs`), which drives it through `parse_one_rule`.
pub(crate) struct RuleParser;

impl<'i> QualifiedRuleParser<'i> for RuleParser {
    type Prelude = SelectorList;
    type QualifiedRule = CssRule;
    type Error = ();

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, CssParseError<()>> {
        // Invalid selector lists drop the rule (Selectors 4 error
        // handling): the selector error propagates as a generic
        // qualified-rule failure and parse_block never runs.
        SelectorList::parse_from(input)
            .map_err(|_| CssParseError::from_basic_kind(BasicParseErrorKind::QualifiedRuleInvalid))
    }

    fn parse_block(
        &mut self,
        selectors: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Self::QualifiedRule, CssParseError<()>> {
        Ok(CssRule::Style(StyleRule {
            selectors,
            declarations: DeclarationList::parse_block_contents(input),
        }))
    }
}

impl<'i> AtRuleParser<'i> for RuleParser {
    type Prelude = AtRulePrelude;
    type AtRule = CssRule;
    type Error = ();

    fn parse_prelude(
        &mut self,
        name: cssparser::CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, CssParseError<()>> {
        parse_at_rule_prelude(&name, input)
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
    ) -> Result<Self::AtRule, ()> {
        Ok(match prelude {
            AtRulePrelude::Import {
                url,
                layer_and_media,
            } => CssRule::Import {
                url,
                layer_and_media,
            },
            AtRulePrelude::Namespace { prefix, url } => CssRule::Namespace { prefix, url },
            AtRulePrelude::Layer(names) if !names.is_empty() => {
                CssRule::Layer(LayerRule::Statement(names))
            }
            AtRulePrelude::Layer(_) => {
                // `@layer;` — the statement form requires at least one
                // name; the rule is dropped.
                return Err(());
            }
            AtRulePrelude::Unknown { name, prelude } => CssRule::Unknown {
                name,
                prelude,
                block: None,
            },
            // Block-only at-rules reaching rule_without_block means the
            // rule had no `{}` block (e.g. `@media screen;`): the
            // grammar requires the block, so the rule is dropped.
            AtRulePrelude::Media(_)
            | AtRulePrelude::Supports(_)
            | AtRulePrelude::FontFace
            | AtRulePrelude::Keyframes(_) => return Err(()),
        })
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Self::AtRule, CssParseError<()>> {
        Ok(match prelude {
            AtRulePrelude::Media(retained) => CssRule::Media {
                prelude: retained,
                rules: parse_nested_rule_list(input),
            },
            AtRulePrelude::Supports(retained) => CssRule::Supports {
                prelude: retained,
                rules: parse_nested_rule_list(input),
            },
            AtRulePrelude::FontFace => {
                CssRule::FontFace(DeclarationList::parse_block_contents(input))
            }
            AtRulePrelude::Keyframes(name) => CssRule::Keyframes {
                name,
                keyframes: parse_keyframe_list(input),
            },
            AtRulePrelude::Layer(names) if names.len() <= 1 => CssRule::Layer(LayerRule::Block {
                name: names.into_iter().next(),
                rules: parse_nested_rule_list(input),
            }),
            // `@layer a, b { … }` — multiple names cannot take a block.
            AtRulePrelude::Layer(_) => {
                return Err(CssParseError::from_basic_kind(
                    BasicParseErrorKind::AtRuleBodyInvalid,
                ));
            }
            AtRulePrelude::Unknown { name, prelude } => CssRule::Unknown {
                name,
                prelude,
                block: Some(RetainedValue::parse(input)),
            },
            AtRulePrelude::Import { .. } | AtRulePrelude::Namespace { .. } => {
                // These at-rules take no block; a block is invalid and
                // the rule is dropped.
                return Err(CssParseError::from_basic_kind(
                    BasicParseErrorKind::AtRuleBodyInvalid,
                ));
            }
        })
    }
}

/// The at-rule prelude parse result, carried to `rule_without_block`/
/// `parse_block`. Visible to the corpus harness.
pub(crate) enum AtRulePrelude {
    Media(RetainedValue),
    Supports(RetainedValue),
    Import {
        url: String,
        layer_and_media: RetainedValue,
    },
    FontFace,
    Keyframes(String),
    Layer(Vec<String>),
    Namespace {
        prefix: Option<String>,
        url: String,
    },
    Unknown {
        name: String,
        prelude: RetainedValue,
    },
}

/// Parses an at-rule prelude by name. The parser sits before the
/// prelude; cssparser has delimited it at `;` or the `{}` block.
fn parse_at_rule_prelude(
    name: &str,
    input: &mut Parser<'_>,
) -> Result<AtRulePrelude, CssParseError<()>> {
    // At-rule names are ASCII case-insensitive.
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        // `@charset` is not a real rule: the stylesheet driver consumes
        // it as the first rule; anywhere else (or in nested blocks) it
        // is invalid and the rule is dropped (corpus-pinned).
        "charset" => Err(CssParseError::from_basic_kind(
            BasicParseErrorKind::AtRuleInvalid,
        )),
        "media" => Ok(AtRulePrelude::Media(RetainedValue::parse(input))),
        "supports" => Ok(AtRulePrelude::Supports(RetainedValue::parse(input))),
        "font-face" => Ok(AtRulePrelude::FontFace),
        "import" => {
            let url_token = input.expect_url_or_string()?.to_string();
            let layer_and_media = RetainedValue::parse(input);
            Ok(AtRulePrelude::Import {
                url: url_token,
                layer_and_media,
            })
        }
        "namespace" => {
            input.skip_whitespace();
            let prefix = input
                .try_parse(|input| input.expect_ident().map(|ident| ident.to_string()))
                .ok();
            if prefix.is_some() {
                input.skip_whitespace();
            }
            let url_token = input.expect_url_or_string()?.to_string();
            Ok(AtRulePrelude::Namespace {
                prefix,
                url: url_token,
            })
        }
        "keyframes" | "-webkit-keyframes" | "-moz-keyframes" | "-ms-keyframes" | "-o-keyframes" => {
            input.skip_whitespace();
            let name_token = input.next()?;
            let animation_name = match name_token {
                cssparser::Token::Ident(value) => value.to_string(),
                cssparser::Token::QuotedString(value) => value.to_string(),
                _ => {
                    return Err(CssParseError::from_basic_kind(
                        BasicParseErrorKind::AtRuleInvalid,
                    ));
                }
            };
            Ok(AtRulePrelude::Keyframes(animation_name))
        }
        "layer" => {
            // Both forms parse to the name list here; `;` (statement:
            // one-or-more names) vs `{}` (block: zero-or-one name) is
            // distinguished by which callback cssparser then calls.
            Ok(AtRulePrelude::Layer(parse_layer_name_list(input)))
        }
        _ => Ok(AtRulePrelude::Unknown {
            name: lower,
            prelude: RetainedValue::parse(input),
        }),
    }
}

/// Parses a dotted layer name list (`@layer a.b, c;`).
fn parse_layer_name_list(input: &mut Parser<'_>) -> Vec<String> {
    let mut names = Vec::new();
    loop {
        input.skip_whitespace();
        let mut name = String::new();
        let mut segment = false;
        while let Ok(ident) = input.try_parse(|input| input.expect_ident().map(|i| i.to_string())) {
            if segment {
                name.push('.');
            }
            name.push_str(&ident);
            segment = true;
            input.skip_whitespace();
            if input.try_parse(|input| input.expect_delim('.')).is_err() {
                break;
            }
            input.skip_whitespace();
        }
        if segment {
            names.push(name);
        }
        input.skip_whitespace();
        match input.next() {
            Ok(&cssparser::Token::Comma) => continue,
            _ => break,
        }
    }
    names
}

/// Parses a nested rule list (inside `@media`/`@supports`/`@layer`
/// blocks): qualified rules and at-rules, no `@charset` handling.
pub(crate) fn parse_nested_rule_list(input: &mut Parser<'_>) -> Vec<CssRule> {
    parse_nested_rule_list_with_markers(input)
        .into_iter()
        .filter_map(|item| item.ok())
        .collect()
}

/// Same as [`parse_nested_rule_list`], keeping dropped-rule markers so
/// the vendored conformance corpus can pin the exact recovery shape
/// (dropped rules map to `["error", "invalid"]` entries).
pub(crate) fn parse_nested_rule_list_with_markers(
    input: &mut Parser<'_>,
) -> Vec<Result<CssRule, ()>> {
    let mut parser = RuleParser;
    RuleBodyParser::new(input, &mut parser)
        .map(|item| item.map_err(|_| ()))
        .collect()
}

/// Parses the body of an `@keyframes` block: keyframe rules whose
/// preludes are position selectors.
fn parse_keyframe_list(input: &mut Parser<'_>) -> Vec<Keyframe> {
    let mut parser = KeyframesParser;
    let mut keyframes = Vec::new();
    keyframes.extend(RuleBodyParser::new(input, &mut parser).filter_map(|item| item.ok()));
    keyframes
}

/// The cssparser adapter for `@keyframes` bodies.
struct KeyframesParser;

impl<'i> QualifiedRuleParser<'i> for KeyframesParser {
    type Prelude = Vec<KeyframeSelector>;
    type QualifiedRule = Keyframe;
    type Error = ();

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, CssParseError<()>> {
        let mut selectors = Vec::new();
        loop {
            input.skip_whitespace();
            let position = match input.next()? {
                cssparser::Token::Ident(value) if value.eq_ignore_ascii_case("from") => {
                    KeyframeSelector::From
                }
                cssparser::Token::Ident(value) if value.eq_ignore_ascii_case("to") => {
                    KeyframeSelector::To
                }
                cssparser::Token::Percentage { unit_value, .. } => {
                    let percent = unit_value * 100.0;
                    if !(0.0..=100.0).contains(&percent) {
                        // The grammar restricts keyframe positions to
                        // 0%..100%; outside values drop the keyframe.
                        return Err(CssParseError::from_basic_kind(
                            BasicParseErrorKind::QualifiedRuleInvalid,
                        ));
                    }
                    KeyframeSelector::Percentage(percent.round() as u16)
                }
                _ => {
                    return Err(CssParseError::from_basic_kind(
                        BasicParseErrorKind::QualifiedRuleInvalid,
                    ));
                }
            };
            selectors.push(position);
            input.skip_whitespace();
            match input.next() {
                Ok(&cssparser::Token::Comma) => continue,
                _ => break,
            }
        }
        Ok(selectors)
    }

    fn parse_block(
        &mut self,
        selectors: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Self::QualifiedRule, CssParseError<()>> {
        Ok(Keyframe {
            selectors,
            declarations: DeclarationList::parse_block_contents(input),
        })
    }
}

impl<'i> cssparser::DeclarationParser<'i> for KeyframesParser {
    type Declaration = Keyframe;
    type Error = ();

    fn parse_value(
        &mut self,
        _name: cssparser::CowRcStr<'i>,
        _input: &mut Parser<'i>,
        _declaration_start: &ParserState,
    ) -> Result<Keyframe, CssParseError<()>> {
        // Keyframe bodies are not declaration lists.
        Err(CssParseError::from_basic_kind(
            BasicParseErrorKind::QualifiedRuleInvalid,
        ))
    }
}

impl<'i> AtRuleParser<'i> for KeyframesParser {
    type Prelude = std::convert::Infallible;
    type AtRule = Keyframe;
    type Error = ();

    fn parse_prelude(
        &mut self,
        _name: cssparser::CowRcStr<'i>,
        _input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, CssParseError<()>> {
        // Keyframe bodies contain no valid at-rules; shapes delimit
        // correctly and the item is dropped.
        Err(cssparser::ParseError::from_basic_kind(
            BasicParseErrorKind::AtRuleInvalid,
        ))
    }

    fn rule_without_block(
        &mut self,
        _prelude: Self::Prelude,
        _start: &ParserState,
    ) -> Result<Keyframe, ()> {
        Err(())
    }

    fn parse_block(
        &mut self,
        _prelude: Self::Prelude,
        _start: &ParserState,
        _input: &mut Parser<'i>,
    ) -> Result<Keyframe, CssParseError<()>> {
        Err(cssparser::ParseError::from_basic_kind(
            BasicParseErrorKind::AtRuleBodyInvalid,
        ))
    }
}

impl<'i> cssparser::DeclarationParser<'i> for RuleParser {
    type Declaration = CssRule;
    type Error = ();

    fn parse_value(
        &mut self,
        _name: cssparser::CowRcStr<'i>,
        _input: &mut Parser<'i>,
        _declaration_start: &ParserState,
    ) -> Result<CssRule, CssParseError<()>> {
        // Rule bodies are not declaration lists; declarations are only
        // parsed inside style-rule blocks (DeclarationListParser).
        Err(CssParseError::from_basic_kind(
            BasicParseErrorKind::QualifiedRuleInvalid,
        ))
    }
}

impl RuleBodyItemParser<'_, CssRule, ()> for RuleParser {
    fn parse_declarations(&self) -> bool {
        false
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

impl RuleBodyItemParser<'_, Keyframe, ()> for KeyframesParser {
    fn parse_declarations(&self) -> bool {
        false
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

/// Parses a complete stylesheet-level rule list.
pub(crate) fn parse_stylesheet_rules(input: &mut Parser<'_>) -> Vec<CssRule> {
    parse_stylesheet_rules_with_markers(input)
        .into_iter()
        .filter_map(|item| item.ok())
        .collect()
}

/// Same as [`parse_stylesheet_rules`], keeping dropped-rule markers for
/// the vendored conformance corpus (see
/// [`parse_nested_rule_list_with_markers`]).
pub(crate) fn parse_stylesheet_rules_with_markers(
    input: &mut Parser<'_>,
) -> Vec<Result<CssRule, ()>> {
    let mut parser = RuleParser;
    StyleSheetParser::new(input, &mut parser)
        .map(|item| item.map_err(|_| ()))
        .collect()
}

#[cfg(test)]
pub(crate) mod conformance {
    use super::*;

    /// A block-contents item for conformance checking: the engine's
    /// [`CssRule`] items plus declaration items (which generic block
    /// contents can contain; the engine drops them in rule contexts, the
    /// conformance corpus pins them).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum ConformanceItem {
        /// A rule (style rule or at-rule) from the engine adapter.
        Rule(CssRule),
        /// A declaration from a rule body (engine-invalid there, but the
        /// block-contents algorithm reports it).
        Declaration(crate::declarations::Declaration),
    }

    /// A delegating adapter for the conformance corpus: at-rule and
    /// qualified-rule dispatch goes through the engine's [`RuleParser`];
    /// declaration shapes are reported instead of dropped. This keeps the
    /// corpus pinning the *engine's* dispatch logic.
    pub(crate) struct CorpusParser {
        /// The engine rule parser doing the real dispatch.
        pub rule: RuleParser,
    }

    impl<'i> QualifiedRuleParser<'i> for CorpusParser {
        type Prelude = SelectorList;
        type QualifiedRule = ConformanceItem;
        type Error = ();

        fn parse_prelude(
            &mut self,
            input: &mut Parser<'i>,
        ) -> Result<Self::Prelude, CssParseError<()>> {
            <RuleParser as QualifiedRuleParser<'i>>::parse_prelude(&mut self.rule, input)
        }

        fn parse_block(
            &mut self,
            prelude: Self::Prelude,
            start: &ParserState,
            input: &mut Parser<'i>,
        ) -> Result<Self::QualifiedRule, CssParseError<()>> {
            <RuleParser as QualifiedRuleParser<'i>>::parse_block(
                &mut self.rule,
                prelude,
                start,
                input,
            )
            .map(ConformanceItem::Rule)
        }
    }

    impl<'i> AtRuleParser<'i> for CorpusParser {
        type Prelude = AtRulePrelude;
        type AtRule = ConformanceItem;
        type Error = ();

        fn parse_prelude(
            &mut self,
            name: cssparser::CowRcStr<'i>,
            input: &mut Parser<'i>,
        ) -> Result<Self::Prelude, CssParseError<()>> {
            <RuleParser as AtRuleParser<'i>>::parse_prelude(&mut self.rule, name, input)
        }

        fn rule_without_block(
            &mut self,
            prelude: Self::Prelude,
            start: &ParserState,
        ) -> Result<Self::AtRule, ()> {
            <RuleParser as AtRuleParser<'i>>::rule_without_block(&mut self.rule, prelude, start)
                .map(ConformanceItem::Rule)
        }

        fn parse_block(
            &mut self,
            prelude: Self::Prelude,
            start: &ParserState,
            input: &mut Parser<'i>,
        ) -> Result<Self::AtRule, CssParseError<()>> {
            <RuleParser as AtRuleParser<'i>>::parse_block(&mut self.rule, prelude, start, input)
                .map(ConformanceItem::Rule)
        }
    }

    impl<'i> cssparser::DeclarationParser<'i> for CorpusParser {
        type Declaration = ConformanceItem;
        type Error = ();

        fn parse_value(
            &mut self,
            name: cssparser::CowRcStr<'i>,
            input: &mut Parser<'i>,
            declaration_start: &ParserState,
        ) -> Result<ConformanceItem, CssParseError<()>> {
            <crate::declarations::DeclarationListParser as cssparser::DeclarationParser<'i>>::parse_value(
                &mut crate::declarations::DeclarationListParser,
                name,
                input,
                declaration_start,
            )
            .map(|item| match item {
                crate::declarations::BlockItem::Declaration(declaration) => {
                    ConformanceItem::Declaration(declaration)
                }
                // Non-declaration shapes reaching the one-declaration
                // conformance path map to the engine's drop.
                _ => ConformanceItem::Rule(CssRule::Unknown {
                    name: String::from("invalid-shape"),
                    prelude: crate::RetainedValue::parse(&mut Parser::new("")),
                    block: None,
                }),
            })
        }
    }

    impl RuleBodyItemParser<'_, ConformanceItem, ()> for CorpusParser {
        fn parse_declarations(&self) -> bool {
            true
        }

        fn parse_qualified(&self) -> bool {
            true
        }
    }
}

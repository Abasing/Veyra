//! The cascade (CSS Cascade 5): stylesheets in, per-element computed
//! styles out.
//!
//! Pipeline: a [`StyleSet`] holds parsed sheets tagged with their
//! [`Origin`] (the user-agent sheet is built in; author sheets — with an
//! optional base URL for `@import` resolution — are appended by the
//! loader). [`compute_styles`] walks the document tree in pre-order and,
//! per element:
//!
//! 1. **Match** — every prepared style rule's selectors run against the
//!    element through the [`crate::matching`] adapter; the rule's
//!    cascade specificity is the most specific *matching* selector.
//! 2. **Sort** — matching declarations compete per longhand (shorthands
//!    are expanded at prepare time so margin/margin-left interleave by
//!    cascade order, not by name). The sort key is the Cascade 5 order:
//!    origin & importance, then layer (unlayered beats layered for
//!    normal declarations; the order inverts for important), then
//!    specificity, then order of appearance. Inline `style` attributes
//!    rank above author style rules within their importance. `revert`
//!    (and `revert-layer`, approximated as `revert` this batch) rolls
//!    its origin's declarations back when it wins.
//! 3. **Compute** — winners resolve to typed values: inherited
//!    properties copy from the parent, non-inherited start at initial;
//!    em/percent resolve against the parent (font-size) or own
//!    (everything else) font size, rem against the root font size
//!    (the root element resolves rem against the initial font size, per
//!    CSS Values 4).
//!
//! # Batch scope (each deferral has a recorded home)
//!
//! - `@media` preludes evaluate to the degenerate subset: (optionally
//!   `only`/`not`-prefixed) bare media types — `screen`/`all` apply,
//!   everything else (features, `print`, `and`-lists) does not. Full
//!   media queries are their own batch.
//! - `@supports` conditions are unevaluated: the block never applies
//!   yet.
//! - `@import` targets are resolved against the sheet's base URL and
//!   *recorded* ([`StyleSet::imports`]) for the loader to fetch — the
//!   fetch loop is shell work (batch 3); cycle/depth guards live there.
//! - `@namespace` prefixes are unresolved: unprefixed selectors match
//!   any namespace (the no-default-namespace Selectors 4 behavior).
//! - `var()` references make a declaration invalid at computed-value
//!   time (custom properties themselves are captured and inherited).
//! - No invalidation sets yet: this batch computes the initial style of
//!   a static document; mutation-driven restyle (R1) is a later batch.

use std::collections::BTreeMap;

use veyra_dom::{NodeData, NodeId, Tree};
use veyra_url::Url;

use crate::declarations::Declaration;
use crate::matching::{MatchOptions, VeyraElement, match_selector, quirks_of};
use crate::rules::{CssRule, LayerRule};
use crate::selectors::SelectorList;
use crate::stylesheet::Stylesheet;
use crate::tokens::RetainedValue;
use crate::values::{
    BorderStyle, Color, ColorValue, ComputedValues, DEFAULT_FONT_SIZE, Display, FontFamily,
    FontSizeSpec, FontStyle, FontWeightSpec, GlobalKeyword, LengthPercent, LineHeight,
    LineHeightSpec, MarginSizing, Sizing, TextAlign, TextDecorationLine, absolute_size_step,
    global_keyword, parse_background_shorthand, parse_border_colors, parse_border_shorthand,
    parse_border_styles, parse_border_widths, parse_color_value, parse_display, parse_font_family,
    parse_font_size, parse_font_style, parse_font_weight, parse_line_height,
    parse_margin_shorthand, parse_margin_value, parse_padding_shorthand, parse_sizing,
    parse_text_align, parse_text_decoration_line, parse_text_decoration_shorthand, references_var,
    relative_weight,
};
use selectors::Element as _;
use selectors::context::SelectorCaches;

/// The cascade origin of a stylesheet (Cascade 5 §6.4: origin and
/// importance form the outermost comparison).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The engine's built-in sheet (lowest normal precedence; its
    /// important declarations win over everything).
    UserAgent,
    /// User preferences (between UA and author for normal declarations).
    User,
    /// Document/inline styles.
    Author,
}

/// One sheet in the set: parsed rules, origin, and the base URL that
/// `@import`/`@namespace` targets resolve against.
#[derive(Debug, Clone)]
struct SheetEntry {
    stylesheet: Stylesheet,
    origin: Origin,
    base_url: Option<Url>,
}

/// An `@import` recorded during preparation: the resolved absolute URL,
/// the layer it imports into, and the retained media prelude. The loader
/// fetches these (cycle/depth guards are loader-owned).
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// The import target, resolved against the sheet's base URL (`None`
    /// if the sheet had no base URL and the target is relative — the
    /// loader reports that).
    pub url: Option<Url>,
    /// The `layer(...)` name, if the import is layered.
    pub layer: Option<String>,
    /// The retained media prelude after the URL.
    pub media: RetainedValue,
}

/// An ordered set of stylesheets: the cascade's input.
///
/// The user-agent sheet is present from construction; author (and user)
/// sheets append in document order. Cloning is cheap-adjacent (sheets
/// own their rules; the set is prepared once per style computation).
#[derive(Debug, Clone, Default)]
pub struct StyleSet {
    sheets: Vec<SheetEntry>,
}

impl StyleSet {
    /// A set with the built-in user-agent sheet.
    #[must_use]
    pub fn new() -> Self {
        let mut set = Self { sheets: Vec::new() };
        // The static UA sheet is valid by construction; an (impossible)
        // parse failure degrades to an empty UA sheet rather than a
        // panic path (engineering standards §2).
        if let Ok(sheet) =
            Stylesheet::parse(crate::ua::USER_AGENT_STYLESHEET, &crate::Limits::default())
        {
            set.sheets.push(SheetEntry {
                stylesheet: sheet,
                origin: Origin::UserAgent,
                base_url: None,
            });
        }
        set
    }

    /// Appends a user-agent sheet (extension of the built-in defaults).
    pub fn append_user_agent(&mut self, stylesheet: Stylesheet) {
        self.sheets.push(SheetEntry {
            stylesheet,
            origin: Origin::UserAgent,
            base_url: None,
        });
    }

    /// Appends a user sheet (user preferences).
    pub fn append_user(&mut self, stylesheet: Stylesheet) {
        self.sheets.push(SheetEntry {
            stylesheet,
            origin: Origin::User,
            base_url: None,
        });
    }

    /// Appends an author sheet; `base_url` is where `@import` targets
    /// resolve against (the `STYLE -> URL` graph edge).
    pub fn append_author(&mut self, stylesheet: Stylesheet, base_url: Option<Url>) {
        self.sheets.push(SheetEntry {
            stylesheet,
            origin: Origin::Author,
            base_url,
        });
    }

    /// Whether no sheets are present (not even the UA sheet — only
    /// possible for a hand-built `Default`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    /// The `@import` requests across all sheets, resolved against their
    /// base URLs, in document order.
    #[must_use]
    pub fn imports(&self) -> Vec<ImportRequest> {
        let mut out = Vec::new();
        for (sheet_index, entry) in self.sheets.iter().enumerate() {
            collect_imports(&entry.stylesheet, entry, sheet_index, &mut out);
        }
        out
    }
}

/// Walks a sheet's rules collecting `@import` requests (top level only —
/// nested imports are invalid CSS and dropped by the parser).
fn collect_imports(
    stylesheet: &Stylesheet,
    entry: &SheetEntry,
    sheet_index: usize,
    out: &mut Vec<ImportRequest>,
) {
    let _ = sheet_index;
    for rule in stylesheet.rules() {
        if let CssRule::Import {
            url,
            layer_and_media,
        } = rule
        {
            out.push(ImportRequest {
                url: resolve_import_url(entry.base_url.as_ref(), url),
                layer: layer_name_of(layer_and_media),
                media: layer_and_media.clone(),
            });
        }
    }
}

/// Resolves an `@import`/`@namespace` URL string against the sheet's
/// base URL (single URL authority: `veyra-url` does the resolution).
fn resolve_import_url(base: Option<&Url>, target: &str) -> Option<Url> {
    let target = target.trim();
    match base {
        Some(base) => base.join(target).ok(),
        None => Url::parse(target).ok(),
    }
}

/// Extracts the `layer(name)` argument from an `@import`'s retained
/// prelude, if present (the prelude's remaining content is media).
fn layer_name_of(prelude: &RetainedValue) -> Option<String> {
    let text = prelude.to_css_string_trimmed();
    let lower = text.to_ascii_lowercase();
    let start = lower.find("layer(")? + "layer(".len();
    let rest = &text[start..];
    let end = rest.find(')')?;
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// The layer order tree: child layers registered in first-seen order
/// across the set; anonymous layers get synthetic names. Comparison
/// (Cascade 5 §6.4 "layer order") walks both paths from the root, at
/// each scope: for normal declarations later siblings win and unlayered
/// content wins over all children; for important declarations the
/// sibling order inverts and children win over unlayered content.
#[derive(Debug, Default)]
struct LayerTree {
    children: Vec<String>,
    subtrees: BTreeMap<String, LayerTree>,
    anonymous: usize,
}

impl LayerTree {
    /// Ensures the path exists, registering missing components in order.
    fn register(&mut self, path: &[String]) {
        let Some((head, rest)) = path.split_first() else {
            return;
        };
        if !self.children.iter().any(|name| name == head) {
            self.children.push(head.clone());
        }
        self.subtrees
            .entry(head.clone())
            .or_default()
            .register(rest);
    }

    /// Registers an anonymous layer under `parent`, returning its
    /// synthetic name.
    fn register_anonymous(&mut self, parent: &[String]) -> String {
        let tree = self.subtree_mut(parent);
        let name = format!("anon/{}", tree.anonymous);
        tree.anonymous += 1;
        tree.children.push(name.clone());
        tree.subtrees.entry(name.clone()).or_default();
        name
    }

    fn subtree_mut(&mut self, path: &[String]) -> &mut LayerTree {
        let mut current = self;
        for component in path {
            current = current.subtrees.entry(component.clone()).or_default();
        }
        current
    }

    /// Resolves a layer path to per-scope sibling indices (the numeric
    /// form the comparator walks).
    fn indices_of(&self, path: &[String]) -> Vec<u32> {
        let mut indices = Vec::with_capacity(path.len());
        let mut current = self;
        for component in path {
            let Some(position) = current.children.iter().position(|name| name == component) else {
                break;
            };
            indices.push(position as u32);
            match current.subtrees.get(component) {
                Some(subtree) => current = subtree,
                None => break,
            }
        }
        indices
    }
}

/// Compares two resolved layer paths for one declaration (a vs b),
/// per the Cascade 5 layer-order rules described on [`LayerTree`].
fn compare_layers(a: &[u32], b: &[u32], important: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut a_iter = a.iter();
    let mut b_iter = b.iter();
    loop {
        match (a_iter.next(), b_iter.next()) {
            (None, None) => return Ordering::Equal,
            // One path ends (unlayered at this scope) where the other
            // descends into a child layer.
            (None, Some(_)) => {
                return if important {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            (Some(_), None) => {
                return if important {
                    Ordering::Greater
                } else {
                    Ordering::Less
                };
            }
            (Some(x), Some(y)) if x == y => continue,
            (Some(x), Some(y)) => {
                // First differing sibling: later wins normally, earlier
                // wins for important.
                return if important { y.cmp(x) } else { x.cmp(y) };
            }
        }
    }
}

/// A style rule flattened out of the set at prepare time, with its
/// cascade metadata and pre-parsed declarations.
struct PreparedRule {
    selectors: SelectorList,
    /// Per-complex-selector specificity, aligned with the list.
    specificities: Vec<u32>,
    origin: Origin,
    /// The layer path as declared (names; resolved to indices after
    /// preparation).
    layer_names: Vec<String>,
    /// Resolved layer path (per-scope sibling indices; empty =
    /// unlayered).
    layer: Vec<u32>,
    /// Declaration order across the whole set (append order).
    order: u32,
    /// Pre-parsed declarations: (normalized name, important flag,
    /// longhand).
    longhands: Vec<(String, bool, Longhand)>,
}

/// Prepares the set: flattens style rules out of every sheet, evaluating
/// the media subset and registering layers in one document-order pass
/// (layer order is first-occurrence per Cascade 5, so a single walk
/// both fixes statement-form ordering and keeps anonymous-layer names
/// consistent).
fn prepare(set: &StyleSet) -> Vec<PreparedRule> {
    let mut rules = Vec::new();
    let mut layers = LayerTree::default();
    let mut order = 0u32;
    for entry in &set.sheets {
        let mut layer_path: Vec<String> = Vec::new();
        prepare_rules(
            entry.stylesheet.rules(),
            entry.origin,
            &mut layer_path,
            &mut layers,
            &mut order,
            &mut rules,
        );
    }
    for rule in &mut rules {
        rule.layer = layers.indices_of(&rule.layer_names);
    }
    rules
}

/// Recursively flattens style rules with cascade context, registering
/// layer names as they are first encountered.
fn prepare_rules(
    rules: &[CssRule],
    origin: Origin,
    layer_path: &mut Vec<String>,
    layers: &mut LayerTree,
    order: &mut u32,
    out: &mut Vec<PreparedRule>,
) {
    for rule in rules.iter() {
        match rule {
            CssRule::Style(style) => {
                let specificities = style
                    .selectors
                    .upstream
                    .slice()
                    .iter()
                    .map(selectors::parser::Selector::specificity)
                    .collect();
                let longhands = style
                    .declarations
                    .declarations()
                    .iter()
                    .filter_map(|declaration| {
                        expand_longhand(&declaration.name, declaration).map(|entries| {
                            entries
                                .into_iter()
                                .map(|(name, longhand)| (name, declaration.important, longhand))
                                .collect::<Vec<_>>()
                        })
                    })
                    .flatten()
                    .collect();
                out.push(PreparedRule {
                    selectors: style.selectors.clone(),
                    specificities,
                    origin,
                    layer_names: layer_path.clone(),
                    layer: Vec::new(),
                    order: *order,
                    longhands,
                });
                *order += 1;
            }
            CssRule::Media { prelude, rules } => {
                if media_prelude_matches(prelude) {
                    prepare_rules(rules, origin, layer_path, layers, order, out);
                }
            }
            CssRule::Supports { rules, .. } => {
                // Condition evaluation is deferred: the block does not
                // apply yet (recorded batch scope).
                let _ = rules;
            }
            CssRule::Layer(LayerRule::Statement(names)) => {
                // Statement form: declares layer order, no rules.
                for name in names {
                    let components: Vec<String> = name.split('.').map(str::to_string).collect();
                    layers.register(&components);
                }
            }
            CssRule::Layer(LayerRule::Block { name, rules }) => {
                match name {
                    Some(name) => {
                        let components: Vec<String> = name.split('.').map(str::to_string).collect();
                        layers.register(&components);
                        layer_path.extend(components);
                    }
                    None => {
                        let synthetic = layers.register_anonymous(layer_path);
                        layer_path.push(synthetic);
                    }
                }
                prepare_rules(rules, origin, layer_path, layers, order, out);
                match name {
                    Some(name) => {
                        let components: usize = name.split('.').count();
                        layer_path.truncate(layer_path.len() - components);
                    }
                    None => {
                        layer_path.pop();
                    }
                }
            }
            // Imports/font-faces/keyframes/namespaces/unknown rules do
            // not contribute declarations.
            _ => {}
        }
    }
}

/// The document-order evaluation of a retained `@media` prelude's
/// degenerate subset: comma-separated terms of (optionally
/// `only`/`not`-prefixed) bare media types. `screen`/`all` apply as a
/// screen renderer; every other type, any feature test, and any
/// `and`/`or` composition does not.
fn media_prelude_matches(prelude: &RetainedValue) -> bool {
    media_text_matches(&prelude.to_css_string_trimmed())
}

/// The degenerate media-query evaluation for a raw media string (the
/// `media` attribute on `<style>`/`<link>`; the same subset as the
/// `@media` prelude evaluation). Empty text matches (the attribute's
/// missing-value default is `all`).
#[must_use]
pub fn media_query_applies(media: &str) -> bool {
    media_text_matches(media)
}

/// The text-level evaluator both call paths share.
fn media_text_matches(text: &str) -> bool {
    if text.trim().is_empty() {
        return true;
    }
    text.split(',').any(|term| media_term_matches(term.trim()))
}

/// One comma-separated media term.
fn media_term_matches(term: &str) -> bool {
    use cssparser::{Parser, Token};
    let mut parser = Parser::new(term);
    let mut negate = false;
    let mut token = match parser.next() {
        Ok(token) => token.clone(),
        Err(_) => return false,
    };
    if let Token::Ident(ref name) = token {
        let lower = name.to_ascii_lowercase();
        if lower == "only" || lower == "not" {
            negate = lower == "not";
            token = match parser.next() {
                Ok(token) => token.clone(),
                Err(_) => return false,
            };
        }
    }
    let media_type = match token {
        Token::Ident(name) => name.to_ascii_lowercase(),
        _ => return false,
    };
    // A bare type term must end here (features/`and` compositions are
    // not in the subset).
    if !parser.is_exhausted() {
        return false;
    }
    let applies = matches!(media_type.as_str(), "all" | "screen");
    applies != negate
}

/// One expanded, pre-parsed declaration (shorthands become per-longhand
/// entries at prepare time so cascade order interleaves correctly).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Longhand {
    /// A CSS-wide keyword (inherit/initial/unset; `revert` wins the
    /// cascade before reaching compute).
    Keyword(GlobalKeyword),
    /// `color`.
    Color(ColorValue),
    /// `background-color` (incl. the `background` shorthand's color
    /// layer).
    BackgroundColor(ColorValue),
    /// `display`.
    Display(Display),
    /// `font-size`.
    FontSize(FontSizeSpec),
    /// `font-weight`.
    FontWeight(FontWeightSpec),
    /// `font-style`.
    FontStyle(FontStyle),
    /// `font-family`.
    FontFamily(FontFamily),
    /// `line-height`.
    LineHeight(LineHeightSpec),
    /// `text-align`.
    TextAlign(TextAlign),
    /// `text-decoration-line` (incl. the shorthand's line layer).
    TextDecorationLine(TextDecorationLine),
    /// One margin side.
    MarginSide(MarginSizing),
    /// One padding side.
    PaddingSide(LengthPercent),
    /// `width`.
    Width(Sizing),
    /// `height`.
    Height(Sizing),
    /// One border side's width (px).
    BorderWidth(f32),
    /// One border side's style.
    BorderStyle(BorderStyle),
    /// One border side's color.
    BorderColor(ColorValue),
    /// A custom property's retained value (`--*`).
    Custom(RetainedValue),
}

/// Lowercases a property name for the cascade key (custom properties
/// keep their case — they are case-sensitive per the spec).
fn normalize_property(name: &str) -> String {
    if name.starts_with("--") {
        name.to_string()
    } else {
        name.to_ascii_lowercase()
    }
}

/// Expands one declaration into longhand entries. Shorthand parse
/// failures drop the whole declaration (the CSS shorthand rule); each
/// returned entry is `(normalized name, longhand)`.
fn expand_longhand(_name: &str, declaration: &Declaration) -> Option<Vec<(String, Longhand)>> {
    let normalized = normalize_property(&declaration.name);
    let value = &declaration.value;
    // The CSS-wide keywords precede any property grammar.
    if let Some(keyword) = global_keyword(value) {
        let longhand = match keyword {
            GlobalKeyword::Revert | GlobalKeyword::RevertLayer => Longhand::Keyword(keyword),
            _ => Longhand::Keyword(keyword),
        };
        return Some(vec![(normalized, longhand)]);
    }
    // var() references are invalid at computed-value time until
    // substitution lands; the declaration is dropped (the spec's
    // fallback rule keeps the property at its inherited/initial value).
    if references_var(value) {
        return None;
    }
    let longhands = if normalized.starts_with("--") {
        vec![(normalized, Longhand::Custom(value.clone()))]
    } else {
        match normalized.as_str() {
            "color" => vec![(normalized, Longhand::Color(parse_color_value(value)?))],
            "background-color" => {
                vec![(
                    normalized,
                    Longhand::BackgroundColor(parse_color_value(value)?),
                )]
            }
            // Shorthands expand to their longhand name (the apply step keys
            // on longhands).
            "background" => vec![(
                "background-color".to_string(),
                Longhand::BackgroundColor(parse_background_shorthand(value)?),
            )],
            "display" => vec![(normalized, Longhand::Display(parse_display(value)?))],
            "font-size" => vec![(normalized, Longhand::FontSize(parse_font_size(value)?))],
            "font-weight" => vec![(normalized, Longhand::FontWeight(parse_font_weight(value)?))],
            "font-style" => vec![(normalized, Longhand::FontStyle(parse_font_style(value)?))],
            "font-family" => vec![(normalized, Longhand::FontFamily(parse_font_family(value)?))],
            "line-height" => vec![(normalized, Longhand::LineHeight(parse_line_height(value)?))],
            "text-align" => vec![(normalized, Longhand::TextAlign(parse_text_align(value)?))],
            "text-decoration-line" => vec![(
                normalized,
                Longhand::TextDecorationLine(parse_text_decoration_line(value)?),
            )],
            "text-decoration" => vec![(
                "text-decoration-line".to_string(),
                Longhand::TextDecorationLine(parse_text_decoration_shorthand(value)?),
            )],
            "margin" => {
                let sides = parse_margin_shorthand(value)?;
                vec![
                    ("margin-top".into(), Longhand::MarginSide(sides.top)),
                    ("margin-right".into(), Longhand::MarginSide(sides.right)),
                    ("margin-bottom".into(), Longhand::MarginSide(sides.bottom)),
                    ("margin-left".into(), Longhand::MarginSide(sides.left)),
                ]
            }
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
                vec![(normalized, Longhand::MarginSide(parse_margin_value(value)?))]
            }
            "padding" => {
                let sides = parse_padding_shorthand(value)?;
                vec![
                    ("padding-top".into(), Longhand::PaddingSide(sides.top)),
                    ("padding-right".into(), Longhand::PaddingSide(sides.right)),
                    ("padding-bottom".into(), Longhand::PaddingSide(sides.bottom)),
                    ("padding-left".into(), Longhand::PaddingSide(sides.left)),
                ]
            }
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                vec![(
                    normalized,
                    Longhand::PaddingSide(crate::values::parse_padding_value(value)?),
                )]
            }
            "width" => vec![(normalized, Longhand::Width(parse_sizing(value)?))],
            "height" => vec![(normalized, Longhand::Height(parse_sizing(value)?))],
            "border" => {
                let (width, style, color) = parse_border_shorthand(value)?;
                let mut out = Vec::with_capacity(12);
                for side in ["top", "right", "bottom", "left"] {
                    out.push((format!("border-{side}-width"), Longhand::BorderWidth(width)));
                    out.push((format!("border-{side}-style"), Longhand::BorderStyle(style)));
                    out.push((format!("border-{side}-color"), Longhand::BorderColor(color)));
                }
                out
            }
            "border-width" => {
                let sides = parse_border_widths(value)?;
                vec![
                    ("border-top-width".into(), Longhand::BorderWidth(sides.top)),
                    (
                        "border-right-width".into(),
                        Longhand::BorderWidth(sides.right),
                    ),
                    (
                        "border-bottom-width".into(),
                        Longhand::BorderWidth(sides.bottom),
                    ),
                    (
                        "border-left-width".into(),
                        Longhand::BorderWidth(sides.left),
                    ),
                ]
            }
            "border-style" => {
                let sides = parse_border_styles(value)?;
                vec![
                    ("border-top-style".into(), Longhand::BorderStyle(sides.top)),
                    (
                        "border-right-style".into(),
                        Longhand::BorderStyle(sides.right),
                    ),
                    (
                        "border-bottom-style".into(),
                        Longhand::BorderStyle(sides.bottom),
                    ),
                    (
                        "border-left-style".into(),
                        Longhand::BorderStyle(sides.left),
                    ),
                ]
            }
            "border-color" => {
                let sides = parse_border_colors(value)?;
                vec![
                    ("border-top-color".into(), Longhand::BorderColor(sides.top)),
                    (
                        "border-right-color".into(),
                        Longhand::BorderColor(sides.right),
                    ),
                    (
                        "border-bottom-color".into(),
                        Longhand::BorderColor(sides.bottom),
                    ),
                    (
                        "border-left-color".into(),
                        Longhand::BorderColor(sides.left),
                    ),
                ]
            }
            // Per-side border longhands reuse the 1-value shorthand parses
            // (a single value expands to equal sides; `.top` is that value).
            name @ ("border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width") => {
                vec![(
                    name.to_string(),
                    Longhand::BorderWidth(parse_border_widths(value)?.top),
                )]
            }
            name @ ("border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style") => {
                vec![(
                    name.to_string(),
                    Longhand::BorderStyle(parse_border_styles(value)?.top),
                )]
            }
            name @ ("border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color") => {
                vec![(
                    name.to_string(),
                    Longhand::BorderColor(parse_border_colors(value)?.top),
                )]
            }
            // Properties outside the working set (including the `font`
            // shorthand and layout-facing suites) are not interpreted yet:
            // their declarations are ignored by the cascade (batch 1 already
            // retains them for serialization).
            _ => return None,
        }
    };
    Some(longhands)
}

/// One matching declaration competing in the cascade.
struct Candidate<'a> {
    longhand: &'a Longhand,
    /// Origin/importance group (0 = UA normal … 5 = UA important).
    group: u8,
    /// The rule's resolved layer path.
    layer: &'a [u32],
    /// The most specific matching selector's specificity.
    specificity: u32,
    /// Declaration order across the set.
    order: u32,
    /// From an inline `style` attribute (ranks above author rules).
    inline: bool,
}

/// The cascade key comparison: origin & importance, then layer order
/// (inverted for important groups), then specificity, then order of
/// appearance, then the inline-style flag.
fn cascade_key(a: &Candidate<'_>, b: &Candidate<'_>) -> std::cmp::Ordering {
    a.group
        .cmp(&b.group)
        .then_with(|| compare_layers(a.layer, b.layer, a.group >= 3))
        .then_with(|| a.specificity.cmp(&b.specificity))
        .then_with(|| a.order.cmp(&b.order))
        .then_with(|| a.inline.cmp(&b.inline))
}

/// The origin/importance group rank (Cascade 5 §6.4 order, ascending
/// loses-to: UA normal < user normal < author normal < author important
/// < user important < UA important).
fn group_of(origin: Origin, important: bool) -> u8 {
    match (origin, important) {
        (Origin::UserAgent, false) => 0,
        (Origin::User, false) => 1,
        (Origin::Author, false) => 2,
        (Origin::Author, true) => 3,
        (Origin::User, true) => 4,
        (Origin::UserAgent, true) => 5,
    }
}

/// The per-element winner map: normalized property name → winning
/// longhand, with `revert` handling (a revert that wins its cascade
/// removes its origin/importance group's declarations and the selection
/// re-runs; nothing below means no declaration).
fn winners_of<'a>(mut candidates: Vec<(&'a str, Candidate<'a>)>) -> Vec<(&'a str, &'a Longhand)> {
    // Group by property (BTreeMap for deterministic iteration).
    let mut per_property: BTreeMap<&str, Vec<Candidate<'a>>> = BTreeMap::new();
    for (name, candidate) in candidates.drain(..) {
        per_property.entry(name).or_default().push(candidate);
    }
    let mut winners = Vec::with_capacity(per_property.len());
    for (name, mut pool) in per_property {
        let winner = loop {
            let Some(best) = pool.iter().max_by(|a, b| cascade_key(a, b)) else {
                break None;
            };
            if matches!(
                best.longhand,
                Longhand::Keyword(GlobalKeyword::Revert | GlobalKeyword::RevertLayer)
            ) {
                let group = best.group;
                pool.retain(|candidate| candidate.group != group);
                if pool.is_empty() {
                    break None;
                }
            } else {
                break Some(best.longhand);
            }
        };
        if let Some(longhand) = winner {
            winners.push((name, longhand));
        }
    }
    winners
}

/// The application order of a winner: font sizing first (own font size
/// feeds em/% resolution everywhere else), then color (currentcolor),
/// then everything else.
fn apply_phase(name: &str, longhand: &Longhand) -> u8 {
    match longhand {
        Longhand::FontSize(_) | Longhand::FontWeight(_) => 0,
        Longhand::Color(_) => 1,
        Longhand::Custom(_) => 3,
        _ if name.starts_with("--") => 3,
        _ => 2,
    }
}

/// Computes one element's style: match → sort → apply.
fn compute_element(
    element: &VeyraElement<'_>,
    rules: &[PreparedRule],
    parent: Option<&ComputedValues>,
    rem_base: f32,
) -> ComputedValues {
    // -- Match ----------------------------------------------------------
    let quirks = quirks_of(element.tree());
    let mut caches = SelectorCaches::default();
    let mut candidates: Vec<(&str, Candidate<'_>)> = Vec::new();
    for rule in rules {
        let mut specificity: Option<u32> = None;
        for (selector, &rule_specificity) in rule
            .selectors
            .upstream
            .slice()
            .iter()
            .zip(&rule.specificities)
        {
            if match_selector(element, selector, quirks, &mut caches) {
                specificity = Some(
                    specificity.map_or(rule_specificity, |best: u32| best.max(rule_specificity)),
                );
            }
        }
        if let Some(specificity) = specificity {
            for (name, important, longhand) in &rule.longhands {
                candidates.push((
                    name.as_str(),
                    Candidate {
                        longhand,
                        group: group_of(rule.origin, *important),
                        layer: &rule.layer,
                        specificity,
                        order: rule.order,
                        inline: false,
                    },
                ));
            }
        }
    }
    // Inline style: author origin, above every author style rule within
    // its importance (Cascade 5's style-attribute cascade slot). The
    // expanded longhands are owned here so the candidates can borrow
    // them for the rest of the computation.
    let mut owned_inline: Vec<(String, bool, Longhand)> = Vec::new();
    let style_attribute = element
        .tree()
        .get_attribute(element.node_id(), "style")
        .map(ToString::to_string);
    if let Some(style) = style_attribute {
        for declaration in crate::declarations::DeclarationList::parse(&style).declarations() {
            if let Some(expanded) = expand_longhand(&declaration.name, declaration) {
                owned_inline.extend(
                    expanded
                        .into_iter()
                        .map(|(name, longhand)| (name, declaration.important, longhand)),
                );
            }
        }
    }
    for (name, important, longhand) in &owned_inline {
        candidates.push((
            name.as_str(),
            Candidate {
                longhand,
                group: group_of(Origin::Author, *important),
                layer: &[],
                specificity: u32::MAX,
                order: u32::MAX,
                inline: true,
            },
        ));
    }

    // -- Sort -----------------------------------------------------------
    let winners = winners_of(candidates);
    let mut ordered: Vec<(&str, &Longhand)> = winners.into_iter().collect();
    ordered.sort_by_key(|&(name, longhand)| apply_phase(name, longhand));

    // -- Compute --------------------------------------------------------
    let mut values = match parent {
        Some(parent) => ComputedValues::inherited_from(parent),
        None => ComputedValues::initial(),
    };
    // Which border-color sides received declarations; the rest resolve
    // to the element's own computed color (initial `currentcolor`).
    let mut border_color_set = [false; 4];
    for &(name, longhand) in &ordered {
        apply_longhand(
            &mut values,
            parent,
            name,
            longhand,
            rem_base,
            &mut border_color_set,
        );
    }
    // Un-declared border-color sides resolve against the own color.
    for (index, side) in [
        "border-top-color",
        "border-right-color",
        "border-bottom-color",
        "border-left-color",
    ]
    .iter()
    .enumerate()
    {
        if !border_color_set[index] {
            let color = values.color;
            match *side {
                "border-top-color" => values.border_color.top = color,
                "border-right-color" => values.border_color.right = color,
                "border-bottom-color" => values.border_color.bottom = color,
                _ => values.border_color.left = color,
            }
        }
    }
    values
}

/// Applies one winning longhand (or keyword) to the style under
/// construction. `parent` is the inherited baseline for `inherit`.
#[allow(clippy::too_many_lines)]
fn apply_longhand(
    values: &mut ComputedValues,
    parent: Option<&ComputedValues>,
    name: &str,
    longhand: &Longhand,
    rem_base: f32,
    border_color_set: &mut [bool; 4],
) {
    let parent_or_initial = || ComputedValues::initial();
    match (name, longhand) {
        // Custom properties: retained values, inherited.
        (custom_name, Longhand::Custom(value)) if custom_name.starts_with("--") => {
            values
                .custom_properties
                .insert(custom_name.to_string(), value.clone());
        }
        (custom_name, Longhand::Keyword(GlobalKeyword::Inherit))
            if custom_name.starts_with("--") =>
        {
            match parent.and_then(|p| p.custom_properties.get(custom_name)) {
                Some(value) => {
                    values
                        .custom_properties
                        .insert(custom_name.to_string(), value.clone());
                }
                None => {
                    values.custom_properties.remove(custom_name);
                }
            }
        }
        (custom_name, Longhand::Keyword(GlobalKeyword::Initial | GlobalKeyword::Unset))
            if custom_name.starts_with("--") =>
        {
            values.custom_properties.remove(custom_name);
        }
        // -- font-size --------------------------------------------------
        ("font-size", Longhand::FontSize(spec)) => {
            values.font_size = match spec {
                FontSizeSpec::Absolute(px) => *px,
                FontSizeSpec::Larger => absolute_size_step(values.font_size, true),
                FontSizeSpec::Smaller => absolute_size_step(values.font_size, false),
                FontSizeSpec::LengthPercent(LengthPercent::Px(px)) => *px,
                FontSizeSpec::LengthPercent(LengthPercent::Em(factor)) => values.font_size * factor,
                FontSizeSpec::LengthPercent(LengthPercent::Percent(p)) => {
                    values.font_size * p / 100.0
                }
                // The root element resolves rem against the initial font
                // size (CSS Values 4).
                FontSizeSpec::LengthPercent(LengthPercent::Rem(rem)) => rem_base * rem,
            };
        }
        ("font-size", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.font_size = parent_or_initial().font_size;
        }
        // `unset` on an inherited property is `inherit` (the initial
        // value only for the root, where there is no parent).
        ("font-size", Longhand::Keyword(GlobalKeyword::Unset)) => {
            values.font_size = parent.map_or(DEFAULT_FONT_SIZE, |p| p.font_size);
        }
        ("font-size", Longhand::Keyword(GlobalKeyword::Inherit)) => {
            values.font_size = parent.map_or(DEFAULT_FONT_SIZE, |p| p.font_size);
        }
        // -- font-weight ------------------------------------------------
        ("font-weight", Longhand::FontWeight(spec)) => {
            values.font_weight = match spec {
                FontWeightSpec::Normal => 400,
                FontWeightSpec::Bold => 700,
                FontWeightSpec::Weight(weight) => *weight,
                FontWeightSpec::Bolder => relative_weight(values.font_weight, true),
                FontWeightSpec::Lighter => relative_weight(values.font_weight, false),
            };
        }
        ("font-weight", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.font_weight = parent_or_initial().font_weight;
        }
        ("font-weight", Longhand::Keyword(GlobalKeyword::Unset | GlobalKeyword::Inherit)) => {
            values.font_weight = parent.map_or(400, |p| p.font_weight);
        }
        // -- color ------------------------------------------------------
        ("color", Longhand::Color(ColorValue::Rgba(color))) => values.color = *color,
        // `color: currentcolor` computes to the inherited color.
        ("color", Longhand::Color(ColorValue::CurrentColor)) => {}
        ("color", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.color = parent_or_initial().color;
        }
        ("color", Longhand::Keyword(GlobalKeyword::Unset | GlobalKeyword::Inherit)) => {
            values.color = parent.map_or(Color::BLACK, |p| p.color);
        }
        // -- line-height ------------------------------------------------
        ("line-height", Longhand::LineHeight(spec)) => {
            values.line_height = match spec {
                LineHeightSpec::Normal => LineHeight::Normal,
                LineHeightSpec::Number(number) => LineHeight::Number(*number),
                LineHeightSpec::LengthPercent(LengthPercent::Px(px)) => LineHeight::Length(*px),
                LineHeightSpec::LengthPercent(LengthPercent::Em(factor)) => {
                    LineHeight::Length(values.font_size * factor)
                }
                LineHeightSpec::LengthPercent(LengthPercent::Percent(p)) => {
                    LineHeight::Length(values.font_size * p / 100.0)
                }
                LineHeightSpec::LengthPercent(LengthPercent::Rem(rem)) => {
                    LineHeight::Length(rem_base * rem)
                }
            };
        }
        ("line-height", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.line_height = LineHeight::Normal;
        }
        ("line-height", Longhand::Keyword(GlobalKeyword::Unset | GlobalKeyword::Inherit)) => {
            values.line_height = parent.map_or(LineHeight::Normal, |p| p.line_height);
        }
        // -- the remaining working set ----------------------------------
        ("background-color", Longhand::BackgroundColor(ColorValue::Rgba(color)))
        | ("background", Longhand::BackgroundColor(ColorValue::Rgba(color))) => {
            values.background_color = *color;
        }
        ("background-color", Longhand::BackgroundColor(ColorValue::CurrentColor))
        | ("background", Longhand::BackgroundColor(ColorValue::CurrentColor)) => {
            values.background_color = values.color;
        }
        ("background-color", Longhand::Keyword(GlobalKeyword::Initial | GlobalKeyword::Unset))
        | ("background", Longhand::Keyword(GlobalKeyword::Initial | GlobalKeyword::Unset)) => {
            values.background_color = Color::TRANSPARENT;
        }
        ("background-color", Longhand::Keyword(GlobalKeyword::Inherit))
        | ("background", Longhand::Keyword(GlobalKeyword::Inherit)) => {
            values.background_color = parent.map_or(Color::TRANSPARENT, |p| p.background_color);
        }
        ("display", Longhand::Display(display)) => values.display = *display,
        ("display", Longhand::Keyword(GlobalKeyword::Initial | GlobalKeyword::Unset)) => {
            values.display = Display::Inline;
        }
        ("display", Longhand::Keyword(GlobalKeyword::Inherit)) => {
            values.display = parent.map_or(Display::Inline, |p| p.display);
        }
        ("font-style", Longhand::FontStyle(style)) => values.font_style = *style,
        ("font-style", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.font_style = FontStyle::Normal;
        }
        ("font-style", Longhand::Keyword(GlobalKeyword::Unset | GlobalKeyword::Inherit)) => {
            values.font_style = parent.map_or(FontStyle::Normal, |p| p.font_style);
        }
        ("font-family", Longhand::FontFamily(family)) => values.font_family = family.clone(),
        ("font-family", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.font_family = parent_or_initial().font_family;
        }
        ("font-family", Longhand::Keyword(GlobalKeyword::Unset | GlobalKeyword::Inherit)) => {
            values.font_family = parent.map_or_else(
                || parent_or_initial().font_family,
                |p| p.font_family.clone(),
            );
        }
        ("text-align", Longhand::TextAlign(align)) => values.text_align = *align,
        ("text-align", Longhand::Keyword(GlobalKeyword::Initial)) => {
            values.text_align = TextAlign::Start;
        }
        ("text-align", Longhand::Keyword(GlobalKeyword::Unset | GlobalKeyword::Inherit)) => {
            values.text_align = parent.map_or(TextAlign::Start, |p| p.text_align);
        }
        ("text-decoration-line", Longhand::TextDecorationLine(line)) => {
            values.text_decoration_line = *line;
        }
        (
            "text-decoration-line",
            Longhand::Keyword(GlobalKeyword::Initial | GlobalKeyword::Unset),
        ) => {
            values.text_decoration_line = TextDecorationLine::default();
        }
        ("text-decoration-line", Longhand::Keyword(GlobalKeyword::Inherit)) => {
            values.text_decoration_line =
                parent.map_or(TextDecorationLine::default(), |p| p.text_decoration_line);
        }
        // Margins: em/rem resolve against the own font size; percentages
        // stay symbolic for layout.
        (side, Longhand::MarginSide(sizing))
            if matches!(
                side,
                "margin-top" | "margin-right" | "margin-bottom" | "margin-left"
            ) =>
        {
            let resolved = resolve_margin(*sizing, values.font_size, rem_base);
            set_margin_side(values, side, resolved);
        }
        (side, Longhand::Keyword(keyword))
            if matches!(
                side,
                "margin-top" | "margin-right" | "margin-bottom" | "margin-left"
            ) =>
        {
            let initial = MarginSizing::LengthPercent(LengthPercent::Px(0.0));
            let resolved = match keyword {
                GlobalKeyword::Inherit => parent.map_or(initial, |p| margin_side_of(p, side)),
                _ => initial,
            };
            set_margin_side(values, side, resolved);
        }
        (side, Longhand::PaddingSide(length))
            if matches!(
                side,
                "padding-top" | "padding-right" | "padding-bottom" | "padding-left"
            ) =>
        {
            let resolved = resolve_padding(*length, values.font_size, rem_base);
            set_padding_side(values, side, resolved);
        }
        (side, Longhand::Keyword(keyword))
            if matches!(
                side,
                "padding-top" | "padding-right" | "padding-bottom" | "padding-left"
            ) =>
        {
            let initial = LengthPercent::Px(0.0);
            let resolved = match keyword {
                GlobalKeyword::Inherit => parent.map_or(initial, |p| padding_side_of(p, side)),
                _ => initial,
            };
            set_padding_side(values, side, resolved);
        }
        ("width", Longhand::Width(sizing)) => {
            values.width = resolve_sizing(*sizing, values.font_size, rem_base);
        }
        ("height", Longhand::Height(sizing)) => {
            values.height = resolve_sizing(*sizing, values.font_size, rem_base);
        }
        (
            name @ ("width" | "height"),
            Longhand::Keyword(GlobalKeyword::Initial | GlobalKeyword::Unset),
        ) => {
            if name == "width" {
                values.width = Sizing::Auto;
            } else {
                values.height = Sizing::Auto;
            }
        }
        (name @ ("width" | "height"), Longhand::Keyword(GlobalKeyword::Inherit)) => {
            if name == "width" {
                values.width = parent.map_or(Sizing::Auto, |p| p.width);
            } else {
                values.height = parent.map_or(Sizing::Auto, |p| p.height);
            }
        }
        (side, Longhand::BorderWidth(px))
            if matches!(
                side,
                "border-top-width"
                    | "border-right-width"
                    | "border-bottom-width"
                    | "border-left-width"
            ) =>
        {
            set_border(values, side, BorderField::Width(*px));
        }
        (side, Longhand::BorderStyle(style))
            if matches!(
                side,
                "border-top-style"
                    | "border-right-style"
                    | "border-bottom-style"
                    | "border-left-style"
            ) =>
        {
            set_border(values, side, BorderField::Style(*style));
        }
        (side, Longhand::BorderColor(color))
            if matches!(
                side,
                "border-top-color"
                    | "border-right-color"
                    | "border-bottom-color"
                    | "border-left-color"
            ) =>
        {
            set_border(
                values,
                side,
                BorderField::Color(resolve_color(*color, values.color)),
            );
            *border_color_set = mark_side(side, *border_color_set);
        }
        (side, Longhand::Keyword(keyword))
            if matches!(
                side,
                "border-top-width"
                    | "border-right-width"
                    | "border-bottom-width"
                    | "border-left-width"
                    | "border-top-style"
                    | "border-right-style"
                    | "border-bottom-style"
                    | "border-left-style"
                    | "border-top-color"
                    | "border-right-color"
                    | "border-bottom-color"
                    | "border-left-color"
            ) =>
        {
            if side.ends_with("-width") {
                let px = match keyword {
                    GlobalKeyword::Inherit => parent.map_or(3.0, |p| border_side_of(p, side).0),
                    _ => 3.0,
                };
                set_border(values, side, BorderField::Width(px));
            } else if side.ends_with("-style") {
                let style = match keyword {
                    GlobalKeyword::Inherit => {
                        parent.map_or(BorderStyle::None, |p| border_side_of(p, side).1)
                    }
                    _ => BorderStyle::None,
                };
                set_border(values, side, BorderField::Style(style));
            } else {
                let color = match keyword {
                    GlobalKeyword::Inherit => {
                        parent.map_or(values.color, |p| border_side_of(p, side).2)
                    }
                    // Initial border-color is currentcolor: the own
                    // computed color.
                    _ => values.color,
                };
                set_border(values, side, BorderField::Color(color));
                *border_color_set = mark_side(side, *border_color_set);
            }
        }
        // Unreachable winners (revert never reaches compute; the parse
        // layer only produces known names) — ignored, not a panic.
        _ => {}
    }
}

/// Resolves a margin side's font-relative units (percent stays for
/// layout).
fn resolve_margin(sizing: MarginSizing, font_size: f32, rem_base: f32) -> MarginSizing {
    match sizing {
        MarginSizing::Auto => MarginSizing::Auto,
        MarginSizing::LengthPercent(length) => {
            MarginSizing::LengthPercent(resolve_padding(length, font_size, rem_base))
        }
    }
}

/// Resolves a padding/length side's font-relative units (em against the
/// own font size, rem against the root; percentages stay for layout).
fn resolve_padding(length: LengthPercent, font_size: f32, rem_base: f32) -> LengthPercent {
    match length {
        LengthPercent::Px(px) => LengthPercent::Px(px),
        LengthPercent::Em(factor) => LengthPercent::Px(font_size * factor),
        LengthPercent::Rem(rem) => LengthPercent::Px(rem_base * rem),
        LengthPercent::Percent(p) => LengthPercent::Percent(p),
    }
}

/// Resolves a width/height form.
fn resolve_sizing(sizing: Sizing, font_size: f32, rem_base: f32) -> Sizing {
    match sizing {
        Sizing::Auto => Sizing::Auto,
        Sizing::LengthPercent(length) => {
            Sizing::LengthPercent(resolve_padding(length, font_size, rem_base))
        }
    }
}

/// Resolves a color value against the element's computed color.
fn resolve_color(value: ColorValue, own: Color) -> Color {
    match value {
        ColorValue::Rgba(color) => color,
        ColorValue::CurrentColor => own,
    }
}

/// Which border field a helper should write.
enum BorderField {
    /// The width (px).
    Width(f32),
    /// The line style.
    Style(BorderStyle),
    /// The color.
    Color(Color),
}

/// Writes one border side field.
fn set_border(values: &mut ComputedValues, side: &str, field: BorderField) {
    match field {
        BorderField::Width(px) => match side {
            "border-top-width" => values.border_width.top = px,
            "border-right-width" => values.border_width.right = px,
            "border-bottom-width" => values.border_width.bottom = px,
            _ => values.border_width.left = px,
        },
        BorderField::Style(style) => match side {
            "border-top-style" => values.border_style.top = style,
            "border-right-style" => values.border_style.right = style,
            "border-bottom-style" => values.border_style.bottom = style,
            _ => values.border_style.left = style,
        },
        BorderField::Color(color) => match side {
            "border-top-color" => values.border_color.top = color,
            "border-right-color" => values.border_color.right = color,
            "border-bottom-color" => values.border_color.bottom = color,
            _ => values.border_color.left = color,
        },
    }
}

/// Marks a border-color side as explicitly declared.
fn mark_side(side: &str, mut flags: [bool; 4]) -> [bool; 4] {
    match side {
        "border-top-color" => flags[0] = true,
        "border-right-color" => flags[1] = true,
        "border-bottom-color" => flags[2] = true,
        _ => flags[3] = true,
    }
    flags
}

/// One margin side of a parent style.
fn margin_side_of(parent: &ComputedValues, side: &str) -> MarginSizing {
    match side {
        "margin-top" => parent.margin.top,
        "margin-right" => parent.margin.right,
        "margin-bottom" => parent.margin.bottom,
        _ => parent.margin.left,
    }
}

/// One padding side of a parent style.
fn padding_side_of(parent: &ComputedValues, side: &str) -> LengthPercent {
    match side {
        "padding-top" => parent.padding.top,
        "padding-right" => parent.padding.right,
        "padding-bottom" => parent.padding.bottom,
        _ => parent.padding.left,
    }
}

/// One border side of a parent style: (width, style, color).
fn border_side_of(parent: &ComputedValues, side: &str) -> (f32, BorderStyle, Color) {
    match side {
        "border-top-width" | "border-top-style" | "border-top-color" => (
            parent.border_width.top,
            parent.border_style.top,
            parent.border_color.top,
        ),
        "border-right-width" | "border-right-style" | "border-right-color" => (
            parent.border_width.right,
            parent.border_style.right,
            parent.border_color.right,
        ),
        "border-bottom-width" | "border-bottom-style" | "border-bottom-color" => (
            parent.border_width.bottom,
            parent.border_style.bottom,
            parent.border_color.bottom,
        ),
        _ => (
            parent.border_width.left,
            parent.border_style.left,
            parent.border_color.left,
        ),
    }
}

/// Writes one margin side.
fn set_margin_side(values: &mut ComputedValues, side: &str, resolved: MarginSizing) {
    match side {
        "margin-top" => values.margin.top = resolved,
        "margin-right" => values.margin.right = resolved,
        "margin-bottom" => values.margin.bottom = resolved,
        _ => values.margin.left = resolved,
    }
}

/// Writes one padding side.
fn set_padding_side(values: &mut ComputedValues, side: &str, resolved: LengthPercent) {
    match side {
        "padding-top" => values.padding.top = resolved,
        "padding-right" => values.padding.right = resolved,
        "padding-bottom" => values.padding.bottom = resolved,
        _ => values.padding.left = resolved,
    }
}

/// The computed styles of a document's elements, keyed by node id. A
/// snapshot: the tree is borrowed immutably for the computation, and
/// later mutations invalidate entries by id (stale ids simply have no
/// entry). The map is a plain hash map — element ids carry no ordering,
/// and the computation's determinism comes from the tree walk, not the
/// map.
#[derive(Debug, Clone, Default)]
pub struct ComputedStyleMap {
    styles: std::collections::HashMap<NodeId, ComputedValues>,
}

impl ComputedStyleMap {
    /// The computed style of an element (or `None` for non-elements and
    /// stale ids).
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&ComputedValues> {
        self.styles.get(&id)
    }

    /// The number of computed elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    /// Whether no styles were computed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }
}

/// Computes the initial style of every element in the document tree.
#[must_use]
pub fn compute_styles(tree: &Tree, set: &StyleSet) -> ComputedStyleMap {
    compute_styles_with(tree, set, &MatchOptions::default())
}

/// [`compute_styles`] with document-level match options (the `:target`
/// element).
#[must_use]
pub fn compute_styles_with(
    tree: &Tree,
    set: &StyleSet,
    options: &MatchOptions,
) -> ComputedStyleMap {
    let rules = prepare(set);
    let mut map = ComputedStyleMap::default();
    let Some(document) = tree.document() else {
        return map;
    };
    // The root element's rem base is the initial font size; everything
    // below resolves against the root's computed size (pre-order
    // guarantees the root is computed first).
    let mut rem_base = DEFAULT_FONT_SIZE;
    for id in tree.descendants(document) {
        let Some(element) = VeyraElement::with_target(tree, id, options.target_id) else {
            continue;
        };
        let is_root = element.is_root();
        let element_rem_base = if is_root { DEFAULT_FONT_SIZE } else { rem_base };
        let parent = parent_computed(tree, id, &map);
        let values = compute_element(&element, &rules, parent, element_rem_base);
        if is_root {
            rem_base = values.font_size;
        }
        map.styles.insert(id, values);
    }
    map
}

/// The nearest ancestor element's computed style (`None` for the root —
/// the initial values are the inherited baseline there).
fn parent_computed<'a>(
    tree: &Tree,
    id: NodeId,
    map: &'a ComputedStyleMap,
) -> Option<&'a ComputedValues> {
    tree.ancestors(id)
        .find(|ancestor| matches!(tree.data(*ancestor), Some(NodeData::Element { .. })))
        .and_then(|ancestor| map.get(ancestor))
}

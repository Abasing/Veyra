//! Property values: retained declaration tokens parsed into typed
//! values, and the computed-value model (CSS Cascade 5 §4/§5, CSS Values
//! 4 unit resolution) for the Phase 3 working property set.
//!
//! The cascade batch's second half. Batch 1 retains declaration values
//! as token trees; this module interprets them. The working set is the
//! inline-layout subset the rendering pipeline documents for Phase 3
//! (color/background, font group, text alignment, box spacing) — every
//! property parses from the retained value's canonical serialization
//! with `cssparser`, so unknown/invalid values are typed `None` (the
//! cascade drops them; nothing panics on malformed input).
//!
//! # Computed-value model
//!
//! [`ComputedValues`] is the per-element result: inherited properties
//! copy from the parent, non-inherited properties start at their
//! initial value, and winning declarations override. Relative units
//! resolve at compute time (em against the element's or parent's font
//! size per property, rem against the root font size, percentages for
//! lengths kept for layout). `currentcolor` resolves against the
//! element's own computed `color`.
//!
//! # Deferred with a roadmap home (each parses to `None` today)
//!
//! `calc()` and the font-relative `ex`/`ch` units (Values 4 math
//! functions are their own work item); `var()` substitution (custom
//! properties are captured and inherited; a `var()` reference makes the
//! declaration invalid at computed-value time per the spec's fallback
//! rule); color spaces beyond rgb/hsl hex and named (lab/lch/oklab/
//! oklch/color(), system colors); the `font`, `background` (beyond the
//! color layer), `text-decoration` (beyond the line-keyword layer), and
//! per-side `border-*` shorthands; logical (flow-relative) box properties
//! (R3, writing modes).

use std::collections::BTreeMap;

use cssparser::{Parser, Token};

use crate::tokens::RetainedValue;

/// The default (initial) font size in px — the root font size absent any
/// declaration (`medium`, 16px, the CSS Fonts 4 initial-value mapping).
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// An sRGB color channel-triplet with alpha (CSS Color 4's legacy form,
/// which is what every Phase 3 working-set production computes to).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
    /// Alpha channel (0–255; 255 opaque).
    pub a: u8,
}

impl Color {
    /// Builds an opaque color.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Fully transparent black (`transparent`).
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// Opaque black (the initial `color`).
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
}

/// A parsed color: concrete RGBA or the `currentcolor` keyword (resolved
/// against the element's computed `color` at compute time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorValue {
    /// A concrete color.
    Rgba(Color),
    /// `currentcolor`.
    CurrentColor,
}

/// A length or percentage. Lengths other than px are converted to px at
/// parse time (CSS Values 4 absolute-unit table, 96dpi reference pixel);
/// `em`/`rem`/percentages stay symbolic until compute, because their
/// resolution target differs per property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthPercent {
    /// An absolute length in px.
    Px(f32),
    /// `em` — font-relative (property-specific resolution).
    Em(f32),
    /// `rem` — root-font-relative.
    Rem(f32),
    /// A percentage (layout-time resolution for box properties).
    Percent(f32),
}

impl LengthPercent {
    /// Resolves against a font size (px): em → size × factor, percent →
    /// size × p/100; px/rem are absolute here (rem resolution happens
    /// with the root size before this helper is reached for those).
    #[must_use]
    pub fn resolve_against(self, font_size: f32) -> f32 {
        match self {
            LengthPercent::Px(px) => px,
            LengthPercent::Em(factor) => font_size * factor,
            LengthPercent::Rem(rem) => rem, // caller resolves first
            LengthPercent::Percent(p) => font_size * p / 100.0,
        }
    }
}

/// The `display` working set (inline-layout Phase 3; flex/grid/table
/// arrive with their layout batches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// `none` — not rendered.
    None,
    /// `block`.
    Block,
    /// `inline` (the initial value).
    Inline,
    /// `inline-block`.
    InlineBlock,
}

/// `font-style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    /// `normal`.
    Normal,
    /// `italic`.
    Italic,
    /// `oblique` (the angle is not modeled yet).
    Oblique,
}

/// One `font-family` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamilyName {
    /// The family name (idents joined with spaces, or the unquoted
    /// string content).
    pub name: String,
    /// Whether it is a generic family keyword (`serif`, `monospace`, …).
    pub generic: bool,
}

/// The `font-family` list (specified form; computed == specified for the
/// list itself — font selection is `veyra-text`'s concern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily(pub Vec<FontFamilyName>);

/// `text-align`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// `start` (the initial value).
    Start,
    /// `end`.
    End,
    /// `left`.
    Left,
    /// `right`.
    Right,
    /// `center`.
    Center,
    /// `justify`.
    Justify,
}

/// `line-height` (computed: normal, a unitless multiplier, or a px
/// length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// `normal` (the initial value; a layout-time default).
    Normal,
    /// A unitless number (multiplies the font size at layout).
    Number(f32),
    /// A computed length in px.
    Length(f32),
}

/// `text-decoration-line`: a set of line keywords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextDecorationLine(u8);

impl TextDecorationLine {
    const UNDERLINE: u8 = 1;
    const OVERLINE: u8 = 2;
    const LINE_THROUGH: u8 = 4;
    const BLINK: u8 = 8;

    /// Whether no line is set (`none`).
    #[must_use]
    pub fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Whether `underline` is set.
    #[must_use]
    pub fn underline(self) -> bool {
        self.0 & Self::UNDERLINE != 0
    }

    /// Whether `overline` is set.
    #[must_use]
    pub fn overline(self) -> bool {
        self.0 & Self::OVERLINE != 0
    }

    /// Whether `line-through` is set.
    #[must_use]
    pub fn line_through(self) -> bool {
        self.0 & Self::LINE_THROUGH != 0
    }
}

/// Margin sizing: a length/percentage or `auto`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarginSizing {
    /// `auto`.
    Auto,
    /// A length or percentage.
    LengthPercent(LengthPercent),
}

/// `width`/`height` sizing: `auto` or a length/percentage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sizing {
    /// `auto` (the initial value).
    Auto,
    /// A length or percentage.
    LengthPercent(LengthPercent),
}

/// Border line styles (CSS Backgrounds 3 §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    /// `none` (the initial value).
    None,
    /// `hidden`.
    Hidden,
    /// `solid`.
    Solid,
    /// `dashed`.
    Dashed,
    /// `dotted`.
    Dotted,
    /// `double`.
    Double,
    /// `groove`.
    Groove,
    /// `ridge`.
    Ridge,
    /// `inset`.
    Inset,
    /// `outset`.
    Outset,
}

/// The four physical sides (logical axes arrive with writing modes, R3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sides<T> {
    /// Top side.
    pub top: T,
    /// Right side.
    pub right: T,
    /// Bottom side.
    pub bottom: T,
    /// Left side.
    pub left: T,
}

impl<T: Copy> Sides<T> {
    /// Builds all four sides from the 1–4-value shorthand expansion
    /// (`top right bottom left`, clockwise from top).
    #[must_use]
    pub fn from_shorthand(values: [T; 4]) -> Self {
        Sides {
            top: values[0],
            right: values[1],
            bottom: values[2],
            left: values[3],
        }
    }
}

/// The `font-size` specified forms (compute-time resolution differs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontSizeSpec {
    /// An absolute-size keyword or length already in px.
    Absolute(f32),
    /// `larger` — one absolute-size step up from the parent.
    Larger,
    /// `smaller` — one absolute-size step down from the parent.
    Smaller,
    /// A length/percentage (em/percent resolve against the parent font
    /// size; rem against the root).
    LengthPercent(LengthPercent),
}

/// The `font-weight` specified forms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontWeightSpec {
    /// `normal` (400).
    Normal,
    /// `bold` (700).
    Bold,
    /// `bolder` — relative to the parent.
    Bolder,
    /// `lighter` — relative to the parent.
    Lighter,
    /// A numeric weight (1–1000).
    Weight(u16),
}

/// The computed style of one element: the cascade output the layout and
/// paint batches consume.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedValues {
    /// `color` (inherited).
    pub color: Color,
    /// `background-color` (initial `transparent`).
    pub background_color: Color,
    /// `display` (initial `inline`).
    pub display: Display,
    /// Computed font size in px (inherited as a length).
    pub font_size: f32,
    /// Computed font weight (1–1000).
    pub font_weight: u16,
    /// `font-style` (inherited).
    pub font_style: FontStyle,
    /// `font-family` list (inherited).
    pub font_family: FontFamily,
    /// `line-height` (inherited).
    pub line_height: LineHeight,
    /// `text-align` (inherited).
    pub text_align: TextAlign,
    /// `text-decoration-line` (not inherited).
    pub text_decoration_line: TextDecorationLine,
    /// Margins (initial 0).
    pub margin: Sides<MarginSizing>,
    /// Paddings (initial 0).
    pub padding: Sides<LengthPercent>,
    /// `width`.
    pub width: Sizing,
    /// `height`.
    pub height: Sizing,
    /// Border styles (initial `none`).
    pub border_style: Sides<BorderStyle>,
    /// Border widths in px (initial `medium`, 3px).
    pub border_width: Sides<f32>,
    /// Border colors (initial `currentcolor`, resolved to the element's
    /// own computed color).
    pub border_color: Sides<Color>,
    /// Captured custom properties (`--*`, values retained; inherited
    /// with the element). `var()` substitution is the recorded later
    /// work item.
    pub custom_properties: BTreeMap<String, RetainedValue>,
}

impl ComputedValues {
    /// The initial values (CSS property index; root font size 16px).
    #[must_use]
    pub fn initial() -> Self {
        ComputedValues {
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            display: Display::Inline,
            font_size: DEFAULT_FONT_SIZE,
            font_weight: 400,
            font_style: FontStyle::Normal,
            font_family: FontFamily(vec![FontFamilyName {
                name: "serif".to_string(),
                generic: true,
            }]),
            line_height: LineHeight::Normal,
            text_align: TextAlign::Start,
            text_decoration_line: TextDecorationLine::default(),
            margin: Sides {
                top: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
                right: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
                bottom: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
                left: MarginSizing::LengthPercent(LengthPercent::Px(0.0)),
            },
            padding: Sides {
                top: LengthPercent::Px(0.0),
                right: LengthPercent::Px(0.0),
                bottom: LengthPercent::Px(0.0),
                left: LengthPercent::Px(0.0),
            },
            width: Sizing::Auto,
            height: Sizing::Auto,
            border_style: Sides {
                top: BorderStyle::None,
                right: BorderStyle::None,
                bottom: BorderStyle::None,
                left: BorderStyle::None,
            },
            border_width: Sides {
                top: 3.0,
                right: 3.0,
                bottom: 3.0,
                left: 3.0,
            },
            border_color: Sides {
                top: Color::BLACK,
                right: Color::BLACK,
                bottom: Color::BLACK,
                left: Color::BLACK,
            },
            custom_properties: BTreeMap::new(),
        }
    }

    /// The style a fresh element starts from: inherited properties from
    /// `parent`, non-inherited at initial. (`currentcolor` border colors
    /// resolve against the element's own color once computed.)
    #[must_use]
    pub fn inherited_from(parent: &ComputedValues) -> Self {
        let mut values = Self::initial();
        values.color = parent.color;
        values.font_size = parent.font_size;
        values.font_weight = parent.font_weight;
        values.font_style = parent.font_style;
        values.font_family = parent.font_family.clone();
        values.line_height = parent.line_height;
        values.text_align = parent.text_align;
        values.custom_properties = parent.custom_properties.clone();
        // Initial border-color is currentcolor; keep the parent color
        // until the element's own color is known (compute fixes it up).
        values.border_color = Sides {
            top: parent.color,
            right: parent.color,
            bottom: parent.color,
            left: parent.color,
        };
        values
    }
}

/// Drives `cssparser` over a value's canonical serialization. The parse
/// function consumes the token stream; the value must be fully consumed
/// (a trailing token is a parse failure — CSS Syntax's definition of a
/// valid component-value list).
pub(crate) fn parse_value<T>(
    value: &RetainedValue,
    parse: impl FnOnce(&mut Parser<'_>) -> Option<T>,
) -> Option<T> {
    let text = value.to_css_string_trimmed();
    let mut parser = Parser::new(&text);
    let result = parse(&mut parser)?;
    // Trailing whitespace/comments are insignificant (is_exhausted
    // ignores them); any other trailing token is a parse failure.
    if parser.is_exhausted() {
        Some(result)
    } else {
        None
    }
}

/// Parses the CSS-wide keywords that precede any property grammar
/// (`inherit`, `initial`, `unset`; `revert`/`revert-layer` are cascade
/// concerns handled before parsing). ASCII case-insensitive per CSS.
pub(crate) fn global_keyword(value: &RetainedValue) -> Option<GlobalKeyword> {
    parse_value(value, |parser| match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "inherit" => Some(GlobalKeyword::Inherit),
            "initial" => Some(GlobalKeyword::Initial),
            "unset" => Some(GlobalKeyword::Unset),
            "revert" => Some(GlobalKeyword::Revert),
            "revert-layer" => Some(GlobalKeyword::RevertLayer),
            _ => None,
        },
        _ => None,
    })
}

/// The CSS-wide keywords the cascade/compute steps implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalKeyword {
    /// `inherit` — the parent's computed value.
    Inherit,
    /// `initial` — the property's initial value.
    Initial,
    /// `unset` — inherit if inherited, initial otherwise.
    Unset,
    /// `revert` — roll the cascade back to the previous origin.
    Revert,
    /// `revert-layer` — behaves as `revert` this batch (layer rollback
    /// is the recorded deferral).
    RevertLayer,
}

/// Whether the value references `var()` — the invalid-at-computed-value-
/// time marker until substitution lands.
pub(crate) fn references_var(value: &RetainedValue) -> bool {
    value
        .to_css_string_trimmed()
        .to_ascii_lowercase()
        .contains("var(")
}

/// Parses a color: named keywords, hex, `rgb()`/`rgba()` (legacy comma
/// and modern space syntax), `hsl()`/`hsla()`, `transparent`,
/// `currentcolor`.
pub(crate) fn parse_color_value(value: &RetainedValue) -> Option<ColorValue> {
    parse_value(value, parse_one_color)
}

/// Parses one color from an in-progress parser.
fn parse_one_color(parser: &mut Parser<'_>) -> Option<ColorValue> {
    match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "transparent" => Some(ColorValue::Rgba(Color::TRANSPARENT)),
            "currentcolor" => Some(ColorValue::CurrentColor),
            other => parse_color::NAMED_COLORS
                .iter()
                .find(|(keyword, _)| *keyword == other)
                .map(|(_, hex)| ColorValue::Rgba(hex_color(hex))),
        },
        Ok(Token::Hash(hash)) | Ok(Token::IDHash(hash)) => parse_color::parse_hex(hash),
        Ok(Token::Function(name)) => {
            let name = name.to_ascii_lowercase();
            // Function arguments live in the nested block (cssparser's
            // pairing of the `(` token); the Option-valued helpers map
            // onto the block parser's Result with a typed failure.
            let mut nested = |parse: fn(&mut Parser<'_>) -> Option<ColorValue>| {
                parser
                    .parse_nested_block(move |input| {
                        parse(input).ok_or(cssparser::ParseError::<()>::from(
                            cssparser::BasicParseError {
                                kind: cssparser::BasicParseErrorKind::EndOfInput,
                            },
                        ))
                    })
                    .ok()
            };
            match name.as_str() {
                "rgb" | "rgba" => nested(parse_color::parse_rgb_function),
                "hsl" | "hsla" => nested(parse_color::parse_hsl_function),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Converts a `#rrggbb`-style hex string (with or without the `#`) to a
/// color.
fn hex_color(hex: &str) -> Color {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    Color::rgb(r, g, b)
}

/// Color parsing helpers (the named table and the function forms).
mod parse_color {
    use super::{Color, ColorValue};

    /// The CSS Color 4 extended named colors (148 keywords including the
    /// `gray`/`grey` pairs, `cyan`/`magenta` aliases, and
    /// `rebeccapurple`), as `#rrggbb`.
    pub(crate) const NAMED_COLORS: &[(&str, &str)] = &[
        ("aliceblue", "#f0f8ff"),
        ("antiquewhite", "#faebd7"),
        ("aqua", "#00ffff"),
        ("aquamarine", "#7fffd4"),
        ("azure", "#f0ffff"),
        ("beige", "#f5f5dc"),
        ("bisque", "#ffe4c4"),
        ("black", "#000000"),
        ("blanchedalmond", "#ffebcd"),
        ("blue", "#0000ff"),
        ("blueviolet", "#8a2be2"),
        ("brown", "#a52a2a"),
        ("burlywood", "#deb887"),
        ("cadetblue", "#5f9ea0"),
        ("chartreuse", "#7fff00"),
        ("chocolate", "#d2691e"),
        ("coral", "#ff7f50"),
        ("cornflowerblue", "#6495ed"),
        ("cornsilk", "#fff8dc"),
        ("crimson", "#dc143c"),
        ("cyan", "#00ffff"),
        ("darkblue", "#00008b"),
        ("darkcyan", "#008b8b"),
        ("darkgoldenrod", "#b8860b"),
        ("darkgray", "#a9a9a9"),
        ("darkgreen", "#006400"),
        ("darkgrey", "#a9a9a9"),
        ("darkkhaki", "#bdb76b"),
        ("darkmagenta", "#8b008b"),
        ("darkolivegreen", "#556b2f"),
        ("darkorange", "#ff8c00"),
        ("darkorchid", "#9932cc"),
        ("darkred", "#8b0000"),
        ("darksalmon", "#e9967a"),
        ("darkseagreen", "#8fbc8f"),
        ("darkslateblue", "#483d8b"),
        ("darkslategray", "#2f4f4f"),
        ("darkslategrey", "#2f4f4f"),
        ("darkturquoise", "#00ced1"),
        ("darkviolet", "#9400d3"),
        ("deeppink", "#ff1493"),
        ("deepskyblue", "#00bfff"),
        ("dimgray", "#696969"),
        ("dimgrey", "#696969"),
        ("dodgerblue", "#1e90ff"),
        ("firebrick", "#b22222"),
        ("floralwhite", "#fffaf0"),
        ("forestgreen", "#228b22"),
        ("fuchsia", "#ff00ff"),
        ("gainsboro", "#dcdcdc"),
        ("ghostwhite", "#f8f8ff"),
        ("gold", "#ffd700"),
        ("goldenrod", "#daa520"),
        ("gray", "#808080"),
        ("green", "#008000"),
        ("greenyellow", "#adff2f"),
        ("grey", "#808080"),
        ("honeydew", "#f0fff0"),
        ("hotpink", "#ff69b4"),
        ("indianred", "#cd5c5c"),
        ("indigo", "#4b0082"),
        ("ivory", "#fffff0"),
        ("khaki", "#f0e68c"),
        ("lavender", "#e6e6fa"),
        ("lavenderblush", "#fff0f5"),
        ("lawngreen", "#7cfc00"),
        ("lemonchiffon", "#fffacd"),
        ("lightblue", "#add8e6"),
        ("lightcoral", "#f08080"),
        ("lightcyan", "#e0ffff"),
        ("lightgoldenrodyellow", "#fafad2"),
        ("lightgray", "#d3d3d3"),
        ("lightgreen", "#90ee90"),
        ("lightgrey", "#d3d3d3"),
        ("lightpink", "#ffb6c1"),
        ("lightsalmon", "#ffa07a"),
        ("lightseagreen", "#20b2aa"),
        ("lightskyblue", "#87cefa"),
        ("lightslategray", "#778899"),
        ("lightslategrey", "#778899"),
        ("lightsteelblue", "#b0c4de"),
        ("lightyellow", "#ffffe0"),
        ("lime", "#00ff00"),
        ("limegreen", "#32cd32"),
        ("linen", "#faf0e6"),
        ("magenta", "#ff00ff"),
        ("maroon", "#800000"),
        ("mediumaquamarine", "#66cdaa"),
        ("mediumblue", "#0000cd"),
        ("mediumorchid", "#ba55d3"),
        ("mediumpurple", "#9370db"),
        ("mediumseagreen", "#3cb371"),
        ("mediumslateblue", "#7b68ee"),
        ("mediumspringgreen", "#00fa9a"),
        ("mediumturquoise", "#48d1cc"),
        ("mediumvioletred", "#c71585"),
        ("midnightblue", "#191970"),
        ("mintcream", "#f5fffa"),
        ("mistyrose", "#ffe4e1"),
        ("moccasin", "#ffe4b5"),
        ("navajowhite", "#ffdead"),
        ("navy", "#000080"),
        ("oldlace", "#fdf5e6"),
        ("olive", "#808000"),
        ("olivedrab", "#6b8e23"),
        ("orange", "#ffa500"),
        ("orangered", "#ff4500"),
        ("orchid", "#da70d6"),
        ("palegoldenrod", "#eee8aa"),
        ("palegreen", "#98fb98"),
        ("paleturquoise", "#afeeee"),
        ("palevioletred", "#db7093"),
        ("papayawhip", "#ffefd5"),
        ("peachpuff", "#ffdab9"),
        ("peru", "#cd853f"),
        ("pink", "#ffc0cb"),
        ("plum", "#dda0dd"),
        ("powderblue", "#b0e0e6"),
        ("purple", "#800080"),
        ("rebeccapurple", "#663399"),
        ("red", "#ff0000"),
        ("rosybrown", "#bc8f8f"),
        ("royalblue", "#4169e1"),
        ("saddlebrown", "#8b4513"),
        ("salmon", "#fa8072"),
        ("sandybrown", "#f4a460"),
        ("seagreen", "#2e8b57"),
        ("seashell", "#fff5ee"),
        ("sienna", "#a0522d"),
        ("silver", "#c0c0c0"),
        ("skyblue", "#87ceeb"),
        ("slateblue", "#6a5acd"),
        ("slategray", "#708090"),
        ("slategrey", "#708090"),
        ("snow", "#fffafa"),
        ("springgreen", "#00ff7f"),
        ("steelblue", "#4682b4"),
        ("tan", "#d2b48c"),
        ("teal", "#008080"),
        ("thistle", "#d8bfd8"),
        ("tomato", "#ff6347"),
        ("turquoise", "#40e0d0"),
        ("violet", "#ee82ee"),
        ("wheat", "#f5deb3"),
        ("white", "#ffffff"),
        ("whitesmoke", "#f5f5f5"),
        ("yellow", "#ffff00"),
        ("yellowgreen", "#9acd32"),
    ];

    /// Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`.
    pub(crate) fn parse_hex(hash: &str) -> Option<ColorValue> {
        let digits: Vec<u8> = hash
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<Vec<_>>>()?;
        match digits.len() {
            3 => Some(ColorValue::Rgba(Color {
                r: expand_nibble(digits[0]),
                g: expand_nibble(digits[1]),
                b: expand_nibble(digits[2]),
                a: 255,
            })),
            4 => Some(ColorValue::Rgba(Color {
                r: expand_nibble(digits[0]),
                g: expand_nibble(digits[1]),
                b: expand_nibble(digits[2]),
                a: expand_nibble(digits[3]),
            })),
            6 => {
                let hex: String = hash.chars().take(6).collect();
                Some(ColorValue::Rgba(super::hex_color(&hex)))
            }
            8 => {
                let hex: String = hash.chars().collect();
                let (rgb, alpha) = hex.split_at(6);
                let r = u8::from_str_radix(&rgb[0..2], 16).ok()?;
                let g = u8::from_str_radix(&rgb[2..4], 16).ok()?;
                let b = u8::from_str_radix(&rgb[4..6], 16).ok()?;
                let a = u8::from_str_radix(alpha, 16).ok()?;
                Some(ColorValue::Rgba(Color { r, g, b, a }))
            }
            _ => None,
        }
    }

    fn expand_nibble(digit: u8) -> u8 {
        digit * 17
    }

    /// Parses `rgb()`/`rgba()` contents (inside the nested block). The
    /// two grammars are disambiguated by attempt: modern space syntax
    /// first (its channel list cannot contain top-level commas), then
    /// the legacy comma form.
    pub(crate) fn parse_rgb_function(parser: &mut cssparser::Parser<'_>) -> Option<ColorValue> {
        let start = parser.state();
        let modern = (|| {
            let r = channel(parser)?;
            let g = channel(parser)?;
            let b = channel(parser)?;
            let a = optional_slash_alpha(parser).unwrap_or(1.0);
            parser.expect_exhausted().ok()?;
            Some(ColorValue::Rgba(rgba_from_channels(r, g, b, a)))
        })();
        if modern.is_some() {
            return modern;
        }
        parser.reset(&start);
        let r = channel(parser)?;
        expect_comma(parser)?;
        let g = channel(parser)?;
        expect_comma(parser)?;
        let b = channel(parser)?;
        let a = optional_comma_alpha(parser).unwrap_or(1.0);
        parser.expect_exhausted().ok()?;
        Some(ColorValue::Rgba(rgba_from_channels(r, g, b, a)))
    }

    /// Parses `hsl()`/`hsla()` contents: hue as angle or number,
    /// saturation/lightness as percentage or number, optional alpha —
    /// same modern-then-legacy disambiguation.
    pub(crate) fn parse_hsl_function(parser: &mut cssparser::Parser<'_>) -> Option<ColorValue> {
        let start = parser.state();
        let modern = (|| {
            let h = hue(parser)?;
            let s = saturation(parser)?;
            let l = saturation(parser)?;
            let a = optional_slash_alpha(parser).unwrap_or(1.0);
            parser.expect_exhausted().ok()?;
            Some(ColorValue::Rgba(hsl_to_rgb(
                h,
                s.clamp(0.0, 1.0),
                l.clamp(0.0, 1.0),
                a,
            )))
        })();
        if modern.is_some() {
            return modern;
        }
        parser.reset(&start);
        let h = hue(parser)?;
        expect_comma(parser)?;
        let s = saturation(parser)?;
        expect_comma(parser)?;
        let l = saturation(parser)?;
        let a = optional_comma_alpha(parser).unwrap_or(1.0);
        parser.expect_exhausted().ok()?;
        Some(ColorValue::Rgba(hsl_to_rgb(
            h,
            s.clamp(0.0, 1.0),
            l.clamp(0.0, 1.0),
            a,
        )))
    }

    fn expect_comma(parser: &mut cssparser::Parser<'_>) -> Option<()> {
        match parser.next() {
            Ok(cssparser::Token::Comma) => Some(()),
            _ => None,
        }
    }

    /// One channel: number (0–255) or percentage (0–100%), clamped.
    fn channel(parser: &mut cssparser::Parser<'_>) -> Option<f32> {
        match parser.next() {
            Ok(cssparser::Token::Number { value, .. }) => Some(value.clamp(0.0, 255.0)),
            Ok(cssparser::Token::Percentage { unit_value, .. }) => {
                Some((unit_value * 255.0).clamp(0.0, 255.0))
            }
            _ => None,
        }
    }

    /// Saturation/lightness: percentage or bare number (modern syntax),
    /// as a 0–1 fraction.
    fn saturation(parser: &mut cssparser::Parser<'_>) -> Option<f32> {
        match parser.next() {
            Ok(cssparser::Token::Percentage { unit_value, .. }) => Some(*unit_value),
            Ok(cssparser::Token::Number { value, .. }) => Some(*value),
            _ => None,
        }
    }

    /// Hue: a number (degrees) or any angle unit.
    fn hue(parser: &mut cssparser::Parser<'_>) -> Option<f32> {
        match parser.next() {
            Ok(cssparser::Token::Number { value, .. }) => Some(value.rem_euclid(360.0)),
            Ok(cssparser::Token::Dimension { value, unit, .. }) => {
                let degrees = match unit.to_ascii_lowercase().as_str() {
                    "deg" => *value,
                    "grad" => *value * 360.0 / 400.0,
                    "rad" => value.to_degrees(),
                    "turn" => *value * 360.0,
                    _ => return None,
                };
                Some(degrees.rem_euclid(360.0))
            }
            _ => None,
        }
    }

    /// Legacy trailing `, alpha`.
    fn optional_comma_alpha(parser: &mut cssparser::Parser<'_>) -> Option<f32> {
        let state = parser.state();
        match parser.next() {
            Ok(cssparser::Token::Comma) => alpha_value(parser),
            _ => {
                parser.reset(&state);
                None
            }
        }
    }

    /// Modern `/ alpha`.
    fn optional_slash_alpha(parser: &mut cssparser::Parser<'_>) -> Option<f32> {
        let state = parser.state();
        match parser.next() {
            Ok(cssparser::Token::Delim('/')) => alpha_value(parser),
            _ => {
                parser.reset(&state);
                None
            }
        }
    }

    fn alpha_value(parser: &mut cssparser::Parser<'_>) -> Option<f32> {
        match parser.next() {
            Ok(cssparser::Token::Number { value, .. }) => Some(value.clamp(0.0, 1.0)),
            Ok(cssparser::Token::Percentage { unit_value, .. }) => Some(unit_value.clamp(0.0, 1.0)),
            _ => None,
        }
    }

    fn rgba_from_channels(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color {
            r: r.round().clamp(0.0, 255.0) as u8,
            g: g.round().clamp(0.0, 255.0) as u8,
            b: b.round().clamp(0.0, 255.0) as u8,
            a: (a * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }

    /// HSL → sRGB (CSS Color 4 §7 conversion, hue in degrees):
    /// f(n) = l - a·max(-1, min(k-3, 9-k, 1)), k = (n + h/30) mod 12.
    fn hsl_to_rgb(h: f32, s: f32, l: f32, a: f32) -> Color {
        let alpha = s * l.min(1.0 - l);
        let channel = |n: f32| -> f32 {
            let k = (n + h / 30.0) % 12.0;
            l - alpha * (-1.0f32).max((k - 3.0).min(9.0 - k).min(1.0))
        };
        Color {
            r: (channel(0.0) * 255.0).round().clamp(0.0, 255.0) as u8,
            g: (channel(8.0) * 255.0).round().clamp(0.0, 255.0) as u8,
            b: (channel(4.0) * 255.0).round().clamp(0.0, 255.0) as u8,
            a: (a * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }
}

/// Absolute unit → px conversion factors (CSS Values 4 reference pixel,
/// 96dpi).
const PX_PER_PT: f32 = 96.0 / 72.0;
const PX_PER_PC: f32 = 16.0;
const PX_PER_IN: f32 = 96.0;
const PX_PER_CM: f32 = 96.0 / 2.54;
const PX_PER_MM: f32 = 96.0 / 25.4;
const PX_PER_Q: f32 = 96.0 / 101.6;

/// Parses a `<length-percentage>`: px/pt/pc/in/cm/mm/q dimensions, em,
/// rem, percentages, and the unitless zero. `calc()` and ex/ch are the
/// recorded deferrals (typed `None`).
pub(crate) fn parse_length_percent(value: &RetainedValue) -> Option<LengthPercent> {
    parse_value(value, length_percent_token)
}

/// One `<length-percentage>` production from an in-progress parser.
fn length_percent_token(parser: &mut Parser<'_>) -> Option<LengthPercent> {
    match parser.next() {
        Ok(Token::Number { value, .. }) if *value == 0.0 => Some(LengthPercent::Px(0.0)),
        Ok(Token::Dimension { value, unit, .. }) => dimension_to_length(*value, unit),
        Ok(Token::Percentage { unit_value, .. }) => {
            Some(LengthPercent::Percent(unit_value * 100.0))
        }
        _ => None,
    }
}

/// Converts one dimension token: absolute units to px, em/rem symbolic.
fn dimension_to_length(value: f32, unit: &str) -> Option<LengthPercent> {
    match unit.to_ascii_lowercase().as_str() {
        "px" => Some(LengthPercent::Px(value)),
        "pt" => Some(LengthPercent::Px(value * PX_PER_PT)),
        "pc" => Some(LengthPercent::Px(value * PX_PER_PC)),
        "in" => Some(LengthPercent::Px(value * PX_PER_IN)),
        "cm" => Some(LengthPercent::Px(value * PX_PER_CM)),
        "mm" => Some(LengthPercent::Px(value * PX_PER_MM)),
        "q" => Some(LengthPercent::Px(value * PX_PER_Q)),
        "em" => Some(LengthPercent::Em(value)),
        "rem" => Some(LengthPercent::Rem(value)),
        // Font-relative x-height/zero-width units and the rest are
        // deferred.
        _ => None,
    }
}

/// The absolute-size keyword table (CSS Fonts 4, 16px base — the
/// browser-standard mapping).
const ABSOLUTE_FONT_SIZES: &[(&str, f32)] = &[
    ("xx-small", 9.0),
    ("x-small", 10.0),
    ("small", 13.0),
    ("medium", 16.0),
    ("large", 18.0),
    ("x-large", 24.0),
    ("xx-large", 32.0),
    ("xxx-large", 48.0),
];

/// One step up/down the absolute-size table (the `larger`/`smaller`
/// keywords' compute-time resolution).
pub(crate) fn absolute_size_step(current: f32, larger: bool) -> f32 {
    let index = ABSOLUTE_FONT_SIZES
        .iter()
        .position(|(_, px)| *px >= current)
        .unwrap_or(ABSOLUTE_FONT_SIZES.len());
    let index = if larger {
        (index + 1).min(ABSOLUTE_FONT_SIZES.len() - 1)
    } else {
        index.saturating_sub(1)
    };
    ABSOLUTE_FONT_SIZES[index].1
}

/// Parses `font-size`: absolute-size keywords, `larger`/`smaller`, and
/// length-percentage forms.
pub(crate) fn parse_font_size(value: &RetainedValue) -> Option<FontSizeSpec> {
    parse_value(value, |parser| match parser.next() {
        Ok(Token::Ident(name)) => {
            let name = name.to_ascii_lowercase();
            ABSOLUTE_FONT_SIZES
                .iter()
                .find(|(keyword, _)| *keyword == name)
                .map_or_else(
                    || match name.as_str() {
                        "larger" => Some(FontSizeSpec::Larger),
                        "smaller" => Some(FontSizeSpec::Smaller),
                        _ => None,
                    },
                    |(_, px)| Some(FontSizeSpec::Absolute(*px)),
                )
        }
        Ok(Token::Number { value, .. }) if *value == 0.0 => {
            Some(FontSizeSpec::LengthPercent(LengthPercent::Px(0.0)))
        }
        Ok(Token::Dimension { value, unit, .. }) => {
            let spec = match unit.to_ascii_lowercase().as_str() {
                "px" | "pt" | "pc" | "in" | "cm" | "mm" | "q" => {
                    let px = match unit.to_ascii_lowercase().as_str() {
                        "px" => *value,
                        "pt" => *value * PX_PER_PT,
                        "pc" => *value * PX_PER_PC,
                        "in" => *value * PX_PER_IN,
                        "cm" => *value * PX_PER_CM,
                        "mm" => *value * PX_PER_MM,
                        _ => *value * PX_PER_Q,
                    };
                    FontSizeSpec::Absolute(px)
                }
                "em" => FontSizeSpec::LengthPercent(LengthPercent::Em(*value)),
                "rem" => FontSizeSpec::LengthPercent(LengthPercent::Rem(*value)),
                _ => return None,
            };
            Some(spec)
        }
        Ok(Token::Percentage { unit_value, .. }) => Some(FontSizeSpec::LengthPercent(
            LengthPercent::Percent(unit_value * 100.0),
        )),
        _ => None,
    })
}

/// Parses `font-weight`: keywords or 1–1000 (clamped; CSS Fonts 4).
pub(crate) fn parse_font_weight(value: &RetainedValue) -> Option<FontWeightSpec> {
    parse_value(value, |parser| match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "normal" => Some(FontWeightSpec::Normal),
            "bold" => Some(FontWeightSpec::Bold),
            "bolder" => Some(FontWeightSpec::Bolder),
            "lighter" => Some(FontWeightSpec::Lighter),
            _ => None,
        },
        Ok(Token::Number { value, .. })
            if (1.0..=1000.0).contains(value) && value.fract() == 0.0 =>
        {
            Some(FontWeightSpec::Weight(*value as u16))
        }
        _ => None,
    })
}

/// The CSS Fonts 4 relative-weight mapping: `bolder` steps up, `lighter`
/// steps down.
pub(crate) fn relative_weight(parent: u16, bolder: bool) -> u16 {
    if bolder {
        if parent < 400 {
            400
        } else if parent < 600 {
            700
        } else {
            900
        }
    } else if parent < 600 {
        100
    } else if parent < 800 {
        400
    } else {
        700
    }
}

/// Parses `font-style`.
pub(crate) fn parse_font_style(value: &RetainedValue) -> Option<FontStyle> {
    parse_value(value, |parser| match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "normal" => Some(FontStyle::Normal),
            "italic" => Some(FontStyle::Italic),
            // `oblique <angle>` — the angle is not modeled (treated as
            // plain oblique); a missing angle parses the same.
            "oblique" => Some(FontStyle::Oblique),
            _ => None,
        },
        _ => None,
    })
}

/// The generic family keywords (CSS Fonts 4 §5 plus the `ui-*` set).
const GENERIC_FAMILIES: &[&str] = &[
    "serif",
    "sans-serif",
    "cursive",
    "fantasy",
    "monospace",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "math",
    "fangsong",
];

/// Parses `font-family`: a comma-separated list of quoted strings,
/// identifier sequences, or generic keywords.
pub(crate) fn parse_font_family(value: &RetainedValue) -> Option<FontFamily> {
    parse_value(value, |parser| {
        let mut families = Vec::new();
        loop {
            let family = parse_family_name(parser)?;
            families.push(family);
            match parser.next() {
                Err(_) => break,
                Ok(Token::Comma) => continue,
                Ok(_) => return None,
            }
        }
        if families.is_empty() {
            None
        } else {
            Some(FontFamily(families))
        }
    })
}

/// One `<family-name>`: a single quoted string, or one-or-more idents
/// joined with spaces (generics are flagged).
fn parse_family_name(parser: &mut Parser<'_>) -> Option<FontFamilyName> {
    match parser.next() {
        Ok(Token::QuotedString(name)) => Some(FontFamilyName {
            name: name.to_string(),
            generic: false,
        }),
        Ok(Token::Ident(first)) => {
            let lower = first.to_ascii_lowercase();
            if GENERIC_FAMILIES.contains(&lower.as_str()) {
                return Some(FontFamilyName {
                    name: lower,
                    generic: true,
                });
            }
            // An identifier sequence: "Times New Roman" written bare.
            let mut name = first.to_string();
            loop {
                let state = parser.state();
                match parser.next() {
                    Ok(Token::Ident(next)) => {
                        name.push(' ');
                        name.push_str(next);
                    }
                    _ => {
                        parser.reset(&state);
                        break;
                    }
                }
            }
            Some(FontFamilyName {
                name,
                generic: false,
            })
        }
        _ => None,
    }
}

/// Parses `text-align`.
pub(crate) fn parse_text_align(value: &RetainedValue) -> Option<TextAlign> {
    parse_value(value, |parser| match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "start" => Some(TextAlign::Start),
            "end" => Some(TextAlign::End),
            "left" => Some(TextAlign::Left),
            "right" => Some(TextAlign::Right),
            "center" => Some(TextAlign::Center),
            "justify" => Some(TextAlign::Justify),
            _ => None,
        },
        _ => None,
    })
}

/// The `line-height` specified forms (em/rem/percent stay symbolic for
/// inheritance — a length computes eagerly, a number inherits as a
/// multiplier).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LineHeightSpec {
    /// `normal`.
    Normal,
    /// A unitless multiplier.
    Number(f32),
    /// A length/percentage form.
    LengthPercent(LengthPercent),
}

/// Parses `line-height`: `normal`, a non-negative unitless number, or a
/// length/percentage.
pub(crate) fn parse_line_height(value: &RetainedValue) -> Option<LineHeightSpec> {
    parse_value(value, |parser| {
        let start = parser.state();
        match parser.next() {
            Ok(Token::Ident(name)) if name.eq_ignore_ascii_case("normal") => {
                Some(LineHeightSpec::Normal)
            }
            Ok(Token::Number { value, .. }) if *value >= 0.0 => {
                Some(LineHeightSpec::Number(*value))
            }
            _ => {
                // Re-read as a length-percentage (dimension, percentage;
                // em/rem keep their property-specific resolution).
                parser.reset(&start);
                length_percent_token(parser).map(LineHeightSpec::LengthPercent)
            }
        }
    })
}

/// Parses `text-decoration-line`: a (whitespace-separated) set of line
/// keywords.
pub(crate) fn parse_text_decoration_line(value: &RetainedValue) -> Option<TextDecorationLine> {
    parse_value(value, |parser| {
        let mut line = TextDecorationLine::default();
        let mut any = false;
        loop {
            match parser.next() {
                Err(_) => break,
                Ok(Token::Ident(name)) => {
                    let name = name.to_ascii_lowercase();
                    match name.as_str() {
                        "none" => {
                            line = TextDecorationLine::default();
                        }
                        "underline" => line.0 |= TextDecorationLine::UNDERLINE,
                        "overline" => line.0 |= TextDecorationLine::OVERLINE,
                        "line-through" => line.0 |= TextDecorationLine::LINE_THROUGH,
                        "blink" => line.0 |= TextDecorationLine::BLINK,
                        _ => return None,
                    }
                    any = true;
                }
                Ok(Token::WhiteSpace(_)) => continue,
                _ => return None,
            }
        }
        if any { Some(line) } else { None }
    })
}

/// Parses the `text-decoration` shorthand: line keywords plus (ignored,
/// recorded-deferral) color and style tokens. A token that is neither a
/// recognized line keyword nor a plausible color/style component
/// invalidates the declaration.
pub(crate) fn parse_text_decoration_shorthand(value: &RetainedValue) -> Option<TextDecorationLine> {
    parse_value(value, |parser| {
        let mut line = TextDecorationLine::default();
        let mut saw_any = false;
        loop {
            match parser.next() {
                Err(_) => break,
                Ok(Token::Ident(name)) => {
                    let name = name.to_ascii_lowercase();
                    match name.as_str() {
                        "none" => {
                            line = TextDecorationLine::default();
                        }
                        "underline" | "overline" | "line-through" | "blink" => {
                            // Recompute the set member-by-member.
                            if name == "underline" {
                                line.0 |= TextDecorationLine::UNDERLINE;
                            } else if name == "overline" {
                                line.0 |= TextDecorationLine::OVERLINE;
                            } else if name == "line-through" {
                                line.0 |= TextDecorationLine::LINE_THROUGH;
                            } else {
                                line.0 |= TextDecorationLine::BLINK;
                            }
                        }
                        // Color keywords/functions are the shorthand's
                        // text-decoration-color layer — skipped (the
                        // recorded deferral).
                        _ if parse_color::NAMED_COLORS.iter().any(|(k, _)| *k == name) => {}
                        _ if name == "currentcolor" || name == "transparent" => {}
                        _ => return None,
                    }
                    saw_any = true;
                }
                Ok(Token::WhiteSpace(_)) => continue,
                Ok(Token::Hash(_)) | Ok(Token::Function(_)) => {
                    saw_any = true;
                }
                _ => return None,
            }
        }
        if saw_any { Some(line) } else { None }
    })
}

/// Parses `display` (the Phase 3 working set; flex/grid/table forms are
/// their layout batches' concern and parse to `None` today).
pub(crate) fn parse_display(value: &RetainedValue) -> Option<Display> {
    parse_value(value, |parser| match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "none" => Some(Display::None),
            "block" => Some(Display::Block),
            "inline" => Some(Display::Inline),
            "inline-block" => Some(Display::InlineBlock),
            _ => None,
        },
        _ => None,
    })
}

/// Parses one margin value: `auto` or length-percentage.
pub(crate) fn parse_margin_value(value: &RetainedValue) -> Option<MarginSizing> {
    parse_value(value, |parser| {
        let start = parser.state();
        match parser.next() {
            Ok(Token::Ident(name)) if name.eq_ignore_ascii_case("auto") => Some(MarginSizing::Auto),
            _ => {
                parser.reset(&start);
                length_percent_token(parser).map(MarginSizing::LengthPercent)
            }
        }
    })
}

/// One margin production from an in-progress parser (`auto` or
/// length-percentage); the parser is positioned at the value's start.
fn margin_token(parser: &mut Parser<'_>) -> Option<MarginSizing> {
    let start = parser.state();
    match parser.next() {
        Ok(Token::Ident(name)) if name.eq_ignore_ascii_case("auto") => Some(MarginSizing::Auto),
        _ => {
            parser.reset(&start);
            length_percent_token(parser).map(MarginSizing::LengthPercent)
        }
    }
}

/// Parses the `margin` shorthand: 1–4 `auto`/length-percentage values.
pub(crate) fn parse_margin_shorthand(value: &RetainedValue) -> Option<Sides<MarginSizing>> {
    let sides = parse_shorthand_sides(value, margin_token)?;
    Some(Sides::from_shorthand(sides))
}

/// Parses the `padding` shorthand: 1–4 length-percentage values (`auto`
/// is invalid for padding).
pub(crate) fn parse_padding_shorthand(value: &RetainedValue) -> Option<Sides<LengthPercent>> {
    let sides = parse_shorthand_sides(value, length_percent_token)?;
    Some(Sides::from_shorthand(sides))
}

/// Parses one padding value: length-percentage (`auto` is invalid).
pub(crate) fn parse_padding_value(value: &RetainedValue) -> Option<LengthPercent> {
    parse_length_percent(value)
}

/// Parses one width/height value: `auto` or length-percentage.
pub(crate) fn parse_sizing(value: &RetainedValue) -> Option<Sizing> {
    parse_value(value, |parser| {
        let start = parser.state();
        match parser.next() {
            Ok(Token::Ident(name)) if name.eq_ignore_ascii_case("auto") => Some(Sizing::Auto),
            _ => {
                parser.reset(&start);
                length_percent_token(parser).map(Sizing::LengthPercent)
            }
        }
    })
}

/// Expands a 1–4 value shorthand into `[top, right, bottom, left]`
/// (CSS Box Model order: clockwise from top; 3 values → [top, horizontal,
/// bottom]; 2 → [vertical, horizontal]).
fn expand_sides<T: Copy>(values: &[T]) -> Option<[T; 4]> {
    match values.len() {
        1 => Some([values[0]; 4]),
        2 => Some([values[0], values[1], values[0], values[1]]),
        3 => Some([values[0], values[1], values[2], values[1]]),
        4 => Some([values[0], values[1], values[2], values[3]]),
        _ => None,
    }
}

/// Parses `border-width`: 1–4 width keywords or lengths.
pub(crate) fn parse_border_widths(value: &RetainedValue) -> Option<Sides<f32>> {
    let sides = parse_shorthand_sides(value, border_width_token)?;
    Some(Sides::from_shorthand(sides))
}

/// Parses `border-style`: 1–4 line-style keywords.
pub(crate) fn parse_border_styles(value: &RetainedValue) -> Option<Sides<BorderStyle>> {
    let sides = parse_shorthand_sides(value, border_style_token)?;
    Some(Sides::from_shorthand(sides))
}

/// One `<line-style>` production.
fn border_style_token(parser: &mut Parser<'_>) -> Option<BorderStyle> {
    match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "none" => Some(BorderStyle::None),
            "hidden" => Some(BorderStyle::Hidden),
            "solid" => Some(BorderStyle::Solid),
            "dashed" => Some(BorderStyle::Dashed),
            "dotted" => Some(BorderStyle::Dotted),
            "double" => Some(BorderStyle::Double),
            "groove" => Some(BorderStyle::Groove),
            "ridge" => Some(BorderStyle::Ridge),
            "inset" => Some(BorderStyle::Inset),
            "outset" => Some(BorderStyle::Outset),
            _ => None,
        },
        _ => None,
    }
}

/// Parses `border-color`: 1–4 colors (currentcolor kept symbolic).
pub(crate) fn parse_border_colors(value: &RetainedValue) -> Option<Sides<ColorValue>> {
    let sides = parse_shorthand_sides(value, parse_one_color)?;
    Some(Sides::from_shorthand(sides))
}

/// The shared 1–4-value shorthand driver: whitespace-separated
/// single-value productions, parsed with `parse_one`.
fn parse_shorthand_sides<T: Copy>(
    value: &RetainedValue,
    parse_one: impl Fn(&mut Parser<'_>) -> Option<T>,
) -> Option<[T; 4]> {
    parse_value(value, |parser| {
        let mut values = Vec::new();
        loop {
            let start = parser.state();
            match parser.next() {
                Err(_) => break,
                Ok(Token::WhiteSpace(_)) => continue,
                Ok(_) => {
                    parser.reset(&start);
                    values.push(parse_one(parser)?);
                }
            }
        }
        expand_sides(&values)
    })
}

/// Parses the `border` shorthand: 1–3 values in any order (width, style,
/// color; omitted components reset to initial).
pub(crate) fn parse_border_shorthand(
    value: &RetainedValue,
) -> Option<(f32, BorderStyle, ColorValue)> {
    parse_value(value, |parser| {
        let mut width: Option<f32> = None;
        let mut style: Option<BorderStyle> = None;
        let mut color: Option<ColorValue> = None;
        let mut any = false;
        loop {
            let start = parser.state();
            match parser.next() {
                Err(_) => break,
                Ok(Token::WhiteSpace(_)) => continue,
                Ok(_) => {
                    parser.reset(&start);
                    if width.is_none() {
                        if let Some(found) = border_width_token(parser) {
                            width = Some(found);
                            any = true;
                            continue;
                        }
                        parser.reset(&start);
                    }
                    if style.is_none() {
                        if let Some(found) = border_style_token(parser) {
                            style = Some(found);
                            any = true;
                            continue;
                        }
                        parser.reset(&start);
                    }
                    if color.is_none() {
                        if let Some(found) = parse_one_color(parser) {
                            color = Some(found);
                            any = true;
                            continue;
                        }
                        parser.reset(&start);
                    }
                    return None;
                }
            }
        }
        if !any {
            return None;
        }
        Some((
            width.unwrap_or(3.0),
            style.unwrap_or(BorderStyle::None),
            color.unwrap_or(ColorValue::CurrentColor),
        ))
    })
}

/// One border-width production (keywords or length).
fn border_width_token(parser: &mut Parser<'_>) -> Option<f32> {
    match parser.next() {
        Ok(Token::Ident(name)) => match name.to_ascii_lowercase().as_str() {
            "thin" => Some(1.0),
            "medium" => Some(3.0),
            "thick" => Some(5.0),
            _ => None,
        },
        Ok(Token::Number { value, .. }) if *value == 0.0 => Some(0.0),
        Ok(Token::Dimension { value, unit, .. }) => {
            match dimension_to_length(*value, unit)? {
                LengthPercent::Px(px) => Some(px),
                // em/rem borders resolve against the font — not modeled
                // in the shorthand path this batch.
                _ => None,
            }
        }
        _ => None,
    }
}

/// Parses the `background` shorthand's color layer: the first color
/// among the tokens; the other background layers (images, position,
/// size, repeat, attachment, box keywords) are the recorded deferral and
/// skipped. Absent a color, the shorthand's reset semantics make the
/// color `transparent` (initial).
pub(crate) fn parse_background_shorthand(value: &RetainedValue) -> Option<ColorValue> {
    parse_value(value, |parser| {
        let mut color: Option<ColorValue> = None;
        let mut any = false;
        loop {
            let start = parser.state();
            match parser.next() {
                Err(_) => break,
                Ok(Token::WhiteSpace(_)) | Ok(Token::Comma) | Ok(Token::Delim('/')) => continue,
                Ok(Token::Hash(_)) | Ok(Token::Ident(_)) => {
                    any = true;
                    if color.is_none() {
                        parser.reset(&start);
                        if let Some(found) = parse_one_color(parser) {
                            color = Some(found);
                        }
                    }
                }
                Ok(Token::Function(_))
                | Ok(Token::UnquotedUrl(_))
                | Ok(Token::Dimension { .. })
                | Ok(Token::Percentage { .. })
                | Ok(Token::Number { .. }) => {
                    any = true;
                }
                _ => return None,
            }
        }
        if !any {
            None
        } else {
            Some(color.unwrap_or(ColorValue::Rgba(Color::TRANSPARENT)))
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::declarations::DeclarationList;

    /// Parses one declaration's value through the batch-1 declaration
    /// parser (the same path the cascade consumes).
    fn value_of(css: &str) -> RetainedValue {
        let list = DeclarationList::parse(css);
        let declaration = list.declarations().first().expect("one declaration");
        declaration.value.clone()
    }

    #[test]
    fn colors_parse_from_every_working_form() {
        let parse = |css: &str| parse_color_value(&value_of(css)).expect("color");

        assert_eq!(
            parse("color: #f00"),
            ColorValue::Rgba(Color::rgb(255, 0, 0))
        );
        assert_eq!(
            parse("color: #ff0000"),
            ColorValue::Rgba(Color::rgb(255, 0, 0))
        );
        assert_eq!(
            parse("color: #00000080"),
            ColorValue::Rgba(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128
            })
        );
        assert_eq!(parse("color: red"), ColorValue::Rgba(Color::rgb(255, 0, 0)));
        assert_eq!(
            parse("color: rebeccapurple"),
            ColorValue::Rgba(Color::rgb(102, 51, 153))
        );
        assert_eq!(
            parse("color: transparent"),
            ColorValue::Rgba(Color::TRANSPARENT)
        );
        assert_eq!(parse("color: currentcolor"), ColorValue::CurrentColor);
        // Legacy comma syntax, clamping.
        assert_eq!(
            parse("color: rgb(255, 0, 0)"),
            ColorValue::Rgba(Color::rgb(255, 0, 0))
        );
        assert_eq!(
            parse("color: rgba(300, -10, 0, 0.5)"),
            ColorValue::Rgba(Color {
                r: 255,
                g: 0,
                b: 0,
                a: 128
            })
        );
        // Modern space syntax with percentage channels.
        assert_eq!(
            parse("color: rgb(100% 0% 0% / 50%)"),
            ColorValue::Rgba(Color {
                r: 255,
                g: 0,
                b: 0,
                a: 128
            })
        );
        // HSL (a pure hue is the channel color), number and angle forms.
        assert_eq!(
            parse("color: hsl(120, 100%, 50%)"),
            ColorValue::Rgba(Color::rgb(0, 255, 0))
        );
        assert_eq!(
            parse("color: hsl(120deg 100% 50%)"),
            ColorValue::Rgba(Color::rgb(0, 255, 0))
        );
        // Case-insensitive function names and keywords.
        assert_eq!(
            parse("color: RGB(0, 0, 255)"),
            ColorValue::Rgba(Color::rgb(0, 0, 255))
        );

        // Invalid forms are None (the cascade drops them).
        assert!(parse_color_value(&value_of("color: 12px")).is_none());
        assert!(parse_color_value(&value_of("color: #ff")).is_none());
        assert!(parse_color_value(&value_of("color: rgb(1 2)")).is_none());
        assert!(parse_color_value(&value_of("color: unknowncolor")).is_none());
        assert!(parse_color_value(&value_of("color: red green")).is_none());
    }

    #[test]
    fn lengths_parse_with_units() {
        let parse = |css: &str| parse_length_percent(&value_of(css)).expect("length");

        assert_eq!(parse("width: 0"), LengthPercent::Px(0.0));
        assert_eq!(parse("width: 10px"), LengthPercent::Px(10.0));
        assert_eq!(parse("width: 12pt"), LengthPercent::Px(16.0));
        assert_eq!(parse("width: 1in"), LengthPercent::Px(96.0));
        assert_eq!(parse("width: 2em"), LengthPercent::Em(2.0));
        assert_eq!(parse("width: 1.5rem"), LengthPercent::Rem(1.5));
        assert_eq!(parse("width: 50%"), LengthPercent::Percent(50.0));
        // Unitless nonzero is invalid; ex/ch/calc are deferred.
        assert!(parse_length_percent(&value_of("width: 10")).is_none());
        assert!(parse_length_percent(&value_of("width: 2ex")).is_none());
        assert!(parse_length_percent(&value_of("width: calc(1px + 2px)")).is_none());
        assert!(parse_length_percent(&value_of("width: 10px 20px")).is_none());
    }

    #[test]
    fn font_and_text_properties_parse() {
        assert_eq!(
            parse_font_size(&value_of("font-size: large")),
            Some(FontSizeSpec::Absolute(18.0))
        );
        assert_eq!(
            parse_font_size(&value_of("font-size: 1.2em")),
            Some(FontSizeSpec::LengthPercent(LengthPercent::Em(1.2)))
        );
        assert_eq!(
            parse_font_size(&value_of("font-size: larger")),
            Some(FontSizeSpec::Larger)
        );

        assert_eq!(
            parse_font_weight(&value_of("font-weight: bold")),
            Some(FontWeightSpec::Bold)
        );
        assert_eq!(
            parse_font_weight(&value_of("font-weight: 550")),
            Some(FontWeightSpec::Weight(550))
        );
        assert!(parse_font_weight(&value_of("font-weight: 1001")).is_none());
        assert!(parse_font_weight(&value_of("font-weight: 550.5")).is_none());

        assert_eq!(
            parse_font_style(&value_of("font-style: italic")),
            Some(FontStyle::Italic)
        );

        let family = parse_font_family(&value_of(
            "font-family: 'Helvetica Neue', Arial, sans-serif",
        ))
        .expect("family");
        assert_eq!(family.0.len(), 3);
        assert_eq!(family.0[0].name, "Helvetica Neue");
        assert!(!family.0[1].generic);
        assert!(family.0[2].generic);

        assert_eq!(
            parse_text_align(&value_of("text-align: center")),
            Some(TextAlign::Center)
        );

        match parse_line_height(&value_of("line-height: 1.5")) {
            Some(LineHeightSpec::Number(n)) => assert_eq!(n, 1.5),
            _ => panic!("number line-height"),
        }
        match parse_line_height(&value_of("line-height: 2em")) {
            Some(LineHeightSpec::LengthPercent(LengthPercent::Em(e))) => assert_eq!(e, 2.0),
            _ => panic!("em line-height"),
        }
        assert!(parse_line_height(&value_of("line-height: -1")).is_none());

        let line =
            parse_text_decoration_line(&value_of("text-decoration: underline")).expect("line");
        assert!(line.underline() && !line.line_through());
        let line = parse_text_decoration_shorthand(&value_of("text-decoration: underline red"))
            .expect("line");
        assert!(line.underline());
    }

    #[test]
    fn box_properties_parse_and_expand() {
        assert_eq!(
            parse_display(&value_of("display: inline-block")),
            Some(Display::InlineBlock)
        );

        match parse_margin_value(&value_of("margin-top: auto")) {
            Some(MarginSizing::Auto) => {}
            _ => panic!("auto margin"),
        }
        let sides = parse_margin_shorthand(&value_of("margin: 1px auto 3%")).expect("margins");
        assert_eq!(
            sides.top,
            MarginSizing::LengthPercent(LengthPercent::Px(1.0))
        );
        assert_eq!(sides.right, MarginSizing::Auto);
        assert_eq!(
            sides.bottom,
            MarginSizing::LengthPercent(LengthPercent::Percent(3.0))
        );
        // Three values: [top, horizontal, bottom] — left mirrors right.
        assert_eq!(sides.left, MarginSizing::Auto);

        let sides = parse_padding_shorthand(&value_of("padding: 1em 2rem")).expect("paddings");
        assert_eq!(sides.top, LengthPercent::Em(1.0));
        assert_eq!(sides.bottom, LengthPercent::Em(1.0));
        assert_eq!(sides.right, LengthPercent::Rem(2.0));

        assert_eq!(parse_sizing(&value_of("width: auto")), Some(Sizing::Auto));

        let borders = parse_border_widths(&value_of("border-width: thin 5px")).expect("widths");
        assert_eq!(borders.top, 1.0);
        assert_eq!(borders.right, 5.0);
        let styles = parse_border_styles(&value_of("border-style: solid dashed")).expect("styles");
        assert_eq!(styles.top, BorderStyle::Solid);
        assert_eq!(styles.right, BorderStyle::Dashed);
        assert_eq!(styles.bottom, BorderStyle::Solid);
        assert_eq!(styles.left, BorderStyle::Dashed);

        let (width, style, color) =
            parse_border_shorthand(&value_of("border: 1px solid red")).expect("border");
        assert_eq!(width, 1.0);
        assert_eq!(style, BorderStyle::Solid);
        assert_eq!(color, ColorValue::Rgba(Color::rgb(255, 0, 0)));
        // Any order works; omitted components take initials.
        let (width, style, color) =
            parse_border_shorthand(&value_of("border: red 2px")).expect("border");
        assert_eq!(width, 2.0);
        assert_eq!(style, BorderStyle::None);
        assert_eq!(color, ColorValue::Rgba(Color::rgb(255, 0, 0)));

        assert_eq!(
            parse_background_shorthand(&value_of("background: red")).expect("bg"),
            ColorValue::Rgba(Color::rgb(255, 0, 0))
        );
        assert_eq!(
            parse_background_shorthand(&value_of("background: url(a.png) no-repeat")).expect("bg"),
            ColorValue::Rgba(Color::TRANSPARENT)
        );
    }

    #[test]
    fn global_keywords_and_var_detection() {
        assert_eq!(
            global_keyword(&value_of("color: INHERIT")),
            Some(GlobalKeyword::Inherit)
        );
        assert_eq!(
            global_keyword(&value_of("color: unset")),
            Some(GlobalKeyword::Unset)
        );
        assert_eq!(
            global_keyword(&value_of("color: revert")),
            Some(GlobalKeyword::Revert)
        );
        assert_eq!(
            global_keyword(&value_of("color: revert-layer")),
            Some(GlobalKeyword::RevertLayer)
        );
        assert_eq!(global_keyword(&value_of("color: red")), None);

        assert!(references_var(&value_of("color: var(--x)")));
        assert!(!references_var(&value_of("color: red")));
    }

    #[test]
    fn relative_weight_table() {
        // CSS Fonts 4: bolder from 100–300 → 400, 400–500 → 700, 600+ → 900.
        assert_eq!(relative_weight(300, true), 400);
        assert_eq!(relative_weight(400, true), 700);
        assert_eq!(relative_weight(700, true), 900);
        // lighter: 100–500 → 100, 600–700 → 400, 800–900 → 700.
        assert_eq!(relative_weight(400, false), 100);
        assert_eq!(relative_weight(700, false), 400);
        assert_eq!(relative_weight(900, false), 700);
    }

    #[test]
    fn absolute_size_steps() {
        assert_eq!(absolute_size_step(16.0, true), 18.0);
        assert_eq!(absolute_size_step(16.0, false), 13.0);
        // Clamped at the table edges.
        assert_eq!(absolute_size_step(48.0, true), 48.0);
        assert_eq!(absolute_size_step(9.0, false), 9.0);
    }
}

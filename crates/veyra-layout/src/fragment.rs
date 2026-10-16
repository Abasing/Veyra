//! The fragment tree: layout's output and the paint batch's input.
//!
//! Every laid-out box becomes a [`Fragment::Box`] carrying its three
//! rectangular extents (border/padding/content), its children, and the
//! paint-affecting computed values it was laid out under (background
//! color, border colors — baked in at box-tree build so `veyra-paint`
//! needs no style access; the dependency-graph edge stays `PAINT -->
//! LAYOUT` only). Every laid-out text run becomes a [`Fragment::Text`]
//! carrying the resolved run, its inline-box rectangle, and its computed
//! text color. Coordinates are **absolute page pixels** (f32, origin
//! top-left) — the raster surface's coordinate system — so the paint
//! batch and the deterministic dump consume fragments without further
//! mapping. A later batch that needs container-relative geometry
//! (scrolling, damage tracking) can rebase; the change is local to this
//! module and the emitters.
//!
//! Vertical geometry of a text fragment: the rect is the font's content
//! area (ascent..descent of the owning style), not the full line box —
//! line-box leading is a line-layout detail the paint batch does not
//! need. With the [`FixedMetrics`](crate::FixedMetrics) provider that is
//! an em-box-sized rect; a real font provider produces its own metrics
//! behind the same fragment shape.

use veyra_dom::NodeId;
use veyra_style::{Color, Sides};

use crate::geometry::PhysicalRect;

/// One laid-out piece.
#[derive(Debug, Clone)]
pub enum Fragment {
    /// A box (element or anonymous block) with its extents and children.
    Box(BoxFragment),
    /// A text run with its inline-box rectangle.
    Text(TextFragment),
}

/// A laid-out box fragment. The three rects share one origin (the
/// fragment's border-box top-left at the time of emission — translated to
/// page coordinates when the fragment is placed); `border` is the outer
/// extent, `padding`/`content` nest inside it.
#[derive(Debug, Clone)]
pub struct BoxFragment {
    /// The owning element (`None` for anonymous boxes).
    pub node: Option<NodeId>,
    /// The element's local name (empty for anonymous boxes).
    pub name: String,
    /// Whether this is an anonymous block box.
    pub anonymous: bool,
    /// The border-box extent (the outer rectangle).
    pub border: PhysicalRect,
    /// The padding-box extent (border box inset by the used border
    /// widths).
    pub padding: PhysicalRect,
    /// The content-box extent (padding box inset by the used paddings).
    pub content: PhysicalRect,
    /// Child fragments (block children, or the line contents of an
    /// inline formatting context).
    pub children: Vec<Fragment>,
    /// The box's computed background color (`background-color` after the
    /// cascade; transparent for anonymous boxes unless inherited — paint
    /// skips fully transparent fills).
    pub background: Color,
    /// The box's computed border colors (used border *widths* are
    /// derivable from the border/padding rects — a side with zero used
    /// width paints nothing).
    pub border_colors: Sides<Color>,
}

/// A laid-out text run.
#[derive(Debug, Clone)]
pub struct TextFragment {
    /// The source text node.
    pub node: NodeId,
    /// The run's text (post whitespace-collapsing, per-line resolved).
    pub text: String,
    /// The run's font content-area rectangle (absolute page coordinates).
    pub rect: PhysicalRect,
    /// The run's computed text color (`color` after the cascade).
    pub color: Color,
}

impl Fragment {
    /// Translates every rectangle in this subtree by `(dx, dy)`.
    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Fragment::Box(box_) => {
                box_.border = box_.border.translated(dx, dy);
                box_.padding = box_.padding.translated(dx, dy);
                box_.content = box_.content.translated(dx, dy);
                for child in &mut box_.children {
                    child.translate(dx, dy);
                }
            }
            Fragment::Text(text) => {
                text.rect = text.rect.translated(dx, dy);
            }
        }
    }
}

/// The result of laying out a document.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    /// The viewport (initial containing block) in pixels.
    pub viewport: (u32, u32),
    /// The root fragment (`<html>`, or a synthetic box for degenerate
    /// documents).
    pub root: Fragment,
    /// Total fragments emitted (boxes + text runs; the limits-table
    /// budget counts these).
    pub fragment_count: usize,
}

impl LayoutResult {
    /// Renders the deterministic fragment-tree dump (the `open --layout`
    /// report): one line per fragment, two-space indentation per depth,
    /// coordinates formatted with two decimal places.
    ///
    /// Text content is quoted with `\` `"` `\n` `\r` `\t` escaped, so the
    /// dump is one physical line per fragment and byte-identical across
    /// runs for identical input.
    #[must_use]
    pub fn to_dump_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "layout v1 viewport={}x{}\n",
            self.viewport.0, self.viewport.1
        ));
        dump_fragment(&self.root, 0, &mut out);
        out
    }
}

/// Formats one fragment line and recurses (depth bounded by the DOM
/// depth limit — the same bound the builder and layout run under).
fn dump_fragment(fragment: &Fragment, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    match fragment {
        Fragment::Box(box_) => {
            let name = if box_.anonymous && box_.name.is_empty() {
                "(anonymous)".to_string()
            } else if box_.anonymous {
                format!("(anonymous {})", box_.name)
            } else {
                box_.name.clone()
            };
            out.push_str(&format!(
                "{indent}box {name} content={} padding={} border={}\n",
                format_rect(&box_.content),
                format_rect(&box_.padding),
                format_rect(&box_.border),
            ));
            for child in &box_.children {
                dump_fragment(child, depth + 1, out);
            }
        }
        Fragment::Text(text) => {
            out.push_str(&format!(
                "{indent}text {} {}\n",
                escape_text(&text.text),
                format_rect(&text.rect),
            ));
        }
    }
}

/// Formats `[x,y wXh]` with two decimals (ASCII separators).
fn format_rect(rect: &PhysicalRect) -> String {
    format!(
        "[{:.2},{:.2} {:.2}x{:.2}]",
        rect.x, rect.y, rect.width, rect.height
    )
}

/// Escapes a text run for the dump.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

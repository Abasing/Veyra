//! Display-list construction: the fragment-tree walk (invariant R2).
//!
//! The walk is an explicit-stack machine (the layout crate's convention —
//! the DOM's full 512-deep cap must never overflow the stack, and a
//! fragment tree is at least that deep) emitting commands in fragment
//! order. Module docs of the crate carry the paint-order argument; this
//! module carries the mechanics:
//!
//! - **Box fragment:** background first (fill the border-box rect —
//!   `background-clip: border-box` is the initial value and the only
//!   clip the working set models), then borders (one command describing
//!   the four side strips; used widths derive from the border/padding
//!   rect pair, so a zero-width side is structurally impossible to
//!   paint), then children in order.
//! - **Text fragment:** one text command (the run, its content-area
//!   rect, its computed color). Glyph rasterization is the raster
//!   batch's concern — the list is resolution- and rasterizer-agnostic.
//! - **Skips:** transparent backgrounds and all-zero border boxes emit
//!   nothing (the empty-document display list is genuinely empty).
//!
//! The dump (`DisplayList::to_dump_string`) is one physical line per
//! command, coordinates at two decimals, colors as `#RRGGBBAA` — the
//! same byte-determinism contract as the layout dump.

use veyra_layout::{BoxFragment, Color, Fragment, LayoutResult, PhysicalRect, Sides};

use crate::error::PaintError;
use crate::limits::PaintLimits;

/// One paint command: a primitive with its paint data, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayCommand {
    /// A solid fill (a box's background; `background-clip: border-box`).
    Rect {
        /// The fill rectangle (border-box extent, page coordinates).
        rect: PhysicalRect,
        /// The fill color (never fully transparent — skips happen at
        /// construction).
        color: Color,
    },
    /// A box's border side strips. Emitted only when at least one used
    /// width is non-zero; each side with a zero width paints nothing.
    Border {
        /// The border-box extent (the strips hug its edges).
        rect: PhysicalRect,
        /// Used widths per side (px; zero-width sides paint nothing).
        widths: Sides<f32>,
        /// Colors per side (a side may be transparent — paints nothing).
        colors: Sides<Color>,
    },
    /// A text run (glyph rasterization happens at raster time).
    Text {
        /// The run's text (post whitespace-collapsing).
        text: String,
        /// The run's font content-area rectangle (page coordinates).
        rect: PhysicalRect,
        /// The run's computed text color.
        color: Color,
    },
}

/// The paint-stage output: an ordered command sequence over one frame's
/// viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayList {
    /// The viewport (initial containing block) in pixels.
    pub viewport: (u32, u32),
    /// The commands in paint order (R2 subset — crate docs).
    pub commands: Vec<DisplayCommand>,
}

impl DisplayList {
    /// Renders the deterministic display-list dump (one physical line
    /// per command; text escaped like the layout dump).
    pub fn to_dump_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "paint v1 viewport={}x{}\n",
            self.viewport.0, self.viewport.1
        ));
        for command in &self.commands {
            match command {
                DisplayCommand::Rect { rect, color } => {
                    out.push_str(&format!(
                        "rect [{}] {}\n",
                        format_rect(rect),
                        format_color(*color)
                    ));
                }
                DisplayCommand::Border {
                    rect,
                    widths,
                    colors,
                } => {
                    out.push_str(&format!(
                        "border [{}] top={} {} right={} {} bottom={} {} left={} {}\n",
                        format_rect(rect),
                        fmt_f32(widths.top),
                        format_color(colors.top),
                        fmt_f32(widths.right),
                        format_color(colors.right),
                        fmt_f32(widths.bottom),
                        format_color(colors.bottom),
                        fmt_f32(widths.left),
                        format_color(colors.left),
                    ));
                }
                DisplayCommand::Text { text, rect, color } => {
                    out.push_str(&format!(
                        "text {} [{}] {}\n",
                        quote_text(text),
                        format_rect(rect),
                        format_color(*color)
                    ));
                }
            }
        }
        out
    }
}

/// Formats a rect as the dump's `[x,y WxH]` form (two decimals — the
/// layout dump's convention).
fn format_rect(rect: &PhysicalRect) -> String {
    format!(
        "{},{},{}x{}",
        fmt_f32(rect.x),
        fmt_f32(rect.y),
        fmt_f32(rect.width),
        fmt_f32(rect.height)
    )
}

/// Formats an f32 with two decimals (deterministic formatting; `-0.00`
/// normalized to `0.00` so negated zeros never split a golden file).
fn fmt_f32(value: f32) -> String {
    let v = if value == 0.0 { 0.0 } else { value };
    format!("{v:.2}")
}

/// Formats a color as `#RRGGBBAA` (uppercase hex — byte-deterministic).
fn format_color(color: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.r, color.g, color.b, color.a
    )
}

/// Quotes text with the layout dump's escape set (`\`, `"`, newline,
/// carriage return, tab) so a dump line is one physical line.
fn quote_text(text: &str) -> String {
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

/// Builds the frame's display list from a laid-out document (crate docs
/// carry the paint-order argument).
///
/// # Errors
///
/// [`PaintError::LimitExceeded`] when the command count would exceed
/// [`PaintLimits::max_commands`]; no partial list is returned.
pub fn build_display_list(
    result: &LayoutResult,
    limits: PaintLimits,
) -> Result<DisplayList, PaintError> {
    let mut builder = Builder {
        commands: Vec::new(),
        budget: limits.max_commands,
    };
    // Explicit stack; children pushed reversed so pops come in document
    // order (R2's tree order).
    let mut stack: Vec<&Fragment> = vec![&result.root];
    while let Some(fragment) = stack.pop() {
        match fragment {
            Fragment::Box(box_fragment) => {
                builder.emit_box(box_fragment)?;
                for child in box_fragment.children.iter().rev() {
                    stack.push(child);
                }
            }
            Fragment::Text(text) => builder.emit_text(text)?,
        }
    }
    Ok(DisplayList {
        viewport: result.viewport,
        commands: builder.commands,
    })
}

/// The walk's accumulator (command buffer + remaining budget).
struct Builder {
    commands: Vec<DisplayCommand>,
    budget: usize,
}

impl Builder {
    /// Charges one command against the budget.
    fn charge(&mut self) -> Result<(), PaintError> {
        if self.budget == 0 {
            return Err(PaintError::LimitExceeded);
        }
        self.budget -= 1;
        Ok(())
    }

    /// Emits a box's own paint commands (background, then borders).
    fn emit_box(&mut self, fragment: &BoxFragment) -> Result<(), PaintError> {
        if fragment.background.a != 0 {
            self.charge()?;
            self.commands.push(DisplayCommand::Rect {
                rect: fragment.border,
                color: fragment.background,
            });
        }
        let widths = used_border_widths(fragment);
        // A side paints only when it has both width and opacity; the
        // command exists only when at least one side paints.
        let colors = fragment.border_colors;
        let paints = |width: f32, color: Color| width > 0.0 && color.a != 0;
        if paints(widths.top, colors.top)
            || paints(widths.right, colors.right)
            || paints(widths.bottom, colors.bottom)
            || paints(widths.left, colors.left)
        {
            self.charge()?;
            self.commands.push(DisplayCommand::Border {
                rect: fragment.border,
                widths,
                colors,
            });
        }
        Ok(())
    }

    /// Emits a text run.
    fn emit_text(&mut self, text: &veyra_layout::TextFragment) -> Result<(), PaintError> {
        self.charge()?;
        self.commands.push(DisplayCommand::Text {
            text: text.text.clone(),
            rect: text.rect,
            color: text.color,
        });
        Ok(())
    }
}

/// Derives the used border widths from the border/padding rect pair (the
/// padding box is the border box inset by exactly the used widths).
fn used_border_widths(fragment: &BoxFragment) -> Sides<f32> {
    Sides {
        top: fragment.padding.y - fragment.border.y,
        left: fragment.padding.x - fragment.border.x,
        right: (fragment.border.x + fragment.border.width)
            - (fragment.padding.x + fragment.padding.width),
        bottom: (fragment.border.y + fragment.border.height)
            - (fragment.padding.y + fragment.padding.height),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use veyra_dom::NodeId;
    use veyra_layout::TextFragment;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect {
        PhysicalRect::new(x, y, w, h)
    }

    fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    fn box_fragment(
        background: Color,
        border_colors: Sides<Color>,
        children: Vec<Fragment>,
    ) -> BoxFragment {
        BoxFragment {
            node: None,
            name: "div".to_string(),
            anonymous: false,
            border: rect(0.0, 0.0, 100.0, 100.0),
            padding: rect(2.0, 3.0, 96.0, 95.0),
            content: rect(4.0, 5.0, 92.0, 91.0),
            children,
            background,
            border_colors,
        }
    }

    fn text_fragment(text: &str, color: Color) -> TextFragment {
        TextFragment {
            node: NodeId::null(),
            text: text.to_string(),
            rect: rect(4.0, 5.0, 20.0, 10.0),
            color,
        }
    }

    fn opaque() -> Sides<Color> {
        Sides {
            top: color(1, 2, 3, 255),
            right: color(1, 2, 3, 255),
            bottom: color(1, 2, 3, 255),
            left: color(1, 2, 3, 255),
        }
    }

    /// Paint order: a box's background and borders precede its
    /// descendants' commands (R2 tree order); descendants follow in
    /// document order.
    #[test]
    fn tree_order_background_before_descendants() {
        let leaf = Fragment::Box(box_fragment(color(0, 0, 255, 255), opaque(), vec![]));
        let root = Fragment::Box(box_fragment(
            color(255, 0, 0, 255),
            opaque(),
            vec![
                leaf,
                Fragment::Text(text_fragment("hi", color(0, 0, 0, 255))),
            ],
        ));
        let result = LayoutResult {
            viewport: (100, 100),
            root,
            fragment_count: 3,
        };
        let list = build_display_list(&result, PaintLimits::default()).unwrap();
        // Root bg + root border + child bg + child border + text.
        assert_eq!(list.commands.len(), 5);
        match (
            &list.commands[0],
            &list.commands[1],
            &list.commands[2],
            &list.commands[3],
            &list.commands[4],
        ) {
            (
                DisplayCommand::Rect { color: c0, .. },
                DisplayCommand::Border { .. },
                DisplayCommand::Rect { color: c2, .. },
                DisplayCommand::Border { .. },
                DisplayCommand::Text { text, .. },
            ) => {
                assert_eq!(*c0, color(255, 0, 0, 255), "root background first");
                assert_eq!(*c2, color(0, 0, 255, 255), "child background next");
                assert_eq!(text, "hi", "text last");
            }
            other => panic!("unexpected command sequence: {other:?}"),
        }
    }

    /// Transparent backgrounds and zero-width borders emit nothing (the
    /// plain box is only its text).
    #[test]
    fn transparent_and_zero_width_skip() {
        let plain = Fragment::Box(box_fragment(
            Color::TRANSPARENT,
            Sides {
                top: Color::TRANSPARENT,
                right: Color::TRANSPARENT,
                bottom: Color::TRANSPARENT,
                left: Color::TRANSPARENT,
            },
            vec![Fragment::Text(text_fragment("only", color(0, 0, 0, 255)))],
        ));
        let result = LayoutResult {
            viewport: (100, 100),
            root: plain,
            fragment_count: 2,
        };
        let list = build_display_list(&result, PaintLimits::default()).unwrap();
        assert_eq!(list.commands.len(), 1);
        assert!(matches!(&list.commands[0], DisplayCommand::Text { text, .. } if text == "only"));
    }

    /// Used border widths derive from the rect pair: the test box has
    /// border 2/3 (padding origin at 2,3 inside border origin 0,0) and
    /// the same inset on the right/bottom edges.
    #[test]
    fn border_widths_derive_from_rects() {
        let root = Fragment::Box(box_fragment(color(10, 20, 30, 255), opaque(), vec![]));
        let result = LayoutResult {
            viewport: (100, 100),
            root,
            fragment_count: 1,
        };
        let list = build_display_list(&result, PaintLimits::default()).unwrap();
        let widths = match &list.commands[1] {
            DisplayCommand::Border { widths, .. } => *widths,
            other => panic!("expected border, got {other:?}"),
        };
        assert_eq!(widths.top, 3.0);
        assert_eq!(widths.left, 2.0);
        assert_eq!(widths.right, 2.0);
        assert_eq!(widths.bottom, 2.0);
    }

    /// The dump is one physical line per command with the documented
    /// formats; text escaping keeps multi-line runs on one line.
    #[test]
    fn dump_is_deterministic_and_escaped() {
        let root = Fragment::Box(box_fragment(
            color(255, 136, 0, 255),
            opaque(),
            vec![Fragment::Text(text_fragment(
                "a \"q\" b\nc",
                color(0, 0, 0, 255),
            ))],
        ));
        let result = LayoutResult {
            viewport: (800, 600),
            root,
            fragment_count: 3,
        };
        let list = build_display_list(&result, PaintLimits::default()).unwrap();
        let dump = list.to_dump_string();
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(lines[0], "paint v1 viewport=800x600");
        assert_eq!(lines[1], "rect [0.00,0.00,100.00x100.00] #FF8800FF");
        assert!(
            lines[2].starts_with("border [0.00,0.00,100.00x100.00] top=3.00 "),
            "border line: {}",
            lines[2]
        );
        assert_eq!(
            lines[3],
            "text \"a \\\"q\\\" b\\nc\" [4.00,5.00,20.00x10.00] #000000FF"
        );
        // Byte-identical across builds of the same input.
        let again = build_display_list(&result, PaintLimits::default())
            .unwrap()
            .to_dump_string();
        assert_eq!(dump, again);
    }

    /// The command limit is typed exhaustion, and no partial list leaks.
    #[test]
    fn limit_is_typed() {
        let leaf = Fragment::Box(box_fragment(color(1, 2, 3, 255), opaque(), vec![]));
        let root = Fragment::Box(box_fragment(
            color(4, 5, 6, 255),
            opaque(),
            vec![
                leaf,
                Fragment::Text(text_fragment("x", color(0, 0, 0, 255))),
            ],
        ));
        let result = LayoutResult {
            viewport: (100, 100),
            root,
            fragment_count: 3,
        };
        // Root bg + root border + child bg + child border + text = 5.
        assert!(build_display_list(&result, PaintLimits { max_commands: 5 }).is_ok());
        assert_eq!(
            build_display_list(&result, PaintLimits { max_commands: 4 }).unwrap_err(),
            PaintError::LimitExceeded
        );
    }

    /// A 512-deep fragment tree (the DOM depth cap) walks without
    /// overflowing the stack — the explicit-stack machine contract.
    #[test]
    fn deep_tree_walks_iteratively() {
        let mut fragment = Fragment::Box(box_fragment(Color::TRANSPARENT, opaque(), vec![]));
        for _ in 0..600 {
            fragment = Fragment::Box(box_fragment(Color::TRANSPARENT, opaque(), vec![fragment]));
        }
        let result = LayoutResult {
            viewport: (100, 100),
            root: fragment,
            fragment_count: 601,
        };
        let list = build_display_list(&result, PaintLimits::default()).unwrap();
        // Only borders (backgrounds transparent): 601 commands.
        assert_eq!(list.commands.len(), 601);
    }
}

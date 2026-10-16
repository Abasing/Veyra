//! The built-in user-agent stylesheet.
//!
//! The rendering-relevant defaults a plain HTML document starts from —
//! the same shape as every browser's html.css, scaled to the Phase 3
//! working property set (display/color/font/text/margins/paddings).
//! Declarations outside the working set are intentionally absent (they
//! would be ignored by the cascade anyway); the sheet grows with the
//! property set, never ahead of it. Font sizes and margins follow the
//! de-facto browser UA values so golden-image fixtures later line up
//! with the reference behavior they are generated against.

/// The user-agent stylesheet source (parsed once into every
/// [`StyleSet`](crate::StyleSet) at construction).
pub const USER_AGENT_STYLESHEET: &str = r#"
html { display: block; }
head, base, link, meta, noscript, script, style, template, title { display: none; }
body { display: block; margin: 8px; }

p, blockquote, figure, dl, dir, menu { display: block; margin: 1em 0; }
dd { display: block; margin-left: 40px; }
blockquote, figure { margin: 1em 40px; }
center { display: block; text-align: center; }

div, address, article, aside, details, dialog, figcaption, footer,
form, header, hgroup, main, nav, section, summary { display: block; }
hr { display: block; margin: 0.5em auto; }

h1, h2, h3, h4, h5, h6 { display: block; font-weight: bold; }
h1 { font-size: 2em; margin: 0.67em 0; }
h2 { font-size: 1.5em; margin: 0.83em 0; }
h3 { font-size: 1.17em; margin: 1em 0; }
h4 { font-size: 1em; margin: 1.33em 0; }
h5 { font-size: 0.83em; margin: 1.67em 0; }
h6 { font-size: 0.67em; margin: 2.33em 0; }

b, strong { font-weight: bolder; }
i, em, cite, dfn, var { font-style: italic; }
u, ins { text-decoration: underline; }
s, strike, del { text-decoration: line-through; }
small, big { font-size: larger; }
code, kbd, samp, tt, plaintext, listing { font-family: monospace; }
pre, xmp { display: block; font-family: monospace; margin: 1em 0; }
nobr { }

a:link { color: blue; text-decoration: underline; }
mark { background-color: yellow; color: black; }

ul, ol { display: block; margin: 1em 0; padding-left: 40px; }
li { display: block; }
fieldset { display: block; margin: 0 2px; padding: 0.35em 0.75em 0.625em; }
legend { display: block; padding: 0 2px; }
"#;

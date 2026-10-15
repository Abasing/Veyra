//! `veyra-style` — Veyra's CSS engine entry: untrusted CSS bytes in,
//! Veyra-owned parse trees out.
//!
//! Batch 1 (roadmap Phase 3): stylesheet parsing per CSS Syntax 3 —
//! tokenization, rule structure with spec error recovery, declaration
//! lists, selector parsing per Selectors 4 (the `selectors` crate behind
//! Veyra types), and canonical serialization. The Servo parsing crates
//! (`cssparser`, `selectors`) are consumed here and never leak: the
//! public surface is [`Stylesheet`], [`CssRule`], [`DeclarationList`],
//! [`SelectorList`], [`RetainedValue`], and [`ParseError`], so a future
//! parser replacement is a one-crate change (dependency-graph §1
//! invariant 5, the same shape as `veyra-url`/`veyra-html`).
//!
//! ADR-0003 adopts cssparser/selectors (MPL-2.0, file-level license
//! isolation); ADR-0014 records the Phase 3 entry spike that chose the
//! hand-built cascade over stylo and quantifies the deferred stylo
//! integration surface.
//!
//! # What is parsed versus retained
//!
//! Structure the engine will act on is parsed into typed values:
//! style-rule selector lists, declaration names/important flags,
//! keyframe selectors. Everything the cascade will interpret *later*
//! (declaration values) is retained as a [`RetainedValue`] — an owned
//! token tree that round-trips through the CSS Syntax canonical
//! serialization. Typed property values arrive with the cascade batch
//! (Phase 3, batch 2); retained values keep that batch's optionality:
//! unknown properties must parse, retain, and re-serialize losslessly
//! (custom properties are CSS).
//!
//! # Scope notes (deliberate deferrals, each with a roadmap home)
//!
//! - **Encoding**: batch 1 parses UTF-8 (with BOM) only; `@charset`
//!   selection and the full charset stack are the charset work item
//!   (recorded in the compatibility matrix, ST6). A UTF-16 BOM is a
//!   typed [`ParseError::NonUtf8`] rejection, not a misparse.
//! - **URL resolution**: `@import`/`@namespace` URLs are retained as
//!   parsed strings; resolution against the stylesheet base URL (the
//!   `STYLE -> URL` graph edge) arrives with the cascade batch.
//! - **CSS nesting**: `&` parent selectors and nested style rules are
//!   deferred (recorded in the compatibility matrix row for CSS
//!   parsing); the structure does not preclude them.
//!
//! # Security posture
//!
//! - Parsing never panics on malformed input (engineering standards §2);
//!   every failure is a typed [`ParseError`] and malformed rules are
//!   dropped per the spec's error-recovery rules.
//! - [`Stylesheet::parse`] enforces the normative stylesheet size limit
//!   ([`MAX_STYLESHEET_BYTES`], configurable via [`Limits`]) before
//!   parsing begins — unbounded input is a memory-amplification vector
//!   (engineering standards §6).
//! - Deeply nested blocks are bounded by the parser's internal nesting
//!   limit; `calc(calc(calc(…)))`-shaped inputs cannot blow the stack
//!   (fuzz-tested).
//!
//! # Examples
//!
//! ```
//! use veyra_style::{Limits, Stylesheet};
//!
//! let sheet = Stylesheet::parse(
//!     "h1 { color: crimson; !important } p:hover { margin: 0 }",
//!     &Limits::default(),
//! )
//! .expect("valid stylesheet");
//! assert_eq!(sheet.rules().len(), 2);
//! assert_eq!(
//!     sheet.rules()[0].to_css_string(),
//!     "h1 { color: crimson; }"
//! );
//! ```

#[cfg(test)]
mod corpus;
mod declarations;
mod error;
mod limits;
mod rules;
mod selector_impl;
mod selectors;
mod stylesheet;
mod tokens;

pub use declarations::{Declaration, DeclarationList};
pub use error::{ParseError, SelectorErrorKind, SyntaxKind};
pub use limits::{Limits, MAX_STYLESHEET_BYTES};
pub use rules::{CssRule, Keyframe, KeyframeSelector, LayerRule};
pub use selector_impl::{
    AttrValue, Identifier, LocalName, NamespacePrefix, NamespaceUrl, PseudoClass, PseudoElement,
    VeyraSelectorImpl,
};
pub use selectors::SelectorList;
pub use stylesheet::Stylesheet;
pub use tokens::RetainedValue;

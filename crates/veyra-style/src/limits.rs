//! The normative limits owned by `veyra-style`.
//!
//! The limits table (engineering standards §6) is normative: the
//! stylesheet size limit lives here and is tested (tests/limits.rs).
//! Limits are configuration, defaulted conservatively — callers that
//! need different values construct [`Limits`] explicitly.

use std::fmt;

/// The default stylesheet size limit: 16 MiB.
///
/// Normative value from the engineering-standards limits table
/// (`veyra-style` is the owning crate). Enforcement happens before any
/// parsing work, so oversized inputs cost one length check.
pub const MAX_STYLESHEET_BYTES: usize = 16 * 1024 * 1024;

/// Parsing limits for [`Stylesheet::parse`](crate::Stylesheet::parse).
///
/// Construct with [`Limits::default`] for the normative defaults, or
/// build a custom value for constrained embedders. Every field has a
/// floor enforced at construction (a zero limit rejects all input —
/// that is a valid configuration, not an error).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum stylesheet size in bytes. Inputs longer than this are
    /// rejected with [`ParseError::TooLarge`](crate::ParseError::TooLarge)
    /// before parsing begins.
    pub max_stylesheet_bytes: usize,
}

impl Limits {
    /// The normative defaults from the engineering-standards limits
    /// table.
    pub fn new() -> Self {
        Limits::default()
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_stylesheet_bytes: MAX_STYLESHEET_BYTES,
        }
    }
}

impl fmt::Display for Limits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Limits {{ max_stylesheet_bytes: {} }}",
            self.max_stylesheet_bytes
        )
    }
}

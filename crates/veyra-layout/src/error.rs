//! Typed layout errors.
//!
//! Layout never panics on any tree shape or style combination (engineering
//! standards §2: panicking on malformed input is a denial-of-service bug).
//! Every failure mode is a typed [`LayoutError`] the caller (the shell) can
//! report and recover from — a page that exceeds the layout limits fails to
//! lay out as a whole; no partial fragments are produced.

use std::fmt;

/// A layout failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutError {
    /// A configured limit was exceeded. The names mirror the
    /// engineering-standards limits table (each limit is configuration,
    /// defaulted conservatively).
    LimitExceeded {
        /// Which limit fired (`boxes`).
        limit: &'static str,
        /// The configured maximum.
        max: usize,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded { limit, max } => {
                write!(f, "layout limit exceeded: {limit} > {max}")
            }
        }
    }
}

impl std::error::Error for LayoutError {}

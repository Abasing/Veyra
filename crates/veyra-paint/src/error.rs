//! Typed paint-stage errors.
//!
//! Construction is a pure tree walk with one failure mode — exceeding
//! [`PaintLimits::max_commands`](crate::PaintLimits::max_commands) —
//! surfaced as a typed error per the engineering standards (§2: no
//! panics; §6: limits are configuration with typed exhaustion).

use std::fmt;

/// Everything that can go wrong building a display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintError {
    /// The display list exceeded
    /// [`PaintLimits::max_commands`](crate::PaintLimits::max_commands)
    /// (the engineering-standards limits row). No partial list escapes —
    /// the caller gets the limit, not a truncated frame.
    LimitExceeded,
}

impl fmt::Display for PaintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaintError::LimitExceeded => {
                write!(f, "display list exceeded the command limit")
            }
        }
    }
}

impl std::error::Error for PaintError {}

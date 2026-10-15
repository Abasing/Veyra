//! Typed errors for `veyra-gfx`.
//!
//! Every recoverable failure is a value, not a panic: the workspace denies
//! `panic`, `unwrap`, and `expect` (engineering standards §2) because a
//! browser must not crash on malformed input. The buffer/raster APIs in this
//! crate validate dimensions, strides, and lengths explicitly and return
//! these errors instead.

use std::fmt;

/// Errors produced by pixel-buffer construction, validation, and drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GfxError {
    /// A dimension (or stride) was zero, which cannot describe an image.
    ZeroDimension {
        /// Name of the offending parameter (`width`, `height`, or `stride`).
        name: &'static str,
    },
    /// A dimension exceeded the crate-wide sanity limit
    /// ([`MAX_DIMENSION`](crate::MAX_DIMENSION)).
    DimensionExceedsLimit {
        /// Name of the offending parameter.
        name: &'static str,
        /// The rejected value.
        value: u32,
        /// The inclusive upper bound that was exceeded.
        limit: u32,
    },
    /// An arithmetic operation on dimensions/strides overflowed its
    /// integer type. Reported instead of wrapping.
    Overflow {
        /// Description of the computation that overflowed.
        operation: &'static str,
    },
    /// A caller-supplied pixel buffer did not match the declared geometry.
    BufferLengthMismatch {
        /// Exact length required by `(width, height, stride)`.
        expected: usize,
        /// The length the caller supplied.
        actual: usize,
    },
    /// A row stride was smaller than `width * 4` bytes.
    StrideTooSmall {
        /// The supplied stride in bytes.
        stride: usize,
        /// The minimum stride required (`width * 4`).
        required: usize,
    },
    /// The allocator refused a (very large) buffer request. Recoverable:
    /// callers may retry with smaller dimensions.
    AllocationFailed {
        /// Number of bytes that were requested.
        bytes: usize,
    },
    /// A caller-supplied rectangle was invalid (zero size or coordinates
    /// that overflow `u32` when combined with its size).
    InvalidRect {
        /// Human-readable reason the rectangle was rejected.
        reason: &'static str,
    },
}

impl fmt::Display for GfxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GfxError::ZeroDimension { name } => {
                write!(f, "{name} must be at least 1 (got 0)")
            }
            GfxError::DimensionExceedsLimit { name, value, limit } => {
                write!(f, "{name} is {value}, which exceeds the limit {limit}")
            }
            GfxError::Overflow { operation } => {
                write!(f, "integer overflow while computing {operation}")
            }
            GfxError::BufferLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "pixel buffer length mismatch: expected {expected} bytes for the \
                     declared (width, height, stride), got {actual}"
                )
            }
            GfxError::StrideTooSmall { stride, required } => {
                write!(
                    f,
                    "row stride {stride} bytes is smaller than the required {required} \
                     bytes (width * 4)"
                )
            }
            GfxError::AllocationFailed { bytes } => {
                write!(
                    f,
                    "allocation of {bytes} bytes failed; dimensions are valid \
                           but memory is unavailable"
                )
            }
            GfxError::InvalidRect { reason } => {
                write!(f, "invalid rectangle: {reason}")
            }
        }
    }
}

impl std::error::Error for GfxError {}

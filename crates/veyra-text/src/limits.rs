//! Resource limits for the text stack (engineering standards §6).
//!
//! The limits table is normative; each limit has an owning crate and tests.
//! The shaping limit bounds a *single* `FontLibrary::shape` call so a
//! pathological input cannot allocate an unbounded glyph buffer: glyph
//! count is O(input length), so a hard character cap bounds memory
//! linearly. Long text is the norm (an 8 MiB text node is allowed by the
//! veyra-html limits), so callers that can exceed the cap chunk their runs
//! at character boundaries and accumulate — the `TextMeasurer` adapter in
//! `veyra-layout` does exactly that (totality contract: measurement never
//! errors).

/// The maximum number of Unicode scalar values a single
/// `FontLibrary::shape` call accepts (`2,000,000`; the
/// engineering-standards limits row). Above it, shaping returns
/// [`TextError::RunTooLong`](crate::TextError::RunTooLong).
///
/// Glyph buffers scale linearly with input, so the cap bounds a shape
/// call's memory deterministically (tens of MB worst case, vs unbounded
/// before the cap).
pub const MAX_RUN_CHARS: usize = 2_000_000;

//! Typed text-stack errors.
//!
//! Every failure mode of the text stack is a typed error — parsing font
//! bytes, selecting an unknown [`FontId`](crate::font::FontId), or
//! exceeding the shaping input limit ([`MAX_RUN_CHARS`], the
//! engineering-standards limits row). No panics on untrusted input
//! (engineering standards §2); external parser error types are deliberately
//! not exposed (dependency-graph §5 isolation): the payload-free variants
//! carry the classification, the cause stays in this crate's internals.

use std::fmt;

use crate::limits::MAX_RUN_CHARS;

/// Everything that can go wrong in the text stack.
#[derive(Debug)]
pub enum TextError {
    /// The provided bytes are not a parseable OpenType/TrueType face.
    FaceParse,
    /// The [`FontId`](crate::font::FontId) does not name a face in this
    /// [`FontLibrary`](crate::font::FontLibrary) (it may belong to another
    /// library instance, or the face was never loaded).
    UnknownFont,
    /// A single [`FontLibrary::shape`](crate::font::FontLibrary::shape)
    /// call exceeded [`MAX_RUN_CHARS`] Unicode scalar values. Callers
    /// chunk long runs; the limit exists so a pathological input cannot
    /// allocate unbounded glyph buffers.
    RunTooLong,
    /// A font file could not be read from disk (I/O — only
    /// [`FontLibrary::load_file`](crate::font::FontLibrary::load_file)/
    /// [`FontLibrary::load_dir`](crate::font::FontLibrary::load_dir) touch
    /// the filesystem; the engine core loads fonts from memory).
    Io(std::io::Error),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::FaceParse => write!(f, "font bytes are not a parseable face"),
            TextError::UnknownFont => write!(f, "font id is not in this library"),
            TextError::RunTooLong => {
                write!(f, "shaping run exceeds the {MAX_RUN_CHARS}-character limit")
            }
            TextError::Io(error) => write!(f, "font file I/O failed: {error}"),
        }
    }
}

impl std::error::Error for TextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TextError::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TextError {
    fn from(error: std::io::Error) -> Self {
        TextError::Io(error)
    }
}

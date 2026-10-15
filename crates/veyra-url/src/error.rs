//! Typed errors for URL parsing.
//!
//! Per engineering standards §2, errors are typed at the boundary and every
//! variant is documented; no stringly-typed errors cross crate lines. The
//! WHATWG "failure" outcomes of the URL parser map one-to-one onto the
//! [`UrlError::Syntax`] variants; [`UrlError::TooLong`] is Veyra's own
//! denial-of-service input limit (limits are configuration, defaulted
//! conservatively — engineering standards §6).

use std::fmt;

/// The single error type of [`crate::Url`] parsing and resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UrlError {
    /// The input exceeded the configured length limit before parsing began.
    ///
    /// URL length is unbounded in the WHATWG standard; browsers impose
    /// implementation limits to bound memory. Veyra's default is
    /// [`crate::MAX_URL_LENGTH`] and is configurable per call via
    /// [`crate::Url::parse_with_limits`].
    TooLong {
        /// The configured maximum input length in bytes.
        max: usize,
        /// The actual input length in bytes.
        actual: usize,
    },
    /// URL parser failure: the host component is empty where a non-empty
    /// host is required (e.g. `http://`).
    EmptyHost,
    /// URL parser failure: IDNA/UTS-46 domain-to-ASCII processing failed
    /// (malformed or forbidden Unicode input).
    IdnaError,
    /// URL parser failure: the port is not a valid 16-bit number.
    InvalidPort,
    /// URL parser failure: the host is not a valid IPv4 address.
    InvalidIpv4Address,
    /// URL parser failure: the host is not a valid IPv6 address.
    InvalidIpv6Address,
    /// URL parser failure: the domain contains a forbidden code point.
    InvalidDomainCharacter,
    /// URL parser failure: a relative URL was supplied with no base URL.
    RelativeUrlWithoutBase,
    /// URL parser failure: a relative URL was supplied against a base that
    /// cannot serve as a base (a cannot-be-a-base URL, e.g. `mailto:`).
    RelativeUrlWithCannotBeABaseBase,
    /// URL parser failure: the host or port of a cannot-be-a-base URL was
    /// mutated, which the standard forbids.
    SetHostOnCannotBeABaseUrl,
    /// URL parser failure: the input overflowed the parser's numeric
    /// limits (e.g. an out-of-range IPv4 piece).
    Overflow,
    /// A failure kind the external parser reports that has no mapping above
    /// yet. Unreachable with the pinned parser version (every current kind
    /// is mapped); it exists so a dependency bump *fails loudly with the
    /// kind name* instead of silently collapsing a new failure mode into an
    /// existing variant. Not attacker-derived data: the string is the
    /// upstream enum's `Debug` rendering.
    Other {
        /// `Debug` rendering of the unmapped upstream error kind.
        kind: String,
    },
}

impl UrlError {
    /// Maps an external parser failure into the Veyra error type.
    ///
    /// This is the only place the external parser's error type appears;
    /// replacement remains local to this crate (dependency-graph §5).
    pub(crate) fn from_external(error: url::ParseError) -> Self {
        match error {
            url::ParseError::EmptyHost => Self::EmptyHost,
            url::ParseError::IdnaError => Self::IdnaError,
            url::ParseError::InvalidPort => Self::InvalidPort,
            url::ParseError::InvalidIpv4Address => Self::InvalidIpv4Address,
            url::ParseError::InvalidIpv6Address => Self::InvalidIpv6Address,
            url::ParseError::InvalidDomainCharacter => Self::InvalidDomainCharacter,
            url::ParseError::RelativeUrlWithoutBase => Self::RelativeUrlWithoutBase,
            url::ParseError::RelativeUrlWithCannotBeABaseBase => {
                Self::RelativeUrlWithCannotBeABaseBase
            }
            url::ParseError::SetHostOnCannotBeABaseUrl => Self::SetHostOnCannotBeABaseUrl,
            url::ParseError::Overflow => Self::Overflow,
            _ => Self::from_external_other(error),
        }
    }

    /// Catch-all for external parser error kinds added after the variants
    /// above were mapped, so a new upstream kind surfaces as itself instead
    /// of silently collapsing into an existing variant.
    fn from_external_other(error: url::ParseError) -> Self {
        Self::Other {
            kind: format!("{error:?}"),
        }
    }
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { max, actual } => {
                write!(
                    f,
                    "URL input of {actual} bytes exceeds the {max}-byte limit"
                )
            }
            Self::EmptyHost => write!(f, "empty host where a non-empty host is required"),
            Self::IdnaError => write!(f, "IDNA domain-to-ASCII processing failed"),
            Self::InvalidPort => write!(f, "invalid port number"),
            Self::InvalidIpv4Address => write!(f, "invalid IPv4 address"),
            Self::InvalidIpv6Address => write!(f, "invalid IPv6 address"),
            Self::InvalidDomainCharacter => write!(f, "forbidden domain code point"),
            Self::RelativeUrlWithoutBase => {
                write!(f, "relative URL with no base URL")
            }
            Self::RelativeUrlWithCannotBeABaseBase => {
                write!(f, "relative URL against a cannot-be-a-base base URL")
            }
            Self::SetHostOnCannotBeABaseUrl => {
                write!(f, "host mutation on a cannot-be-a-base URL")
            }
            Self::Overflow => write!(f, "URL parser numeric overflow"),
            Self::Other { kind } => {
                write!(f, "unmapped URL parser failure kind: {kind}")
            }
        }
    }
}

impl std::error::Error for UrlError {}

//! `veyra-url` — Veyra's single URL parsing authority.
//!
//! Every URL in the engine — navigation targets, request URLs in
//! `veyra-net`, base URLs on DOM documents, origins on nodes — is created
//! through this crate, which wraps the WHATWG URL parser
//! ([`url`](https://docs.rs/url) 2.x, the audit-adopted Phase 2 dependency)
//! behind Veyra-owned types (`docs/architecture/dependency-graph.md` §1
//! invariant 5, §5). The external crate never leaks: callers see [`Url`],
//! [`Origin`], [`Host`], and [`UrlError`] only, so a future parser swap is a
//! one-crate change.
//!
//! Scope (roadmap Phase 2): parsing, serialization, relative resolution
//! ([`Url::join`]), origin computation, scheme/host/port accessors, and the
//! denial-of-service input limit. Setter APIs arrive with the script phase
//! that needs them; the type is deliberately read-only until then.
//!
//! # Security posture
//!
//! - Parsing never panics on malformed input (engineering standards §2);
//!   every failure is a typed [`UrlError`].
//! - [`Url::parse`] enforces a conservative input-length limit
//!   ([`MAX_URL_LENGTH`], configurable) before parsing begins — unbounded
//!   input is a memory-amplification vector.
//! - [`fmt::Debug`] on [`Url`] redacts credentials: URLs end up in
//!   diagnostics, and default logs must not contain session URLs with
//!   credentials (engineering standards §10).
//!
//! # Examples
//!
//! ```
//! use veyra_url::Url;
//!
//! let base = Url::parse("https://example.org/dir/page.html").expect("valid base");
//! let joined = base.join("./img/a.png").expect("valid relative");
//! assert_eq!(joined.as_str(), "https://example.org/dir/img/a.png");
//! assert_eq!(joined.origin().ascii_serialization(), "https://example.org");
//! ```

#![forbid(unsafe_code)]

mod error;
mod origin;

use std::fmt;
use std::path::PathBuf;

pub use error::UrlError;
pub use origin::{Host, Origin};

/// Default maximum URL input length: 2 MiB of bytes.
///
/// The standard imposes no limit; browsers do (Chromium's URL bar limit is
/// ~2 MB, its internal limits vary). 2 MiB comfortably covers every
/// legitimate web URL — including long `data:` URLs — while bounding parser
/// memory.
pub const MAX_URL_LENGTH: usize = 2 * 1024 * 1024;

/// Input limits for URL parsing (engineering standards §6: limits are
/// configuration, defaulted conservatively).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum input length in bytes; longer input is rejected with
    /// [`UrlError::TooLong`] before parsing begins.
    pub max_length: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_length: MAX_URL_LENGTH,
        }
    }
}

/// A parsed, validated URL.
///
/// Cheap to clone; equality compares the serialized form. The type is
/// read-only by design at this stage (see crate docs).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Url {
    inner: url::Url,
}

impl Url {
    /// Parses an absolute URL with the default input limit. Relative input
    /// fails with [`UrlError::RelativeUrlWithoutBase`] — use
    /// [`Url::parse_with_base`] or [`Url::join`] for resolution against a
    /// base.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError::TooLong`] for input over the default limit and
    /// the mapped parser failures otherwise.
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        Self::parse_with_limits(input, &Limits::default())
    }

    /// Parses with an explicit input limit (see [`Limits`]).
    ///
    /// # Errors
    ///
    /// As [`Url::parse`]; [`UrlError::TooLong`] uses the supplied limit.
    pub fn parse_with_limits(input: &str, limits: &Limits) -> Result<Self, UrlError> {
        if input.len() > limits.max_length {
            return Err(UrlError::TooLong {
                max: limits.max_length,
                actual: input.len(),
            });
        }
        let inner = url::Url::parse(input).map_err(UrlError::from_external)?;
        Ok(Self { inner })
    }

    /// Parses `input` resolved against `base` (the standard's basic parser
    /// with a base URL), with the default input limit.
    ///
    /// # Errors
    ///
    /// [`UrlError::TooLong`] for over-limit input; parser failures
    /// otherwise, including [`UrlError::RelativeUrlWithCannotBeABaseBase`]
    /// when `base` cannot serve as a base.
    pub fn parse_with_base(base: &Url, input: &str) -> Result<Self, UrlError> {
        Self::parse_with_base_and_limits(base, input, &Limits::default())
    }

    /// Parses with a base and an explicit input limit.
    ///
    /// # Errors
    ///
    /// As [`Url::parse_with_base`].
    pub fn parse_with_base_and_limits(
        base: &Url,
        input: &str,
        limits: &Limits,
    ) -> Result<Self, UrlError> {
        if input.len() > limits.max_length {
            return Err(UrlError::TooLong {
                max: limits.max_length,
                actual: input.len(),
            });
        }
        let inner = url::Url::options()
            .base_url(Some(&base.inner))
            .parse(input)
            .map_err(UrlError::from_external)?;
        Ok(Self { inner })
    }

    /// Resolves `input` relative to this URL (the standard's basic parser
    /// with this URL as base), with the default input limit.
    ///
    /// # Errors
    ///
    /// As [`Url::parse_with_base`].
    pub fn join(&self, input: &str) -> Result<Self, UrlError> {
        let limits = Limits::default();
        if input.len() > limits.max_length {
            return Err(UrlError::TooLong {
                max: limits.max_length,
                actual: input.len(),
            });
        }
        let inner = self.inner.join(input).map_err(UrlError::from_external)?;
        Ok(Self { inner })
    }

    /// The serialized URL (the standard's serialization, e.g.
    /// `"https://example.org/dir/page.html"`).
    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// Consumes the URL, returning its serialization.
    pub fn into_string(self) -> String {
        self.inner.into()
    }

    /// The scheme, ASCII-lowercased, without the trailing colon.
    pub fn scheme(&self) -> &str {
        self.inner.scheme()
    }

    /// `true` for the standard's "special" schemes (`http`, `https`, `ws`,
    /// `wss`, `ftp`, `file`) — those with default ports and
    /// slash-normalized paths.
    pub fn is_special(&self) -> bool {
        self.inner.is_special()
    }

    /// `true` for URLs whose path cannot be expressed as `/`-separated
    /// segments (e.g. `mailto:`); such URLs have an opaque path.
    pub fn cannot_be_a_base(&self) -> bool {
        self.inner.cannot_be_a_base()
    }

    /// The origin of this URL (HTML standard §7.5): a tuple for
    /// network-scheme URLs, opaque otherwise. `file:` URLs serialize to
    /// `"null"` here — the standard leaves the file origin
    /// implementation-defined, and Veyra's Phase 2 choice (opaque,
    /// non-equal across documents) is recorded in the compatibility matrix.
    pub fn origin(&self) -> Origin {
        origin::origin_of(&self.inner)
    }

    /// The host component, or `None` for URLs without one.
    pub fn host(&self) -> Option<Host> {
        self.inner.host().map(origin::host_borrowed)
    }

    /// The host as an ASCII string (IDNA-serialized, IPv6 without
    /// brackets), or `None` for URLs without a host.
    pub fn host_str(&self) -> Option<&str> {
        self.inner.host_str()
    }

    /// The explicit port, or `None` when absent.
    pub fn port(&self) -> Option<u16> {
        self.inner.port()
    }

    /// The explicit port, or the scheme's default port when the URL has no
    /// explicit port (`None` for non-special schemes without a port).
    pub fn port_or_known_default(&self) -> Option<u16> {
        self.inner.port_or_known_default()
    }

    /// The path (`"/"`-prefixed for base-able URLs; the opaque path
    /// otherwise).
    pub fn path(&self) -> &str {
        self.inner.path()
    }

    /// The query component without the leading `?`, or `None`.
    pub fn query(&self) -> Option<&str> {
        self.inner.query()
    }

    /// The fragment component without the leading `#`, or `None`.
    pub fn fragment(&self) -> Option<&str> {
        self.inner.fragment()
    }

    /// The username (empty when absent).
    pub fn username(&self) -> &str {
        self.inner.username()
    }

    /// The password without the leading colon, or `None`.
    pub fn password(&self) -> Option<&str> {
        self.inner.password()
    }

    /// `true` when the URL carries credentials (username or password) —
    /// callers that forward requests must consult this before crossing
    /// origins.
    pub fn has_credentials(&self) -> bool {
        !self.inner.username().is_empty() || self.inner.password().is_some()
    }

    /// Converts a `file:` URL into a local filesystem path.
    ///
    /// Used by the shell's `file://` navigation path. Not a `file:` URL, a
    /// cannot-be-a-base URL, or a non-local file host are all reported as
    /// [`UrlError::RelativeUrlWithoutBase`] — the "input cannot serve as a
    /// local file reference" outcome — rather than inventing a variant per
    /// upstream cause; the distinction is not load-bearing for callers at
    /// this phase and is documented here instead of silently collapsed.
    ///
    /// # Errors
    ///
    /// See above: [`UrlError::RelativeUrlWithoutBase`] for every
    /// non-convertible input.
    pub fn to_file_path(&self) -> Result<PathBuf, UrlError> {
        self.inner
            .to_file_path()
            .map_err(|_| UrlError::RelativeUrlWithoutBase)
    }
}

impl fmt::Display for Url {
    /// The full serialization, including credentials. Prefer [`fmt::Debug`]
    /// in diagnostics (it redacts).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.inner.as_str())
    }
}

impl fmt::Debug for Url {
    /// Credential-redacting debug form: the password is always masked, and
    /// the username is dropped when a password is present — diagnostics and
    /// default logs must not leak credentials (engineering standards §10).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.has_credentials() {
            return f.write_str(self.inner.as_str());
        }
        // Strip the userinfo segment textually: everything between the
        // scheme's `://` (or the first `:`) and the `@` is credentials.
        let serialized = self.inner.as_str();
        let after_scheme = match serialized.find("://") {
            Some(index) => index + 3,
            None => match serialized.find(':') {
                Some(index) => index + 1,
                None => return f.write_str(serialized),
            },
        };
        match serialized[after_scheme..].find('@') {
            Some(at_offset) => {
                let host_and_rest = &serialized[after_scheme + at_offset..];
                write!(
                    f,
                    "{}[credentials redacted]{}",
                    &serialized[..after_scheme],
                    host_and_rest
                )
            }
            None => f.write_str(serialized),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn parses_absolute_and_rejects_relative_without_base() {
        let url = Url::parse("https://Example.ORG/dir/page.html").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("example.org"));
        assert_eq!(url.as_str(), "https://example.org/dir/page.html");

        let relative = Url::parse("/just/a/path");
        assert_eq!(relative.unwrap_err(), UrlError::RelativeUrlWithoutBase);
    }

    #[test]
    fn resolves_relative_urls_against_a_base() {
        let base = Url::parse("https://example.org/dir/page.html").unwrap();
        assert_eq!(
            base.join("img/a.png").unwrap().as_str(),
            "https://example.org/dir/img/a.png"
        );
        assert_eq!(
            base.join("/root").unwrap().as_str(),
            "https://example.org/root"
        );
        assert_eq!(
            base.join("../up").unwrap().as_str(),
            "https://example.org/up"
        );
        assert_eq!(
            base.join("#frag").unwrap().as_str(),
            "https://example.org/dir/page.html#frag"
        );
    }

    #[test]
    fn origin_is_tuple_for_network_schemes_and_opaque_otherwise() {
        let https = Url::parse("https://example.org:8443/x").unwrap();
        assert_eq!(
            https.origin(),
            Origin::Tuple {
                scheme: "https".to_string(),
                host: Host::Domain("example.org".to_string()),
                port: 8443,
            }
        );
        assert_eq!(
            https.origin().ascii_serialization(),
            "https://example.org:8443"
        );

        // The effective port (default applied) is stored; serialization
        // elides default ports even when they were explicit in the input.
        let explicit_default = Url::parse("https://example.org:443/x").unwrap();
        assert_eq!(
            explicit_default.origin(),
            Origin::Tuple {
                scheme: "https".to_string(),
                host: Host::Domain("example.org".to_string()),
                port: 443,
            }
        );
        assert_eq!(
            explicit_default.origin().ascii_serialization(),
            "https://example.org"
        );

        let implicit = Url::parse("https://example.org/x").unwrap();
        assert_eq!(explicit_default.origin(), implicit.origin());

        let data = Url::parse("data:text/plain,hi").unwrap();
        assert!(data.origin().is_opaque());
        assert_eq!(data.origin().ascii_serialization(), "null");
    }

    #[test]
    fn same_origin_compares_components_and_opaque_never_matches() {
        let a = Url::parse("https://example.org/x").unwrap();
        let b = Url::parse("https://example.org:443/y").unwrap();
        let c = Url::parse("https://example.com/x").unwrap();
        assert!(a.origin().is_same_origin(&b.origin()));
        assert!(!a.origin().is_same_origin(&c.origin()));

        let d1 = Url::parse("data:text/plain,a").unwrap();
        let d2 = Url::parse("data:text/plain,b").unwrap();
        assert!(!d1.origin().is_same_origin(&d2.origin()));
    }

    #[test]
    fn debug_redacts_credentials_display_does_not() {
        let url = Url::parse("https://user:secret@example.org/x").unwrap();
        let debug = format!("{url:?}");
        assert!(!debug.contains("secret"), "debug leaked password: {debug}");
        assert!(
            !debug.contains("user:secret"),
            "debug leaked userinfo: {debug}"
        );
        assert!(debug.contains("[credentials redacted]"));
        assert!(debug.contains("example.org/x"));

        let display = format!("{url}");
        assert_eq!(display, "https://user:secret@example.org/x");

        let no_creds = Url::parse("https://example.org/x").unwrap();
        assert_eq!(format!("{no_creds:?}"), "https://example.org/x");
    }

    #[test]
    fn length_limit_is_enforced_and_configurable() {
        let long = format!("https://example.org/{}", "a".repeat(64));
        let tight = Limits { max_length: 32 };
        let error = Url::parse_with_limits(&long, &tight).unwrap_err();
        assert_eq!(
            error,
            UrlError::TooLong {
                max: 32,
                actual: long.len()
            }
        );

        // Default limit accepts this size comfortably.
        Url::parse(&long).unwrap();
    }

    #[test]
    fn malformed_input_is_typed_error_never_panic() {
        let cases = [
            "http://",              // empty host
            "http://[::99999]/",    // bad IPv6
            "http://1.2.3.4.5/",    // bad IPv4
            "http://exa mple.org/", // forbidden domain code point
        ];
        for case in cases {
            let error = Url::parse(case).unwrap_err();
            // Every case must be a *syntax* outcome, not the input limit.
            assert_ne!(error, UrlError::TooLong { max: 0, actual: 0 });
        }
    }

    #[test]
    fn ports_and_defaults_are_reported() {
        let explicit = Url::parse("http://example.org:8080/").unwrap();
        assert_eq!(explicit.port(), Some(8080));
        assert_eq!(explicit.port_or_known_default(), Some(8080));

        let implicit = Url::parse("http://example.org/").unwrap();
        assert_eq!(implicit.port(), None);
        assert_eq!(implicit.port_or_known_default(), Some(80));

        let unknown = Url::parse("foo://example.org/").unwrap();
        assert_eq!(unknown.port_or_known_default(), None);
    }

    #[test]
    fn file_urls_convert_to_paths() {
        let url = Url::parse("file:///tmp/veyra/index.html").unwrap();
        let path = url.to_file_path().unwrap();
        assert_eq!(path.to_string_lossy(), "/tmp/veyra/index.html");

        let not_file = Url::parse("https://example.org/x").unwrap();
        assert!(not_file.to_file_path().is_err());
    }

    #[test]
    fn idna_and_unicode_domains_serialize_to_ascii() {
        let url = Url::parse("https://日本.example/").unwrap();
        assert_eq!(url.host_str(), Some("xn--wgv71a.example"));
        let punycode = Url::parse("https://xn--wgv71a.example/").unwrap();
        assert_eq!(url.as_str(), punycode.as_str());
    }

    #[test]
    fn credentials_flag_and_accessors() {
        let plain = Url::parse("https://example.org/").unwrap();
        assert!(!plain.has_credentials());
        assert_eq!(plain.username(), "");
        assert_eq!(plain.password(), None);

        let user_only = Url::parse("https://alice@example.org/").unwrap();
        assert!(user_only.has_credentials());
        assert_eq!(user_only.username(), "alice");
        assert_eq!(user_only.password(), None);
    }
}

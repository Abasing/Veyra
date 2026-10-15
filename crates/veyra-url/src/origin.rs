//! Origins and hosts.
//!
//! An origin is the security principal of web content (HTML standard §7.5):
//! either an (scheme, host, port) tuple or an opaque origin that is unique
//! and unserializable ("null"). Every document, worker, and resource in the
//! engine carries an origin; `veyra-dom` stamps it onto document nodes and
//! `veyra-net` compares origins for cookie SameSite and redirect decisions.
//! Origin construction is deliberately parse-only: there is no way to
//! synthesize a tuple origin without going through the URL parser
//! (dependency-graph §1 invariant 5 — one parsing authority).

use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU64, Ordering};

/// The host component of an origin or URL.
///
/// Domains are stored already ASCII-lowercased and IDNA-processed by the
/// parser; the type is a Veyra-owned mirror of the external parser's host
/// representation so that crate never leaks (dependency-graph §5).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Host {
    /// A registered domain, already IDNA-serialized to ASCII.
    Domain(String),
    /// An IPv4 address literal.
    Ipv4(Ipv4Addr),
    /// An IPv6 address literal (the `Host::ascii_serialization` form adds
    /// the brackets).
    Ipv6(Ipv6Addr),
}

impl Host {
    /// The ASCII serialization of this host: domains as stored, address
    /// literals in standard notation, IPv6 in brackets per the URL
    /// standard's host serializer.
    pub fn ascii_serialization(&self) -> String {
        match self {
            Self::Domain(domain) => domain.clone(),
            Self::Ipv4(address) => address.to_string(),
            Self::Ipv6(address) => format!("[{address}]"),
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ascii_serialization())
    }
}

/// The origin of a URL: a tuple origin or an opaque origin.
///
/// The tuple's port is the *effective* port (the URL's explicit port, or the
/// scheme's default when absent), which makes equality the same-origin test:
/// `http://example.org` and `http://example.org:80` are equal. Serialization
/// elides the port when it equals the scheme's default, per the standard.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Origin {
    /// A tuple origin (scheme, host, effective port).
    Tuple {
        /// The scheme, ASCII-lowercased.
        scheme: String,
        /// The host, IDNA-processed.
        host: Host,
        /// The effective port (explicit port, or the scheme default).
        port: u16,
    },
    /// An opaque origin: serializes to `"null"` and is never same-origin
    /// with anything but itself. The payload is a process-unique identity
    /// minted at derivation time (per the standard, opaque origins are
    /// globally unique; equality is identity, never structure).
    Opaque(u64),
}

/// Monotonic source of opaque-origin identities. Ids are unique within a
/// process; they never serialize (opaque origins render as `"null"`) and are
/// never persisted, so cross-run values are unobservable.
static OPAQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_opaque_id() -> u64 {
    OPAQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// The standard's default-port table. Only schemes that can produce tuple
/// origins appear here; every other scheme is opaque (`Origin::Opaque`) and
/// never consults this table.
pub(crate) fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

impl Origin {
    /// `true` for opaque origins (serializes to `"null"`).
    pub fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque(_))
    }

    /// Same-origin comparison: tuple origins compare component-wise (the
    /// effective-port representation makes this exact); opaque origins are
    /// same-origin only with themselves (identity, not structure).
    pub fn is_same_origin(&self, other: &Origin) -> bool {
        self == other
    }

    /// The ASCII serialization: `scheme://host[:port]` for tuple origins —
    /// the port elided when it equals the scheme's default — and `"null"`
    /// for opaque origins.
    pub fn ascii_serialization(&self) -> String {
        match self {
            Self::Tuple { scheme, host, port } => {
                if default_port(scheme) == Some(*port) {
                    format!("{scheme}://{host}")
                } else {
                    format!("{scheme}://{host}:{port}")
                }
            }
            Self::Opaque(_) => "null".to_string(),
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ascii_serialization())
    }
}

/// Builds the Veyra origin of an already-parsed URL.
///
/// Tuple ports are the effective port: the external origin carries the
/// resolved port, and there is no path back to "explicit or default" — which
/// is exactly the representation same-origin comparison needs.
pub(crate) fn origin_of(parsed: &url::Url) -> Origin {
    match parsed.origin() {
        url::Origin::Tuple(scheme, host, port) => Origin::Tuple {
            scheme,
            host: host_owned(host),
            port,
        },
        url::Origin::Opaque(_) => Origin::Opaque(next_opaque_id()),
    }
}

/// Maps an owned external host into the Veyra [`Host`] type.
pub(crate) fn host_owned(host: url::Host<String>) -> Host {
    match host {
        url::Host::Domain(domain) => Host::Domain(domain),
        url::Host::Ipv4(address) => Host::Ipv4(address),
        url::Host::Ipv6(address) => Host::Ipv6(address),
    }
}

/// Maps a borrowed external host into the Veyra [`Host`] type.
pub(crate) fn host_borrowed(host: url::Host<&str>) -> Host {
    match host {
        url::Host::Domain(domain) => Host::Domain(domain.to_string()),
        url::Host::Ipv4(address) => Host::Ipv4(address),
        url::Host::Ipv6(address) => Host::Ipv6(address),
    }
}

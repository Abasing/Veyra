//! `veyra-net` — Veyra's network service: the policy layer over
//! hyper/rustls/hickory (ADR-0008).
//!
//! The crate owns *semantics*, not transport: scheme/port policy, the
//! redirect loop, cookie storage (RFC 6265 subset), and the response
//! limits from the engineering-standards table. The hyper/rustls/hickory
//! stack is consumed behind wrappers (dependency-graph §5): transport
//! types never appear in the public API — callers see [`Client`],
//! [`LoadedResource`], [`CookieJar`], and [`NetError`].
//!
//! Runtime model (engineering standards §5): services use tokio, the
//! engine core stays synchronous. [`Client::fetch`] is the async surface;
//! [`blocking`] is the bridge for the shell's worker threads.
//!
//! Phase 2 policy subset (documented in the compatibility matrix):
//! navigation GETs over http/https, the fetch bad-port list, redirect
//! following (limit 20) with the HTTP status semantics, cookies per the
//! RFC 6265 subset in [`cookie`], response body cap, response deadline,
//! and header count/size caps. CORS/CSP/mixed-content/fetch-metadata
//! arrive with their consumers; caching is a later phase.
//!
//! # Examples
//!
//! ```no_run
//! use veyra_net::{Client, NetworkConfig};
//! use veyra_url::Url;
//!
//! # async fn demo() -> Result<(), veyra_net::NetError> {
//! let client = Client::new(NetworkConfig::default())?;
//! let url = Url::parse("https://example.org/").expect("url");
//! let resource = client.fetch(&url, None).await?;
//! println!("status {} ({} bytes)", resource.status, resource.body.len());
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

pub mod blocking;
pub mod cookie;
pub mod dns;
pub mod policy;

use std::sync::Arc;
use std::time::Duration;

use http::header::{ACCEPT, COOKIE, USER_AGENT};
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use veyra_url::{Origin, Url};

use crate::cookie::CookieJar;

/// Normative limits (engineering standards §6 — owner: veyra-net).
pub mod limits {
    /// Maximum redirect hops followed before giving up.
    pub const MAX_REDIRECTS: usize = 20;

    /// Maximum header count accepted from a response.
    pub const MAX_HEADER_COUNT: usize = 128;

    /// Maximum total header-block size accepted from a response, bytes.
    pub const MAX_HEADER_BYTES: usize = 64 * 1024;

    /// Default in-memory response cap (limits table: 2 GiB streamed with
    /// memory-capped buffers; Phase 2 buffers documents in memory, so the
    /// default cap matches the document limit and is configurable).
    pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
}

/// Network service configuration (limits are configuration — §6).
#[derive(Clone, Debug)]
pub struct NetworkConfig {
    /// Maximum redirect hops ([`limits::MAX_REDIRECTS`] default).
    pub max_redirects: usize,
    /// Response body cap in bytes (in-memory Phase 2 model).
    pub max_response_bytes: u64,
    /// Timeout for the whole fetch — connect, headers, and body, across
    /// all redirect hops.
    pub response_deadline: Duration,
    /// TCP connect timeout per hop.
    pub connect_timeout: Duration,
    /// The User-Agent sent on requests.
    pub user_agent: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            max_redirects: limits::MAX_REDIRECTS,
            max_response_bytes: limits::DEFAULT_MAX_RESPONSE_BYTES,
            response_deadline: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(30),
            user_agent: concat!("Veyra/0.1 (+", env!("CARGO_PKG_REPOSITORY"), ")").to_string(),
        }
    }
}

/// Injected time source for cookie expiry (determinism: tests inject
/// time; no wall-clock reads in engine logic).
pub trait Clock: Send + Sync {
    /// Current time as unix seconds.
    fn now_unix(&self) -> u64;
}

/// The system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// A fixed clock for tests.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_unix(&self) -> u64 {
        self.0
    }
}

/// Network fetch failure (typed; the transport error text is carried for
/// diagnostics only — never attacker-formatted assertions).
#[derive(Debug)]
#[non_exhaustive]
pub enum NetError {
    /// The URL's scheme is not fetchable (see [`policy::ALLOWED_SCHEMES`]).
    SchemeNotAllowed(String),
    /// The URL's port is on the fetch bad-port list.
    PortBlocked(u16),
    /// The redirect chain exceeded [`NetworkConfig::max_redirects`].
    TooManyRedirects {
        /// The configured hop limit.
        max: usize,
    },
    /// A response header block exceeded the limits table caps.
    HeaderLimitExceeded,
    /// The response body exceeded [`NetworkConfig::max_response_bytes`].
    ResponseTooLarge {
        /// The configured cap.
        max: u64,
    },
    /// The fetch exceeded [`NetworkConfig::response_deadline`].
    Timeout,
    /// DNS, connection, TLS, or HTTP-protocol failure.
    Transport(String),
    /// The response target was invalid (e.g. unresolvable redirect).
    InvalidResponseTarget(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemeNotAllowed(scheme) => write!(f, "scheme {scheme:?} is not fetchable"),
            Self::PortBlocked(port) => write!(f, "port {port} is blocked by fetch policy"),
            Self::TooManyRedirects { max } => write!(f, "more than {max} redirects"),
            Self::HeaderLimitExceeded => write!(f, "response headers exceed limits"),
            Self::ResponseTooLarge { max } => {
                write!(f, "response body exceeds the {max}-byte cap")
            }
            Self::Timeout => write!(f, "fetch exceeded the response deadline"),
            Self::Transport(message) => write!(f, "network failure: {message}"),
            Self::InvalidResponseTarget(message) => {
                write!(f, "invalid response target: {message}")
            }
        }
    }
}

impl std::error::Error for NetError {}

/// A loaded network resource: the terminal response of one fetch.
#[derive(Clone, Debug)]
pub struct LoadedResource {
    /// The URL of the response actually received (after redirects).
    pub final_url: Url,
    /// The HTTP status code.
    pub status: u16,
    /// Response headers, in arrival order.
    pub headers: Vec<(String, String)>,
    /// The response body, capped by [`NetworkConfig::max_response_bytes`].
    pub body: Vec<u8>,
    /// The redirect chain, in hop order (empty when unredirected).
    pub redirect_chain: Vec<Url>,
}

impl LoadedResource {
    /// The origin of the final URL (parse-derived; the origin-tag
    /// invariant's network side).
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.final_url.origin()
    }

    /// The value of the first header with this name (ASCII
    /// case-insensitive), or `None`.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The declared content type (the `Content-Type` header's media type
    /// part, lowercased, parameters stripped), or `None`.
    #[must_use]
    pub fn content_type(&self) -> Option<String> {
        let raw = self.header("content-type")?;
        let media_type = raw.split(';').next()?.trim().to_ascii_lowercase();
        if media_type.is_empty() {
            None
        } else {
            Some(media_type)
        }
    }
}

/// The network service: one client, one configuration, shared cookie
/// jars passed per fetch (a session owns its jar).
pub struct Client {
    config: NetworkConfig,
    clock: Box<dyn Clock>,
    inner: HyperClient<
        hyper_rustls::HttpsConnector<HttpConnector<dns::HickoryDns>>,
        http_body_util::Full<bytes::Bytes>,
    >,
}

impl Client {
    /// Builds a client with the given configuration and the system clock.
    ///
    /// # Errors
    ///
    /// [`NetError::Transport`] when the DNS resolver cannot be constructed
    /// from the system configuration.
    pub fn new(config: NetworkConfig) -> Result<Self, NetError> {
        Self::with_clock(config, SystemClock)
    }

    /// Builds a client with an injected clock (tests).
    ///
    /// # Errors
    ///
    /// As [`Client::new`].
    pub fn with_clock(
        config: NetworkConfig,
        clock: impl Clock + 'static,
    ) -> Result<Self, NetError> {
        let resolver = dns::HickoryDns::from_system_config()
            .map_err(|error| NetError::Transport(error.to_string()))?;
        let mut http = HttpConnector::new_with_resolver(resolver);
        http.set_connect_timeout(Some(config.connect_timeout));
        http.enforce_http(false);
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .wrap_connector(http);
        // hyper's own header cap sits above Veyra's so the policy limit
        // (not the transport) decides — transport types never make
        // security decisions (ADR-0008).
        let inner = HyperClient::builder(TokioExecutor::new())
            .timer(TokioTimer::new())
            .http1_max_headers(limits::MAX_HEADER_COUNT * 2)
            .build(https);
        Ok(Self {
            config,
            clock: Box::new(clock),
            inner,
        })
    }

    /// Fetches `url` (a navigation GET), following redirects, applying
    /// cookies from `jar` when provided, and enforcing the configured
    /// limits.
    ///
    /// # Errors
    ///
    /// [`NetError`] for policy violations and transport failures; HTTP
    /// error statuses (4xx/5xx) are *loaded resources*, not errors — the
    /// navigation layer decides what to show.
    pub async fn fetch(
        &self,
        url: &Url,
        jar: Option<&CookieJar>,
    ) -> Result<LoadedResource, NetError> {
        let now = self.clock.now_unix();
        if let Err(reason) = policy::check_request_url(url) {
            return Err(policy_error(url, reason));
        }

        let deadline = self.config.response_deadline;
        tokio::time::timeout(deadline, self.fetch_inner(url, jar, now))
            .await
            .map_err(|_| NetError::Timeout)?
    }

    async fn fetch_inner(
        &self,
        start: &Url,
        jar: Option<&CookieJar>,
        now: u64,
    ) -> Result<LoadedResource, NetError> {
        let mut current = start.clone();
        let mut redirect_chain: Vec<Url> = Vec::new();

        loop {
            let request = self.build_request(&current, jar, now)?;
            let response = self
                .inner
                .request(request)
                .await
                .map_err(|error| NetError::Transport(error.to_string()))?;

            let status = response.status().as_u16();
            let headers = collect_headers(response.headers())?;

            // Cookies from this response, before the redirect decision.
            if let Some(jar) = jar {
                let set_cookies: Vec<&str> = headers
                    .iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
                    .map(|(_, value)| value.as_str())
                    .collect();
                if !set_cookies.is_empty() {
                    jar.set_cookies(&current, &set_cookies, now);
                }
            }

            if policy::is_redirect(status) {
                if redirect_chain.len() >= self.config.max_redirects {
                    return Err(NetError::TooManyRedirects {
                        max: self.config.max_redirects,
                    });
                }
                let Some(location) = headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                    .map(|(_, value)| value)
                else {
                    // A redirect status without Location: terminal (some
                    // servers do this); return it as the loaded resource.
                    let body = self.read_body(response).await?;
                    return Ok(LoadedResource {
                        final_url: current.clone(),
                        status,
                        headers,
                        body,
                        redirect_chain,
                    });
                };
                let next = current
                    .join(location)
                    .map_err(|error| NetError::InvalidResponseTarget(error.to_string()))?;
                if let Err(reason) = policy::check_redirect(&current, &next) {
                    return Err(policy_error(&next, reason));
                }
                redirect_chain.push(current.clone());
                current = next;
                continue;
            }

            let body = self.read_body(response).await?;
            return Ok(LoadedResource {
                final_url: current,
                status,
                headers,
                body,
                redirect_chain,
            });
        }
    }

    fn build_request(
        &self,
        url: &Url,
        jar: Option<&CookieJar>,
        now: u64,
    ) -> Result<http::Request<http_body_util::Full<bytes::Bytes>>, NetError> {
        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(url.as_str())
            .header(USER_AGENT, self.config.user_agent.as_str())
            .header(ACCEPT, "text/html,application/xhtml+xml,*/*;q=0.8");
        if let Some(jar) = jar
            && let Some(cookie_header) = jar.cookie_header(url, now)
        {
            builder = builder.header(COOKIE, cookie_header);
        }
        builder
            .body(http_body_util::Full::new(bytes::Bytes::new()))
            .map_err(|error| NetError::Transport(error.to_string()))
    }

    async fn read_body(
        &self,
        response: http::Response<hyper::body::Incoming>,
    ) -> Result<Vec<u8>, NetError> {
        let mut body = response.into_body();
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            let Some(frame) = body.frame().await else {
                break;
            };
            let frame = frame.map_err(|error| NetError::Transport(error.to_string()))?;
            if let Some(data) = frame.data_ref() {
                buffer.extend_from_slice(data);
                if buffer.len() as u64 > self.config.max_response_bytes {
                    return Err(NetError::ResponseTooLarge {
                        max: self.config.max_response_bytes,
                    });
                }
            }
        }
        Ok(buffer)
    }

    /// A shared handle factory for sessions that want an owned jar.
    #[must_use]
    pub fn new_cookie_jar() -> CookieJar {
        CookieJar::new()
    }
}

/// Shared convenience: an `Arc`-wrapped client for multi-session use.
pub type SharedClient = Arc<Client>;

/// Collects headers into the Veyra form, enforcing the limits-table caps.
fn collect_headers(headers: &http::HeaderMap) -> Result<Vec<(String, String)>, NetError> {
    if headers.len() > limits::MAX_HEADER_COUNT {
        return Err(NetError::HeaderLimitExceeded);
    }
    let mut out = Vec::with_capacity(headers.len());
    let mut total = 0usize;
    for (name, value) in headers {
        let name = name.as_str().to_string();
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        total += name.len() + value.len() + 4;
        if total > limits::MAX_HEADER_BYTES {
            return Err(NetError::HeaderLimitExceeded);
        }
        out.push((name, value));
    }
    Ok(out)
}

/// Maps a policy reason string to the typed error.
fn policy_error(url: &Url, reason: &'static str) -> NetError {
    if reason.contains("port") {
        NetError::PortBlocked(url.port_or_known_default().unwrap_or(0))
    } else {
        NetError::SchemeNotAllowed(url.scheme().to_string())
    }
}

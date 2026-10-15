//! Cookie storage: RFC 6265 (bis) subset — parsing, storage, matching,
//! SameSite — with injected time for determinism (engineering standards
//! §8: "all tests inject time/RNG; no wall-clock sleeps").
//!
//! Phase 2 subset (recorded in the compatibility matrix):
//!
//! - Set-Cookie parsing: name=value, Domain, Path, Expires (IMF-fixdate),
//!   Max-Age, Secure, HttpOnly, SameSite=Lax/Strict/None (default Lax,
//!   matching modern browser behavior);
//! - storage keyed (domain, path, name), host-only vs domain cookies,
//!   first-match-wins overwrite on (name, domain, path) collision;
//! - request matching: domain-match, path-match, Secure, expiry;
//! - SameSite: attribute stored and exposed; Phase 2 issues only
//!   top-level navigation GETs, for which Lax/Strict cookies for the
//!   target site are sent (same-site by construction) — subresource
//!   requests in Phase 3 will consult [`CookieJar::is_same_site`].
//!   Site comparison is exact-host until the public-suffix list lands.

use std::collections::HashMap;
use std::sync::Mutex;

use veyra_url::Url;

/// Defensive cap on jar size (6265bis: "evict expired, then oldest").
pub const MAX_COOKIES: usize = 3_000;

/// A parsed cookie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cookie {
    /// The cookie name (non-empty; `=`-free per RFC 6265 §5.2).
    pub name: String,
    /// The cookie value.
    pub value: String,
    /// The domain the cookie is stored under (lowercase, no leading dot).
    pub domain: String,
    /// True for host-only cookies (no Domain attribute).
    pub host_only: bool,
    /// The cookie path (default-path computed from the request URI).
    pub path: String,
    /// Secure cookies travel only over https.
    pub secure: bool,
    /// HttpOnly cookies are unavailable to scripts (recorded now; the
    /// script engine arrives in Phase 4).
    pub http_only: bool,
    /// SameSite strictness.
    pub same_site: SameSite,
    /// Creation time (unix seconds, injected clock).
    pub creation_time: u64,
    /// Expiry (unix seconds), or `None` for a session cookie.
    pub expiry_time: Option<u64>,
}

/// SameSite strictness (6265bis §5.4.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SameSite {
    /// Sent only in same-site contexts.
    Strict,
    /// Sent in same-site contexts and top-level cross-site navigations.
    Lax,
    /// Sent everywhere (requires Secure per the modern default rules).
    None,
}

/// Shared, thread-safe cookie jar (engineering standards §5: services are
/// message-passing-first; this is the one shared-state structure a
/// session owns — a single `Mutex`, never held across `await`).
#[derive(Clone, Default)]
pub struct CookieJar {
    inner: std::sync::Arc<Mutex<JarLayout>>,
}

#[derive(Default)]
struct JarLayout {
    /// Keyed by (domain, path, name) — the RFC's storage identity.
    cookies: HashMap<(String, String, String), Cookie>,
}

impl CookieJar {
    /// An empty jar.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored cookies (live + expired; expiry filters on read).
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map(|jar| jar.cookies.len()).unwrap_or(0)
    }

    /// Whether the jar is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops all cookies (session reset).
    pub fn clear(&self) {
        if let Ok(mut jar) = self.inner.lock() {
            jar.cookies.clear();
        }
    }

    /// Processes `Set-Cookie` header values from a response to `url`.
    ///
    /// Invalid cookies are dropped (never panic); expired-at-parse
    /// cookies are stored-then-dropped on read, matching browsers.
    pub fn set_cookies(&self, url: &Url, set_cookie_values: &[&str], now: u64) {
        let Ok(mut jar) = self.inner.lock() else {
            return;
        };
        for value in set_cookie_values {
            if let Some(cookie) = parse_set_cookie(url, value, now) {
                if jar.cookies.len() >= MAX_COOKIES {
                    evict_one(&mut jar.cookies);
                }
                jar.cookies.insert(
                    (
                        cookie.domain.clone(),
                        cookie.path.clone(),
                        cookie.name.clone(),
                    ),
                    cookie,
                );
            }
        }
    }

    /// The `Cookie` header value for a request to `url` at time `now`,
    /// or `None` when no cookie matches. Cookies are sorted by
    /// (path length desc, creation asc) per RFC 6265 §5.4.
    #[must_use]
    pub fn cookie_header(&self, url: &Url, now: u64) -> Option<String> {
        let jar = self.inner.lock().ok()?;
        let host = url.host_str()?.to_ascii_lowercase();
        let path = request_path(url);
        let secure_request = url.scheme() == "https";

        let mut matched: Vec<&Cookie> = jar
            .cookies
            .values()
            .filter(|cookie| {
                if cookie.secure && !secure_request {
                    return false;
                }
                if let Some(expiry) = cookie.expiry_time
                    && now >= expiry
                {
                    return false;
                }
                domain_match(&host, cookie) && path_match(&path, &cookie.path)
            })
            .collect();
        matched.sort_by(|a, b| {
            b.path
                .len()
                .cmp(&a.path.len())
                .then(a.creation_time.cmp(&b.creation_time))
        });
        if matched.is_empty() {
            return None;
        }
        let header = matched
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ");
        Some(header)
    }

    /// Whether a cookie stored for `cookie_host` is same-site with a
    /// request to `request_host` (exact-host comparison until the
    /// public-suffix list is adopted — Phase 2 subset).
    #[must_use]
    pub fn is_same_site(cookie_host: &str, request_host: &str) -> bool {
        cookie_host.eq_ignore_ascii_case(request_host)
    }
}

/// The request path for cookie matching (RFC 6265 §5.1.4).
fn request_path(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() || !path.starts_with('/') {
        return "/".to_string();
    }
    path.to_string()
}

/// RFC 6265 §5.1.3 domain-match against the cookie's storage domain.
fn domain_match(host: &str, cookie: &Cookie) -> bool {
    if cookie.host_only {
        return host == cookie.domain;
    }
    host == cookie.domain
        || (host.len() > cookie.domain.len() && host.ends_with(&format!(".{}", cookie.domain)))
}

/// RFC 6265 §5.1.4 path-match.
fn path_match(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if request_path.starts_with(cookie_path) {
        if cookie_path.ends_with('/') {
            return true;
        }
        if request_path.as_bytes().get(cookie_path.len()) == Some(&b'/') {
            return true;
        }
    }
    cookie_path == "/" && request_path.starts_with('/')
}

/// The default path (RFC 6265 §5.1.4): up to the last `/`, exclusive.
fn default_path(request_path: &str) -> String {
    if request_path.is_empty() || !request_path.starts_with('/') {
        return "/".to_string();
    }
    match request_path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(index) => request_path[..index].to_string(),
        None => "/".to_string(),
    }
}

/// Evicts one cookie: expired first (earliest expiry), else oldest
/// creation time (6265bis eviction order).
fn evict_one(cookies: &mut HashMap<(String, String, String), Cookie>) {
    let key = cookies
        .iter()
        .min_by_key(|(_, cookie)| (cookie.expiry_time.unwrap_or(u64::MAX), cookie.creation_time))
        .map(|(key, _)| key.clone());
    if let Some(key) = key {
        cookies.remove(&key);
    }
}

/// Parses one Set-Cookie value against the request `url` (RFC 6265 §5.2
/// + 6265bis SameSite). Returns `None` for unusable cookies.
fn parse_set_cookie(url: &Url, value: &str, now: u64) -> Option<Cookie> {
    let host = url.host_str()?.to_ascii_lowercase();
    let mut parts = value.split(';');
    let name_value = parts.next()?.trim();
    // No '=': the pair is unusable (§5.2).
    let (name, cookie_value) = name_value.split_once('=')?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let cookie_value = cookie_value.trim().to_string();

    let mut domain = host.clone();
    let mut host_only = true;
    let mut path = default_path(url.path());
    let mut expires: Option<u64> = None;
    let mut max_age: Option<i64> = None;
    let mut secure = false;
    let mut http_only = false;
    let mut same_site = SameSite::Lax;

    for attribute in parts {
        let attribute = attribute.trim();
        let (attr_name, attr_value) = match attribute.split_once('=') {
            Some(pair) => (pair.0.trim(), pair.1.trim()),
            None => (attribute, ""),
        };
        match attr_name.to_ascii_lowercase().as_str() {
            "domain" => {
                let stripped = attr_value.trim_start_matches('.').to_ascii_lowercase();
                if !stripped.is_empty() {
                    // Public-suffix rejection is Phase 3 (no PSL yet);
                    // store the domain cookie as requested.
                    domain = stripped;
                    host_only = false;
                }
            }
            "path" => {
                if attr_value.starts_with('/') {
                    path = attr_value.to_string();
                }
            }
            "expires" => {
                expires = parse_http_date(attr_value);
            }
            "max-age" => {
                // §5.2.2: non-digit-leading or invalid forms are ignored;
                // negative or zero expires immediately.
                let trimmed = attr_value.trim_start_matches(['+', '-']);
                if trimmed.chars().all(|c| c.is_ascii_digit()) && !trimmed.is_empty() {
                    let parsed: i64 = attr_value.parse().unwrap_or(i64::MAX);
                    max_age = Some(parsed);
                }
            }
            "secure" => secure = true,
            "httponly" => http_only = true,
            "samesite" => {
                same_site = match attr_value.to_ascii_lowercase().as_str() {
                    "strict" => SameSite::Strict,
                    "none" => SameSite::None,
                    _ => SameSite::Lax,
                };
            }
            _ => {}
        }
    }

    let expiry_time = if let Some(max_age) = max_age {
        if max_age <= 0 {
            Some(now) // already expired
        } else {
            Some(now.saturating_add(max_age as u64))
        }
    } else {
        expires
    };

    // SameSite=None requires Secure (modern default rules).
    if same_site == SameSite::None && !secure {
        same_site = SameSite::Lax;
    }

    Some(Cookie {
        name,
        value: cookie_value,
        domain,
        host_only,
        path,
        secure,
        http_only,
        same_site,
        creation_time: now,
        expiry_time,
    })
}

/// Parses an HTTP IMF-fixdate (RFC 9110 §5.6.7.1):
/// `Sun, 06 Nov 1994 08:49:37 GMT`. The lenient RFC 850 / asctime forms
/// are deferred (cookies with those formats become session cookies) —
/// recorded in the compatibility matrix.
fn parse_http_date(value: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let value = value.trim();
    let rest = value.split_once(", ")?.1.trim();
    let mut parts = rest.split_whitespace();
    let day: u32 = parts.next()?.trim_end_matches('-').parse().ok()?;
    let month = parts.next()?;
    let month_index = MONTHS
        .iter()
        .position(|m| month.to_ascii_lowercase().starts_with(m))? as u32;
    let year: i64 = parts.next()?.parse().ok()?;
    let time = parts.next()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next().unwrap_or("0").parse().ok()?;
    // Days since epoch via the civil-calendar formula (Howard Hinnant's
    // days_from_civil), then to unix seconds.
    let y = if year < 100 { year + 2000 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month_index + 10) % 12;
    let doy: i64 = i64::from((153 * mp + 2) / 5) + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) as u64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn url(input: &str) -> Url {
        Url::parse(input).unwrap()
    }

    #[test]
    fn basic_set_and_echo() {
        let jar = CookieJar::new();
        let page = url("https://example.org/dir/page.html");
        jar.set_cookies(&page, &["a=1; Path=/dir"], 1_000);
        // Distinct creation times make the tie-break deterministic (the
        // two cookies share a path, so RFC ordering falls back to age).
        jar.set_cookies(&page, &["b=2"], 1_001);
        assert_eq!(jar.cookie_header(&page, 1_000).unwrap(), "a=1; b=2");
        assert_eq!(jar.len(), 2);
    }

    #[test]
    fn path_scoping_and_default_path() {
        let jar = CookieJar::new();
        let page = url("https://example.org/dir/sub/page.html");
        jar.set_cookies(&page, &["deep=1; Path=/dir/sub", "shallow=2"], 1);
        let deeper = url("https://example.org/dir/sub/other.html");
        let header = jar.cookie_header(&deeper, 1).unwrap();
        assert!(header.contains("deep=1"), "path match: {header}");
        assert!(
            header.contains("shallow=2"),
            "default path /dir/sub matches subrequest"
        );

        let other = url("https://example.org/elsewhere.html");
        assert!(
            jar.cookie_header(&other, 1).is_none(),
            "no stored cookie matches a different path"
        );
    }

    #[test]
    fn domain_cookies_apply_to_subdomains_host_cookies_do_not() {
        let jar = CookieJar::new();
        let apex = url("https://example.org/");
        jar.set_cookies(&apex, &["dom=1; Domain=example.org", "host=2"], 1);
        let sub = url("https://api.example.org/");
        let header = jar.cookie_header(&sub, 1).unwrap();
        assert!(header.contains("dom=1"), "domain cookie covers subdomain");
        assert!(!header.contains("host=2"), "host-only cookie does not");

        // Domain cookie does not apply to a different site.
        let stranger = url("https://stranger.example/");
        assert!(jar.cookie_header(&stranger, 1).is_none());
    }

    #[test]
    fn secure_cookies_require_https() {
        let jar = CookieJar::new();
        let https = url("https://example.org/");
        jar.set_cookies(&https, &["s=1; Secure", "p=2"], 1);
        assert!(jar.cookie_header(&https, 1).unwrap().contains("s=1"));
        let http = url("http://example.org/");
        let header = jar.cookie_header(&http, 1).unwrap();
        assert!(!header.contains("s=1"), "secure cookie withheld over http");
        assert!(header.contains("p=2"));
    }

    #[test]
    fn max_age_and_expires_expiry_with_injected_clock() {
        let jar = CookieJar::new();
        let page = url("https://example.org/");
        jar.set_cookies(&page, &["ttl=1; Max-Age=10", "imm=2; Max-Age=0"], 1_000);
        assert_eq!(jar.cookie_header(&page, 1_005).unwrap(), "ttl=1");
        assert!(jar.cookie_header(&page, 1_011).is_none(), "expired at 1010");

        let jar2 = CookieJar::new();
        // Injected "now" sits between the two absolute expiries.
        let now = 800_000_000;
        jar2.set_cookies(
            &page,
            &[
                "d=1; Expires=Sun, 06 Nov 1994 08:49:37 GMT",
                "s=2; Expires=Mon, 06 Nov 1995 08:49:37 GMT",
            ],
            now,
        );
        let header = jar2.cookie_header(&page, now).unwrap();
        assert!(!header.contains("d=1"), "past expiry dropped");
        assert!(header.contains("s=2"), "future expiry kept");
    }

    #[test]
    fn samesite_attribute_is_stored_and_none_requires_secure() {
        let jar = CookieJar::new();
        let page = url("https://example.org/");
        jar.set_cookies(
            &page,
            &["x=1; SameSite=Strict", "n=2; SameSite=None", "l=3"],
            1,
        );
        let locked = jar.inner.lock().unwrap();
        let strict = locked
            .cookies
            .get(&("example.org".to_string(), "/".to_string(), "x".to_string()))
            .unwrap();
        assert_eq!(strict.same_site, SameSite::Strict);
        let none = locked
            .cookies
            .get(&("example.org".to_string(), "/".to_string(), "n".to_string()))
            .unwrap();
        // SameSite=None without Secure downgrades to Lax.
        assert_eq!(none.same_site, SameSite::Lax);
        drop(locked);

        let jar2 = CookieJar::new();
        jar2.set_cookies(&page, &["n=2; SameSite=None; Secure"], 1);
        let locked = jar2.inner.lock().unwrap();
        let none = locked
            .cookies
            .get(&("example.org".to_string(), "/".to_string(), "n".to_string()))
            .unwrap();
        assert_eq!(none.same_site, SameSite::None);
    }

    #[test]
    fn malformed_cookies_are_dropped_never_panic() {
        let jar = CookieJar::new();
        let page = url("https://example.org/");
        jar.set_cookies(
            &page,
            &[
                "novalue",  // no '='
                "=novalue", // empty name
                "",         // empty
                "; Path=/", // empty pair
                "ok=1",     // valid, for contrast
            ],
            1,
        );
        assert_eq!(jar.len(), 1);
    }

    #[test]
    fn overwrite_is_by_name_domain_path() {
        let jar = CookieJar::new();
        let page = url("https://example.org/");
        jar.set_cookies(&page, &["k=first"], 1);
        jar.set_cookies(&page, &["k=second"], 2);
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.cookie_header(&page, 2).unwrap(), "k=second");
    }

    #[test]
    fn cookie_ordering_longest_path_first_then_oldest() {
        let jar = CookieJar::new();
        let page = url("https://example.org/a/b/c");
        jar.set_cookies(&page, &["root=1"], 1);
        jar.set_cookies(&page, &["deep=2; Path=/a/b"], 2);
        // Longest path first (RFC 6265 §5.4): /a/b/c sorts before /a/b.
        let header = jar.cookie_header(&page, 3).unwrap();
        assert_eq!(header, "root=1; deep=2");
    }

    #[test]
    fn path_match_edges() {
        assert!(path_match("/a/b", "/a/b"));
        assert!(path_match("/a/b/c", "/a/b"));
        assert!(path_match("/a/b/c", "/a/b/"));
        assert!(!path_match("/a/bc", "/a/b"));
        assert!(path_match("/anything", "/"));
    }

    #[test]
    fn http_date_parsing() {
        let seconds = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert_eq!(seconds, 784_111_777);
        assert!(parse_http_date("not a date").is_none());
    }
}

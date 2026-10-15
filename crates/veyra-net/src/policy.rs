//! Request policy: the checks that run before and between network hops.
//!
//! The transport never makes security decisions (ADR-0008 decision 2):
//! scheme allow-listing, bad-port blocking, and redirect eligibility live
//! here, as pure functions with unit tests. The Phase 2 subset of the
//! fetch policy (documented in the compatibility matrix): navigation
//! GETs over http/https, redirect handling per the HTTP status semantics,
//! and the standard's blocked-port list. CORS, CSP/fetch-metadata, and
//! mixed-content checks arrive with their consumers in later phases.

use veyra_url::Url;

/// Schemes veyra-net will fetch. `file:` is handled by the shell's local
/// path (not a network concern); everything else is rejected here.
pub const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// The fetch standard's bad-port list: requests to these ports are
/// blocked regardless of scheme.
pub const BAD_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 25, 37, 42, 43, 53, 77, 79, 87, 95, 101, 102, 103,
    104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 139, 143, 179, 389, 427, 465, 512, 513, 514,
    515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993, 995, 1719,
    1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669, 6697,
];

/// Whether `url` passes the fetch policy for a network request.
///
/// Returns the reason string on failure for diagnostics; the caller turns
/// it into the typed error (the strings are static, not attacker data).
pub fn check_request_url(url: &Url) -> Result<(), &'static str> {
    if !ALLOWED_SCHEMES.contains(&url.scheme()) {
        return Err("scheme not allowed for network fetch");
    }
    if let Some(port) = url.port_or_known_default()
        && BAD_PORTS.contains(&port)
    {
        return Err("port is on the fetch bad-port list");
    }
    Ok(())
}

/// Whether a redirect may be followed to `next` from `current`.
pub fn check_redirect(current: &Url, next: &Url) -> Result<(), &'static str> {
    check_request_url(next)?;
    let _ = current;
    Ok(())
}

/// Whether a redirect response rewrites the method to GET.
///
/// 303 always; 301/302 rewrite in practice for anything that is not GET
/// (Phase 2 only sends GET, so this is documentation of intent for the
/// form-submission phase); 307/308 preserve.
pub fn redirect_rewrites_to_get(status: u16) -> bool {
    (301..=303).contains(&status)
}

/// Whether a status is a redirect.
pub fn is_redirect(status: u16) -> bool {
    (301..=303).contains(&status) || (307..=308).contains(&status)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn http_and_https_pass_on_normal_ports() {
        let http = Url::parse("http://example.org/x").unwrap();
        assert!(check_request_url(&http).is_ok());
        let https = Url::parse("https://example.org:8443/x").unwrap();
        assert!(check_request_url(&https).is_ok());
    }

    #[test]
    fn non_network_schemes_are_rejected() {
        for input in [
            "file:///etc/passwd",
            "data:text/plain,hi",
            "ftp://h/x",
            "javascript:alert(1)",
        ] {
            let url = Url::parse(input).unwrap();
            assert!(check_request_url(&url).is_err(), "{input} must be rejected");
        }
    }

    #[test]
    fn bad_ports_are_blocked_including_defaults() {
        // Explicit bad port.
        let url = Url::parse("http://example.org:5060/").unwrap();
        assert!(check_request_url(&url).is_err());
        // Bad default port via scheme? None of the allowed schemes have a
        // bad default; block an explicit port 6667 (IRC) and 22 (ssh).
        for port in [22u16, 25u16, 6000u16, 6667u16] {
            let url = Url::parse(&format!("http://example.org:{port}/")).unwrap();
            assert!(
                check_request_url(&url).is_err(),
                "port {port} must be blocked"
            );
        }
        // Normal ports pass.
        let url = Url::parse("http://example.org:8080/").unwrap();
        assert!(check_request_url(&url).is_ok());
    }

    #[test]
    fn redirects_follow_the_scheme_and_port_policy() {
        let current = Url::parse("https://a.example/").unwrap();
        let good = Url::parse("https://b.example/y").unwrap();
        assert!(check_redirect(&current, &good).is_ok());
        let evil = Url::parse("http://b.example:22/y").unwrap();
        assert!(check_redirect(&current, &evil).is_err());
    }

    #[test]
    fn redirect_semantics_match_the_http_statuses() {
        assert!(is_redirect(301) && is_redirect(302) && is_redirect(303));
        assert!(is_redirect(307) && is_redirect(308));
        assert!(!is_redirect(200) && !is_redirect(404));
        assert!(redirect_rewrites_to_get(303));
        assert!(redirect_rewrites_to_get(301));
        assert!(!redirect_rewrites_to_get(307));
        assert!(!redirect_rewrites_to_get(308));
    }
}

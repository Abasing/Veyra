# ADR 0008: Networking stack — policy layer over hyper/rustls/hickory

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

The browser's network obligations are mostly *semantics*: the fetch
algorithm, CORS, CSP/mixed-content/fetch-metadata enforcement, cookie
rules (RFC 6265 + SameSite + CHIPS [N5][N10]), HTTP caching, referrer
policies, MIME sniffing [ST4][ST32][ST33][ST5]. Protocol transport
(HTTP/1.1, H2, TLS, DNS) is commoditized in Rust with excellent,
license-clean candidates [E1, audit §6]: hyper 1.x, h2, rustls,
rustls-platform-verifier, hickory. HTTP/3 (`h3` 0.0.8, stale >1y [E1])
and PAC (no mature pure-Rust evaluator) are the recorded weak spots.
Chromium's design lesson (network in a sandboxed service; renderers never
touch sockets [N1][N3]) shapes the trust boundary regardless of stack.

## Decision

1. **Adopt, don't rebuild, transport:** hyper (HTTP/1.1 + H2), rustls
   (TLS 1.2/1.3) + rustls-platform-verifier (OS trust stores), hickory
   (DNS/DoH), tokio runtime — behind `veyra-net`.
2. **`veyra-net` owns the policy layer** as a spec-shaped fetch state
   machine: redirects, credentials, referrer, CORS, CSP/mixed-content/
   metadata/PNA checks, cookie jar, HTTP cache semantics, MIME checking.
   The transport never makes security decisions (invariant I5's home).
3. **H2 from Phase 2; H3/QUIC (quinn+h3) deferred to Phase 7–8** pending
   `h3` maturity [E1]; HTTP/1.1+H2+TLS covers the real-site baseline.
4. **WebSocket Phase 8** (tungstenite-class selection at that phase);
   WebTransport later with H3.
5. **Network service placement:** in-process Phase 2–5; candidate for
   utility-process extraction with the Phase 6 split (mirrors Chromium's
   network-service architecture [N3]).

## Alternatives considered

- **reqwest** — rejected as a browser layer: it is a client convenience
  wrapper; the fetch policy needs hyper-level control (connection pools,
  streams, interception).
- **Custom HTTP stack** — rejected: hyper/h2/rustls are the ecosystem's
  hardest, best-audited components; rebuilding them adds years and risk
  for zero independence gain (independence is about the engine, not TCP).
- **BoringSSL/native TLS via bindings** — rejected: rustls is
  memory-safe and license-clean; platform trust integration handles the
  OS-store requirement.

## Consequences

- Cookie/cache/CORS correctness is Veyra-owned test surface (WPT fetch/
  cookies/cors suites are the bar) — the stack choice does not reduce
  that work, only the transport risk.
- rustls's FIPS/cert-policy limitations (if they matter later) are
  recorded as open items, not discovered live.
- H3 deferral is a real, user-visible modernity gap in Phases 2–7 —
  honest in the matrix.

## Revisit conditions

`h3` 1.0 + quinn stability → Phase 7–8 adoption; a PAC requirement
arriving before Phase 9 → custom evaluator decision; rustls platform
verifier gaps on a target OS → ADR-0007 exception analysis.

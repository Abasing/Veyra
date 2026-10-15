# ADR 0013: Phase 2 document-pipeline dependencies (url, hyper/rustls/hickory stack, html5ever, webpki-roots)

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Phase 2 (roadmap) delivers the first real document pipeline: `veyra-url`
(the single parsing authority, dependency-graph §1 invariant 5),
`veyra-dom` (arena tree), `veyra-html` (HTML parsing per ADR-0003's
reserved decision point), `veyra-net` (the ADR-0008 policy layer over
the adopted transport), and the navigation/session layer in
`veyra-shell`. ADR-0008 already adopts hyper/rustls/hickory/tokio as the
networking stack and ADR-0003 names html5ever as the HTML parsing
candidate; this ADR records the concrete versions, the two decisions the
earlier ADRs left open (TLS trust roots; conformance corpora as vendored
test data), and the license-table additions the supply-chain gate needs.
Every version below was re-verified against crates.io on 2026-10-15
(license, MSRV, activity); MSRV of the set is ≤ 1.88 against our stable
1.99 toolchain.

## Decision

1. **URL parsing: `url` 2.5.8** (MIT OR Apache-2.0) behind `veyra-url`,
   which is the only crate allowed to parse URLs (dependency-graph §5).
   IDNA/UTS-46 arrives transitively through `url`'s host parser (`idna`
   1.x, `icu_*`, `utf8-ranges` — Unicode-3.0/MIT/Apache-2.0). Conformance
   is pinned by the vendored WPT `urltestdata.json` corpus (896 assertion
   entries: 827 pass, 69 documented deltas in five categories — see
   `crates/veyra-url/tests/data/deltas.md`); the delta table fails
   loudly when a dependency bump resolves an entry, so it shrinks rather
   than rots.
2. **Transport (ADR-0008 concrete): `hyper` 1.8 (HTTP/1.1 client),
   `hyper-util` 0.1.21 legacy client + `HttpConnector`, `rustls` 0.23
   (ring provider), `hickory-resolver` 0.26.3 (system `/etc/resolv.conf`
   configuration) wrapped as hyper's `Service<Name>`, `tokio` 1.48 as
   the runtime — all consumed only inside `veyra-net`; transport types
   never appear in public APIs. H2 remains deferred (ADR-0008 schedules
   it "from Phase 2" — the Phase 2 *document* load path needs HTTP/1.1;
   H2 enablement is one feature flip in `veyra-net` when a test demands
   it, recorded in the compatibility matrix as not-yet-claimed).
3. **TLS trust roots: `webpki-roots` 1.0 (bundled Mozilla root store,
   CDLA-Permissive-2.0), not `rustls-platform-verifier`, for Phase 2.**
   Rationale: hermetic CI (no OS trust-store variance between runner
   images), zero native dependencies beyond ring, and a deterministic
   root set for the conformance record. This is a recorded deviation
   from ADR-0008's "rustls-platform-verifier (OS trust stores)" — the
   platform verifier is adopted when Veyra runs on end-user machines
   (Phase 6 process-model work); until then the bundled store is the
   honest, testable choice. Revisit condition: first release consumed
   outside CI.
4. **HTML parsing: `html5ever` 0.40.1 (+ `markup5ever` 0.40, `tendril`
   0.5 — MIT OR Apache-2.0) feeding `veyra-dom` through a `TreeSink`
   that emits only Veyra handles (dependency-graph §5). ADR-0003's
   "adopt crate vs reimplement against html5lib-tests" decision point
   resolves to **adopt**: the vendored html5lib tree-construction subset
   (8 files, 324 cases, MIT, upstream commit `9329e64694e7`) passes
   320/324 (98.8%) with four documented deltas (deferred
   `<selectedcontent>` cloning — `crates/veyra-html/tests/data/deltas.md`).
   Reimplementing the tree builder to close four cases is not a rational
   trade at this phase.
5. **Vendored conformance corpora as test data:** WPT `urltestdata.json`
   (W3C 3-clause BSD) and html5lib tree-construction `.dat` files (MIT)
   live under the owning crates' `tests/data/` with provenance, license,
   and revision READMEs. Vendoring (rather than fetching at test time)
   keeps CI hermetic and the conformance claim reproducible at a pinned
   revision; refreshes are deliberate commits that must shrink or
   explain the delta tables.
6. **`deny.toml` license allow-list addition, reviewed:** `CDLA-Permissive-2.0`
   (webpki-roots' Mozilla CA bundle data). ring 0.17.14 — the rustls
   provider adopted by ADR-0008's stack — publishes the license
   expression `Apache-2.0 AND ISC` (the historical OpenSSL advertising
   clause over vendored BoringSSL-derived code no longer appears in the
   crates.io metadata), so no OpenSSL allowance is required; both
   licenses are already on the allow-list. Everything else the five
   crates pull in already sits in the allow-list.
7. **`veyra-dom` stays dependency-free** except `veyra-url` (origins
   live on nodes — dependency-graph invariant 2). The parser bridge adds
   the small API surface the sink needs (exact-name attribute lookup,
   template-contents attach/query, `Text.data` setter, `NodeId::null`
   sentinel) — all within the arena design the Phase 5 GC work builds
   on.

## Alternatives considered

- **rustls-platform-verifier now** — rejected for Phase 2 (see
  decision 3): OS-trust variance makes the conformance record
  nondeterministic and CI image-dependent; revisit at Phase 6.
- **GaiResolver (getaddrinfo) instead of hickory** — rejected: ADR-0008
  adopts hickory as the DNS path, and the custom `Service<Name>` wrapper
  is ~60 lines; getaddrinfo's blocking threadpool also sits outside the
  runtime model we control.
- **reqwest** — already rejected by ADR-0008 (client convenience wrapper;
  the fetch policy needs connector-level control).
- **markup5ever_rcdom as the DOM** — rejected: the engine's DOM is the
  arena in `veyra-dom` (origin-tagged documents, mutation algorithms,
  event skeleton); rcdom is a reference sink, not an engine DOM.
- **Fetching corpora at test time** — rejected: non-hermetic CI and
  unpinned conformance claims.

## Consequences

- `veyra-net` exposes a synchronous `blocking` facade over a
  per-call current-thread runtime; the engine core stays synchronous
  (engineering standards §5). Async remains the service-internal shape.
- Cookie SameSite site-comparison is exact-host until the public-suffix
  list lands (recorded in the matrix); the jar's API shape anticipates
  the PSL swap.
- The two delta files (`veyra-url`, `veyra-html`) are living documents:
  dependency bumps that resolve entries must remove them; bumps that
  regress entries fail CI.
- `deny.toml` gains `CDLA-Permissive-2.0` with this ADR as the citation;
  the dependency-audit document gains matching rows.
- H2, platform trust roots, PSL, MIME sniffing, and the full charset
  stack are explicitly *not* claimed by Phase 2 — each has a matrix row.

## Revisit conditions

- webpki-roots → platform verifier: first out-of-CI usage (Phase 6).
- H2: any real-world document load observed requiring it, or Phase 3
  subresource loading.
- Delta tables: any `url`/`idna`/`html5ever` version bump (mandatory
  re-run).
- html5ever adopt-vs-reimplement: if the delta count exceeds ~10% or a
  correctness bug in tree construction cannot be fixed upstream in a
  release cycle.

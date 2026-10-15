# Research: Networking and Storage

status: Investigated (first pass) | last-updated: 2026-10-15 | maintainer: foundation work

## Evidence basis

Chromium: network-stack design doc [N1], network service README [N3],
disk-cache README [N2], cookie/CHIPS [N4 dropped; N5 verified], network
sandboxing within the security docs [S2]. Standards/RFCs: HTTP semantics
RFC 9110 [N6], HTTP/3 RFC 9114 [N7], QUIC RFC 9000 [N8], TLS 1.3 RFC 8446
[N9], cookies RFC 6265 [N10] (6265bis draft tracked [U]), WHATWG Fetch [ST4],
URL [ST3], MIME sniffing [ST5], Storage [ST42], IndexedDB [ST26], File
System Access [ST27], Permissions Policy [ST34], fetch metadata [ST20],
WebSocket [ST44]. Protocol/stack behavior detail is `[K]` cross-checked
against the listed design docs; internal Chromium cache/cookie implementation
details are `[K]/[U]` pending a source-level pass.

## 1. Network stack shape (observation)

Chromium's network stack [N1][N3][K]:

- Lives in a **sandboxed network service process**; the browser and renderers
  talk to it over IPC. Renderers never touch sockets.
- **Socket pools** per proxy/scheme/privacy-mode with connection
  multiplexing; request scheduling with priorities; sticky assignments for
  H2/H3 streams.
- **HTTP/1.1, HTTP/2, HTTP/3 (QUIC)** with ALPN; QUIC connection migration
  and 0-RTT handling; TLS via BoringSSL with certificate verification,
  certificate transparency, HSTS, and platform trust-store integration.
- **Disk cache** [N2]: keyed cache with distinct backends (simple/blockfile
  historically) implementing HTTP caching semantics (freshness, validators,
  vary).
- **Cookie jar** with SameSite, partitioning (CHIPS [N5]), and per-renderer
  isolation of cookie access at IPC time.
- **Proxy support**: PAC scripts, auth (including SOCKS and auth challenge
  delegation), proxy-bypass rules.

**Assessment for Veyra:** the browser-critical parts are *semantics*
(caching rules, cookie rules, security headers) rather than raw protocol
performance; Rust gives mature protocol cores (hyper/h2/quinn/rustls [E])
but **not** browser semantics — caching, cookie policy, CORS, and fetch
algorithm state are Veyra-owned regardless of stack. Mapping in
[ADR-0008](../decisions/0008-networking-stack.md).

## 2. URL and origin security (observation, standards-anchored)

URL parsing/canonicalization [ST3] is a security component, not a utility:
special-scheme handling, host parsing (incl. IDNA/UTS#46 [ST19]), path
normalization, and the origin computation rules feed every security decision.
Chromium centralizes URL handling and treats it as hardened surface [K].
**Veyra rule (assessment):** a single `veyra-url` authority type wraps the
`url` crate [E, MIT OR Apache-2.0] — no component parses URLs itself;
`url` crate has deep WPT url-spec test coverage which Veyra inherits by
adoption and must keep passing.

## 3. The fetch pipeline (observation, spec-anchored)

The Fetch standard [ST4] is the browser's central request/response algorithm:
scheme fetch dispatch, redirect limits and cross-origin redirect rules,
credential modes, referrer policy application, CORS checks (simple vs
preflight), MIME checks before response handoff, and the integration points
with service workers, cache mode, and range requests. Around it sit:
content-security-policy enforcement [ST32], mixed-content upgrades/blocks
[ST33], fetch metadata injection [ST20], private-network access checks
[ST46], and permissions policy gates [ST34].

**Assessment:** implement fetch as a *spec-shaped state machine* in Veyra
(tested against WPT fetch suites), with protocol transport as a pluggable
backend. The transport (hyper et al.) must never make security decisions —
policy lives in Veyra's pipeline. This ordering (policy layer ≠ transport)
is the core of ADR-0008.

## 4. DNS, connection management, and proxies (observation)

Async DNS resolution with DoH upgrade paths, connection pooling/keepalive,
happy-eyeballs racing, and proxy resolution with PAC [N1][K]. **Veyra
candidates [E]:** `hickory-resolver` (MIT OR Apache-2.0, active) for DNS;
hyper's connector stack for pooling; PAC support is a later-phase feature
(no mature pure-Rust PAC evaluator — gap recorded).

## 5. TLS and trust (observation)

rustls [E, Apache-2.0 OR ISC OR MIT] is the Rust TLS stack (memory-safe,
no C TLS); platform root-of-trust verification via
`rustls-platform-verifier` [E] which delegates to OS trust stores (this is
an OS-API boundary integration, not a C-subsystem dependency — policy
analysis in [ADR-0007](../decisions/0007-native-dependency-policy.md)).
Certificate transparency, HSTS, and pinning semantics are Veyra-owned
policy layers on top [K]. CT is a Phase 9+ feature (risk: log-list
distribution is operationally nontrivial — recorded honestly).

## 6. Browser storage systems (observation)

Chromium's storage landscape [K, corroborated by ST42/ST26/ST27 and
storage-partitioning documentation]:

| Store | Contents | Notes |
|---|---|---|
| Cookies | RFC 6265 + SameSite + CHIPS partitioned | Per-profile DB; IPC-brokered access |
| LocalStorage | Key/value per origin | Synchronous API over async backing store |
| SessionStorage | Per-tab key/value | Dies with tab; per browsing session |
| IndexedDB | Structured, transactional, async | Large-object aware; quota-managed |
| Cache API | Request/response pairs (service workers) | Persistent; partitioned |
| File System Access | User-granted file handles | Explicit permission gating [ST27] |
| HTTP cache | Response bodies/headers | See §1 |

Cross-cutting: **storage partitioning** (per top-level-site keys for
third-party contexts — the CHIPS/storage-partitioning direction [N5][K]),
**quotas and eviction** (per-origin/partition bookkeeping with LRU-ish
pressure eviction [U for exact heuristics]), and **clear-browsing-data**
semantics with retention windows.

**Veyra mapping (assessment):** a `veyra-storage` crate providing
origin/partition-keyed namespacing over a single embedded engine — `redb`
[E, MIT OR Apache-2.0, active] chosen over C SQLite (dependency policy) and
over `sled` (alpha/1.0 not released [E]) — detailed in
[ADR-0010](../decisions/0010-storage-engine.md). IndexedDB semantics
(transactions, indexes, structured-clone values) are a large Veyra-owned
layer; scheduled Phase 6+ with WPT indexeddb suites as the bar.

## 7. Profiles, user data, and credentials (observation)

User-data-dir layout per profile: preferences, history, cookies, cache,
extensions, sessions [K]. Password management: encrypted credential store
keyed via OS keystore integration; autofill heuristics on top [K]. History/
bookmarks: local databases; sync in Chrome is a proprietary service (see
[browser-product-features.md](browser-product-features.md) for the
proprietary-service inventory). Private browsing (incognito-class): in-memory
profile variants with no persistence [K].

**Veyra mapping:** profile = directory + `redb` stores under
`veyra-profile`; credentials via OS `keyring` [E, MIT OR Apache-2.0]
integration with an explicit threat-model note (OS keystore trust); private
mode = in-memory store instantiation with process-lifetime encryption
(implementation detail deferred to Phase 6 with the storage design).

## 8. Crash safety, migration, corruption recovery (observation)

Chromium stores use journaling/WAL-style transactional backends and versioned
schemas with migration; corruption handling ranges from rebuild to
quarantine-and-reset [K, corroborated by N2 cache design]. **Assessment for
Veyra:** `redb` provides ACID transactions; Veyra adds (a) schema-version
table + forward migrations, (b) per-store corruption counters that trip
quarantine (rename aside, recreate) rather than hard failure, (c) crash-safe
writes for non-DB state (sessions, downloads) via temp-file+rename. These
are Phase 6 acceptance criteria.

## 9. What Veyra needs (mapping)

| Capability | Veyra subsystem | Phase |
|---|---|---|
| URL/origin authority | `veyra-url` | 2 |
| HTTP/1.1 + TLS fetch | hyper + rustls via `veyra-net` | 2 |
| Cookie jar (RFC 6265 + SameSite) | `veyra-net::cookies` | 2 |
| HTTP cache (freshness/validators) | `veyra-net::cache` | 5 |
| H2/H3 | hyper-h2 / quinn+h3 backends | 7–8 (h3 crate maturity noted [E: 0.0.8]) |
| Fetch algorithm + CORS + CSP enforcement | `veyra-net::fetch` policy layer | 4–6 |
| LocalStorage/SessionStorage | `veyra-storage` | 6 |
| IndexedDB | `veyra-storage::idb` | 6+ |
| Profiles/credentials | `veyra-profile` + `keyring` | 6, 10 |
| WebSocket | tokio-tungstenite-class or hyper upgrade | 8 |
| Proxy/PAC | connector layer; PAC evaluator gap | 9+ |

## 10. Open questions

1. **[U] h3 crate readiness** — 0.0.8 versioning signals pre-stability; H3
   timing is perf-motivated, not correctness-motivated; revisit at Phase 7.
2. **[U] Exact Chromium eviction heuristics** (quota accounting, LRU details)
   — behavioral docs only; a source-level pass before quota enforcement lands.
3. **[U] 6265bis final shape** — cookie draft in flux; Veyra implements 6265
   baseline + SameSite + CHIPS and tracks bis.
4. **Pure-Rust PAC evaluator** — none identified [E]; implement or defer
   (decision at Phase 9; gap recorded in the dependency audit).
5. **Video-capable cache ranges** (range-request backed media caching) —
   deferred with media work; no early design commitment.

## Confidence and gaps

- High: fetch/URL/cookie/cache *spec-level* obligations; network-service
  process shape [N1][N3]; storage partitioning direction [N5].
- Medium: Chromium cache backends' internals, quota heuristics [K/U].
- Absent: throughput/latency numbers (none measured); claims about
  HTTP/3-in-Chrome current deployment percentages (not verified this pass).

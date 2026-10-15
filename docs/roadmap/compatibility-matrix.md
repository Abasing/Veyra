# Compatibility Matrix

status: Seeded (Phase 0) | last-updated: 2026-10-15

The single source of truth for what Veyra supports. Seeded 2026-10-15 at
project foundation: **every row is honestly "Not investigated" or
"Investigated" at the platform level** — no engine code exists yet.
States are defined in the [compatibility program](compatibility-program.md);
transitions cite evidence (suite runs, commits, documents).

Per-area pass-rate tables and suite revisions are appended here as phases
complete; the current tables only reference research/design artifacts.

## Platform areas

| Area | State | Evidence / notes | Known failures | Remaining work |
|---|---|---|---|---|
| URL parsing (WHATWG URL) | Investigated | [ST3]; `url` crate adoption planned Phase 2 (ADR-0008) | none yet (no code) | Phase 2 suite runs |
| HTML parsing & tree construction | Investigated | [ST1]; html5lib-tests corpus selected | none yet | Phase 2 |
| DOM core (trees, events, mutation) | Investigated | [ST2]; architecture in system-overview | none yet | Phase 2, 5 |
| CSS syntax & selectors | Investigated | [ST13]; cssparser/selectors per ADR-0003 | none yet | Phase 3 |
| Cascade & computed values | Investigated | [ST12] | none yet | Phase 3 |
| Block/inline layout | Investigated | custom (dossier gap analysis) | none yet | Phase 3 |
| Flexbox | Investigated | taffy per ADR-0003 | none yet | Phase 3/7 |
| Grid | Investigated | [ST22] | none yet | Phase 7 |
| Text shaping & fonts | Investigated | parley/rustybuzz stack (ADR-0003) | none yet | Phase 3/7 |
| Writing modes & bidi | Investigated | [ST37][ST18] | none yet | Phase 7 |
| Painting & compositing | Designed | rendering-pipeline.md | none yet | Phase 3 (CPU), 7 (GPU) |
| Scrolling (compositor) | Designed | rendering-pipeline.md §4 | none yet | Phase 7 |
| JavaScript (ECMAScript) | Investigated | ADR-0002 (Boa); Test262 runner Phase 4 | none yet | Phase 4 |
| Event loop & timers | Designed | execution-and-web-apis.md | none yet | Phase 4 |
| Web IDL bindings | Designed | execution-and-web-apis.md §4 | none yet | Phase 4–5 |
| Fetch & CORS | Investigated | [ST4]; ADR-0008 policy layer | none yet | Phase 5 |
| HTTP/1.1, TLS, DNS transport | Investigated | hyper/rustls/hickory (ADR-0008) | none yet | Phase 2 |
| HTTP/2 | Investigated | h2 (ADR-0008) | none yet | Phase 2 |
| HTTP/3 (QUIC) | Not investigated | h3 maturity risk [E1] | — | Phase 7–8 gate |
| Cookies (RFC 6265, SameSite, CHIPS) | Investigated | [N10][N5] | none yet | Phase 2 (jar), 5 (policy) |
| Caching (HTTP semantics) | Investigated | [N2] research; Veyra cache Phase 5 | none yet | Phase 5 |
| CSP / mixed content / secure contexts | Investigated | [ST32][ST33][ST35] | none yet | Phase 5–6 |
| Permissions & Permissions Policy | Investigated | [ST34] | none yet | Phase 6 (backend), 10 (UI) |
| Storage: local/session | Investigated | [ST42]; ADR-0010 | none yet | Phase 5 (API), 6 (persist) |
| IndexedDB | Investigated | [ST26] | none yet | Phase 6+ |
| Cache API / service workers | Not investigated | — | — | Phase 8 |
| WebAssembly | Investigated | [J5][J6]; ADR-0009 | none yet | Phase 8 |
| Workers | Not investigated | — | — | Phase 8 |
| WebSockets | Investigated | [ST44] | none yet | Phase 8 |
| Streams | Not investigated | — | — | Phase 8 |
| SVG | Investigated | resvg/usvg (audit §5) | none yet | Phase 7 |
| Canvas 2D | Investigated | rendering-pipeline §5 | none yet | Phase 7 |
| WebGL | Not investigated | — | — | Phase 9 |
| WebGPU | Not investigated | — | — | Phase 9 |
| Media (audio/video) | Investigated (gap analysis) | audio Symphonia candidate; video = recorded gap (audit §8) | — | Phase 9 gate |
| WebRTC | Not investigated | hardening survey pre-Phase 9 | — | Phase 9 gate |
| EME/DRM | Not investigated | documented limitation (dossier §5) | — | out of scope |
| Accessibility (ARIA/platform trees) | Investigated | accesskit (audit §7); [ST17] | none yet | Phase 10 |
| Forms & validation | Not investigated | — | — | Phase 5 |
| Navigation & session history | Designed | chromium-architecture §5; Phase 2 machine | none yet | Phase 2, 5 |
| Extensions (MV3-shaped subset) | Designed | ADR-0006; per-API matrix Phase 9 | — | Phase 9+ |
| DevTools protocol subset | Designed | ADR-0006; CDP research [P2] | — | Phase 9–10 |
| Encoding (WHATWG) | Investigated | [ST6] | none yet | Phase 2 |
| Internationalization (Intl) | Not investigated | ICU4X candidate [E1] | — | Phase 7+ |
| Printing / PDF | Not investigated | pure-Rust PDF gap (audit §8) | — | Phase 10 |
| Private browsing | Designed | ADR-0010 in-memory stores | none yet | Phase 6 |
| Profiles & credentials | Designed | ADR-0010 + keyring | none yet | Phase 6, 10 |
| Sandbox (Linux) | Designed | ADR-0011 | none yet | Phase 6 |
| Sandbox (Windows/macOS) | Not investigated | recorded gap (ADR-0011) | — | pre-Phase 9 spike |
| Updates & release engineering | Not investigated | — | — | Phase 10–11 |

## Pass-rate tables (appended as phases complete)

*None yet — no conformance runs have been executed. The first entries
appear at Phase 2 (html5lib-tests, WPT url subset), each with suite
revision, runner, date, and directory set per the compatibility
program's honest-claim rules.*

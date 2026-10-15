# Compatibility Matrix

status: Phase 1 recorded (2026-10-15) | last-updated: 2026-10-15

The single source of truth for what Veyra supports. Seeded 2026-10-15 at
project foundation; Phase 1 (the shell and deterministic test
infrastructure) is recorded below with its evidence. **No web-platform
capability is implemented yet** — the rows below that are marked complete
are *infrastructure* (a window, a test rig), not browser features. States
are defined in the [compatibility program](compatibility-program.md);
transitions cite evidence (suite runs, commits, documents).

Per-area pass-rate tables and suite revisions are appended here as phases
complete; the current tables only reference research/design artifacts.

## State vocabulary

The matrix uses exactly the seven states defined in the
[compatibility program](compatibility-program.md), in their canonical
spelling, because automated checks enforce the vocabulary:

1. **Not investigated**
2. **Investigated**
3. **Designed**
4. **In development**
5. **Partially implemented**
6. **Tested with known failures**
7. **Meets defined acceptance criteria**

A row moves forward only with dated evidence; movement backward (a
regression) is recorded, never silently rewritten. Suite-derived pass
rates appear only in "Tested with known failures" and "Meets defined
acceptance criteria" rows, cited with the suite revision.

## Platform areas

| Area | State | Evidence / notes | Known failures | Remaining work |
|---|---|---|---|---|
| Native shell (window, event loop, input plumbing) | Meets defined acceptance criteria | Phase 1 (2026-10-15): winit 0.30.13 + softbuffer 0.4.8 per ADR-0012; window smoke tests pass under Xvfb locally and in the CI `headless-shell` job; clean-shutdown and no-display paths tested | Linux only so far (X11/Xvfb tested; Wayland compiled but untested; Windows/macOS untested) | Phase 4 (event-loop integration), Phase 10 (product UI) |
| Deterministic rendering infrastructure (CPU raster, screenshot harness, golden images) | Meets defined acceptance criteria | Phase 1 (2026-10-15): `veyra-gfx` integer-only CPU raster (ADR-0005); `veyra-shell screenshot` CLI emits PNG + canonical SHA-256 (hash of the `VEYRA-CANON1` buffer form: `9e473fb9615949d9b57516dc0413bc2a2f7954f9004063db403757c0f08696d9` at 800x600, scene v1); golden fixtures reviewed and pixel-compared in CI | Linux (CI) evidence only; hash definition documented in `veyra-shell::hashing` | Phase 3 replaces the fixture scene with the display-list pipeline (rendering-pipeline.md) |
| Injectable frame clock | Meets defined acceptance criteria | Phase 1 (2026-10-15): `FrameClock` trait with `SystemFrameClock` (production) and `FixedFrameClock` (tests); deterministic progression, zero-step rejection, and overflow tests; no wall-clock sleeps in the suite | none | Phase 4 adopts it in `veyra-executor` (rAF, timers) |
| Configuration & diagnostics | Meets defined acceptance criteria | Phase 1 (2026-10-15): strict TOML schema (`deny_unknown_fields`, reserved `[policy]`, `schema_version` gate), tracing on stderr with documented `RUST_LOG` precedence; valid/invalid/defaults tests | none | Phase 6+ extends the reserved policy section |
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

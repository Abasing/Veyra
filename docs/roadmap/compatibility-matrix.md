# Compatibility Matrix

status: Phase 2 recorded (2026-10-15) | last-updated: 2026-10-15

The single source of truth for what Veyra supports. Seeded 2026-10-15 at
project foundation; Phases 1-2 (the shell and test infrastructure, then
the first document pipeline) are recorded below with their evidence. **No web-platform
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
| URL parsing (WHATWG URL) | Tested with known failures | Phase 2 (2026-10-15): `url` 2.5.8 behind `veyra-url` (ADR-0013); vendored WPT `urltestdata.json` (fetched 2026-10-15, W3C BSD-3): 827/896 assertion entries pass (92.3%), 69 documented deltas in five categories (`crates/veyra-url/tests/data/deltas.md`: extra-slash authority, IDNA lenient punycode, file-host/path edges, blob-origin derivation, U+0020/backslash encoding); origins (tuple w/ effective port, opaque with minted identity), relative resolution, input-length limits | the 69 documented corpus deltas | Delta retirement per category (upstream-tracked); setter APIs at the script phase |
| HTML parsing & tree construction | Tested with known failures | Phase 2 (2026-10-15): `html5ever` 0.40.1 behind `veyra-html` (ADR-0003 decision point resolved: adopt — ADR-0013); vendored html5lib tree-construction subset (8 files, 324 cases, MIT, upstream commit `9329e64694e7`): **320/324 pass (98.8%)**, 4 documented deltas (deferred `<selectedcontent>` option cloning — `crates/veyra-html/tests/data/deltas.md`); error-list assertions not part of the claim (tree shape is the conformance surface); limits tested (document 64 MiB, attr/text 8 MiB, depth 512 counting the whole tree, node count via veyra-dom) with typed rejections; deterministic fuzz smoke 20,000 inputs, zero crashes | `<selectedcontent>` cloning (4 cases, needs subtree cloning in veyra-dom) | Retire the delta; fragment-parsing conformance breadth; full charset stack ([ST6]) |
| DOM core (trees, events, mutation) | Partially implemented | Phase 2 (2026-10-15): `veyra-dom` arena with generational ids (stale-id detection), origin-tagged documents (parse-derived origins only), node kinds incl. template contents, mutation algorithms (append/insert/remove/detach, cycle+depth rejection), iterative traversal, events skeleton (capture/target/bubble, cancelation); 32 seeded property-test sequences audit structural invariants; no WebIDL surface, live NodeLists, or GC yet | none within the claimed subset | Phase 5 (bindings, GC integration, live collections) |
| CSS syntax & selectors | Partially implemented | Phase 3 batch 1 (2026-10-16): CSS Syntax 3 parsing on cssparser 0.38 behind `veyra-style` (ADR-0003/0014); vendored css-parsing-tests subset (7 files, MIT, upstream `css-parsing-tests` as of 2026-10-15) pins tokenizer/rule error recovery; Selectors 4 parsing via `selectors` 0.41 behind Veyra types (complex selectors, attr selectors, `:is()`/`:where()`/`:not()`/`:has()`, `:nth-child(an+b of S)`, standard pseudo set; unknown pseudos are typed drops). Batch 2 (2026-10-16): matching against `veyra-dom` trees — combinators, logical/tree-structural pseudo-classes, `:lang()` (RFC 4647 basic on inherited `lang`), static-attribute forms pseudo-classes (each deviation documented in `veyra-style::matching`), quirks-mode class/id case behavior | pseudo-element matching (rules parse; generated-content consumption is its own batch); prefixed namespace selectors (needs `@namespace` application); `:dir()` uses the inherited attribute, not content-derived directionality | WPT selectors-4 subset run with recorded pass rate (Phase 3 AC 1); `:dir()`/`:lang()` full algorithms; pseudo-element style collection |
| Cascade & computed values | Partially implemented | Phase 3 batch 2 (2026-10-16): Cascade 5 subset in `veyra-style::cascade` — origin/importance order (UA < user < author, importance inverted), layer order tree (first-occurrence registration, unlayered-vs-layered with important inversion, anonymous layers), specificity (most specific *matching* selector), source order, inline `style` attribute slot, `revert` (origin rollback; `revert-layer` approximated as `revert`), CSS-wide keywords with per-property `unset` semantics; working property set (color/background-color incl. `background` color layer, display, font-size/weight/style/family, line-height, text-align, text-decoration-line incl. shorthand, margin/padding/width/height, border shorthand family) with em/%/rem resolution (root resolves rem against the initial size), `currentcolor`, custom properties captured + inherited (`var()` = invalid at computed-value time until substitution); built-in UA sheet (rendering-relevant defaults, browser-metric values); `@media` degenerate subset (bare `screen`/`all` types, also gating the `media` attribute), `@supports` unevaluated; Phase 3 batch 3 (2026-10-16): the shell navigation pipeline loads styles — `browser/style_loader.rs` collects `<style>`/`<link rel=stylesheet>` sheets in document order, follows `@import` (depth cap 8, visited-set cycle guard, 64-sheet / 4 MiB-total caps; failures are counted diagnostics, never load errors; UTF-8 discipline), and every loaded document carries computed styles (`veyra-shell open --styles` prints the deterministic per-element report; file:// + HTTP integration tests) | `calc()`/ex/ch, `font` shorthand, per-side `border-*` shorthands, lab/lch/oklab/oklch color spaces, media features, `@supports` conditions, `var()` substitution, incremental restyle (R1) | grow the property set per WPT area; R1 mutation-sequence tests; per-`@import` media/layer scoping (imports currently load unscoped; media-gated ones apply the degenerate evaluator) |
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
| Fetch & CORS | In development | Phase 2 (2026-10-15): navigation-GET subset of the fetch policy in `veyra-net` — scheme allow-list, bad-port list, redirect loop (limit 20, 301/302/303→GET), cookie application, response/header limits with typed rejections; CORS/fetch-metadata/CSP checks arrive with their consumers ([ST4] subset recorded) | CORS unimplemented (no cross-origin reads exist yet) | Phase 5 (full fetch algorithm), 3 (subresources) |
| HTTP/1.1, TLS, DNS transport | Partially implemented | Phase 2 (2026-10-15): hyper 1.8 HTTP/1.1 + rustls 0.23 (ring) + hickory-resolver 0.26.3 (system config) behind `veyra-net` (ADR-0008/0013); webpki-roots bundle (deviation from platform-verifier recorded in ADR-0013, revisit Phase 6); verified against a hand-rolled local HTTP/1.1 fixture (10 integration tests) and a real-HTTPS load (`https://example.org/` → status 200 → DOM dump, re-run 2026-10-16; evidence home: the Phase 2 status note in implementation-plan.md); H2 not claimed | none within the claimed subset | H2 enablement; connection pooling surfaces; platform trust roots (Phase 6) |
| HTTP/2 | Investigated | h2 (ADR-0008); not enabled in Phase 2 — the document load path needs HTTP/1.1; one feature flip in `veyra-net` when demanded (ADR-0013) | none yet | Enable + fixture tests on demand |
| HTTP/3 (QUIC) | Not investigated | h3 maturity risk [E1] | — | Phase 7–8 gate |
| Cookies (RFC 6265, SameSite, CHIPS) | Partially implemented | Phase 2 (2026-10-15): RFC 6265 jar subset in `veyra-net` — Domain/Path/Expires (IMF-fixdate)/Max-Age/Secure/HttpOnly/SameSite (None requires Secure, modern default Lax), host-only vs domain cookies, RFC ordering, 6265bis eviction, injected-clock expiry tests; exercised over the fixture server (redirect + terminal Set-Cookie paths); SameSite site-comparison is exact-host until the public-suffix list lands (recorded subset); CHIPS not started | exact-host same-site (no PSL yet); non-IMF date formats become session cookies | PSL adoption; Phase 3 subresource SameSite enforcement; Phase 5 policy integration |
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

# Source Register

Every source consulted for the research dossier, with the access date and the
verification status of the URL as checked from the research environment on
**2026-10-15**. Verification codes:

- **verified** — HTTP 200 retrieved.
- **bot-blocked** — HTTP 403 to automated fetch; the URL is the canonical
  registry home and content knowledge is corroborated via listed mirrors or
  sibling drafts. Not silently substituted.
- **transient-5xx** — googlesource rate-limiting during the check window;
  sibling documents from the same tree verified. Re-checked by the weekly
  scheduled workflow.
- **dns-failed** — research-environment DNS failure for the host; canonical
  URL recorded; re-checked weekly.

Tier: **P1** = required for the dossier's load-bearing claims (weekly
scheduled check enforces liveness); **P2** = supporting/contextual.

## Chromium: architecture, processes, IPC, security

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| D1 | https://chromium.googlesource.com/chromium/src/ | verified | P1 | Source-tree organization; directory READMEs | 2026-10-15 |
| D2 | https://www.chromium.org/developers/design-documents/ | verified | P1 | Design-doc index; subsystem doc discovery | 2026-10-15 |
| D3 | https://www.chromium.org/developers/design-documents/multi-process-architecture/ | verified | P1 | Process roles, trust split, IPC rationale | 2026-10-15 |
| D4 | https://www.chromium.org/developers/design-documents/site-isolation/ | verified | P1 | Site isolation goals/behavior | 2026-10-15 |
| D6 | https://chromium.googlesource.com/chromium/src/+/main/docs/mojo_and_services.md | verified | P1 | Mojo IPC model, service architecture | 2026-10-15 |
| D7 | https://chromium.googlesource.com/chromium/src/+/main/docs/process_model_and_site_isolation.md | transient-5xx | P1 | Process allocation/reuse policy | 2026-10-15 |
| D8 | https://developer.chrome.com/docs/chromium/renderingng | verified | P1 | RenderingNG pipeline, property trees | 2026-10-15 |
| D13 | https://www.chromium.org/developers/design-documents/gpu-accelerated-compositing-in-chrome/ | verified | P2 | Compositing history/design | 2026-10-15 |
| S1 | https://www.chromium.org/Home/chromium-security/ | verified | P1 | Security program overview | 2026-10-15 |
| S2 | https://chromium.googlesource.com/chromium/src/+/main/docs/design/sandbox_faq.md | verified | P1 | Sandbox goals/mechanisms per OS | 2026-10-15 |
| S3 | https://chromium.googlesource.com/chromium/src/+/main/docs/design/sandbox.md | verified | P1 | Sandbox design detail | 2026-10-15 |
| S4 | https://www.chromium.org/Home/chromium-security/memory-safety/ | verified | P1 | Chromium memory-safety program | 2026-10-15 |
| M3 | https://www.chromium.org/Home/chromium-security/memory-safety | verified | P2 | Same program (no trailing slash variant) | 2026-10-15 |
| N1 | https://www.chromium.org/developers/design-documents/network-stack/ | verified | P1 | Network stack design | 2026-10-15 |
| N2 | https://chromium.googlesource.com/chromium/src/+/main/net/disk_cache/README.md | verified | P1 | Disk cache organization | 2026-10-15 |
| N3 | https://chromium.googlesource.com/chromium/src/+/main/services/network/README.md | verified | P1 | Network service structure | 2026-10-15 |
| B1 | https://www.chromium.org/blink/ | verified | P1 | Blink project docs entry | 2026-10-15 |
| B2 | https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/dom/README.md | verified | P1 | DOM layer organization | 2026-10-15 |
| B3 | https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/layout/README.md | verified | P1 | Layout layer organization | 2026-10-15 |
| B4 | https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/css/README.md | verified | P1 | CSS/style layer organization | 2026-10-15 |

## V8, JavaScript, WebAssembly

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| J1 | https://v8.dev/docs | verified | P1 | V8 architecture docs index | 2026-10-15 |
| J2 | https://v8.dev/blog/sparkplug | verified | P1 | Baseline compiler tier | 2026-10-15 |
| J3 | https://v8.dev/blog/maglev | verified | P1 | Mid-tier optimizing compiler | 2026-10-15 |
| J4 | https://v8.dev/blog/orinoco | verified | P1 | GC architecture | 2026-10-15 |
| J5 | https://webassembly.github.io/spec/core/ | verified | P1 | WASM core semantics | 2026-10-15 |
| J6 | https://webassembly.github.io/spec/js-api/ | verified | P1 | WASM JS API obligations | 2026-10-15 |
| J8 | https://github.com/tc39/test262/blob/main/INTERPRETING.md | verified | P1 | Test262 result interpretation | 2026-10-15 |
| J7 | https://docs.rs/boa_engine/latest/boa_engine/ | verified | P2 | Boa embedding API surface | 2026-10-15 |
| J7b | https://github.com/boa-dev/boa | verified | P1 | Boa repo; README claims (>90% ECMAScript; "experimental") fetched 2026-10-15 | 2026-10-15 |
| J9 | https://boajs.dev/conformance | verified | P2 | Boa conformance tracking page (client-rendered; numbers pending extraction) | 2026-10-15 |
| J10 | https://trynova.dev | verified | P2 | Nova project status/Test262 tracking | 2026-10-15 |

## Standards

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| ST1 | https://html.spec.whatwg.org/multipage/ | verified | P1 | HTML parsing/event loop/browsing contexts | 2026-10-15 |
| ST2 | https://dom.spec.whatwg.org/ | verified | P1 | DOM trees/mutation/events/ranges | 2026-10-15 |
| ST3 | https://url.spec.whatwg.org/ | verified | P1 | URL parsing/origin | 2026-10-15 |
| ST4 | https://fetch.spec.whatwg.org/ | verified | P1 | Fetch/CORS pipeline | 2026-10-15 |
| ST5 | https://mimesniff.spec.whatwg.org/ | verified | P1 | MIME sniffing rules | 2026-10-15 |
| ST6 | https://encoding.spec.whatwg.org/ | verified | P1 | Encoding/decoding | 2026-10-15 |
| ST7 | https://webidl.spec.whatwg.org/ | verified | P1 | Binding-layer semantics | 2026-10-15 |
| ST8 | https://streams.spec.whatwg.org/ | verified | P1 | Streams (fetch/SW prerequisites) | 2026-10-15 |
| ST9 | https://workers.spec.whatwg.org/ | dns-failed | P2 | Workers (canonical home; corroborated by ST1) | 2026-10-15 |
| ST10 | https://service-workers.spec.whatwg.org/ | dns-failed | P2 | Service workers (canonical home) | 2026-10-15 |
| ST11 | https://www.w3.org/Style/CSS/ | bot-blocked | P2 | CSS specs directory (canonical; drafts verified below) | 2026-10-15 |
| ST12 | https://drafts.csswg.org/css-cascade-5/ | verified | P1 | Cascade/layers/importance | 2026-10-15 |
| ST13 | https://drafts.csswg.org/selectors-4/ | verified | P1 | Selector semantics/specificity | 2026-10-15 |
| ST14 | https://drafts.csswg.org/css-display-4/ | verified | P1 | Box tree/formatting contexts | 2026-10-15 |
| ST21 | https://drafts.csswg.org/css-flexbox-1/ | verified | P1 | Flexbox layout | 2026-10-15 |
| ST22 | https://drafts.csswg.org/css-grid-2/ | verified | P1 | Grid layout/subgrid | 2026-10-15 |
| ST25 | https://drafts.csswg.org/css-color-4/ | verified | P1 | Color parsing/spaces | 2026-10-15 |
| ST37 | https://drafts.csswg.org/css-writing-modes-4/ | verified | P1 | Writing modes/logical axes | 2026-10-15 |
| ST49 | https://drafts.csswg.org/css-text-4/ | verified | P1 | Text processing/line breaking | 2026-10-15 |
| ST47 | https://drafts.csswg.org/css-animations-2/ | verified | P2 | Animations | 2026-10-15 |
| ST48 | https://drafts.csswg.org/css-transforms-2/ | verified | P2 | Transforms/3D | 2026-10-15 |
| ST23 | https://drafts.fxtf.org/compositing-2/ | verified | P1 | Blend/compositing paint model | 2026-10-15 |
| ST24 | https://drafts.css-houdini.org/css-paint-api/ | verified | P2 | Paint API (extensibility reference) | 2026-10-15 |
| ST50 | https://www.w3.org/TR/CSS22/visuren.html | bot-blocked | P2 | Visual formatting model (canonical; corroborated by ST14) | 2026-10-15 |
| ST15 | https://tc39.es/ecma262/ | verified | P1 | ECMAScript semantics | 2026-10-15 |
| ST36 | https://tc39.es/proposal-temporal/ | verified | P2 | Temporal (future i18n/date work) | 2026-10-15 |
| ST16 | https://w3c.github.io/webdriver/ | verified | P2 | WebDriver (DevTools/testing transport) | 2026-10-15 |
| ST17 | https://www.w3.org/TR/wai-aria-1.2/ | bot-blocked | P2 | ARIA role/state semantics | 2026-10-15 |
| ST18 | https://unicode.org/reports/tr9/ | verified | P1 | UAX#9 bidi algorithm | 2026-10-15 |
| ST19 | https://unicode.org/reports/tr46/ | verified | P1 | UTS#46 IDNA processing | 2026-10-15 |
| ST20 | https://w3c.github.io/webappsec-fetch-metadata/ | verified | P1 | Sec-Fetch-* headers | 2026-10-15 |
| ST26 | https://w3c.github.io/IndexedDB/ | verified | P1 | IndexedDB semantics | 2026-10-15 |
| ST27 | https://wicg.github.io/file-system-access/ | verified | P2 | File System Access | 2026-10-15 |
| ST28 | https://webgpu.github.io/webgpu-samples/ | verified | P2 | WebGPU API surface examples | 2026-10-15 |
| ST29 | https://registry.khronos.org/webgl/specs/latest/2.0/ | bot-blocked | P2 | WebGL 2 spec (canonical) | 2026-10-15 |
| ST30 | https://www.w3.org/TR/webrtc/ | bot-blocked | P2 | WebRTC spec (canonical) | 2026-10-15 |
| ST31 | https://www.w3.org/TR/encrypted-media/ | bot-blocked | P2 | EME spec (documented limitation) | 2026-10-15 |
| ST32 | https://w3c.github.io/webappsec-csp/ | verified | P1 | CSP | 2026-10-15 |
| ST33 | https://w3c.github.io/webappsec-mixed-content/ | verified | P1 | Mixed content | 2026-10-15 |
| ST34 | https://w3c.github.io/webappsec-permissions-policy/ | verified | P1 | Permissions Policy | 2026-10-15 |
| ST35 | https://w3c.github.io/webappsec-secure-contexts/ | verified | P1 | Secure contexts | 2026-10-15 |
| ST40 | https://compat.spec.whatwg.org/ | verified | P2 | Compatibility quirks | 2026-10-15 |
| ST41 | https://fullscreen.spec.whatwg.org/ | verified | P2 | Fullscreen | 2026-10-15 |
| ST42 | https://storage.spec.whatwg.org/ | verified | P1 | Storage/quotas/partitioning model | 2026-10-15 |
| ST43 | https://notifications.spec.whatwg.org/ | verified | P2 | Notifications | 2026-10-15 |
| ST44 | https://websockets.spec.whatwg.org/ | verified | P1 | WebSocket API | 2026-10-15 |
| ST45 | https://w3c.github.io/webappsec-subresource-integrity/ | verified | P2 | SRI | 2026-10-15 |
| ST46 | https://wicg.github.io/private-network-access/ | verified | P1 | PNA/LNA draft | 2026-10-15 |

## Protocols and platform

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| N6 | https://www.rfc-editor.org/rfc/rfc9110.html | verified | P1 | HTTP semantics | 2026-10-15 |
| N7 | https://www.rfc-editor.org/rfc/rfc9114.html | verified | P2 | HTTP/3 | 2026-10-15 |
| N8 | https://www.rfc-editor.org/rfc/rfc9000.html | verified | P2 | QUIC | 2026-10-15 |
| N9 | https://www.rfc-editor.org/rfc/rfc8446.html | verified | P2 | TLS 1.3 | 2026-10-15 |
| N10 | https://www.rfc-editor.org/rfc/rfc6265.html | verified | P1 | Cookies (6265bis tracked as draft [U]) | 2026-10-15 |
| N5 | https://github.com/privacycg/CHIPS | verified | P1 | Partitioned cookies | 2026-10-15 |
| K1 | https://docs.kernel.org/userspace-api/landlock.html | verified | P1 | Landlock LSM | 2026-10-15 |
| K2 | https://docs.kernel.org/userspace-api/seccomp_filter.html | verified | P1 | seccomp filters | 2026-10-15 |
| M1 | https://security.googleblog.com/2024/09/eliminating-memory-safety-vulnerabilities-android.html | verified | P1 | Memory-safety statistics (76%→24%) | 2026-10-15 |
| M2 | https://msrc.microsoft.com/blog/2019/07/why-we-need-to-use-safe-programming-languages-and-how-to-get-there/ | verified | P2 | ~70% memory-safety CVE statistic | 2026-10-15 |

## Product, extensions, testing

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| P1 | https://developer.chrome.com/docs/extensions/develop/ | verified | P1 | MV3 extension platform | 2026-10-15 |
| P4 | https://developer.chrome.com/docs/extensions/reference/declarativeNetRequest | verified | P1 | DNR rule model | 2026-10-15 |
| P5 | https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions | verified | P2 | Cross-browser extension interop | 2026-10-15 |
| P2 | https://chromedevtools.github.io/devtools-protocol/ | verified | P1 | CDP domains/shape | 2026-10-15 |
| T1 | https://web-platform-tests.org/ | verified | P1 | WPT organization/harness docs | 2026-10-15 |
| T2 | https://github.com/web-platform-tests/wpt/tree/master/tools/wptrunner | verified | P1 | wptrunner architecture | 2026-10-15 |
| T3 | https://github.com/tc39/test262 | verified | P1 | Test262 corpus | 2026-10-15 |
| T6 | https://wpt.fyi/results | verified | P2 | Cross-engine pass-rate dashboards | 2026-10-15 |
| T7 | https://wpt.live/ | verified | P2 | Live WPT server | 2026-10-15 |
| T4 | https://browserbench.org/Speedometer3.0/ | verified | P2 | Speedometer 3 (license review pending) | 2026-10-15 |
| T5 | https://github.com/WebKit/Speedometer | verified | P2 | Speedometer source (license NOASSERTION via API — manual review) | 2026-10-15 |
| V1 | https://github.com/servo/servo | verified | P1 | Servo architecture reference | 2026-10-15 |
| V2 | https://github.com/servo/stylo | verified | P1 | Stylo extraction | 2026-10-15 |
| V3 | https://github.com/servo/html5ever | verified | P1 | html5ever | 2026-10-15 |
| V4 | https://github.com/servo/rust-url | verified | P2 | rust-url | 2026-10-15 |
| V5 | https://wpt.servo.org/ | verified | P2 | Servo WPT dashboard (integration example) | 2026-10-15 |

## Ecosystem data sources (API pulls, 2026-10-15)

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| E1 | https://crates.io/api/v1/crates/{name} | template | P1 | All license/version/activity rows in the dependency audit | 2026-10-15 |
| E2 | https://api.github.com/repos/{owner}/{repo} | template | P1 | Repo licenses/activity (WPT, Test262, servo, html5lib-tests, quickjs-ng, Speedometer) | 2026-10-15 |
| E3 | https://raw.githubusercontent.com/boa-dev/boa/main/README.md | verified | P1 | Boa self-description quoted in javascript-and-wasm.md | 2026-10-15 |
| E4 | https://raw.githubusercontent.com/trynova/nova/main/README.md | verified | P2 | Nova status claims | 2026-10-15 |

## Retired during verification (probed, dead — removed from citations)

- D5 `chromium.googlesource.com/.../docs/site_isolation/oopif.md` (503
  during the initial pass, then confirmed 404; the doc path is unresolved.
  OOPIF behavior is cited via D4, which is verified. Re-search scheduled
  with the next googlesource source-level pass.)

- D9 `developer.chrome.com/blog/renderingng-architecture` (404 — superseded by D8)
- D10 `docs/how_blink_works.md` (404; "How Blink works" is a Google Doc — cited instead via B2/B3/B4 READMEs)
- P3, S5, S6 (404 probes; not load-bearing)
- D8b, D12 (404 probes; alternates live)

## Remaining research gaps (top of the next pass)

1. Source-level reading of Chromium navigation/content-layer paths (D7, D5
   re-verification included) — highest priority before Phase 6 process work.
2. wptrunner product-integration survey (hands-on clone + dry run).
3. Boa embedding spike (rooting/DOM-wrapper patterns) — Phase 4 entry.
4. stylo integration spike (build, API surface, MSRV/licensing hygiene) — Phase 3 entry.
5. Chromium cache/quota heuristics source pass — before Phase 6 storage.
6. Sandbox internals per-OS syscall sets — Phase 6 design input.

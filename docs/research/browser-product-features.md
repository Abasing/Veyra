# Research: Browser Product Features, Extensions, and Developer Tools

status: Investigated (first pass) | last-updated: 2026-10-15 | maintainer: foundation work

## Evidence basis

Chromium/Chrome product surface: extensions docs [P1 — verified at
developer.chrome.com/docs/extensions/develop/], declarativeNetRequest
reference [P4], DevTools protocol [P2 verified at chromedevtools.github.io],
WebExtensions comparison via MDN [P5]. Product-behavior detail (tab
lifecycle, omnibox providers, autofill) is `[K]` — documented behavior,
source-level pass pending. The **proprietary-service inventory** in §5 is an
assessment based on documented dependencies (safe browsing list distribution,
sync infrastructure, store signing) and is marked accordingly.

## 1. The complete browser experience (inventory)

A competitive browser is more than an engine. The user-facing capability
inventory, organized by dependency depth:

**Engine-adjacent (must exist before the product is credible):**

- Tabs/windows: creation, closing, tearing out, tab groups, tab lifecycle
  (discarding under memory pressure) [K].
- Address bar: URL entry with scheme inference, search-keyword handling,
  autocomplete over history/bookmarks [K].
- Navigation controls: back/forward with per-entry state (scroll position,
  form data), reload (normal/hard), stop [K].
- Find-in-page, page zoom (per-origin persisted), full-screen, print [K].
- History, bookmarks, downloads UI with safe-download warnings [K].
- Profiles: multi-profile, private windows, profile picker [K].
- Permissions UI: camera/mic/location/notifications prompts with
  origin-scoped remembered grants [K] (spec basis [ST34]).
- Context menus, page info (site permissions/certificate), view-source [K].
- Crash recovery: session restore after renderer or browser crash [K].
- Accessibility UX: full keyboard navigation, screen-reader support via the
  platform tree (engine work: [web-platform-and-rendering.md]).

**Product-layer (built on engine APIs):**

- Extensions: install/update/disable UI, permission prompts, extension
  badges/actions, content-script isolation indicators [P1][K].
- Password autofill, form autofill; settings UI; import/export of bookmarks/
  passwords [K].
- Site settings (cookie/JS/camera per-origin), clear-browsing-data flows [K].
- Reading list, tab search, PWA install prompts [K].

**Service-dependent (see §5 for what is proprietary):**

- Safe browsing lists, update channel, sync, store distribution, crash
  reporting backend, translate, spellcheck dictionaries [K].

**Assessment:** Veyra's product layer is planned for Phase 9–10 *after* the
engine serves real pages; the Phase 1 "browser shell" is a diagnostic
window, not a product. The essential-vs-optional split is formalized in the
roadmap's milestone definitions.

## 2. Extensions (observation)

Chromium's extension platform (Manifest V3 era) [P1][P4][K]:

- **Manifest** (v3): static declarations of permissions, host permissions,
  content scripts, background service worker, action, options.
- **Permission model**: install-time API permissions + runtime host
  permissions with user-grantable scope tightening; enterprise policies can
  force-install (product layer).
- **Content scripts**: run in an **isolated world** in the renderer — same
  DOM, separate JS globals; communicate via messaging, not shared objects.
- **Background**: service worker (event-driven, terminated when idle) —
  MV2 background pages are deprecated in Chromium.
- **Messaging**: runtime messaging, port-based connections, external
  messaging; storage APIs with sync (service-dependent) and local areas.
- **DeclarativeNetRequest**: rule-based network modification replacing the
  blocking webRequest API [P4] — a major semantic shift for ad-blockers;
  rule compilation and static rule sets.
- **Web-compatible APIs in extensions**: standard JS plus chrome.* namespace.

**Interoperability note [P5]:** Firefox/Safari implement the WebExtensions
model with documented deltas; there is no full cross-browser spec —
"Chrome Web Store compatible" is a distribution claim, not an API contract.

**Assessment / staged strategy (feeds ADR-0006 and the roadmap):**

1. **Do not claim Chrome Web Store compatibility — ever — until the
   required `chrome.*` API subset, isolated worlds, service-worker
   lifecycle, and DNR are implemented and tested.** Store distribution
   itself is Google's; Veyra would need its own distribution channel
   regardless.
2. Phase 9 implements a *subset*: manifest parsing, content scripts
   (isolated worlds), storage, tabs/messaging APIs, and a documented
   compatibility matrix per `chrome.*` namespace.
3. DNR-class network modification requires the fetch-pipeline interception
   points designed in Phase 5 — designed for, not implemented, there.
4. An honest alternative path exists: ship a WebExtension-*compatible*
   subset and treat unimplemented namespaces as explicit failures.

## 3. Developer tools (observation)

Chrome DevTools = a **frontend** (HTML/JS app) + **backend** (in-browser
inspection domains) speaking the **Chrome DevTools Protocol (CDP)** [P2]:
Runtime (evaluation, console), Debugger (breakpoints, stepping, source maps),
DOM (tree queries), CSS, Network (request/response capture), Performance
(tracing), Profiler, Accessibility, Emulation, and more [K].

**Assessment for Veyra:**

1. The *protocol* is the reusable artifact: Veyra implements a CDP-shaped
   subset (`veyra-devtools`) over its own internals — Runtime/Debugger
   require the JS-engine debugging hooks (Phase 9); DOM/Network inspection
   require instrumented veyra-dom/net (Phase 9–10).
2. The frontend is separately reusable in principle (Chromium's DevTools
   frontend source is publicly available; license compatibility must be
   evaluated before any reuse — recorded as an open item in the dependency
   audit, **not** assumed here). Alternatives: build a minimal inspector
   UI, or integrate with VS Code-class debugger UIs over a protocol.
3. Debugging hooks must be designed into the JS-engine facade (breakpoint
   callbacks, pause/resume) from Phase 4 — retrofitting debugging is a known
   pain point in embedded engines.

## 4. Update, crash reporting, policies, safe browsing (product services)

- **Updates** [K]: background updater with signed packages and staged
  rollout. Veyra: deferred to Phase 10+; design note — updates require a
  signing infrastructure decision (TUF-style or OS-store distribution).
- **Crash reporting** [K]: minidump collection with symbolication service.
  Veyra: local-only diagnostics first; privacy-preserving upload is an
  opt-in product decision.
- **Enterprise policies** [K]: registry/policy-file/managed-OS mechanisms.
  Veyra: policy file format reserved in the configuration design (Phase 1)
  so enterprise control is additive later.
- **Safe browsing** [K]: list-based phishing/malware lookup — Chrome uses
  Google's proprietary lists/service. **Veyra cannot and will not depend on
  proprietary services** (see §5); options: community-maintained lists,
  opt-in third-party APIs, or local heuristics — each with honest efficacy
  trade-offs documented at decision time (Phase 10).

## 5. Proprietary-service inventory (assessment)

| Capability | Chromium/Chrome dependence | Veyra stance |
|---|---|---|
| Sync (bookmarks/passwords/tabs) | Google account infrastructure | Independent design or opt-in third-party; not a Phase 1–10 commitment |
| Safe browsing lists | Google service | Community/open lists or none, with documented trade-offs |
| Chrome Web Store | Google signing/distribution | Own distribution; compatibility is API-level only |
| Widevine/EME | Licensed CDM modules | **Out of scope** for the foreseeable roadmap; honest limitation (streaming DRM content will not play) |
| Translate/spellcheck cloud | Google services | Local-first alternatives (OS spellcheck, local models) |
| Crash analytics | Google backend | Local diagnostics; opt-in self-hosted upload |
| Autofill data server assist | Google | Purely local implementation |

**Assessment:** none of the above blocks a *competitive browser*; each is a
product decision with a documented independent path or an explicit,
user-visible limitation. The DRM gap is the most material real-world
limitation and is stated plainly rather than hidden.

## 6. What Veyra needs (mapping)

| Capability | Phase | Notes |
|---|---|---|
| Diagnostic shell window (Phase 1 "browser") | 1 | Not product UI |
| Tab/window model + navigation UI | 9–10 | After engine maturity |
| History/bookmarks/downloads + persistence | 9–10 | Storage from Phase 6 |
| Permissions UI + site settings | 9–10 | Gated on permission backend |
| Extension subset (MV3-shaped) | 9+ | ADR-0006; explicit matrix per API |
| DevTools protocol subset | 9–10 | ADR-0006; needs JS debug hooks from Phase 4 |
| Settings, policies, profiles UI | 10 | Config schema reserved Phase 1 |
| Update/distribution channel | 11 | Signing decision required |
| Safe browsing (independent) | 11 | Open-list strategy decision |

## 7. Open questions

1. **[U] DevTools frontend reuse licensing** — evaluate license/terms before
   any frontend code is considered; otherwise plan a native inspector.
2. **[U] Extension isolated-world implementation depth** in a Rust DOM +
   Boa engine (two global scopes over one DOM) — design spike at Phase 9
   entry.
3. **[U] DNR rule-compilation semantics** sufficient for major blockers —
   requires studying the documented rule grammar [P4] against Veyra's
   interception points.
4. **[U] Private-window guarantee strength** — what "no persistence" means
   under memory-disclosure attacks; scope at Phase 6 with the threat model.

## Confidence and gaps

- High: extension platform structure (MV3) [P1][P4], CDP existence/shape
  [P2], the essential-product inventory [K].
- Medium: fine-grained product behaviors (omnibox ranking, autofill
  heuristics) — documented only in aggregate.
- Explicitly not claimed: any Chrome Web Store compatibility, DRM support,
  or parity with Google's service ecosystem.

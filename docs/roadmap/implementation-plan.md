# Implementation Plan

status: Proposed (Phase 0 baseline) | last-updated: 2026-10-15 | owner: project lead

This is an ordered implementation program. Phases are gated by acceptance
criteria with evidence — a demo is never an exit condition. The sequence
follows dependency analysis (see
[`../architecture/dependency-graph.md`](../architecture/dependency-graph.md));
security work starts early and never stops. Where numbers are required
(performance budgets, pass rates), they are **set from measured baselines
at the phase that owns them** — this plan deliberately does not invent
them.

Conventions: each phase lists Objectives, Prerequisites, Deliverables,
Tests, Acceptance criteria (AC), Risks, Exit conditions, and Dependencies.
Crate names refer to the boundaries in the architecture docs; a crate is
created only when its phase arrives.

---

## Phase 0 — Repository foundation, research, architecture, and threat model

**Objectives/scope.** Establish the safe development base: governance,
evidence-backed Chromium research, proposed architecture, phased roadmap,
validation tooling, and honest CI.

**Prerequisites.** None.

**Deliverables (this commit series).**
- Governance: README (explicit early-stage status), CONTRIBUTING, SECURITY,
  CODE_OF_CONDUCT, Apache-2.0 LICENSE.
- Research dossier: 9 documents + coverage matrix + source register
  (93/110 sources verified live on 2026-10-15; failures recorded).
- Architecture: 6 documents; 11 ADRs (language, JS engine, rendering,
  process model, graphics, frontend/extensions, native deps, networking,
  WASM, storage, sandboxing).
- Roadmap, compatibility program + matrix (seeded at "Not investigated"),
  engineering standards.
- Tooling: `veyra-doctor` (Rust, zero-dependency) validating repository
  invariants; Python link/source checkers with tests.
- CI: `ci.yml` (doctor, rust, python, cargo-deny) and `scheduled.yml`
  (source liveness, advisories, doctor); templates; dependabot.

**Tests.** `veyra-doctor` unit tests; Python tool tests; the CI set runs
green on `develop`.

**AC.**
1. All CI jobs green on the pushed `develop` head (link to run recorded
   in the completion report).
2. `veyra-doctor` passes locally and in CI: required docs present, no
   forbidden claims, ADR/roadmap structure valid, source register dated,
   actions SHA-pinned.
3. `cargo clippy -D warnings`, `cargo fmt --check`, `cargo test`,
   `cargo doc --no-deps`, `ruff`, `pytest`, `cargo-deny` all clean.
4. Repo description states early-stage research status; default branch is
   `develop`; branch protection verified via API (no force-push, no
   delete); protection state documented honestly.
5. Compatibility matrix exists with all platform areas at explicit states.

**Risks.** Documentation drift (mitigated by doctor checks + weekly
scheduled validation); research gaps (tracked in coverage matrix with
priorities).

**Exit conditions.** AC 1–5 hold. **Dependencies.** None.

---

## Phase 1 — Minimal executable browser shell, diagnostics, deterministic test infrastructure

**Objectives/scope.** A runnable `veyra-shell` binary (winit window,
clear-color surface, screenshot-to-file harness), configuration loading,
structured diagnostics (`tracing`), and the deterministic test substrate
used by every later phase. No web content yet — this is the rig, not the
product.

**Prerequisites.** Phase 0.

**Deliverables.** `veyra-shell` crate (window + input plumbing + config
schema with reserved policy fields); screenshot harness (render buffer →
PNG + hash); golden-image test runner; injectable frame clock; `veyra-gfx`
trait with CPU raster backend; benchmark scaffolding (criterion) with the
**first measured baselines** (shell startup, buffer present) recorded in
`docs/benchmarks/baseline-2026-phase1.md` (created in-phase).

**Tests.** Golden-image round-trip; config parsing; screenshot harness
unit tests; input event plumbing smoke tests.

**AC.**
1. `veyra-shell` opens a window and renders a deterministic pattern;
   CI (headless xvfb) produces identical hashes across runs.
2. Screenshot harness emits buffer+hash via CLI for automation.
3. Baselines exist with variance data (N-run median, reported spread);
   regression budgets set from them (documented numbers, not guesses).
4. CI runs the shell harness headlessly on every push.

**Risks.** winit 0.31 beta pinning [E1 — audit]; headless rendering
variance across CI runners (mitigate: software rendering, fixed viewport,
bundled fonts).

**Exit conditions.** AC 1–4. **Dependencies.** Phase 0.

---

## Phase 2 — Networking foundation, URL handling, HTML parsing, DOM construction, initial navigation

**Objectives/scope.** Load a document over HTTP(S) or file:// and produce
a DOM tree; navigation state machine; the security-critical foundations
(`veyra-url` authority, origin tagging, input limits).

**Prerequisites.** Phase 1 (harness for URL/parse dumps).

**Deliverables.** `veyra-url` (wrapper over `url` crate; single parsing
authority); `veyra-net` (HTTP/1.1 via hyper + rustls + hickory; cookie
jar per RFC 6265 + SameSite; response size/time limits); `veyra-html`
(html5ever-based or per ADR-0003 spike) feeding `veyra-dom` (arena tree,
mutation algorithms, events skeleton); navigation/session state machine
in the browser module (browser-owned transitions only); crash-safe
session file (temp+rename).

**Standards refs.** WHATWG HTML (parsing) [ST1], URL [ST3], Encoding
[ST6], MIME sniffing [ST5], Fetch (subset) [ST4], RFC 9110 [N6], RFC 6265
[N10].

**Tests.** html5lib-tests suite (MIT-attributed corpus [E2]) for tree
construction; WPT `url` subset; unit tests for cookie parsing/SameSite;
navigation state machine property tests; HTML parser fuzz target
(cargo-fuzz) with CI smoke budget; limits tests (oversized
documents/attributes rejected, not crashed).

**AC.**
1. html5lib-tests tree-construction results recorded in the
   compatibility matrix with pass counts (evidence, not claims).
2. WPT url subset passes via `url` crate adoption (documented deltas).
3. Fuzz target runs ≥ a fixed CI budget with zero crashes/hangs.
4. Document loads over HTTPS render a DOM dump; navigation back/forward
   within session history works in the shell.
5. Origin tag present on every document/frame/resource type (compile-
   time enforcement where possible).

**Risks.** html5ever integration depth; cookie edge semantics; CI
network-dependent tests (mitigate: local fixture server).

**Exit conditions.** AC 1–5. **Dependencies.** Phase 1; ADR-0003 spike
(parsing arm).

---

## Phase 3 — CSS parsing, cascade, basic layout, text rendering, painting, input handling

**Objectives/scope.** Pixels for static pages: tokenized CSS, cascade +
computed values, block/inline layout with intrinsic sizing, shaped text,
display lists, CPU rasterization, hit testing basics.

**Prerequisites.** Phase 2 (DOM); ADR-0003 CSS spike (cssparser/selectors
vs stylo) **completed with recorded findings** before this phase starts.

**Deliverables.** `veyra-style` (cascade origins/layers, inheritance,
computed values, selector matching, invalidation sets v1);
`veyra-layout` (BFC, IFC, intrinsic sizing modes); `veyra-text`
(parley/fontique/rustybuzz/unicode-bidi); `veyra-paint` (display lists,
paint order); `veyra-gfx` CPU raster path wired to shell; flexbox via
`taffy`; input hit testing (basic); logical-axis abstraction (R3).

**Standards refs.** CSS Cascade 5 [ST12], Selectors 4 [ST13], Display
[ST14], Flexbox [ST21], Color 4 [ST25], Writing Modes (axes only in this
phase) [ST37], CSS Text (line breaking basics) [ST49], UAX#9 [ST18].

**Tests.** WPT css-syntax/selectors subsets; golden-image suite for
layout fixtures; display-list snapshot tests (paint order); mutation-
sequence incremental tests (R1); CSS parser fuzz target; text shaping
fixtures (bundled fonts).

**Performance.** Layout/paint baselines on the fixture corpus; budgets
set from measured numbers (recorded in-phase benchmark note).

**AC.**
1. WPT selectors-4 subset pass-rate recorded in matrix; no regressions
   across two consecutive scheduled runs.
2. Golden images stable across CI runs; paint-order unit tests pass.
3. Incremental relayout tests prove R1 (mutation sequences, not just
   initial render).
4. Layout benchmark suite established with budgets derived from
   baselines (numbers in-repo, dated).
5. Flexbox basics pass a documented taffy-interop fixture set.

**Risks.** Inline-layout gap (dossier) — scope held to documented
subset; taffy intrinsic-sizing interop; MPL-2.0 license hygiene for
cssparser/selectors (deny.toml updated at adoption, ADR referenced).

**Exit conditions.** AC 1–5. **Dependencies.** Phase 2; ADR-0003 spike.

---

## Phase 4 — JavaScript execution, DOM bindings, event-loop integration, essential Web APIs

**Objectives/scope.** Script runs, events flow, the page becomes
programmable — with the engine swappable behind `veyra-script`.

**Prerequisites.** Phase 3 (DOM/layout/paint exist); **Boa embedding
spike** (rooting/GC interop — ADR-0002) completed first.

**Deliverables.** `veyra-executor` (spec event loop: task queues,
microtask checkpoints, timers w/ clamping, rAF via injected clock);
`veyra-script` facade + Boa implementation; `veyra-webid` hand-written
bindings for the curated core set (~30 interfaces, list in
[execution-and-web-apis.md](../architecture/execution-and-web-apis.md));
structured clone v1; debug-hook stubs.

**Standards refs.** ECMAScript [ST15], HTML event loop [ST1], Web IDL
[ST7], DOM events [ST2].

**Tests.** Microtask/timer ordering suites; Test262 runner + tracked
results (Veyra-measured — engine self-reports never load-bearing);
event-dispatch tests vs DOM spec; binding round-trip tests; GC-interop
stress tests (wrappers ↔ arena).

**AC.**
1. Test262 results (chosen subset: language essentials + harness-
   compatible features) recorded in the compatibility matrix with pass
   rates and the exact suite revision used.
2. WPT testharness infrastructure executes inside the engine (bootstrap
   for Phase 5's WPT program).
3. A scripted page mutates DOM and animation-frame callbacks drive
   re-render end-to-end in the shell.
4. GC stress suite: no leaks (wrapper counts stable) and no use-after-
   free across 1e6 allocation cycles.
5. Facade indirection cost measured (benchmark note) — baseline only.

**Risks.** Boa rooting/trace API fit (spike gate); bindings volume;
engine bug triage upstream (mitigate: upstream issue workflow).

**Exit conditions.** AC 1–5. **Dependencies.** Phase 3; ADR-0002 spike.

---

## Phase 5 — Core web-platform coverage, forms, navigation semantics, compatibility improvement

**Objectives/scope.** Broaden correctness: fetch API, forms/validation,
navigation/history semantics, storage-API surface reserved, WPT program
industrialized.

**Prerequisites.** Phase 4.

**Deliverables.** fetch API + CORS enforcement path in `veyra-net`;
forms (controls, constraint validation, submission); history API +
traversal semantics; sessionStorage/localStorage API surface (in-memory
backing; persistence lands Phase 6); binding generator decision point
(>50 interfaces?); wptrunner integration evaluation (evidence recorded
either way).

**Standards refs.** Fetch [ST4], HTML forms/history [ST1], Storage
[ST42], DOM ranges/selection v1 [ST2].

**Tests.** WPT fetch/cors/html forms/history subsets tracked in the
matrix; regression triage process live (reproduce → minimize → fix →
regression test, per compatibility program); content-sniffing [ST5]
tests.

**AC.**
1. WPT pass rates for fetch/forms/history recorded with suite revision;
   each phase's exit adds ≥ the previous phase's documented target for
   the *tracked* directories (targets set from Phase 4 baselines).
2. A static-content site corpus (documented list) renders with all
   failures filed as triaged issues.
3. Every fixed conformance failure has a regression test (spot-audit
   required at exit).
4. Real-world interactive pages remain out of scope honestly (matrix
   shows partial states).

**Risks.** WPT harness integration effort [U]; conformance volume vs
review capacity (mitigate: tracked-subset discipline).

**Exit conditions.** AC 1–4. **Dependencies.** Phase 4.

---

## Phase 6 — Process isolation, sandboxing, security enforcement, storage, profiles, crash recovery

**Objectives/scope.** The security phase: content out-of-process, Linux
sandbox, capability enforcement live, persistent storage, profiles, crash
recovery. (Design obligations for all of this were created in Phases 1–2;
this phase activates them.)

**Prerequisites.** Phase 5; capability seams audited (ADR-0004); syscall
budget experiments.

**Deliverables.** `veyra-ipc` (typed, length-bounded, versioned messages;
fuzz targets per message type); content-process split; `veyra-sandbox`
(namespaces + seccomp allow-list artifact + Landlock rules; negative
test suite); `veyra-storage` (redb-backed stores: cookies, local/session
storage persistence, IndexedDB v1, cache metadata; origin/partition
keying; quota accounting v1; corruption quarantine); `veyra-profile`
(profile dirs, private mode = in-memory stores); crash recovery
(renderer death → recoverable tab state); storage partitioning keys.

**Standards refs.** Storage [ST42], IndexedDB [ST26], cookies/SameSite
[N10][N5], sandbox kernel docs [K1][K2].

**Tests.** Sandbox negative tests (file/socket/syscall denial);
IPC fuzzer; crash-recovery fault injection; storage crash-safety tests
(kill -9 during writes); IndexedDB WPT subset; quota tests.

**Performance.** IPC round-trip and storage latency baselines; budgets
recorded (multi-process overhead measured against Phase 5 numbers —
documented delta, not hidden).

**AC.**
1. Sandbox negative test suite passes: content process cannot open
   files, create sockets, or invoke non-allow-listed syscalls.
2. Renderer kill mid-navigation recovers to a usable tab with session
   integrity (fault-injection suite).
3. Storage survives crash-injection with no torn state (kill -9 matrix
   across store operations).
4. IPC fuzz targets pass CI budget with zero crashes.
5. Multi-process memory/latency delta measured and recorded vs Phase 5.

**Risks.** seccomp budget too tight/loose (iterate with experiments);
Windows/macOS gap remains (documented in ADR-0011 — not hidden).

**Exit conditions.** AC 1–5. **Dependencies.** Phase 5; ADR-0004/0011.

---

## Phase 7 — Modern CSS, advanced layout, fonts, SVG, canvas, graphics acceleration, internationalization

**Objectives/scope.** The rendering maturity phase: GPU compositing path,
modern CSS coverage, SVG/canvas, international text.

**Prerequisites.** Phase 6 (or Phase 5 for CPU-path work items that do
not touch process boundaries — sequenced per-PR); ADR-0005 vello spike.

**Deliverables.** GPU path via wgpu behind `veyra-gfx` (compositor with
property trees: transform/clip/effect/scroll; compositor scrolling);
`vello` adoption decision recorded (spike evidence); grid (full), tables
(fixed), positioned/stacking complete, animations/transitions v1;
SVG (resvg integration); Canvas 2D (CPU-backed); writing modes v
(horizontal-only logical axes → vertical), bidi full, CSS Color 4
parsing; H2→H3 transport decision point [E1 — h3 maturity]; image
formats incl. WebP (AVIF gap re-checked).

**Standards refs.** CSS Grid 2 [ST22], Transforms [ST48], Animations
[ST47], Compositing [ST23], Color 4 [ST25], Writing Modes [ST37], SVG
(specs via resvg conformance), WebGL (later phase), RFC 9114 [N7].

**Tests.** WPT css grid/flexbox/writing-modes subsets; golden-image GPU
vs CPU parity suite (tolerances documented); canvas/SVG fixtures;
shaping/bidi fixtures (CJK/RTL corpora); scroll-perf benchmarks
(compositor path vs main-thread baseline — measured delta recorded).

**Performance.** Frame-time budgets on animated fixtures derived from
measured baselines; scroll latency vs Phase 3 CPU path recorded.

**AC.**
1. WPT grid/flex/writing-modes tracked pass rates recorded and
   non-regressing.
2. GPU path renders the golden corpus identically to CPU path within
   documented tolerances.
3. Compositor scrolling runs without main-thread involvement on the
   scroll benchmark fixture (tracing-verified).
4. International fixtures (bidi, CJK shaping, vertical writing v1)
   pass the documented fixture set.
5. SVG/canvas fixtures stable.

**Risks.** vello readiness [U]; GPU CI variance (CPU path remains the
CI gate — ADR-0005); scope (largest phase — staged sub-milestones
documented in-phase).

**Exit conditions.** AC 1–5 (sub-milestone sequencing documented at
phase start). **Dependencies.** Phase 5/6; ADR-0005.

---

## Phase 8 — Workers, service workers, WebAssembly, advanced networking, persistent web applications

**Objectives/scope.** Concurrency and offline: worker event loops,
service workers, WASM runtime + JS API, WebSocket/streams.

**Prerequisites.** Phase 6 (storage/partitioning for SW), Phase 7 (fetch
maturity).

**Deliverables.** `veyra-workers` (dedicated workers; shared worker
eval); service workers (lifecycle, fetch interception — intercepts at
the `veyra-net` policy layer designed in Phase 5); `veyra-wasm` (wasmi
per ADR-0009; `WebAssembly` JS API in `veyra-webid`); streams [ST8];
WebSocket [ST44]; Cache API persistence; Web App Manifest basics
(instill later).

**Standards refs.** Workers/ServiceWorkers (canonical spec homes
[ST9][ST10 — dns-failed at check; content corroborated via HTML spec]),
Streams [ST8], WebSocket [ST44], WASM core+JS API [J5][J6].

**Tests.** WASM spec suite against wasmi (tracked pass rate);
structured-clone fuzz targets; SW lifecycle tests; WebSocket WPT
subset; worker event-loop ordering suites.

**AC.**
1. WASM spec-suite pass rate recorded in the matrix (suite revision
   cited); JS API integration tests pass.
2. A documented offline demo (SW + Cache API) passes the fixture test
   end-to-end.
3. Worker postMessage/structured-clone round-trips fuzzed clean.
4. Streams-backed fetch works for the fetch test subset.

**Risks.** SW spec complexity (largest remaining spec surface; staged
sub-milestones); wasmi throughput (interpreter — documented, JIT path
is ADR-0009's Phase 9+ gate).

**Exit conditions.** AC 1–4. **Dependencies.** Phases 5–7.

---

## Phase 9 — Media, WebRTC, WebGPU, extensions, developer tools, major platform capabilities

**Objectives/scope.** The heavy-platform phase, entered only with the
engine stable enough to carry it; several items are *evaluation-gated*
and may exit as documented gaps rather than features — the matrix
records the outcome either way.

**Prerequisites.** Phase 7–8.

**Deliverables (gated).** Audio decode (Symphonia [E1 — MPL gate]);
**video decode: ADR-0007 exception analysis** (pure-Rust gap — outcome
may be "documented limitation" honestly); WebRTC (webrtc-rs hardening
evaluation); WebGPU (wgpu-native surface); extension platform subset
per ADR-0006 (isolated worlds, storage, tabs/messaging; DNR interception
points); `veyra-devtools` CDP subset (Runtime/Debugger/DOM/Network);
media playback pipeline v1.

**Standards refs.** WebGPU [ST28], WebGL 2 [ST29 — bot-blocked
canonical], WebRTC [ST30 — bot-blocked], EME [ST31 — out of scope,
documented].

**Tests.** Protocol-level tests for each capability; extension matrix
updates per namespace; CDP conformance fixtures; A11y baseline via
accesskit tree dumps.

**AC.**
1. Each capability ends in one of exactly two states: implemented with
   tracked tests, or a documented gap in the compatibility matrix with
   the ADR/decision citation. **No silent partial states.**
2. Extension subset: manifest→install→content-script→messaging flow
   passes the fixture test.
3. CDP subset drives the engine from an external client (fixture
   harness).
4. WebGPU/WebGL: conformance *sampling* recorded (not full suites) with
   the sample list documented.

**Risks.** Everything in this phase is maturity-risked [U per audit
§8]; the gate protects the roadmap from overcommitment.

**Exit conditions.** AC 1–4. **Dependencies.** Phases 7–8; ADR-0006/0007.

---

## Phase 10 — Browser-product completeness, platform integrations, accessibility, updates, performance, reliability

**Objectives/scope.** The product phase: full tab/window model, omnibox,
bookmarks/history/downloads UI, permissions UI, profiles UI, settings,
find-in-page/zoom/fullscreen/print, self-hosted TypeScript chrome
(ADR-0006), accessibility via accesskit platform adapters, autofill,
crash reporting (local-first), update mechanism design+implementation,
policy/enterprise config.

**Prerequisites.** Phase 7–9.

**Deliverables.** Product crates (boundaries decided at phase entry per
ADR process); TypeScript chrome UI rendered by Veyra (dogfooding);
a11y tree projection to OS APIs; import/export; print-to-PDF (pure-Rust
writer per audit gap); update channel with signed packages (signing
infrastructure decision documented); safe-browsing strategy decision
(independent lists — dossier §5); benchmark program matured (Speedometer
3 evaluation after license review [E2]).

**Tests.** UI automation via the CDP/WebDriver surfaces; a11y tree
conformance vs ARIA mappings [ST17 — bot-blocked canonical];
startup/memory budgets from measured baselines; update rollback tests.

**AC.**
1. Documented user journeys (open→browse→bookmark→restore) pass UI
   automation end-to-end.
2. A11y: platform tree exposed and navigable with screen-reader smoke
   tests on each OS (documented per-OS status).
3. Startup and memory budgets hold (measured baselines, recorded).
4. Update channel installs/rolls back in the test rig.
5. Product features that depend on proprietary services are either
   independent implementations or user-visible documented limitations.

**Risks.** Scope (product surface is wide — sub-milestones);
self-hosted chrome stability (ADR-0006 fallback pre-registered).

**Exit conditions.** AC 1–5. **Dependencies.** Phases 7–9.

---

## Phase 11 — Sustained compatibility, security audits, interoperability, release readiness

**Objectives/scope.** Operational maturity: continuous conformance
programs, external security audit, release engineering, honest
compatibility reporting.

**Prerequisites.** Phase 10.

**Deliverables.** Scheduled WPT/Test262/WASM reporting with trend
tracking; real-site corpus program (diffed screenshots, triaged);
continuous fuzzing with corpus growth; external audit scope/RFP
documented; release/channel engineering (semver, signing, SBOM);
compatibility-matrix-driven public reporting.

**Tests.** All of the above; audit findings triaged with SLAs.

**AC.**
1. Public compatibility reports generated from the matrix (no
   hand-claimed numbers).
2. External audit completed or formally scheduled with scope document.
3. Release engineering: reproducible builds, signed artifacts, SBOM
   generation proven in CI.
4. Fuzzing findings mean-time-to-fix tracked and improving.

**Risks.** Audit scheduling (external dependency); sustaining
volunteer/contributor throughput (process documented in CONTRIBUTING).

**Exit conditions.** AC 1–4; the program is steady-state thereafter.
**Dependencies.** Phase 10.

---

## Minimum useful milestone

**"Veyra reads the web":** the Phase 5 exit state — a browser shell that
navigates real URLs over HTTPS, renders static pages with correct
block/inline layout, styled text, and forms, executes page JavaScript
sufficiently for Test262-measured conformance, and reports its exact
compatibility state from tracked suites. This is the first state a
contributor or evaluator can *use* for its documented purpose and *verify*
its claims independently. Everything after it broadens toward the
platform-completeness phases.

## Path to broader compatibility (credible, not aspirational)

1. Tracked-suite pass rates per directory (fetch, selectors, html,
   IndexedDB, …) drive prioritization — the matrix, not intuition.
2. Real-site corpus diffs catch the "spec-passing but site-breaking"
   class (quirks [ST40], engine-interop realities).
3. The subsystem strategy (stylo-class style engine if adopted, mature
   protocol stacks, proven WASM runtime) buys the years of compatibility
   work where reinvention adds no differentiation.
4. The recorded gaps (inline layout, video, PAC, PDF) each have owners
   via ADR/audit entries — none silently dropped.
5. Performance targets are set from baselines per phase and enforced by
   scheduled benchmark runs with regression budgets.

## Deferred decisions register (must not be decided prematurely)

| Decision | When it is decided | Evidence required |
|---|---|---|
| stylo adoption | Phase 3 entry spike | Integration surface + license hygiene |
| vello vs custom compositor | Phase 7 spike | Perf/parity data |
| binding generator | Phase 5 (>50 interfaces) | Interface count + ergonomics |
| wptrunner adoption | Phase 5 evaluation | Integration cost measurement |
| wasmtime JIT | Phase 9 gate | Workload baselines |
| video decode exception | Phase 9 ADR-0007 analysis | Pure-Rust gap re-survey |
| DevTools frontend reuse | Phase 9 license review | License terms |
| update signing infra | Phase 10 | Threat model for updates |

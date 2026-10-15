# Research: JavaScript, WebAssembly, and Execution Semantics

status: Investigated (first pass) | last-updated: 2026-10-15 | maintainer: foundation work

## Evidence basis

V8: official docs and engineering blog — Sparkplug [J2], Maglev [J3], GC
(Orinoco) [J4], v8.dev/docs [J1] — all verified. Standards: ECMAScript
specification [ST15], WebAssembly core spec [J5] and JS API [J6], Test262
documentation [T3][J8]. Ecosystem data: crates.io/GitHub API pull on
2026-10-15 [E] — licenses, versions, downloads, and activity quoted from that
pull (raw JSON archived at repository-research time; see
[dependency-and-license-audit.md](dependency-and-license-audit.md)). Boa's
self-description quoted from its README as fetched 2026-10-15. **No benchmark
numbers for any engine are claimed** — none were measured in this pass.

## 1. What the browser asks of a JavaScript engine

A browser embeds an ECMAScript engine as a *service* under strict constraints
[ST1][ST15][K]:

1. **Spec semantics.** ECMAScript language semantics [ST15] plus the HTML
   integration: the event loop's task queues and microtask checkpoint points,
   timers (clamping, nesting depth), module loading integrated with the fetch
   algorithm, and error reporting. `job` scheduling in the spec maps to host
   enqueue callbacks; a browser must honor ordering or async code breaks.
2. **Heap integration.** JS objects hold DOM nodes; DOM holds JS callbacks.
   The host must expose rooting so the GC can see browser-side references and
   finalization can drop DOM wrappers. Getting this wrong leaks or
   use-after-frees — in Rust, the equivalent is a careful ownership protocol
   (see §5).
3. **Deterministic-ish time behavior.** Timers, `performance.now()`, and GC
   must be controllable for tests; the engine must be embeddable with
   injected clocks and stepped execution (Chromium does this for DevTools and
   testing).
4. **Security.** The engine runs untrusted code in the renderer's process —
   it is the primary attack surface. JIT code generation creates
   W^X/executable-memory obligations (see [security-model.md](security-model.md));
   spec-level ` realm `/` origin ` separation maps to per-context isolation.

## 2. What V8 actually is (observation, not aspiration)

V8 [J1–J4][K] is a multi-tier engine: a baseline interpreter, tiering
optimizing compilers (Sparkplug baseline compiler, Maglev mid-tier [J3],
Turbofan top tier), an incremental/concurrent generational GC (Orinoco
architecture [J4]), inline caches, and a large set of intrinsics implementing
the host environment (via embedder API + bindings). It also ships the
WebAssembly runtime (Liftoff/TurboFan-tiered compilation) and the JS-to-WASM
interop layer [J6][K].

**Assessment, stated plainly:** matching V8's *conformance* is a multi-year
program (a full ECMAScript implementation plus host integration); matching
V8's *performance* is a second, equally long program (tiering, ICs, GC
concurrency). **This document makes no promise that a new engine will match
V8.** The strategic question — embed an existing engine vs. grow one — is
decided with evidence in [ADR-0002](../decisions/0002-javascript-engine-strategy.md)
and revisited on defined metrics.

## 3. The event loop and host semantics Veyra must implement itself

Regardless of which engine executes JS, Veyra owns:

- **Event loop** [ST1]: task queues with ordered same-origin task sources,
  microtask checkpoints, `queueMicrotask`, rAF alignment to frame callbacks,
  `setTimeout` semantics (including minimum delay and nesting clamping).
- **Timers & scheduling integration** with the page lifecycle (background
  throttling later).
- **Structured clone** [ST2/HTML]: the value-serialization algorithm for
  postMessage/IndexedDB/History — a self-contained, testable subsystem.
- **Module map and import resolution** integrated with fetch (Phase 4/5).

These are pure-Rust, spec-testable components independent of the JS engine
choice — deliberately engineered that way so the engine can be swapped.

## 4. Workers and service workers

Dedicated/Shared/Service workers [ST9 — DNS-blocked at check time; behavior
documented via HTML spec and MDN, marked K] are separate event loops with
separate global scopes communicating via structured clone and
`MessagePort`s. Service workers add a lifecycle (install/activate, fetch
interception, waitUntil promises) — the largest single feature gap for any
new engine. **Assessment:** worker *event loop* machinery is engine-agnostic
and lands with the Phase 8 plan; service workers are Phase 8 with an explicit
risk note (spec complexity + WPT coverage dependence).

## 5. The Rust JavaScript-engine landscape (live data, 2026-10-15)

Data from the ecosystem pull [E]; interpretation follows the table.

| Engine | Language | License (crates.io) | Version | Activity | Maturity signal |
|---|---|---|---|---|---|
| Boa (`boa_engine`) | Rust | Unlicense OR MIT (crates.io field; repo shows MIT) | 0.22.0 | pushed 2026-10-14; 7,605 stars | 5.5M downloads; self-describes as "experimental"; README claims support for ">90% of the latest ECMAScript specification"; conformance tracked at boajs.dev/conformance (verified live; numbers not extracted — client-rendered) |
| Nova (`nova_vm`) | Rust | MPL-2.0 | 1.0.0 | pushed 2026-03-15 | 1,754 downloads — early; project states Test262 pass-rate tracking on trynova.dev |
| Kiesel | **Zig** | (codeberg-hosted) | — | GitHub mirrors stale | **Excluded from Rust candidates — fails language policy [E: GitHub search]** |
| Starlight | Rust | (GitHub) | — | stalled (no releases in years) | Excluded — dormant [U] |
| `rquickjs` | Rust bindings → **C** QuickJS-ng | MIT | 0.14.0 | active | Rejected as runtime dependency (C core); usable as a *reference oracle* |
| `v8` crate | Rust bindings → **C++** V8 | MIT | 152.2.0 | very active | Rejected — violates independence objective (see ADR-0001/ADR-0002) |

**Assessment:** the honest reading is:

1. **Boa is the only mature-ish pure-Rust, permissively licensed general JS
   engine.** Its own README says "experimental" — treat "90% ECMAScript" as a
   self-reported, suite-version-dependent claim to be re-measured against the
   Test262 subset Veyra adopts, not accepted as gospel.
2. **Nova is the credible emerging option** but pre-production by every
   signal (downloads, own project messaging).
3. **The gap between "runs the Ember tutorial" and "runs real sites" is
   enormous** regardless of engine: host bindings, GC integration, and
   Web-API surface dominate real-world JS compatibility.
4. Conformance measurement must be Veyra-run (Test262 via a harness over the
   embedded engine) — engine self-reports are never load-bearing. This is
   written into the testing program.

## 6. The Rust WebAssembly-runtime landscape (live data, 2026-10-15)

| Runtime | License [E] | Version | Notes |
|---|---|---|---|
| `wasmtime` | Apache-2.0 WITH LLVM-exception | 49.0.3 | Bytecode Alliance; Cranelift JIT; production-grade; 40.0M downloads; very active |
| `wasmi` | MIT/Apache-2.0 | 2.0.0 | Interpreter; 25.0M downloads; embedded-friendly; Parity lineage |
| `wasmer` | MIT | 7.5.0 | Multi-backend; 8.7M downloads |

The **WebAssembly JS API** (`WebAssembly.Module/instantiate/Memory/Table/...`)
[J6] is *host* work — it binds a runtime into the JS engine's type system.
Whoever the JS engine is, Veyra implements this JS-API shim over the chosen
WASM runtime. **Assessment:** start with `wasmi` (small audit surface,
interpreter — adequate for Phase 8 functionality), plan `wasmtime` for the
JIT path when performance work begins; both licenses clear policy. Details in
[ADR-0009](../decisions/0009-webassembly-runtime.md).

## 7. Bindings: Web IDL and the Web API surface

Web IDL [ST7] defines how specs declare interfaces; the browser generates
(Chromium: a bindings codegen layer over V8 API [K]) the glue: attributes,
operations, dictionaries, sequences, promises, `this` semantics, exceptions,
and [LegacyLenientThis]-class edge cases. For Veyra the options are:

1. **Hand-written bindings** for a curated API set (Phase 4: ~30 core
   interfaces) — simplest, most auditable, slowest to scale.
2. **A Rust proc-macro/IDL-driven binding generator** consuming Web IDL
   fragments — the long-term path; investment justified once the API count
   exceeds ~50 (Phase 5 decision point).
3. **Codegen from the spec's IDL bundles** (webref) — maximum coverage,
   highest complexity; deferred.

**Assessment:** (1) now, (2) at Phase 5, (3) not before Phase 7. The binding
layer must keep a strict rule: **no JS engine types leak past the bindings
module** — Web API implementations are pure Rust against `veyra-dom` types,
which is what makes the engine swap in ADR-0002 survivable.

## 8. Developer-facing execution infrastructure

Debugging/profiling obligations researched for Phase 9+: a debugger protocol
(Chromium's CDP [P2, verified live at chromedevtools.github.io]) covering
runtime evaluation, breakpoints, DOM/network inspection, and profiling;
source maps; and error/stack semantics (spec 1.9.x + host stack augmentation
[K]). **Assessment:** Veyra implements a CDP-*shaped* protocol subset; using
the Chrome DevTools *frontend* is a licensing-evaluated option recorded in
[browser-product-features.md](browser-product-features.md) — not assumed.

## 9. What Veyra needs (mapping)

| Capability | Veyra subsystem | Phase |
|---|---|---|
| JS execution behind a stable facade | `veyra-script` (engine-agnostic trait; Boa-backed per ADR-0002) | 4 |
| Event loop, timers, microtasks | `veyra-executor` (injectable clock) | 4 |
| Core Web API bindings | `veyra-webid` bindings module | 4–5 |
| Structured clone | `veyra-serialize` | 5 |
| WASM runtime + JS API | `veyra-wasm` (wasmi → wasmtime per ADR-0009) | 8 |
| Workers/service workers | `veyra-workers` | 8 |
| Debugger/profiler protocol | `veyra-devtools` | 9–10 |

## 10. Open questions

1. **[U] Boa's real Test262 numbers on Veyra's chosen subset** — must be
   measured by Veyra's own harness in Phase 4 entry; engine self-report
   (">90%") is not load-bearing.
2. **[U] Boa embedding ergonomics for the DOM ownership protocol** (rooting,
   wrappers, weakrefs) — a Phase 4 pre-spike is scheduled in the roadmap.
3. **[U] Nova trajectory** — re-evaluate at Phase 5 (metrics in ADR-0002).
4. **[U] WASM JS-API coverage in Boa** (does the engine already ship a
   `WebAssembly` host object over an embedded runtime?) — determines whether
   Veyra writes the shim itself (likely; assumed in ADR-0009).
5. **[U] GC-pressure interplay** between a foreign engine's heap and a
   Rust DOM arena — needs a design spike before Phase 4 implementation.

## Confidence and gaps

- High: V8 tiering/GC architecture as documented [J2][J3][J4]; spec
  obligations (event loop, structured clone, IDL) [ST1][ST2][ST7][J5][J6];
  the Rust-landscape table (live API data) [E].
- Medium: embedding-level details of Boa/Nova (interfaces exist; depth of
  DOM-integration support unverified).
- Explicitly absent: performance claims of any kind; "V8-class" promises.

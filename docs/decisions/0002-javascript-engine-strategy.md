# ADR 0002: JavaScript engine strategy — embed Boa behind a facade

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Veyra must execute JavaScript (the web's language) and later WebAssembly.
The research dossier ([javascript-and-wasm.md](../research/javascript-and-wasm.md))
establishes: (a) V8-class performance and conformance is a multi-year
program — no new engine should promise it; (b) the pure-Rust engine
landscape in 2026-10 consists of **Boa** (active, permissively licensed,
self-reported ">90% of the latest ECMAScript" [E3], self-described
"experimental") and **Nova** (early: 1.8K downloads, MPL-2.0 [E1]);
Kiesel is Zig [E2 — excluded]; Starlight is dormant [U]; rquickjs and
rusty_v8 bind to C/C++ cores and violate ADR-0001 as runtime choices;
(c) regardless of engine, Veyra owns the event loop, structured clone,
and Web API surface, all of which are engine-agnostic if the binding
layer keeps engine types contained.

Risk register: Boa's conformance is self-reported and suite-dependent;
DOM-embedding ergonomics (rooting, GC interop with a Rust DOM arena) are
unverified [U — Phase 4 entry spike]. Adopting an engine that later
proves inadequate must not strand the Web API layer.

## Decision

1. **Embed an existing Rust JS engine; do not write one.** Concretely:
   `boa_engine` [E1] is the Phase 4 implementation, wrapped behind the
   `veyra-script` facade (see [execution-and-web-apis.md](../architecture/execution-and-web-apis.md)).
2. **The facade is the contract.** All Web API code (via `veyra-webid`)
   is engine-agnostic; engine types never leak past `veyra-script`/
   `veyra-webid`. This is the explicit escape hatch that makes the
   decision reversible.
3. **No performance promises.** Phase 4's success criteria are
   conformance and integration (Test262 subset results published in the
   compatibility matrix; WPT testharness runnable), not speed.
4. **Test262 truth is self-measured.** Veyra's own runner produces the
   numbers it acts on; engine self-reports are never load-bearing.
5. **Re-evaluation gate at Phase 5 entry** against explicit metrics
   (below). Options then: deepen Boa investment (incl. upstream
   contribution), adopt a different engine behind the same facade, or
   (last resort, requiring its own ADR) begin a purpose-built engine
   program.

## Alternatives considered

- **Write a new engine now** — rejected: spending years before the
  browser renders anything is a strategy failure; the facade preserves
  the option without paying it today.
- **Bind V8/SpiderMonkey/QuickJS** — rejected: violates the independence
  objective (V8/SpiderMonkey) or the language policy (C cores); noted as
  permanent non-options for the runtime, usable as reference oracles.
- **Nova immediately** — rejected for Phase 4: too early by every
  maturity signal [E1]; retained as the primary alternative at the Phase 5
  gate.
- **No JS in early phases** — rejected: the event loop, bindings, and
  structured clone are the project's integration spine; deferring JS
  defers them all.

## Consequences

- Phase 4 can integrate a real engine and measure real conformance.
- Boa's GC interop with the Rust DOM (rooting/trace protocol) is a
  Phase 4-entry spike with a fallback plan (simpler ownership model).
- Real-world JS-heavy sites remain out of reach initially — recorded
  honestly in the compatibility matrix as partial states, not marketing.
- The facade adds an indirection layer; its cost is measured at Phase 5
  before any optimization work.

## Revisit conditions

Phase 5 entry evaluates: (a) measured Test262 pass rate on Veyra's chosen
subset < the phase's documented threshold; (b) embedding defects
(GC/ownership) that block the bindings roadmap; (c) upstream Boa activity
ceases. Any of these triggers the options process above with evidence.

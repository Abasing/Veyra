# ADR 0009: WebAssembly runtime — wasmi first, wasmtime when JIT is justified

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

WebAssembly is a charter-level execution target. The JS API surface
(`WebAssembly.Module/Instance/Memory/Table/...` [J6]) is host work Veyra
implements over any runtime. Rust candidates [E1, audit §2]:
`wasmtime` 49.x (Apache-2.0 WITH LLVM-exception; Cranelift JIT;
production-grade, Bytecode Alliance; 40.0M downloads) and `wasmi` 2.0
(MIT/Apache-2.0; interpreter; 25.0M downloads; embedded-friendly);
`wasmer` as an alternative. Security considerations: JIT introduces
executable-memory obligations (W^X — invariant I8); a proven sandbox
matters more than raw speed for a browser embedding.

## Decision

1. **Phase 8 functionality: `wasmi`** (interpreter) — small audit
   surface, deterministic, license-clean, adequate for correctness
   milestones.
2. **Phase 9+ performance path: `wasmtime`** evaluated by spike when
   WASM workloads are real (baselines exist); its sandboxing track record
   (production embedder) and Cranelift W^X discipline satisfy I8.
3. **The `WebAssembly` JS API is implemented in `veyra-webid`** over the
   runtime behind a thin `veyra-wasm` facade (same swap-ability doctrine
   as ADR-0002).
4. **Conformance:** the official WASM spec test suite [J5] runs against
   the runtime; JS-API integration tests via Test262 wasm staging where
   engine support permits.

## Alternatives considered

- **wasmtime from Phase 8** — deferred: heavier dependency and JIT
  surface before functionality exists; interpreter-first matches the
  project's correctness-first doctrine (mirrors ADR-0002's shape).
- **wasmer** — rejected for now: license fine, but overlapping value
  with wasmtime; fewer browser-relevant proven deployments in the
  research gathered [E1].
- **Custom runtime** — rejected outright: this is exactly the class of
  system where a proven implementation beats an ambitious one.

## Consequences

- Early WASM execution is interpreter-speed — acceptable and honest
  (the matrix says so); JIT adoption is a measured decision, not a
  vibe.
- Two runtime facades over time means `veyra-wasm`'s trait must cover
  instantiation, memory, tables, imports/exports — kept minimal.

## Revisit conditions

WASM-heavy workload baselines (Phase 9); wasmi maintenance stall; a
need for components (e.g., WASI-style capabilities) that only one
runtime provides.

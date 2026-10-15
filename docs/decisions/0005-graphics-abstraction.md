# ADR 0005: Graphics abstraction — CPU-first raster, wgpu for the GPU path

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Rendering needs a rasterization backend from Phase 3 and GPU acceleration
eventually (compositor scrolling, canvas, effects). Rust ecosystem
signals [E1, dependency audit §5]: `tiny-skia` (BSD-3 per crates.io —
license conflict to re-verify at adoption), `wgpu` 30.x (MIT OR
Apache-2.0, very active), `vello` (pre-1.0 GPU 2D), `resvg` for SVG,
`glyphon` for GPU text. Chromium's pipeline (RenderingNG [D8]) separates
display lists from rasterization and uses property trees for compositing;
Veyra adopts those *shapes* (rendering invariants R1–R4) while choosing
its own backends.

## Decision

1. **Phase 3: CPU rasterization** (`tiny-skia`-class) behind the
   `veyra-gfx` trait. The screenshot/golden-image harness consumes the
   CPU path — determinism by construction (no GPU in the test loop).
2. **Phase 7: GPU path via `wgpu`** behind the same trait — Vulkan/
   Metal/DX12/GL backends; `vello` evaluated by spike (performance,
   text, clip/effect coverage) vs a custom compositor over wgpu; decision
   recorded as an amendment to this ADR.
3. **`veyra-gfx` stays thin:** buffer/texture/surface/pipeline
   abstractions only; no scene graph (display lists live in
   `veyra-paint`).
4. **Driver-facing code via wgpu only** — Veyra writes no Vulkan/GL code
   directly. wgpu's use of native graphics APIs is a platform boundary
   per ADR-0007, not a language-policy violation.

## Alternatives considered

- **GPU-only from Phase 3** — rejected: non-deterministic CI (driver
  variance), no headless determinism, and the screenshot harness — the
  project's visual-truth mechanism — must be reproducible.
- **skia (via skia-safe)** — rejected: C++ core (ADR-0001); the dependency
  audit lists it as rejected-class.
- **vello immediately** — deferred: pre-1.0 [E1]; evaluate at Phase 7 by
  spike rather than committing blind.
- **Custom Vulkan abstraction** — rejected: unjustifiable scope; wgpu
  exists, is active, and is license-clean.

## Consequences

- Two raster backends must satisfy the same display-list semantics; the
  trait + snapshot tests on both paths are the guarantee (GPU snapshot
  tests are best-effort/diagnostic, CPU tests are the CI gate).
- Phase 3 rendering is slower than GPU-accelerated engines — accepted;
  performance baselines/budgets in Phase 3 apply to the CPU path.

## Revisit conditions

vello 1.0 + positive spike → adopt for the vector path; wgpu API
instability harming Phase 7 → pin exact version + shim; a memory-safe
pure-Rust Skia-class rasterizer emerging → re-evaluate tiny-skia.

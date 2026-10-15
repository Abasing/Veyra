# ADR 0012: Phase 1 shell and test-infrastructure dependencies

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Phase 1 (roadmap) delivers the first executable: `veyra-shell` (window,
event loop, input plumbing), the screenshot harness, golden-image
infrastructure, the injectable frame clock, configuration, diagnostics,
and criterion benchmarks. ADR-0005 fixes the graphics shape (thin
`veyra-gfx` trait, CPU-first determinism) and ADR-0007 sanctions
windowing through winit as an acceptable native boundary. The dependency
audit flagged "`winit` 0.31 beta — pin stable at Phase 1" [E1].

Adopting these crates is exactly the ADR trigger listed in engineering
standards §12 (new external dependency; new crate boundaries
`veyra-gfx`/`veyra-shell`). Every version below was re-verified against
crates.io on 2026-10-15 (license, MSRV, activity); all licenses are in
`deny.toml`'s allow-list. Evidence rows were added to the source
register (E-series, accessed 2026-10-15).

## Decision

1. **Windowing: `winit` pinned exactly to `=0.30.13`** (Apache-2.0;
   MSRV 1.70). The 0.31 line is still beta on crates.io (max stable
   0.30.13, checked 2026-10-15) — the audit's instruction resolves to the
   stable line. The *exact* pin (no `^` range) is deliberate: windowing
   behavior drift inside a SemVer-compatible range is precisely the
   nondeterminism the rendering harness must not inherit. Dependabot
   proposes bumps; each is reviewed and re-pinned explicitly.
2. **CPU-buffer presentation: `softbuffer` 0.4.8** (MIT OR Apache-2.0;
   MSRV 1.71). The missing piece between our RGBA8 `PixelBuffer` and the
   winit window: pure-Rust platform blits (XCB via `x11rb`, Wayland,
   Win32, …), no GPU in the loop, no C build-time dependencies. It is
   consumed **only** inside `veyra-shell`'s window module; the
   RGBA8→`0x00RRGGBB` conversion lives there, keeping `veyra-gfx`
   presentation-agnostic. Maintained under rust-windowing (same org as
   winit), so platform coverage tracks winit's.
3. **PNG encoding: `png` 0.18.1** (MIT OR Apache-2.0; MSRV 1.73). The
   screenshot harness's file step. Chosen over the `image` meta-crate:
   encode/decode of PNG only, a fraction of the transitive surface;
   `image` arrives with content phases if/when decoders are needed
   (roadmap Phase 7).
4. **Canonical hashing: `sha2` 0.11.1** (MIT OR Apache-2.0; MSRV 1.88).
   SHA-256 over the documented canonical pixel representation (see
   `veyra-shell::hashing`). Pure Rust (RustCrypto), tiny, no transitive
   C. A hand-rolled FNV would avoid the dependency but buys nothing and
   weakens collision robustness for regression tracking.
5. **Diagnostics: `tracing` 0.1.44 + `tracing-subscriber` 0.3.23** (MIT).
   Prescribed by system-overview §4 (span taxonomy, privacy rules) since
   Phase 0; Phase 1 is the first crate that can log.
6. **Configuration: `serde` 1.0.229 (derive) + `toml` 1.1.8** (both
   MIT OR Apache-2.0). TOML is the ecosystem-standard app-config format;
   hand-rolling a parser for a long-lived, schema-gated surface is
   liability, not frugality. The `deny_unknown_fields` + validation
   discipline in `veyra-shell::config` is where the honesty guarantees
   live. serde_derive is a proc-macro outside the content layer (the
   dependency-graph's proc-macro restriction covers content crates only).
7. **Benchmarks: `criterion` 0.8.2** (Apache-2.0 OR MIT; dev-dependency
   only). Named by the roadmap; dev-deps never ship to users.
8. **CLI parsing and error definitions stay dependency-free.** Hand-rolled
   argument parser and hand-written typed errors (no `clap`, no
   `thiserror`): the surface is small, the proc-macro weight is not
   justified, and the error text is part of the automation contract.
9. **`veyra-gfx` adopts zero dependencies in Phase 1.** The CPU raster
   path is integer math over owned memory; `tiny-skia` enters only at
   Phase 3 after the ADR-0005 license re-verification (its crates.io
   BSD-3-Clause marking conflicts with the repo's README record and must
   be confirmed at adoption).

## Alternatives considered

- **winit 0.31.0-beta.3** — rejected for Phase 1: beta quality on the
  windowing surface is exactly the "platform assumptions that silently
  fail" class Phase 1 must exclude; revisit when 0.31 stabilizes.
- **`image` crate for PNG** — rejected: carries decoders/formats Phase 1
  does not use; the audit's dependency-frugality rule applies.
- **`pixels` crate (winit+wgpu wrapper)** — rejected: pulls a GPU stack
  the CPU path must not depend on (ADR-0005: no GPU in the test loop).
- **`minifb`** — rejected: reimplements window management (softbuffer's
  own docs note its flaws); duplicates winit's role.
- **`clap`/`thiserror`** — deferred: justified when CLI/error surface
  grows; Phase 1's hand-rolled versions are fully tested and
  dependency-free.
- **Hand-rolled TOML subset** — rejected: config is schema-gated and
  long-lived; parser bugs there are silent-corruption bugs.

## Consequences

- `Cargo.lock` grows from 1 to ~100 packages; all are license-allow-listed
  and pure Rust (`cargo-deny` enforces continuously). The windowing path
  dlopens `libxkbcommon`/`libxkbcommon-x11` at runtime on Linux — a
  documented runtime platform requirement (CI installs it; the headless
  screenshot path needs none of it).
- The presentation boundary is fixed: application code converts buffers
  for softbuffer in one module; replacing presentation (e.g., wgpu
  surface in Phase 7) touches that module only.
- winit's event-loop semantics leak slightly into the automation hook:
  frames for `--exit-after-frames` are presented from `about_to_wait`
  because a redraw requested *during* `RedrawRequested` dispatch can be
  coalesced on X11 (observed; see the `about_to_wait` doc comment). This
  is recorded as a Phase 1 workaround, revisited on winit upgrades.
- Exact-pinning winit means security fixes arrive via explicit bumps,
  not range updates — accepted for determinism; dependabot + the
  scheduled advisory scan cover the gap.

## Revisit conditions

- winit 0.31 reaching stable: evaluate upgrade (with beta-period issue
  review) as its own commit; the exact pin makes this a deliberate act.
- softbuffer stagnation or a wasm-gap blocking Phase 7 GPU: re-evaluate
  against a wgpu-surface presentation path.
- `png`/`sha2` MSRV exceeding the pinned toolchain at bump time: pin
  last-compatible or bump toolchain per its documented procedure.
- Phase 3 `tiny-skia` adoption: re-verify license marking (ADR-0005
  revisit condition) before the dependency lands.

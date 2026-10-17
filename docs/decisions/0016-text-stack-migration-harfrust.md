# ADR 0016: Text stack migration — harfrust replaces rustybuzz/ttf-parser (RUSTSEC-2026-0192)

- **Status:** Accepted
- **Date:** 2026-10-17
- **Deciders:** Veyra foundation
- **Supersedes:** ADR-0015 decision 1 (the shaping/font-data crate choice only; ADR-0015 decisions 2–7 stand unchanged)

## Context

The CI supply-chain gate (cargo-deny) failed on every push since the
Phase 3 text batch: **RUSTSEC-2026-0192** declared `rustybuzz` and its
parse foundation `ttf-parser` unmaintained (the author's announcement,
harfbuzz/ttf-parser#217 — both crates last released November 2024). The
same CI runs also surfaced a second, older supply-chain regression this
batch fixes alongside: `veyra-style`/`veyra-layout`/`veyra-paint` had
gained **version-less path dependencies** (cargo-deny `wildcards =
"deny"`), failing the bans check since Phase 3 batch 2.

The project's constraints for the replacement are unchanged from
ADR-0015: shaping types never escape `veyra-text` (dependency-graph
§5), pure Rust with no C/C++ runtime dependencies (ADR-0001),
deterministic bit-for-bit output (the golden-dump contract), typed
errors on untrusted input.

Evidence gathered this session (crates.io API + crate sources read
directly, 2026-10-17):

- **harfrust 0.14.0** (harfbuzz org, MIT, MSRV 1.85, released
  2026-10-04; monthly releases since at least 0.12.0 in July 2026;
  ~12.9M downloads): the official HarfBuzz Rust port that *started as a
  fork of RustyBuzz specifically to move off ttf-parser onto
  read-fonts* — the succession from our adopted stack is the HarfBuzz
  project's own position, not an inference. Matches HarfBuzz 14.5.1
  behavior; passes most of the HarfBuzz test suite (upstream HARFBUZZ.md
  records the known gaps); shapes in font units with manual scaling —
  the same contract `veyra-text` already had.
- **read-fonts 0.45.1** (googlefonts/fontations, MIT OR Apache-2.0,
  2026-10-15): the shared parse foundation harfrust builds on. Its new
  `model` layer provides `Font` (lazy view over shared bytes), `Charmap`,
  `Names`, and `Metrics` — the entire metadata surface `FontInfo` needs.
  Already in the lockfile via harfrust; adding it as a direct dependency
  of `veyra-text` adds no new packages.
- **skrifa 0.48.0** (fontations, MIT OR Apache-2.0, 2026-10-03):
  evaluated and **not adopted this batch** — its high-level metadata API
  duplicates what read-fonts' model layer already provides here; its
  distinctive value (hinting-aware glyph outline instances) is the raster
  batch's concern, to be evaluated when that batch needs it.
- **swash 0.2.10** (2026-07-17): slower release cadence, spans parsing
  and rendering concerns; not chosen.
- **Verification on the embedded set:** after the swap, all 25
  `veyra-text` unit tests pass with **identical golden constants** —
  'W' 2025 units, GPOS-kerned "AV" 2671 (vs 2802 unshaped), `fi` liga
  1290 with cluster 0, combining-mark composition, Arabic RTL 4735 vs
  LTR 4859 (direction-sensitive GPOS preserved) — and the layout (36)
  and paint (11) golden fixtures are byte-identical through the
  `TextMeasurer` seam. Full workspace: 216 tests green (the 2
  window-smoke failures are the pre-existing sandbox-only
  libxkbcommon-x11 gap; CI runs them under Xvfb), `cargo fmt` and
  `clippy -D warnings` clean, and `Cargo.lock` contains neither
  `rustybuzz` nor `ttf-parser` at any level.

## Decision

1. **Adopt `harfrust` 0.14** as the shaping engine inside `veyra-text`,
   wrapped behind the same Veyra-owned `ShapedRun`/`ShapedGlyph` types
   (supersedes ADR-0015 decision 1's rustybuzz half). No public API
   changes; the seam did its job — the swap touched one crate.
2. **Adopt `read-fonts` 0.45** as a direct dependency for the OS/2/post
   table views (weight class, slant classification — the ttf-parser
   `Face::style()` semantics reproduced from `fsSelection` + `post`
   italic angle) and, via harfrust's re-exports, the model `Font` for
   names/charmap/metrics (supersedes ADR-0015 decision 1's ttf-parser
   half). Family-name selection becomes **English-or-first** (model
   `Names::english_or_first`) — was "first decodable record"; identical
   for the embedded set.
3. **Recorded semantic differences** (both verified by the tests before
   and after): (a) harfrust does **not** guess buffer properties —
   `guess_segment_properties()` is called after `set_direction`, and
   without it the plan selects no script shaper and applies no
   GSUB/GPOS at all (the kerning/ligature/Arabic goldens caught exactly
   this during the migration); (b) shaping can now *fail* on malformed
   layout tables (`TextError::Shape`, a new typed variant — rustybuzz's
   `shape` was infallible), keeping the no-panics-on-untrusted-input
   contract.
4. **Version every path dependency** in the workspace manifests
   (`{ path = "...", version = "0.1.0" }` in veyra-style,
   veyra-layout, veyra-paint normal and dev dependencies) — publishable
   crate graph, cargo-deny wildcards-clean.

## Alternatives considered

- **Ignore RUSTSEC-2026-0192 in deny.toml** — rejected: the advisories
  gate exists precisely to force this decision; an unmaintained parse
  layer under untrusted font bytes is a standing risk, and the check
  would have to be re-litigated forever.
- **skrifa for metadata now** — deferred (see evidence): nothing this
  batch needs is skrifa-only; one fontations foundation is enough until
  the raster batch evaluates outline extraction.
- **swash** — rejected: cadence and scope (it bundles parsing + glyph
  rendering concerns we deliberately split).
- **Fork/vendor rustybuzz** — rejected: maintaining a shaping-engine
  fork contradicts the entire point of the swappable seam (ADR-0015's
  "Fully hand-written shaper" reasoning, applied to a fork).
- **C/C++ HarfBuzz or FreeType bindings** — prohibited by ADR-0001
  (no new C/C++ runtime dependencies).

## Consequences

- `Cargo.lock` drops `rustybuzz` and `ttf-parser` entirely; it gains
  `harfrust`, `read-fonts` (direct), `font-types`, `yoke` — all MIT or
  MIT OR Apache-2.0, already inside `deny.toml`'s allow list. No new
  license classes, no new registries, no build scripts, no unsafe.
- The CI supply-chain job is expected green again (advisories: no
  unmaintained crates; bans: no wildcard deps; licenses/sources
  unchanged).
- Shaping output on the embedded DejaVu set is bit-identical to the
  rustybuzz era (all goldens unchanged), so no downstream fixture —
  layout, paint, or shell — moved.
- `Arc<[u8]>` cannot unsize-coerce to the trait-object form the
  fontations `Source` takes, so face views ride one outer `Arc` (a
  pointer hop per `load_face` call, never a byte copy) — documented at
  the helper.
- ADR-0015's evidence section remains an accurate historical record of
  the rustybuzz verification; only its decision 1 is superseded by this
  ADR.

## Revisit conditions

- The raster batch needs glyph outlines/hinting → evaluate skrifa
  (instance/outline machinery) against read-fonts' glyph model — a new
  ADR, not an amendment.
- harfrust conformance or cadence regresses → the seam still holds;
  re-evaluate on the same evidence pattern as this ADR.
- ADR-0015's revisit conditions (fontique-class fallback, webfonts,
  shaping cache) carry over unchanged.

# ADR 0015: Text stack — rustybuzz shaping behind veyra-text; parley/fontique deferred

- **Status:** Accepted — decision 1 superseded by [ADR-0016](0016-text-stack-migration-harfrust.md) (rustybuzz/ttf-parser declared unmaintained, RUSTSEC-2026-0192; migrated to harfrust + read-fonts with identical golden constants). Decisions 2–7 stand unchanged.
- **Date:** 2026-10-17
- **Deciders:** Veyra foundation

## Context

Phase 3's deliverables include `veyra-text` (rendering-pipeline.md §2
anticipated "the Linebender stack (parley/fontique) + rustybuzz
shaping + unicode-bidi"). The Phase 3 batch-4 layout batch shipped the
inline formatting context with text measurement behind the
`TextMeasurer` seam, served by a fully specified fixed-advance interim
provider (`FixedMetrics`) — matrix row 48 records the resulting deltas
(no kerning, no ligatures, no real fonts) and names the `veyra-text`
batch as the next increment, realizing the dependency-graph edge
`LAYOUT --> TEXT`.

The decision this ADR records: which crates `veyra-text` adopts for
shaping/font data/bidi, and what is deferred. Constraints from the
architecture docs: shaping types never escape the crate
(dependency-graph §5); the engine core is synchronous and
deterministic (all tests agree bit for bit — the golden-dump contract);
no unsafe code (workspace lint); the crate is a leaf-ish fuzz-able
parser boundary (dependency-graph §3).

Evidence gathered for this decision (all measured in this repository's
development sandbox, 2026-10-17):

- **rustybuzz 0.20.1** (HarfBuzz port, pure Rust, Apache-2.0 OR MIT):
  shaped the bundled DejaVu Sans face correctly on every probe — GPOS
  shaping ("AV" = 2671 units — see the kerning note below), `liga` substitution
  ("fi" → one glyph), combining-mark composition (e + U+0301 → é),
  Arabic contextual forms with direction-sensitive kerning (4735 units
  RTL vs 4859 LTR — GPOS behavior differs by direction, as the spec
  requires), byte-offset cluster maps, `.notdef` for uncovered
  codepoints.
  (GPOS kerning: "AV" = 2671 units vs 2802 unshaped — 131 units tighter.)
- **ttf-parser 0.25.1** (same maintainer, Apache-2.0 OR MIT): face
  parsing, `name`/hhea/OS-2 tables, `cmap` coverage queries — all
  exercised by the metadata layer `veyra-text` needs regardless of the
  shaper choice. `rustybuzz::Face` is built on it, so the parse layer
  is shared rather than duplicated.
- **unicode-bidi 0.3.18** (Apache-2.0 OR MIT): UAX#9 levels + L2
  visual-run reordering, verified against hand-checked mixed
  LTR/RTL/number cases.
- **unicode-segmentation 1.13.3** (MIT OR Apache-2.0): UAX#29 word and
  grapheme boundaries for the itemization the inline layout's
  line-breaking already models at space granularity.
- **parley 0.x** (Linebender): a *paragraph layout* engine — line
  breaking, run management, its own inline model. Adopting it would
  embed a second inline-layout algorithm alongside `veyra-layout`'s
  IFC (ADR-0003 decision 3: custom block/inline layout is the adopted
  approach), with two sources of truth for line breaking.
- **fontique 0.x** (Linebender): system font enumeration + script
  fallback. Veyra's determinism contract currently requires *no*
  platform font discovery; fallback breadth (CJK et al.) is a
  Phase 7-scale concern per the rendering pipeline's recorded scope.

## Decision

1. **Adopt `rustybuzz` 0.20** as the shaping engine and **`ttf-parser`
   0.25** as the font-data layer, consumed only inside `veyra-text` and
   wrapped behind Veyra-owned result types (`ShapedRun`, `ShapedGlyph`)
   per dependency-graph §5. Default OpenType features (`kern`, `liga`)
   apply; `font-kerning`/`font-variant` control is a later batch.
2. **Adopt `unicode-bidi` 0.3 and `unicode-segmentation` 1.12+** for
   UAX#9/UAX#29 analysis. `veyra-text` exposes levels and visual runs
   (`bidi_runs`); the layout seam remains single-run this phase —
   consuming these runs is the bidi-aware layout increment (matrix row
   48's recorded delta), which is why the analysis ships now.
3. **Defer parley** — the reason is architectural, not quality: its
   value proposition is the very thing Veyra implements itself (inline
   layout). Revisit only if ADR-0003's custom-inline decision is
   revisited.
4. **Defer fontique/fontdb** (system collection + script fallback):
   `FontLibrary` loads explicit sources only — the **embedded default
   set** (six DejaVu faces compiled in via `include_bytes!`,
   `assets/fonts/`, Bitstream Vera license text included) plus
   `load_bytes`/`load_file`/`load_dir`. Face 0 is the universal
   fallback; generics map to the embedded families. Revisit when
   real-world font breadth (CJK/script fallback, user font dirs,
   webfonts) is the active batch — Phase 7 per the roadmap.
5. **`veyra-layout` realizes `LAYOUT --> TEXT`**: `ShapedMetrics`
   implements `TextMeasurer` over a shared `FontLibrary` (CSS Fonts §5.2
   family/style/weight matching subset, never fails — unmatched
   requests serve face 0), chunking runs above the shaping limit and
   degrading deterministically to `FixedMetrics` arithmetic if a shape
   call fails. `FixedMetrics` stays: golden-fixture provider and the
   degraded-mode fallback. The shell's `open --layout` uses the shaped
   provider built once per process.
6. **Bundled fonts are DejaVu (Sans/Serif/Mono × Regular/Bold)** —
   the classic browser-testing family (WPT/Servo heritage), permissive
   redistribution license with mandatory notices (`assets/fonts/
   LICENSE.txt`), upem 2048 with dyadic metric factors (1901/−483/0)
   so measurements at integer px sizes are exact in `f32`. The set has
   no italics and no CJK coverage — both recorded deltas, not defects:
   italic requests match the regular face (no synthesis at the metrics
   layer), CJK codepoints shape to `.notdef` until a CJK face is
   adopted.
7. **License hygiene**: all four adopted crates are Apache-2.0 OR MIT —
   already inside `deny.toml`'s allow list; no new license classes. No
   `build.rs`, no proc-macros, no unsafe, no network access at build
   or run time (the font bytes are in-repo). The dependency-audit
   document gains matching rows.

## Alternatives considered

- **parley now** — rejected (decision 3): overlaps the custom IFC;
  would make line breaking correct-by-construction in two places.
- **fontdb instead of hand-rolled collection** — deferred with
  fontique: an explicit-source library is ~200 lines and fully
  deterministic; adopting a collection crate buys nothing until system
  enumeration is actually the batch's subject.
- **swash** — rejected for this batch: scale-model rendering concerns
  (outline/glyph rasterization) belong to the paint batch; shaping is
  rustybuzz's role.
- **Fully hand-written shaper** — rejected: HarfBuzz-grade complex
  text (Arabic joining, mark positioning, GSUB/GPOS) is not a
  plausible in-scope build; the whole point of the isolation boundary
  is that this choice remains swappable.
- **No bundled fonts (CI-installed)** — rejected: conformance and
  golden claims must be hermetic; system font variance is exactly
  what the determinism contract forbids.

## Consequences

- `veyra-text` is a leaf-ish crate: external deps are
  rustybuzz/ttf-parser/unicode-bidi/unicode-segmentation only; no
  engine-crate imports. It owns the fuzz-target-class surface (font
  bytes and text are untrusted inputs; typed errors, no panics).
- The engineering-standards limits table gains the shaping-run row
  (`MAX_RUN_CHARS` = 2,000,000 scalars per `shape` call); the layout
  adapter chunks above it so measurement stays total.
- Matrix row 51 moves from *Investigated* to *Partially implemented*
  with the recorded deltas (no system discovery/fallback, no italics,
  no CJK coverage, no shaping cache, single-run layout).
- No shaping cache this batch: every measure call shapes its input
  (O(n)); the layout benchmark suite (Phase 3 acceptance criterion 4)
  is the revisit trigger if inline layout needs one.
- `open --layout` output numbers change from `FixedMetrics`
  approximations to real shaped advances (e.g. h1 32px line box
  37.25px = 2384/2048 × 32 from the face's hhea). No golden fixture
  depended on the old numbers — the fixtures pin `FixedMetrics`
  explicitly.

## Revisit conditions

- Shaping/perf pressure from the Phase 3 benchmark budgets → shaping
  cache design (LRU keyed by face/size/text-hash).
- Phase 7 vertical text / complex-script breadth → fontique-class
  collection + script itemization ADR (supersedes decision 4's scope).
- Webfonts (`@font-face`) — Phase 7: loads through `load_bytes` behind
  the net policy layer; no API change anticipated.

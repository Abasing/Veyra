# ADR 0003: Rendering architecture — display-list pipeline, cssparser/selectors-first CSS, taffy for flex/grid

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

The rendering research ([web-platform-and-rendering.md](../research/web-platform-and-rendering.md))
fixes the correctness requirements: incremental invalidation (real sites
mutate constantly), CSS paint order, logical-axis abstraction for writing
modes, and text shaping integration. The Rust ecosystem offers: the Servo
family (`cssparser` 0.38, `selectors` 0.41, `stylo` 0.22 — all MPL-2.0,
active [E1]), `taffy` 0.15 (MIT, flexbox+grid, active [E1]), the
Linebender text stack (`parley`/`fontique`, Apache-2.0 OR MIT [E1]), and
`html5ever` (MIT OR Apache-2.0) for parsing. `stylo` is the extracted
Gecko style engine — maximum CSS coverage, highest integration complexity,
and MPL-2.0 licensing (file-level copyleft requiring license hygiene).

Open risk [U]: no Rust crate covers spec-grade inline layout (CSS Text)
or block/inline formatting completely — custom work is required either
way; the spike at Phase 3 entry quantifies it.

## Decision

1. **Pipeline shape (fixed):** DOM → style → layout → display list →
   raster/composite, with dirty-bit protocols and the four rendering
   invariants (R1–R4) in
   [rendering-pipeline.md](../architecture/rendering-pipeline.md).
   Display lists — not direct painting — are the internal representation;
   they enable deterministic snapshot tests, damage tracking, and later
   GPU compositing.
2. **CSS parsing/matching, Phase 3:** build cascade + inheritance +
   computed values on `cssparser` + `selectors` (MPL-2.0 adoption
   accepted with file-level license isolation and NOTICE discipline).
   `stylo` adoption is evaluated by a dedicated spike at Phase 3 entry:
   if the integration surface is tractable, `stylo` replaces the
   hand-built cascade for Phase 7+ coverage; if not, the hand-built
   cascade grows per WPT area. The spike's findings amend this ADR.
3. **HTML parsing:** adopt `html5ever` behavior; implementation decision
   (adopt crate vs reimplement against html5lib-tests as oracle) made at
   Phase 2 with the same evidence bar.
4. **Flexbox/Grid:** `taffy` provides the algorithms behind a Veyra
   interface layer; block/inline layout and intrinsic sizing are custom.
5. **Text:** `parley`+`fontique`+`rustybuzz`+`unicode-bidi` in
   `veyra-text`.

## Alternatives considered

- **Adopt stylo immediately** — deferred, not rejected: highest leverage,
  but integration risk before the DOM/layout interfaces exist; the spike
  decides with evidence.
- **Direct painting (no display lists)** — rejected: forfeits snapshot
  testing and damage tracking; contradicts the incremental-correctness
  requirement.
- **WebRender (Servo)** — rejected for now [U]: tightly coupled to
  Gecko/Servo integration surfaces; revisit only if the GPU path stalls.
- **Custom flex/grid** — rejected: taffy is mature, MIT, and actively
  maintained; reimplementing it adds years for no correctness gain.
- **Full custom text stack** — rejected: the Linebender/rustybuzz stack
  is spec-aligned and maintained; custom work is reserved for the
  recorded gaps (inline layout, CSS Text).

## Consequences

- MPL-2.0 files enter the tree (cssparser/selectors, possibly stylo):
  license hygiene obligations (isolation, notices) are accepted and
  enforced in review; `deny.toml` gains MPL-2.0 at adoption time with the
  ADR referenced in its comment.
- Two layout codepaths (taffy flex/grid vs custom block/inline) must
  share the fragment/box model — the interface layer is the guarantee.
- Display-list snapshot tests become the primary paint-order defense.

## Revisit conditions

Phase 3 entry spike outcomes; taffy intrinsic-sizing interop failures on
the WPT subset; stylo spike success (then stylo adoption ADR amends this
one); a viable pure-Rust CSS engine appearing.

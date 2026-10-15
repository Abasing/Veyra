# ADR 0014: Phase 3 entry spike — stylo evaluation; hand-built cascade on cssparser/selectors proceeds

- **Status:** Accepted (amends ADR-0003 decision 2's spike clause)
- **Date:** 2026-10-16
- **Deciders:** Veyra foundation

## Context

ADR-0003 conditions Phase 3 on a dedicated spike: "`stylo` adoption is
evaluated by a dedicated spike at Phase 3 entry: if the integration
surface is tractable, `stylo` replaces the hand-built cascade for Phase
7+ coverage; if not, the hand-built cascade grows per WPT area." The
implementation plan makes the spike a hard prerequisite ("completed with
recorded findings before this phase starts"). This document records the
spike's findings and the resulting decision.

Method: static surface measurement of the published `stylo` 0.22.0
crate (crates.io, 2026-09-30, MPL-2.0, 406k downloads) — the required
DOM trait method counts, the servo-backend reference glue (the code
Servo itself maintains to host stylo), and the resolved dependency
tree (`cargo tree` on a probe project, 156 unique packages) — against
Veyra's Phase 3 deliverables (WPT selectors/syntax subsets, cascade +
computed values, incremental R1 tests, golden-image layout fixtures).

## Findings

1. **DOM trait surface (what every embedding must implement).**
   `dom.rs` requires `TDocument` (5 methods), `TNode` (20),
   `TElement` (78), `TShadowRoot` (6) — **109 required methods**, with
   trait bounds demanding `Copy` element handles, `Send + Sync`,
   `Eq + Hash`, and stylo's `NodeInfo`. Veyra's arena `NodeId`
   satisfies the handle bounds; the *method* contracts (element state,
   attributes, classes, first-child/prev-sibling fast paths, author
   shadow trees, style sharing hints) reach deep into `veyra-dom`'s
   internals.
2. **Backend glue (what Servo itself maintains to host stylo).** The
   `servo/` reference backend adds `selector_parser.rs` (75 methods:
   pseudo-element/pseudo-class grammar, namespaces, selector parser
   implementation), `attr.rs` (41: attribute value coercions),
   `animation.rs` (53), `media_features.rs` (18), `restyle_damage.rs`
   (9), `shadow_parts.rs` (8), `encoding_support.rs` (5) — **~209
   further integration points**. Total measured surface: **~318
   methods across ~12 integration surfaces** before a single computed
   style exists in Veyra types.
3. **Protocol coupling, not just code volume.** Stylo's incremental
   model requires its snapshot/invalidation protocol (explicit
   `NoteDirtyDescendants`/snapshot bookkeeping around every mutation)
   to be threaded through `veyra-dom`'s mutation algorithms; stylesheet
   loading, font metrics, media-feature evaluation, and quirks-mode
   callbacks are mandatory provider hooks. R1 (incremental correctness)
   would be proven against stylo's protocol rather than Veyra's own
   dirty-bit design (rendering-pipeline.md).
4. **Dependency footprint.** The probe locked **137 packages** for the
   probe alone (156 unique in the full tree), pulling the Servo support
   stack (`stylo_dom`, `stylo_traits`, `stylo_atoms`, `servo_arc`,
   `string_cache`, `to_shmem`, `malloc_size_of`/servo) on top of
   `cssparser`/`selectors` — i.e., adopting stylo does not replace the
   cssparser/selectors adoption, it adds to it. (The probe's `cargo
   check` was not completed in the development sandbox due to disk
   pressure; the recorded evidence is the manifest/tree analysis, and
   no compile-pass claim is made.)
5. **Coverage leverage is a Phase 7+ concern.** ADR-0003 already
   scopes Phase 3 to "pixels for static pages" with a documented
   inline-layout subset; stylo's advantage is breadth (full property
   system), which the Phase 3 acceptance criteria (selectors subset
   pass-rate, golden images, R1 tests, taffy interop) do not require.

## Decision

1. **The hand-built cascade proceeds**: `veyra-style` is built on
   `cssparser` 0.38 + `selectors` 0.41 (MPL-2.0, adoption already
   accepted by ADR-0003 with file-level license isolation), growing per
   WPT area. This is the ADR-0003 default branch, taken on measured
   evidence.
2. **stylo remains deferred to Phase 7+** with this surface
   measurement (~318 methods / 12 surfaces / 137-package probe lock)
   as the quantitative baseline any future adoption spike must beat;
   the revisit conditions of ADR-0003 are unchanged.
3. **deny.toml gains `MPL-2.0` now** (cssparser/selectors enter the
   tree in Phase 3), citing this ADR and ADR-0003; the
   dependency-audit document gains matching rows with crates.io-verified
   metadata.
4. The spike is recorded as **completed** — the implementation plan's
   Phase 3 prerequisite is satisfied by this document.

## Alternatives considered

- **Adopt stylo now** — rejected on the measured evidence: ~318
  integration methods plus protocol coupling into `veyra-dom` exceeds
  the phase's scope, and its dependency stack duplicates what Phase 3
  adopts anyway (cssparser/selectors).
- **Hand-build the parser too (no cssparser/selectors)** — rejected:
  ADR-0003 already accepted the MPL-2.0 adoption for parsing/matching;
  reimplementing CSS tokenization and selector matching adds years for
  no license or correctness gain.

## Consequences

- `veyra-style`'s public surface stays Veyra-typed (dependency-graph
  invariant 5): cssparser/selectors types are consumed inside the
  crate and converted at the boundary.
- MPL-2.0 hygiene obligations (file-level isolation, NOTICE
  discipline) are active from Phase 3; `deny.toml`'s header comment
  records the gate change.
- The Phase 3 status note must cite this ADR as the recorded spike
  findings; the compatibility matrix's CSS rows move from
  "Investigated" with this evidence.

## Revisit conditions

- stylo adoption at Phase 7+ (unchanged from ADR-0003), now with this
  measurement as the baseline.
- If WPT selector-subset pass rates stall on matching semantics that
  `selectors` 0.41 cannot express, re-open the parsing/matching choice
  (not the cascade ownership).

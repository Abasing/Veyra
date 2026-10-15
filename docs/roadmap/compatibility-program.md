# Standards-First Compatibility Program

status: Defined (Phase 0) | last-updated: 2026-10-15 | companion: matrix below, ../research/testing-and-performance.md

Veyra's compatibility claims come from specifications and test suites —
never from demonstration pages. This document defines the program; the
[compatibility matrix](compatibility-matrix.md) is its output surface.

## 1. Test-suite integration plan

| Suite | License & attribution | Integration path | Phase |
|---|---|---|---|
| Web Platform Tests (WPT) | W3C 3-clause BSD + W3C Test Suite License (dual; vendored license texts mandatory) | Bootstrap: run testharness.js inside Veyra's JS engine (Phase 4–5). Graduate: wptrunner product integration (evaluation recorded Phase 5). Reftests via the screenshot harness (Phase 5+). | 4–5 |
| Test262 | BSD-style + W3C-style dual per repo (license files vendored on import) | Custom Rust runner over the JS engine facade; `$262` host object; results JSON tracked per suite revision | 4 |
| WebAssembly spec suite | WASM spec license (vendored) | Run against `veyra-wasm` runtime; results per runtime version | 8 |
| html5lib-tests | MIT [E2] | Tree-construction corpus for `veyra-html` | 2 |
| CSS snapshots (csswg drafts' test dependencies) | Per-file headers (reviewed at import) | Only after license review; WPT covers most CSS areas first | 7 |
| Security regression suites | WPT webappsec directories + custom | Policy-layer tests in `veyra-net` | 5–6 |
| Real-world site corpus | Not redistributed (URLs + local fixtures only) | Periodic rendered diffs, triaged | 5+ |

**Import rules:** test assets are vendored via pinned submodule or
checkpointed snapshot with (a) license files, (b) upstream revision
record, (c) NOTICE entries; no test asset is committed without its
license text. The matrix cites suite revisions alongside pass rates so
numbers are reproducible.

## 2. Matrix states (the only allowed status values)

1. **Not investigated** — nothing beyond this program's existence.
2. **Investigated** — research pass done; documented in `docs/research/`.
3. **Designed** — architecture exists (ADR/architecture doc citation).
4. **In development** — active implementation, no conformance evidence yet.
5. **Partially implemented** — some behavior with recorded limits.
6. **Tested with known failures** — suite-tracked with counted failures.
7. **Meets defined acceptance criteria** — the phase's AC pass with
   evidence links.

Rules: status transitions cite evidence (suite run, commit, document);
a standard is never marked supported because a few basic tests pass —
state 7 requires the phase's documented AC, and the matrix shows the
per-directory pass rates behind it.

## 3. Failure lifecycle (how a failing test becomes a fix)

1. **Reproduce** — the failure runs in the deterministic harness
   (fixtures server, injected clock); flakes are reproduced before
   triage.
2. **Minimize** — reduce to a fixture in `tests/fixtures/` with the spec
   citation (section reference) and the failing assertion.
3. **Triage** — classify: engine bug / spec-ambiguity / interop-quirk
   ([ST40] class) / infra. Spec-ambiguities get a research note, not a
   hack.
4. **Prioritize** — matrix-area priority (roadmap phase) + breadth
   (how many tracked tests the root cause blocks).
5. **Fix + prevent** — the fix lands with the minimized case as a
   permanent regression test; the matrix entry's failure count drops
   with the next recorded run.
6. **Review** — recurring failure areas adjust roadmap priorities at
   phase boundaries (documented in the phase's exit review).

## 4. Honest-claim rules

- Pass rates are reported with: suite revision, runner, date, and the
  exact directory/file set — no aggregate "94% compatible" claims.
- Partial support is labeled partial; "Not investigated" is an
  acceptable, expected state for most of the platform for a long time.
- No compatibility statement appears outside the matrix (marketing
  surfaces link to it).
- Interop behavior (non-spec'd) is implemented with a documented
  interoperability note citing observed behavior [ST40] — never
  invented.

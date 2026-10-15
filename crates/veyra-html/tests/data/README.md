# Vendored test corpora — `veyra-html`

## `html5lib-tree-construction/*.dat`

- **Source:** html5lib/html5lib-tests, `tree-construction/`
  (https://github.com/html5lib/html5lib-tests). Upstream moved the suite
  to web-platform-tests (see their README); the vendored revision is the
  last maintained state: commit `9329e64694e7`, 2026-06-22, which includes
  the final adoption-agency and `selectedcontent` adjustments.
- **License:** MIT (upstream `LICENSE` file vendored alongside).
- **Files:** tests1.dat, tests4.dat, tests5.dat, adoption01.dat,
  adoption02.dat, inbody01.dat, template.dat, webkit02.dat (324 cases).
- **Purpose:** WHATWG HTML tree-construction conformance for `veyra-html`
  (roadmap Phase 2, acceptance criterion 1: pass counts recorded in the
  compatibility matrix).
- **Runner:** `tests/html5lib_tree.rs`. Error-list assertions are
  intentionally not part of the claim (the tree shape is the conformance
  surface; parse errors are counted only). Excluded cases must be listed
  in `deltas.md` — the runner's `DELTAS` table is the only skip mechanism.

## `deltas.md`

One section per excluded case group with the reason and the retirement
path. Current deltas: `<selectedcontent>` option cloning (4 cases).

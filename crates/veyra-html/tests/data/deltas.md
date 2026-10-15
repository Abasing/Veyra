# Documented html5lib tree-construction deltas

Corpus: `html5lib-tree-construction/*.dat` (provenance and revision in
`README.md`), 324 cases across 8 vendored files.

Result at Phase 2 adoption (html5ever 0.40.1): **320/324 cases pass
(98.8%)**; 4 documented deltas, all one cause:

## 1. `<selectedcontent>` option cloning — webkit02.dat #45–#48

The `maybe_clone_an_option_into_selectedcontent` tree-builder callback
(selectedcontent element, added to the HTML standard in 2025) is left
unimplemented: the sink's default is a no-op, and the standard notes that
leaving it unimplemented "will result in a (slightly) incorrect DOM tree"
— the option's text is not cloned into the `<selectedcontent>` element.
No engine behavior outside `<select><selectedcontent>` interaction depends
on it; the cloning path (which needs subtree cloning in `veyra-dom`)
arrives with the form-controls work and the delta is retired then.

Skips are keyed by `(file, case index)` in the runner's `DELTAS` table —
the only skip mechanism. A delta that stops reproducing is a hard failure
so this list shrinks as the engine improves.

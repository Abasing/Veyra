# Documented css-parsing-tests deltas

Corpora: `css-parsing-tests` as pinned by cssparser 0.38.0 (provenance in
`README.md`), vendored 2026-10-16. This file documents every corpus entry
the engine does not satisfy, as required by the runner
(`src/corpus.rs` `DOCUMENTED_DELTAS` — the only skip mechanism, keyed by
corpus file stem + input).

Result with `cssparser` 0.38 + `selectors` 0.41 at adoption time:

| Corpus | Cases | Strict pass | Documented deltas |
|---|---|---|---|
| `component_value_list` (round-trip) | 44 | 44 | 0 |
| `one_component_value` | 10 | 10 | 0 |
| `declaration_list` | 9 | 9 | 0 |
| `one_declaration` | 22 | 22 | 0 |
| `An+B` | 128 | 128 | 0 |
| `one_rule` | 14 | 11 | 3 |
| `rule_list` | 16 | 9 | 7 |
| `stylesheet` | 18 | 10 | 8 |

The rule/stylesheet deltas fall into two categories. Neither is a
parse-structure divergence: the surviving rule lists, dropped-rule
positions, declaration parsing, and `!important` handling match the
corpus; the differences are in *token layout reporting* (category 1) and
*prelude validity enforcement* (category 2).

## 1. Canonical layout regeneration (3 + 3 + 3 entries)

The corpus's reference mapping reports each rule body/prelude as the
**raw token sequence** (whitespace tokens and comment positions
preserved exactly as lexed). Veyra parses declarations into a typed
model and re-serializes canonically: runs of whitespace collapse to one
space, leading/trailing whitespace at boundaries is not regenerated, and
comments are dropped. The corpus cases below compare a raw-layout
expectation against a canonical-layout rendering, so they differ by
those whitespace/comment positions only. The token *structure*
(tokens and their order, modulo whitespace runs) is identical.

`/**/` in the inputs below marks comments (dropped by both sides).

- `one_rule` + `rule_list` + `stylesheet`: `" /**/ div > p { color: #aaa;  } /**/ "`
  (leading comment-whitespace run, trailing double space before `}`)
- `rule_list` + `stylesheet`: `"div { color: #aaa; } p{}"` (trailing
  double space in the first rule's block)
- `rule_list` + `stylesheet`: `"div {} -->"` (trailing space in the
  empty rule's prelude)

Retirement path: retain raw rule-block token streams alongside the
parsed model (the cascade batch needs neither; a CSSOM-text round-trip
work item would).

## 2. Selector-validated preludes vs. reported shapes (2 + 4 + 5 entries)

The corpus's generic block-contents algorithm reports qualified-rule
*shapes* — including preludes that are empty or contain CDO/CDC tokens.
Veyra's engine validates style-rule preludes as selector lists
(Selectors 4): an empty prelude or a prelude containing CDO/CDC is not a
valid selector list, so the rule is dropped (the same behavior browsers
apply; `div{} -->` at rule level survives as the corpus's error entry
where the grammar also fails). The corpus cases below expect the shape
reported where Veyra drops the rule.

- `" /**/ { color: #aaa  "` — empty prelude (`one_rule`, `rule_list`,
  `stylesheet`)
- `"{}a"`, `"{}@a"` — empty-prelude shape (`rule_list`, `stylesheet`)
- `" /* CDO/CDC are not special */ <!-- --> {"` — CDO/CDC prelude
  (`one_rule`, `rule_list`)
- `" <!-- --> a<!---->{"` — CDO/CDC after the type selector
  (`stylesheet`)

Retirement path: none — this is the intended engine behavior (Selectors
4 §error-handling). The entries stay documented so a future corpus
refresh that reshapes them is caught.

## Notes

- `@charset` handling matches the corpus after the rule-adapter
  special-case (never a real rule; consumed only as the first
  stylesheet rule by the stylesheet driver).
- A dependency bump that resolves any entry shrinks this file and the
  `DOCUMENTED_DELTAS` table; the runner fails loudly on both
  undocumented failures and resolved-but-still-documented entries.

# Vendored conformance corpus: css-parsing-tests

- **Source**: https://github.com/SimonSapin/css-parsing-tests
- **Revision**: commit `203ce36bffd617db7f118c551e32794561fb273d` (2025-09-24)
- **License**: public domain (dedicated by the author, see the upstream
  `LICENSE`; Veyra treats it as CC0-1.0, which is on the cargo-deny
  allow-list)
- **Fetched**: 2026-10-16

## Files

| File | Tests | Consuming test |
|---|---|---|
| `component_value_list.json` | tokenization + component-value parsing | `src/corpus.rs::corpus_component_value_list` (round-trip serialization) |
| `one_component_value.json` | single component value | `src/corpus.rs::corpus_one_component_value` |
| `declaration_list.json` | declaration-list parsing and error recovery | `src/corpus.rs::corpus_declaration_list` |
| `one_declaration.json` | single declaration | `src/corpus.rs::corpus_one_declaration` |
| `rule_list.json` | rule-list parsing (block contents) | `src/corpus.rs::corpus_rule_list` |
| `stylesheet.json` | full stylesheet parsing (incl. `@charset`) | `src/corpus.rs::corpus_stylesheet` |
| `one_rule.json` | single rule (CSSOM `insertRule` shape) | `src/corpus.rs::corpus_one_rule` |
| `An+B.json` | `An+B` microgrammar (nth-* arguments) | `src/corpus.rs::corpus_anb` |

## Refresh policy

Vendoring (rather than fetching at test time) keeps CI hermetic and the
conformance claim reproducible at a pinned revision, per ADR-0013's
corpus policy (extended to Phase 3 by ADR-0014). Refreshes are
deliberate commits that must shrink or explain
[`deltas.md`](deltas.md).

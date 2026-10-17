# Architecture Decision Log

Live index of architectural decision records. ADRs are immutable once
accepted — changes happen as new ADRs that supersede them. Status values:
Proposed | Accepted | Superseded(by NNNN) | Rejected.

| ADR | Title | Status | Date |
|---|---|---|---|
| [0001](../decisions/0001-rust-first-language-policy.md) | Rust-first language and dependency policy | Accepted | 2026-10-15 |
| [0002](../decisions/0002-javascript-engine-strategy.md) | JavaScript engine strategy: embed Boa behind a facade | Accepted | 2026-10-15 |
| [0003](../decisions/0003-rendering-architecture.md) | Rendering architecture: display-list pipeline, cssparser/selectors-first CSS, taffy for flex/grid | Accepted | 2026-10-15 |
| [0004](../decisions/0004-process-model.md) | Process model: single-process start with capability seams, out-of-process content at Phase 6 | Accepted | 2026-10-15 |
| [0005](../decisions/0005-graphics-abstraction.md) | Graphics abstraction: CPU-first raster, wgpu for GPU path | Accepted | 2026-10-15 |
| [0006](../decisions/0006-frontend-and-extension-strategy.md) | Frontend integration and staged extension strategy | Accepted | 2026-10-15 |
| [0007](../decisions/0007-native-dependency-policy.md) | Native (non-Rust) dependency policy | Accepted | 2026-10-15 |
| [0008](../decisions/0008-networking-stack.md) | Networking stack: policy layer over hyper/rustls/hickory | Accepted | 2026-10-15 |
| [0009](../decisions/0009-webassembly-runtime.md) | WebAssembly runtime: wasmi first, wasmtime when JIT is justified | Accepted | 2026-10-15 |
| [0010](../decisions/0010-storage-engine.md) | Storage engine: redb-based origin/partition stores | Accepted | 2026-10-15 |
| [0011](../decisions/0011-sandbox-technology.md) | Sandboxing technology: Linux-first, Windows/macOS as recorded gaps | Accepted | 2026-10-15 |
| [0012](../decisions/0012-phase1-shell-dependencies.md) | Phase 1 shell and test-infrastructure dependencies (winit, softbuffer, png, sha2, tracing, serde/toml, criterion) | Accepted | 2026-10-15 |
| [0013](../decisions/0013-phase2-document-pipeline-dependencies.md) | Phase 2 document-pipeline dependencies (url, hyper/rustls/hickory, html5ever, webpki-roots) | Accepted | 2026-10-15 |
| [0014](../decisions/0014-phase3-entry-spike-stylo.md) | Phase 3 entry spike: stylo evaluation — hand-built cascade on cssparser/selectors proceeds | Accepted | 2026-10-16 |
| [0015](../decisions/0015-text-stack-rustybuzz.md) | Text stack: rustybuzz/ttf-parser/unicode-bidi behind veyra-text; parley/fontique deferred | Accepted (decision 1 superseded by 0016) | 2026-10-17 |
| [0016](../decisions/0016-text-stack-migration-harfrust.md) | Text stack migration: harfrust 0.14 + read-fonts 0.45 replace rustybuzz/ttf-parser (RUSTSEC-2026-0192); path deps versioned | Accepted | 2026-10-17 |

## Index

The table above is the complete, ordered index. Column meanings:
Status is the ADR's own declared status; the log is regenerated manually
with each ADR addition (veyra-doctor verifies every ADR file is listed).

## ADR process

1. Copy the template in [README of decisions](../decisions/README.md) →
   `NNNN-kebab-title.md` (sequential, zero-padded).
2. Fill Context (with evidence links into `../research/`), Decision,
   Alternatives, Consequences, Revisit conditions.
3. Add the row to the table above. CI (`veyra-doctor`) verifies structure
   and that every ADR is indexed here.

## Status values

Proposed (under discussion) | Accepted (load-bearing; may be cited as
evidence) | Superseded by NNNN (kept for history) | Rejected (kept for
the alternatives record).

## Superseding and amending

Decisions are never edited in place. A new ADR that supersedes an old
one cites the invalidating evidence (research document + source-register
entry), states what changed, and the old ADR's status line is updated to
`Superseded by NNNN` — the only permitted edit to an accepted ADR.

## Revisit rules

An ADR states the conditions under which it is revisited (metrics, dates,
or external events). When evidence invalidates a decision, the new ADR
cites the evidence (research doc + register entry) and the old one is
marked Superseded — the log is history, not a wiki.

# Architecture Decision Log

Live index of architectural decision records. ADRs are immutable once
accepted — changes happen as new ADRs that supersede them. Status values:
Proposed | Accepted | Superseded(by NNNN) | Rejected.

| ADR | Title | Status | Date |
|---|---|---|---|
| [0001](0001-rust-first-language-policy.md) | Rust-first language and dependency policy | Accepted | 2026-10-15 |
| [0002](0002-javascript-engine-strategy.md) | JavaScript engine strategy: embed Boa behind a facade | Accepted | 2026-10-15 |
| [0003](0003-rendering-architecture.md) | Rendering architecture: display-list pipeline, cssparser/selectors-first CSS, taffy for flex/grid | Accepted | 2026-10-15 |
| [0004](0004-process-model.md) | Process model: single-process start with capability seams, out-of-process content at Phase 6 | Accepted | 2026-10-15 |
| [0005](0005-graphics-abstraction.md) | Graphics abstraction: CPU-first raster, wgpu for GPU path | Accepted | 2026-10-15 |
| [0006](0006-frontend-and-extension-strategy.md) | Frontend integration and staged extension strategy | Accepted | 2026-10-15 |
| [0007](0007-native-dependency-policy.md) | Native (non-Rust) dependency policy | Accepted | 2026-10-15 |
| [0008](0008-networking-stack.md) | Networking stack: policy layer over hyper/rustls/hickory | Accepted | 2026-10-15 |
| [0009](0009-webassembly-runtime.md) | WebAssembly runtime: wasmi first, wasmtime when JIT is justified | Accepted | 2026-10-15 |
| [0010](0010-storage-engine.md) | Storage engine: redb-based origin/partition stores | Accepted | 2026-10-15 |
| [0011](0011-sandbox-technology.md) | Sandboxing technology: Linux-first, Windows/macOS as recorded gaps | Accepted | 2026-10-15 |

## ADR process

1. Copy the template in [README of decisions](README.md) →
   `NNNN-kebab-title.md` (sequential, zero-padded).
2. Fill Context (with evidence links into `../research/`), Decision,
   Alternatives, Consequences, Revisit conditions.
3. Add the row to the table above. CI (`veyra-doctor`) verifies structure
   and that every ADR is indexed here.

## Revisit rules

An ADR states the conditions under which it is revisited (metrics, dates,
or external events). When evidence invalidates a decision, the new ADR
cites the evidence (research doc + register entry) and the old one is
marked Superseded — the log is history, not a wiki.

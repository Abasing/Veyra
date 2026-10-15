# ADR 0010: Storage engine — redb-based origin/partition stores

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Veyra needs per-profile, per-origin(-partitioned) persistent stores:
cookies, localStorage/sessionStorage, IndexedDB, Cache API, history/
bookmarks, downloads metadata. The language policy (ADR-0001) rejects
SQLite (C core) for engine stores. Rust candidates [E1, audit §7]:
`redb` 4.4 (MIT OR Apache-2.0, ACID B-tree, active — updated
2026-10-13), `fjall` 3.1 (LSM, active), `sled` 1.0.0-alpha (unreleased
1.0, stale since 2024-10). Chromium's semantics that matter (dossier
networking §6–8): origin/partition keying, quotas/eviction, crash-safe
persistence, schema migration, corruption recovery.

## Decision

1. **`redb` is the embedded store** for profile databases; `veyra-storage`
   layers: origin/partition keying, schema versioning + migrations,
   quota accounting, corruption quarantine (rename-aside + recreate),
   and crash-safe non-DB writes (temp+rename) for sessions/downloads.
2. **LSM (`fjall`) is reserved** for write-heavy cache-like stores (HTTP
   cache metadata candidate) — adopted per-store if benchmarks justify.
3. **IndexedDB semantics** (transactions, indexes, structured-clone
   values) are a `veyra-storage` module over the same base — WPT
   indexeddb suites are the bar.
4. **Private browsing** = in-memory store instances with the same API —
   no persistence path by construction.

## Alternatives considered

- **SQLite via rusqlite** — rejected for engine stores (C core, ADR-0001
  blanket prohibition list); revisable only via ADR-0007 exception
  process. Honest note: SQLite's maturity is unmatched; the trade-off is
  accepted deliberately, and per-store performance gaps are a risk the
  benchmarks watch.
- **sled** — rejected: unreleased 1.0 after years, stale [E1].
- **Custom storage engine** — rejected: this is infrastructure, not
  differentiation; correctness (ACID, crash safety) is exactly what a
  new engine should not hand-roll.

## Consequences

- redb's concurrency model (single-writer) shapes storage APIs;
  synchronous browser-service semantics (LocalStorage) are served through
  an internal async facade.
- Per-store size/performance envelopes need benchmarking before IndexedDB
  volume work (Phase 6 criteria).

## Revisit conditions

redb maintenance stall; workload benchmarks showing structural
limitations (e.g., large-value handling); a pure-Rust SQLite-compatible
engine emerging (would reduce reimplementation pressure for IndexedDB
internals).

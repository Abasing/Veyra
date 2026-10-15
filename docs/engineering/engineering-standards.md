# Veyra Engineering Standards

status: Defined (Phase 0) | last-updated: 2026-10-15 | enforced-by: CI, review, workspace lints

Practical policy, not ceremony: each rule states its reasoning and its
exception path. Rules scale with the codebase — what is mandatory for a
parser crate is not mandatory for a docs-only change.

## 1. Formatting and linting

- `rustfmt` (edition 2024, width 100) is the only accepted format;
  `cargo fmt --check` gates CI. No style debates in review.
- Clippy: `-D warnings` on the workspace; `unwrap_used`, `expect_used`,
  `panic` are **denied** everywhere (a browser must not crash on
  malformed input), `unsafe_code` is **forbidden** (see §4). Additional
  pedantic lints may be adopted per-crate, never blanket.
- Python: `ruff` (config in `pyproject.toml`); pytest for tests.
- Markdown: `veyra-doctor` validates structure and claim-safety (§9).

## 2. Error handling and panic policy

- Typed errors per crate (`thiserror`-style definitions; no stringly
  errors at boundaries). Public APIs document error variants.
- **No panics on untrusted paths.** Any code that can observe
  attacker-controlled input must return errors; this is why
  `unwrap/expect/panic` are workspace-denied rather than style advice.
- Internal invariant violations: return `Err` or log-and-degrade; a
  panic is acceptable only where the input is provably internal
  (documented at the call site) — and never in `veyra-html`, CSS/URL/
  protocol parsing, decoders, or IPC message handling.
- Rationale: denial-of-service via panic is a browser bug class; the
  lint converts the class into a compile-time review artifact.

## 3. Dependency selection and auditing

- New dependency checklist: pure Rust (or ADR-0007 exception),
  license in `deny.toml` allow-list, maintained (activity + issue
  responsiveness), no build-time network, no unnecessary transitive
  weight, and a stated removal strategy.
- `cargo-deny` (licenses, advisories, bans, sources) gates CI;
  advisories are triaged on the scheduled run.
- The workspace begins with **zero external dependencies** in tooling;
  engine crates add the audit-adopted dependencies per phase.
- Dependency bumps: routine (patch) via dependabot PRs; minor/major
  bumps get a review note in the PR.

## 4. Unsafe Rust review

- `unsafe_code = "forbid"` workspace-wide. There is currently no unsafe
  code and no planned unsafe code.
- If an ADR ever justifies unsafe (expected candidates: FFI to OS
  sandbox APIs), it is crate-local (`#![forbid(unsafe_code)]` lifted
  there only), every block carries a `// SAFETY:` argument, and the PR
  requires a security review per [SECURITY.md](../../SECURITY.md).
- Miri runs in CI for any crate that introduces unsafe (added then).

## 5. Thread safety and synchronization

- Engine core (dom/style/layout/paint) is single-threaded by design —
  the event loop owns it (determinism; data-race freedom by
  construction). Parallelism enters via explicit, measured work
  partitioning (style/layout parallelism) behind stable interfaces.
- Services (net/storage/ipc/shell) use tokio; shared state is
  message-passing-first; `Mutex`/`RwLock` require a comment stating the
  lock hierarchy; no lock across `await` without justification.
- `Send`/`Sync` boundary crossings into the content layer are
  forbidden (content is owned by the event loop; this is what makes
  Phase 6's process split clean).

## 6. Resource limits and denial-of-service resistance

- The **limits table** (below) is normative; each limit has an owning
  crate and tests. Limits are configuration, defaulted conservatively.

| Surface | Limit (initial) | Owner |
|---|---|---|
| Document size | 64 MiB | veyra-html |
| Single attribute/text node | 8 MiB | veyra-html |
| DOM node count | 2,000,000 | veyra-dom |
| Nesting depth (elements) | 512 | veyra-html |
| Stylesheet size | 16 MiB | veyra-style |
| Response body (unbounded streams aside) | 2 GiB streamed, memory-capped buffers | veyra-net |
| Redirect chain | 20 | veyra-net |
| Header count/size | 128 / 64 KiB | veyra-net |
| Timer count per page | 10,000 | veyra-executor |
| IPC message size | 64 MiB | veyra-ipc |
| Storage quota | spec/phase-defined | veyra-storage |

- Parse budgets (time-sliced) apply to untrusted parsing once engines
  exist; fuzzers assert "no hang" with run budgets.

## 7. Security-sensitive API review

- Any API crossing a trust boundary (capability services, IPC,
  origin-tagged requests) requires: threat-model citation (which
  invariant it serves), validation notes, and a test proving rejection
  of malformed use. The [process and security model](../architecture/process-and-security-model.md)
  invariants I1–I10 are the checklist.
- Security review is a PR label; reviewers with security context
  self-assign. security-sensitive PRs cannot merge self-approved.

## 8. Testing expectations by subsystem

| Subsystem | Expectation |
|---|---|
| Parsers (html/css/url/protocol/decoders) | Unit + conformance suites + fuzz target (CI smoke budget) + limits tests |
| DOM/logic crates | Unit tests; property tests for mutation/invalidation |
| Services (net/storage/ipc) | Integration tests against local fixtures; crash-safety fault injection where persistence exists |
| Rendering | Golden images + display-list snapshots + mutation-sequence tests |
| JS bindings | Spec-derived unit tests; Test262 tracked runs |
| Tooling | Unit tests (doctor has them; link checker has them) |

Coverage: no numeric coverage mandate (gaming metric incentives);
review judges whether failure paths are tested. Determinism: all tests
inject time/RNG; no wall-clock sleeps in engine tests.

## 9. Documentation and claims

- Public API docs on every crate (`missing_docs = warn` workspace lint);
  architecture docs updated with behavior changes.
- Research claims cite the source register with access dates; the
  forbidden-claim scan (veyra-doctor) blocks marketing language of
  universal compatibility or readiness (the canonical forbidden list
  lives in the veyra-doctor source) anywhere in the repository.
- Compatibility statements live only in the matrix.

## 10. Logging, diagnostics, and privacy

- `tracing` with span taxonomy per subsystem; levels: error/warn/info
  default, debug/trace opt-in.
- **No page content, URLs of browsing sessions, cookies, credentials,
  or storage values in default logs.** Redaction helpers exist before
  content crates log anything.
- Crash diagnostics: local-first; anything leaving the machine is an
  opt-in product decision (Phase 10).

## 11. Benchmark methodology

- Criterion micro-benchmarks per hot path; scenario benchmarks via the
  shell harness (startup, navigate, layout mutation storms).
- Protocol: fixed runner class, N=10 runs, median + spread reported,
  outlier trimming documented; baselines recorded in dated benchmark
  notes; regression budgets derived from baselines (±10% default,
  adjustable with justification).
- Scheduled (weekly) benchmark runs compare against the baseline note;
  regressions open issues automatically (filed by workflow, triaged by
  owner).

## 12. Architectural decision records

ADR triggers (non-exhaustive): new crate boundary; new external or
native dependency; changes to trust boundaries/IPC; JS/WASM engine
changes; storage engine changes; changes to the process model; public
API shape of core crates (dom/net/script). Process and template:
[../decisions/README.md](../decisions/README.md).

## 13. Release and versioning policy

- Workspace versioning: crates version together (0.x during
  foundation; 1.0 only when a phase's compatibility claims justify
  it — the matrix is the gate).
- Releases: tagged from `develop`-cut release branches; changelog from
  commit history; signed artifacts (Phase 10+ per roadmap);
  SBOM (cargo-deny generated) attached from Phase 11.
- Pre-1.0 breaking changes are expected and documented in changelogs;
  no stability promises before Phase 11's release engineering.

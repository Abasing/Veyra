# Contributing to Veyra

Thank you for your interest in contributing. Veyra is a long-term engineering
program: correctness, honesty of claims, and maintainability matter more than
speed or volume. This document describes the workflow, the local commands, and
what CI enforces.

## Ground rules

- **All development happens on the `develop` branch.** PRs target `develop`.
  `main` reflects the project's release/announcement history and is not a
  working branch. Never force-push, rewrite remote history, or delete branches.
- **No Chromium code.** Studying Chromium's public design docs, source
  organization, and observable behavior is encouraged; copying source code into
  Veyra is forbidden. Every dependency must pass the license and language rules
  in [`docs/research/dependency-and-license-audit.md`](docs/research/dependency-and-license-audit.md).
- **No unimplemented-feature marketing.** Documentation must not claim
  capabilities the repository does not demonstrate with evidence. The
  compatibility matrix is the single source of truth for what exists.
- **Consequential choices get an ADR.** If a change adds a dependency, alters
  a subsystem boundary, introduces native code, or revises the process/security
  model, add or amend a decision record in `docs/decisions/` *before* merging.
  Triggers are listed in
  [`docs/engineering/engineering-standards.md`](docs/engineering/engineering-standards.md#12-architectural-decision-records).

## Workflow

1. Sync `develop` and create a feature branch:
   ```sh
   git checkout develop && git pull --ff-only
   git checkout -b feat/<short-topic>
   ```
2. Make the change. Keep commits coherent and reviewable; write messages that
   explain *why* (`feat:`, `fix:`, `docs:`, `tools:`, `ci:`, `chore:` prefixes
   are conventional, not mandatory).
3. Run the full local check suite (below) before pushing.
4. Open a PR against `develop`. Use the PR template; fill the evidence and
   security checklists honestly — reviewers reject unverifiable claims.
5. Address review; re-run checks; merge after approval. Squash or rebase-merge
   only via GitHub's merge tools (never history rewriting on `develop`).

## Local checks

| Command | Purpose |
|---|---|
| `cargo fmt --all -- --check` | Formatting gate (CI fails on diffs) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint gate; `unwrap`/`expect`/`panic` are denied in workspace code |
| `cargo test --workspace` | Unit tests, including `veyra-doctor` self-tests |
| `cargo run -p veyra-doctor` | Repository invariants: required docs, forbidden-claim scan, ADR/roadmap structure, source-register dating, action SHA pinning |
| `python3 -m pytest tests/ -q` | Tests for Python tooling |
| `python3 -m ruff check tools/ tests/` | Python lint |
| `python3 tools/verify_sources.py --fail-on-required` | Source-register reachability (network; run before touching `docs/research/`) |
| `python3 tools/check_external_links.py docs/ --warn-only` | External-link hygiene (network; advisory) |
| `cargo bench -p veyra-shell` | Criterion benchmarks (Phase 1 baselines; results in `docs/benchmarks/`) |

Window smoke tests need a display server; CI runs them under `xvfb-run`.
Locally, either use a desktop session or:

```sh
xvfb-run -a cargo test -p veyra-shell --test window_smoke
```

The screenshot/golden/CLI tests never require a display. Regenerating
golden fixtures (deliberate act, review the diff):

```sh
VEYRA_UPDATE_GOLDEN=1 cargo test -p veyra-shell --test golden \
    regenerates_fixtures -- --exact --ignored --nocapture
```

Rust toolchain: pinned by `rust-toolchain.toml`; `rustup` selects it
automatically. Formatting: `rustfmt.toml` (edition 2024, width 100).
Python: configured in `pyproject.toml` (ruff + pytest).

## CI workflows

### `ci.yml` — every push to `develop` and every PR targeting `develop`

| Job | Checks |
|---|---|
| `repository-checks` | `veyra-doctor` full run (docs structure, claims, registers, ADRs, roadmap, workflow pinning) |
| `rust` | `rustfmt --check`, `clippy -D warnings`, `cargo test`, `cargo doc --no-deps` (doc-link honesty) |
| `python-tools` | `ruff check`, `pytest` |
| `supply-chain` | `cargo-deny` (advisories, licenses, bans, sources) |

Path filters are intentionally **not** used for the doctor and supply-chain
jobs: repository-wide invariants (documentation consistency, license policy)
must not be silently skipped. Third-party actions are pinned to full commit
SHAs; workflows hold `contents: read` only.

### `scheduled.yml` — weekly (Monday 03:00 UTC, plus manual dispatch)

- Source-register reachability with `--fail-on-required` (required-tier
  references must stay live; informational failures are reported, not fatal).
- `veyra-doctor` and `cargo-deny` re-runs to catch advisory-DB drift and rot.

Results of scheduled runs are reviewed in the next work session; failures open
a `chore` issue rather than being ignored.

### Policy

- Checks are never weakened to make a PR pass. If a check is genuinely
  environmentally limited, the change is documented in the PR and the check
  keeps running.
- Required-status-check enforcement on `develop` is *recommended* once the
  current check set has been green across several weeks of real changes; the
  current protection state is recorded in
  [`docs/roadmap/implementation-plan.md`](docs/roadmap/implementation-plan.md#phase-0--repository-foundation-research-architecture-and-threat-model).

## Documentation expectations

- New subsystem documentation goes in `docs/` with the evidence notation from
  [`docs/research/README.md`](docs/research/README.md); research claims cite the
  [source register](docs/research/source-register.md) and carry access dates.
- Architecture documents describe *proposed* Veyra behavior; research documents
  describe *observed* Chromium behavior. The two are never conflated.
- Roadmap changes keep phase acceptance criteria testable; the
  compatibility matrix is updated in the same PR as any implementation change.

## Reporting problems

Bugs and security issues: see [`SECURITY.md`](SECURITY.md) (security) and the
issue templates (everything else). Include reproduction steps and the commit
hash; for engine bugs (once they exist), include a minimal input file — the
goal is a deterministic, reproducible report.

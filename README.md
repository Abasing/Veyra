# Veyra

**An independent, open-source, Rust-first browser and browser-engine project.**

Veyra's long-term objective is a complete, production-grade web browser with an
independently implemented engine — web compatibility, security, and performance
comparable to Chromium-class browsers — built in Rust without depending on
Chromium, Blink, or V8 as its underlying engine.

[![CI](https://github.com/Abasing/Veyra/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/Abasing/Veyra/actions/workflows/ci.yml?query=branch%3Adevelop)

> **Status: research and foundation stage — early.**
> Veyra is an **early-stage, independently implemented browser-engine project**.
> There is **no browser engine implemented yet**: no rendering, no JavaScript
> execution, no web compatibility, and **not yet a usable browser**. What exists
> today is a researched foundation — a Chromium investigation dossier, a proposed
> architecture, a phased roadmap, and validation tooling. Do not rely on Veyra
> for anything. Capabilities are claimed only when evidence in
> [`docs/roadmap/compatibility-matrix.md`](docs/roadmap/compatibility-matrix.md)
> supports the claim.

"Chromium-class" is a **target**, not a milestone. The web platform is many
independently difficult systems (HTML/DOM, CSS layout, JavaScript, networking,
security, storage, media, accessibility). The research dossier in `docs/`
identifies which parts can be built incrementally and which require years of
compatibility work. Independence does not require reinventing every execution
engine on day one; execution-engine trade-offs are documented in
[ADR-0002](docs/decisions/0002-javascript-engine-strategy.md).

## Current status

| Area | State (2026-10-15) |
|---|---|
| Chromium research dossier (9 documents) | First pass complete — evidence-tiered, gaps documented |
| System architecture & decision records | Proposed (11 ADRs); pre-implementation |
| Phased implementation roadmap | Defined (Phases 0–11 with acceptance criteria) |
| Compatibility program | Defined; matrix seeded at "Not investigated" |
| Validation tooling (`veyra-doctor`, link/source checkers) | Working; enforced in CI |
| Browser engine (HTML/CSS/JS/network/rendering) | **Not started** |
| Browser UI | **Not started** |

## What Veyra is not

- Not a Chromium fork, and not derived from Chromium source code.
- Not an HTML demo, WebView wrapper, CEF/Electron shell, or UI prototype.
- Not a browser you can use today.
- Not affiliated with, endorsed by, or using trademarks of the Chromium project,
  Google, Mozilla, or any browser vendor.

## Principles

1. **Rust-first.** Rust is the primary language for the engine, browser
   services, security boundaries, networking, storage, and performance-critical
   systems. TypeScript is preferred for the browser's application interface and
   frontend tooling. Python is for test orchestration and analysis tooling.
   WebAssembly is an execution target that must be supported.
2. **No C/C++ subsystems.** New Veyra subsystems are not implemented in C or
   C++, and Chromium/Blink/V8/CEF/Electron/WebView are never adopted as runtime
   dependencies. Native dependencies are exceptional and require a recorded ADR.
3. **Security before APIs.** Trust boundaries, threat model, and sandboxing are
   designed before unrestricted internal APIs. See
   [ADR-0004](docs/decisions/0004-process-model.md) and the
   [security model](docs/architecture/process-and-security-model.md).
4. **Evidence-based claims.** Compatibility is measured with Web Platform
   Tests, Test262, and WebAssembly conformance suites — not demos. Statuses use
   explicit states from "Not investigated" to "Meets defined acceptance
   criteria".
5. **Study Chromium, copy nothing.** Chromium's public design docs, source
   organization, and behavior are studied as references; implementation is
   independent. See [the source register](docs/research/source-register.md).

The full engineering policy is in
[`docs/engineering/engineering-standards.md`](docs/engineering/engineering-standards.md).

## Repository layout

```
docs/
  research/        Chromium investigation dossier (evidence-tiered)
  architecture/    Veyra's proposed system architecture
  decisions/       Architectural decision records (ADRs)
  roadmap/         Implementation plan, compatibility program, matrix
  engineering/     Engineering standards
tools/
  veyra-doctor/    Rust CLI validating repo invariants (docs, claims, CI pinning)
tests/             Python tests for repository tooling
.github/           Workflows, issue/PR templates, dependabot
```

Engine crates under `crates/` will be introduced only when subsystem boundaries
are justified by an ADR — no placeholder crates exist by design.

## Reading order for contributors

1. [`docs/research/README.md`](docs/research/README.md) — dossier index and evidence notation
2. [`docs/architecture/system-overview.md`](docs/architecture/system-overview.md)
3. [`docs/roadmap/implementation-plan.md`](docs/roadmap/implementation-plan.md)
4. [`CONTRIBUTING.md`](CONTRIBUTING.md) — workflow, commands, CI
5. [`docs/decisions/`](docs/decisions/) — when touching anything consequential

## Development quickstart

Prerequisites: a current stable Rust toolchain (pinned in `rust-toolchain.toml`), Python 3.11+.

```sh
cargo fmt --all -- --check        # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace            # unit tests (incl. veyra-doctor self-tests)
cargo run -p veyra-doctor         # repository invariant checks (docs, claims, CI)
python3 -m pip install pytest ruff
python3 -m pytest tests/ -q       # tooling tests
python3 -m ruff check tools/ tests/
```

CI runs exactly these checks on every push to `develop` and every PR — see
[`CONTRIBUTING.md`](CONTRIBUTING.md#ci-workflows) for what each job validates.

## License

Apache License 2.0 — see [LICENSE](LICENSE). Chromium is referenced for study
only; no Chromium source code is included in this repository.

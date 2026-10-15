# ADR 0011: Sandboxing technology — Linux-first, Windows/macOS as recorded gaps

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Invariants I6–I7 ([process-and-security-model.md](../architecture/process-and-security-model.md))
require content processes without ambient authority. Chromium's per-OS
mechanisms are documented ([S2][S3]): Windows (restricted tokens, job
objects, AppContainer-class), Linux (namespaces, seccomp-BPF, zygote),
macOS (Seatbelt). Rust-native paths: Landlock (LSM filesystem
restriction [K1]) via `landlock` [E1], seccomp filters via `seccompiler`
[E1], namespaces via `nix` [E1]. Windows/macOS equivalent tooling in
safe Rust is possible but unproven for this use [U] — Seatbelt-class
profile compilation on macOS is historically C-adjacent.

## Decision

1. **Phase 6 (Linux):** content processes get network+PID namespaces,
   a generated seccomp allow-list (`seccompiler`), Landlock rulesets
   denying filesystem writes, and dropped capabilities. The exact
   syscall budget is derived from Phase 6 experiments with the real
   runtime profile and committed as a reviewed artifact.
2. **Windows/macOS:** process isolation without OS sandboxing is the
   interim state; a feasibility spike (safe-bindings paths for
   restricted tokens/job objects; sandbox profile generation) is
   scheduled before Phase 9. Outcomes amend this ADR; until then the
   platform gap is stated in documentation and (eventually) release
   notes.
3. **No C helper processes**; if an OS forces one, it becomes an
   ADR-0007 exception with security review.
4. **Sandbox tests are acceptance criteria:** a Phase 6 test suite
   proves the content process cannot open files/sockets, execute
   disallowed syscalls, or escape via resource limits (negative tests,
   not just configuration assertions).

## Alternatives considered

- **Ship Linux-only until all OSes have sandboxes** — rejected: blocks
  all progress on one platform's governance; the gap is instead made
  explicit and scheduled.
- **Containers/VMs as the sandbox** — rejected: not a user-software
  deployment model; wrong trust boundary for a desktop browser.
- **Landlock-only (no seccomp)** — rejected: filesystem restriction
  alone leaves syscall attack surface (dossier security §6); layers
  match the Chromium lesson.

## Consequences

- Linux users get real containment at Phase 6; Windows/macOS users get
  process isolation only until the spike lands — a documented,
  time-bounded security gap, not a silent one.
- Seccomp budgets are living artifacts: every new content-process
  capability requires updating the allow-list — review checklist item.

## Revisit conditions

Spike outcomes (Phase 8–9); kernel feature changes (e.g., new LSMs);
any exploit-class incident in the wild that changes the calculus.

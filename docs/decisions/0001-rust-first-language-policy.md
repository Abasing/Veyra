# ADR 0001: Rust-first language and dependency policy

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation (project charter)

## Context

Veyra's objective is an independently implemented, Chromium-class-capable
browser engine. The language choice is the project's most consequential
long-term decision because the web platform's attack surface is dominated
by parsing untrusted input — the domain where memory-safety defects have
historically dominated CVE statistics. Published evidence: Microsoft MSRC
attributed ~70% of CVEs to memory-safety issues [M2], and Google's Android
report documents memory-safety vulnerabilities falling from 76% to 24% of
Android vulnerabilities as Rust adoption grew [M1]. Chromium's own security
program treats memory safety as a first-class mitigation layer [M3][S4].
The ecosystem also matters: the Rust ecosystem now provides credible
candidates for every major subsystem except video decode and a few
recorded gaps ([dependency audit](../research/dependency-and-license-audit.md)).

Constraints from the project charter: no C/C++ for new subsystems; no
Chromium/Blink/V8/CEF/Electron/WebView runtime dependencies; TypeScript
for application-interface tooling; Python for test orchestration;
WebAssembly as a supported execution target.

## Decision

1. Rust is the implementation language for all engine, service, security,
   networking, storage, and performance-critical code.
2. TypeScript is the preferred language for the browser's application
   interface and frontend tooling (rendered by Veyra itself once capable —
   see ADR-0006).
3. Python is used for test orchestration, automation, analysis, and
   development tooling.
4. `unsafe_code = "forbid"` is a workspace-wide lint; any future opt-out
   requires an ADR (per [native dependency policy](0007-native-dependency-policy.md))
   plus security review.
5. External dependencies are phase-gated, license-allow-listed
   (`deny.toml`), and recorded in the dependency audit before adoption.

## Alternatives considered

- **C++ engine** — rejected: forfeits the documented memory-safety
  elimination that is the strongest published lever against the engine's
  dominant vulnerability class [M1][M2].
- **Mixed Rust core + C++ acceleration components** — rejected as a
  default: reintroduces the memory-unsafe boundary precisely in
  performance-critical (hence attractive-to-attackers) code; acceptable
  only per-ADR under ADR-0007.
- **Full GC'd-language engine (e.g., JVM/Go)** — rejected: runtime
  footprint, GC control, and ecosystem fit are wrong for an engine.

## Consequences

- Rust's learning curve and slower compile times are accepted project
  costs.
- The recorded ecosystem gaps (video decode, PDF writer, PAC, inline
  layout depth) become either long-lead custom work or explicit,
  user-visible limitations — never silent C dependencies.
- The workspace lints (`deny(unwrap_used/expect_used/panic)`) establish
  untrusted-input discipline from day one.

## Revisit conditions

If a subsystem proves infeasible in Rust after a documented spike (e.g.,
a media codec path where no viable pure-Rust or Rust-wrapped option
exists and the feature is deemed essential), the ADR-0007 exception
process handles it; this ADR itself is not expected to be revisited.

# ADR 0007: Native (non-Rust) dependency policy

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

"Rust-first" does not mean "no native code ever": OS integration
(trust stores, windowing, platform accessibility, OS keystores) and
graphics drivers sit beneath any browser. The policy question is where
the boundary lies and what process governs exceptions — so that no C/C++
dependency is introduced quietly (charter principle 3/4). The dependency
audit ([dependency-and-license-audit.md](../research/dependency-and-license-audit.md) §8)
records the pressure points: video codecs, PDF writing, PAC, DRM/EME,
inline layout.

## Decision

1. **Acceptable native boundaries (no ADR needed, documented):** OS API
   bindings consumed through maintained, memory-safe Rust crates —
   windowing (winit), graphics drivers (wgpu), platform trust-store
   verification (rustls-platform-verifier), OS keystores (keyring),
   platform accessibility (accesskit adapters), Unix APIs (nix),
   sandboxing syscalls (landlock, seccompiler).
2. **Exception process (required for everything else):** a native
   dependency needs an ADR that documents: purpose, why no pure-Rust
   path exists after a recorded search, license, audit/supply-chain
   surface (build system, transitive C deps), sandbox implications
   (does it parse untrusted input outside Rust?), and removal plan.
3. **Blanket prohibitions:** Chromium/Blink/V8/WebKit-derived code;
   Skia; FFmpeg; C TLS stacks; SQLite (engine stores use redb per
   ADR-0010) — each revisable only by this ADR's process with evidence.
4. **Untrusted-input rule:** native code must never parse web content.
   If a future exception would (e.g., a video decoder), the sandbox
   boundary must contain it (decode in a restricted process), and the
   ADR must say so explicitly.

## Alternatives considered

- **Zero native code, ever** — rejected: impractical (OS/driver
  boundaries) and dishonest about what "independent engine" requires.
- **Any C library when convenient** — rejected: erodes the memory-safety
  thesis (ADR-0001) and the audit discipline; the exception process
  exists precisely to make this hard.

## Consequences

- Known future pressure points get early governance: video decode (most
  likely first exception candidate — Phase 9), PDF writer (prefer
  pure-Rust custom work), PAC (pure-Rust custom).
- The audit's "gaps" table and this ADR are the two places where feature
  limitations are recorded; release notes must reference them.

## Revisit conditions

Each exception ADR carries its own revisit conditions; this policy is
stable. If a pure-Rust alternative emerges for an accepted exception, the
exception ADR is superseded.

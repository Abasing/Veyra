# ADR 0004: Process model — single-process start with capability seams, out-of-process content at Phase 6

- **Status:** Accepted
- **Date:** 2026-10-15
- **Deciders:** Veyra foundation

## Context

Chromium's multiprocess architecture is, fundamentally, a security
architecture ([chromium-architecture.md](../research/chromium-architecture.md)
§1–2, [security-model.md](../research/security-model.md) §2): untrusted
content runs with no ambient authority; the trusted core brokers
everything. Veyra must reproduce those *ends*. The question is sequencing:
build the process split from Phase 1, or build single-process with
enforced internal boundaries first and split when there is content worth
isolating.

Considerations: (a) early multiprocess spends scarce review/infra capacity
on transport before any behavior exists; (b) late splitting without
preparation forces a rewrite; (c) the project's security invariants
(I4–I7) require boundary *discipline* — message-only communication,
origin attribution, capability-based services — which is a design
property, not a deployment property.

## Decision

1. **Phases 1–5: one process.** Content modules (parsers, DOM, style,
   layout, paint, script) are leaf crates with no ambient authority; all
   privileged operations (network, storage, files) go through service
   traits. Security invariants I1–I3, I5, I8–I10 are enforced from
   Phase 1–2 as code structure, lints, and fuzzing.
2. **Capability rule from Phase 1:** content code receives capability
   handles (connection objects) minted by the browser module; it cannot
   construct them. This is what makes the later split a transport change.
3. **Phase 6: content moves to a sandboxed process** (Linux sandboxing per
   ADR-0011; IPC via `veyra-ipc` typed messages per I4). Browser module
   retains navigation/session/storage-broker/permissions/supervision —
   mirroring the essential Chromium division [D3][D6].
4. **Site isolation is staged:** origin-tagging everywhere from Phase 2
   (invariant I5); per-site processes are a later deployment policy, not
   a redesign. Cross-origin rendering policy before isolation: clip/blind
   cross-origin frames (rendering pipeline R4).
5. **Crash recovery ships with the split** (invariant I6): renderer death
   = recoverable tab state.

## Alternatives considered

- **Multiprocess from Phase 1** — rejected: infrastructure before
  behavior; the single-process interim keeps honest trade-offs (documented
  here and in release notes) instead of imaginary security.
- **Permanent single process** — rejected: contradicts the threat model
  (compromised-content adversary) and the Chromium-class objective.
- **One process per tab (no site isolation ever)** — rejected as an end
  state: origin confusion is the web's dominant cross-origin threat class
  (dossier security §6); staged site isolation is retained.

## Consequences

- Phases 1–5 security posture is "memory-safe + no-ambient-authority +
  fuzzed parsers" — strong for a single process, and honestly labeled
  interim in the threat model.
- Service traits and capability plumbing are Phase 1–2 deliverables
  (they are not optional scaffolding; the roadmap pins them).
- Phase 6 is a large, well-defined migration with its own acceptance
  criteria (sandbox works, messages validated, crash recovery proven).

## Revisit conditions

If the Phase 4/5 codebase shows the capability seams are being bypassed
for convenience (review-detectable), the Phase 6 split moves earlier.
If sandboxing feasibility fails on a platform, ADR-0011 records the gap.

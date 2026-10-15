# Veyra Process and Security Model

status: Proposed (Phase 0) | last-updated: 2026-10-15 | derived-from: ../research/security-model.md, ../research/chromium-architecture.md

This is Veyra's **threat model and process architecture proposal**. It is
written now — before engine code — because security decisions deferred until
"the browser is big" become rewrites. Evidence for the Chromium-side
observations is in the research dossier; this document says what Veyra does.

## 1. Assets and adversaries

**Assets:** user data (history, cookies, credentials, profile stores, file
grants), cross-origin page data, the browser's own integrity (process,
update channel, configuration), and the OS/user consent surfaces
(permissions).

**Adversaries:**

1. **Malicious web content** — any URL the user visits: hostile
   HTML/CSS/JS/WASM, image formats, protocol responses.
2. **Compromised content process** — an attacker who has achieved code
   execution inside the content sandbox.
3. **Malicious local actor** — a local process attacking Veyra's IPC or
   profile files on a shared machine.
4. **Malicious supply chain** — a dependency attempting to compromise the
   build or runtime.

Non-goals (explicit): defending against a compromised OS/kernel, a
compromised browser process, or physical attackers. These match the
Chromium trust model's limits (see dossier §1).

## 2. Security invariants (testable statements)

| # | Invariant | Enforced by |
|---|---|---|
| I1 | Untrusted bytes never reach parsing code without size limits applied | `veyra-html`/`veyra-net` input gates + limits table (engineering standards) |
| I2 | Parsing/handling of untrusted input cannot panic, hang unbounded, or abort the process | deny(`panic`, `unwrap_used`, `expect_used`) + fuzzing + parse budgets |
| I3 | No unsafe code in the workspace | `unsafe_code = "forbid"` (workspace lint; exceptions require ADR + review) |
| I4 | Cross-trust-boundary communication is only via declared, validated, size-bounded messages | `veyra-ipc` message types; no shared memory by default |
| I5 | Every brokered request carries its origin, and services authorize against it | origin-tagged request types in `veyra-net`/`veyra-storage` |
| I6 | A content-process death is recoverable without losing browser state | navigation/session state machine owns truth (Phase 6 crash recovery) |
| I7 | Content processes hold no ambient OS authority | sandbox: Linux landlock+seccomp+namespaces (Phase 6); Windows/macOS per ADR-0011 |
| I8 | Executable memory (when JIT lands) is W^X and runtime-created only | WASM runtime selection criteria (ADR-0009) |
| I9 | Dependencies are license-allow-listed, advisory-scanned, and minimal | `deny.toml` + CI + dependency policy |
| I10 | No web content is executed or rendered before URL/origin security policy passes | navigation state machine order (Phase 2) |

Each invariant gets a named enforcement mechanism and a test (unit,
property, fuzz, or integration) in the phase that introduces the surface —
tracked in the roadmap's per-phase acceptance criteria.

## 3. Process model: now, then, and the seam in between

**Phase 1–5 (single process, boundaries internal).** Veyra runs one process.
The security work is *structural*: untrusted-input modules (`veyra-html`,
`veyra-net` parsing, decoders) are leaf crates with no authority; all
privileged operations (network, storage, files) go through service traits
that could be backed by IPC without changing call sites. Rationale: early
multiprocess would spend the project's scarcest resource (review capacity)
on infrastructure before any behavior exists to protect. Honest trade-off
recorded: until Phase 6, a content bug is a browser bug.

**Phase 6+ (content out of process).** Content (DOM/style/layout/paint/
script) moves to a sandboxed content process; the browser module retains
navigation, session, storage broker, permissions, and supervision. IPC via
`veyra-ipc` (local sockets/pipes with typed messages). The seam is prepared
from Phase 1 by the *capability rule*: content code never calls OS services
directly — it holds capability handles (connection objects) minted by the
browser module, so the split is a transport change, not an API redesign.
This mirrors the essential property of Chromium's broker design (dossier
§2–3) without copying Mojo.

**Site isolation (Phase 6+, staged).** V1: one content process per
*browsing context group*, origin-tagged throughout. V2 (later): per-site
processes for cross-site frames. The origin-tagging cost is paid from
Phase 2 (every frame/document/resource carries its origin); the process
split is a deployment policy then, not a redesign. Design note in
[`rendering-pipeline.md`](rendering-pipeline.md) (compositing must tolerate
cross-process frames eventually).

## 4. Sandboxing technology (Linux first, honest gaps elsewhere)

- **Linux (Phase 6):** content processes get: new network+PID namespaces,
  seccomp allow-list (generated via `seccompiler`), Landlock ruleset
  (no filesystem write access), noCapabilities. The content process never
  handles raw sockets; `veyra-net` runs in the browser (or a network
  utility process later, mirroring the dossier's network-service finding).
- **Windows/macOS:** safe-bindings paths exist (restricted tokens/job
  objects; sandbox profiles) but are **not designed yet** — recorded as a
  gap in [ADR-0011](../decisions/0011-sandbox-technology.md) with a
  feasibility spike scheduled before Phase 9. Until then, those platforms
  get process isolation without OS sandboxing — a documented limitation,
  shipped honestly in release notes when releases exist.
- **No C helper processes.** If an OS forces C glue, that becomes an
  ADR-0007 native-dependency exception with security review — not a quiet
  dependency.

## 5. Permission and consent model

Origin-scoped permission store (camera, mic, location, notifications,
persistent storage) owned by the browser module; content asks via a
capability API; the UI layer (Phase 9–10) renders prompts. Permissions
Policy [ST34] gates embedding/ delegation. Secure-context rules [ST35]
apply. Deny-by-default, no ambient grants, remembered grants are per
(profile, origin, permission).

## 6. Incident classes and required defenses (summary table)

| Class (from dossier §6) | Defense | Test |
|---|---|---|
| Memory corruption | Rust; forbid unsafe; fuzz | fuzz targets CI-scheduled |
| Origin confusion | origin-tagged messages; broker checks | unit + WPT webappsec |
| Confused deputy | capabilities bind origin at mint time | property tests |
| IPC injection | typed length-bounded messages, reject-by-default | fuzz per message type |
| Resource exhaustion | limits table per subsystem; budgets | property tests + limits fuzz |
| Supply chain | deny.toml, minimal deps, review | cargo-deny CI |
| JIT-class (future) | no JIT early; wasmtime sandbox when JIT lands | ADR-0009 gate |

## 7. Open questions

1. Exact seccomp allow-list for the content process (needs Phase 6
   experiments with the real runtime's syscall profile).
2. Shared-memory strategy for raster/compositing across processes
   (zero-copy vs copy) — performance decision at Phase 7 with security
   review (I4 default: no shared memory).
3. Crash-reporting content (minidumps) and its privacy boundary — Phase 10
   product decision.
4. Windows/macOS sandbox feasibility — ADR-0011 spike.

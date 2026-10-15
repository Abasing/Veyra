# Research: Chromium Security Model

status: Investigated (first pass) | last-updated: 2026-10-15 | maintainer: foundation work

## Evidence basis

Chromium security documentation: security overview [S1], sandbox FAQ [S2],
sandbox design [S3], memory-safety pages [S4][M3] — verified reachable.
Kernel primitives: Landlock [K1], seccomp [K2] — verified. Memory-safety
evidence: Google Security Blog (Android memory-safety elimination) [M1],
Microsoft MSRC (70% of CVEs are memory-safety) [M2] — verified. Site
isolation security rationale [D4]. WebAppSec specs: CSP [ST32], mixed content
[ST33], Permissions Policy [ST34], secure contexts [ST35], Fetch Metadata
[ST20], SRI [ST45], Private Network Access [ST46]. Detailed sandbox
mechanism internals (syscall sets, token levels) are `[K]` from documented
material; a source-level pass is pending (coverage matrix). **No specific CVE
identifiers are cited here — none were individually re-verified in this pass;
vulnerability-class reasoning does not require them.**

## 1. The threat model (observation)

Chromium's security architecture assumes, in order of severity:

1. **Malicious web content** — arbitrary HTML/CSS/JS/WASM/headers attempt to
   read cross-origin data, escape the sandbox, exploit the engine, or attack
   the user (phishing, drive-by download). Every parser and every engine
   subsystem processes untrusted bytes.
2. **A compromised renderer** — after any renderer exploit, the design goal
   is that the attacker holds *only that renderer*: no file access, no
   network sockets, no access to other origins' renderers, no profile data.
   Everything valuable lives across an IPC boundary in the browser process.
3. **A compromised GPU or utility process** — containable damage scopes
   (no user data beyond what that service holds).
4. **A compromised browser process** — game over by design; therefore the
   browser process is the *trusted computing base* to be kept small [D6][K].
5. **Physical/network adversaries** — TLS downgrade/mitM protection
   (HSTS, CT), safe-browsing-class threats (product layer, see
   [browser-product-features.md](browser-product-features.md)).

**Assessment for Veyra:** adopt this threat model verbatim as the starting
point — it is the industry's distilled experience. Veyra's formal threat
model document (Veyra-side) is
[../architecture/process-and-security-model.md](../architecture/process-and-security-model.md);
this research document records what Chromium's model is and why.

## 2. Process isolation as the primary mitigation

The multiprocess architecture ([chromium-architecture.md](chromium-architecture.md))
is a security architecture: renderers hold no authority (no sockets, no
filesystem), all requests are brokered by the browser, and site isolation
[D4] ensures cross-origin data never shares an address space — the mitigation
for both memory-disclosure bugs and speculative-execution attacks.

**Why this matters even for a memory-safe engine [K, from M1/M2 reasoning]:**
memory safety eliminates one bug class; it does not eliminate logic bugs,
origin confusion, or sandbox-escape bugs in unsafe glue or OS interfaces.
Isolation limits blast radius regardless of implementation language. Veyra's
process model ([ADR-0004](../decisions/0004-process-model.md)) therefore
plans out-of-process renderers even though the engine is Rust.

## 3. OS sandboxing mechanisms (observation)

Chromium sandboxes renderers with OS primitives, layered [S2][S3][K]:

| Platform | Mechanism |
|---|---|
| Windows | Restricted tokens, job objects, integrity levels, AppContainer-class isolation |
| Linux | Namespaces (network/PID), seccomp-BPF syscall filtering, zygote inheritance; historically setuid/chrome-sandbox helpers for the initial drop |
| macOS | Seatbelt (sandbox profiles) + hardened runtime + mach-port restrictions |

The renderer's syscall budget is intentionally tiny; anything needing OS
access goes through brokered IPC to the browser/network/utility processes.

**Veyra mapping (assessment):** on Linux, Landlock (LSM-based filesystem
restriction [K1]) + seccomp filters [K2] via `landlock`/`seccompiler` crates
[E, MIT OR Apache-2.0 / Apache-2.0 OR BSD-3-Clause] and namespace setup via
`nix` [E] provide a Rust-only path for renderer restriction. **Windows and
macOS sandbox parity is an open gap** — `windows`-crate safe bindings can
drive Job Objects/restricted tokens, but this is unproven for Veyra and is
recorded as a risk in [ADR-0011](../decisions/0011-sandbox-technology.md).
No C helper processes are planned; if OS APIs force one, it becomes an
ADR-0007 native-dependency exception.

## 4. Memory safety as a layer (evidence)

Google's Android memory-safety report [M1] documents memory-safety
vulnerabilities dropping from 76% of Android vulnerabilities (2019) to 24%
(2024) as Rust adoption increased — the largest published dataset on the
effect. Microsoft's MSRC analysis [M2] attributes ~70% of CVEs to
memory-safety issues. Chromium's own memory-safety pages [M3][S4] describe
the mitigation stack. **Assessment:** Rust-first is Veyra's *foundation*
layer, not its security story: it buys the elimination of a class, while
process isolation, sandboxing, and the broker pattern remain mandatory
layers. This layering is formalized in the Veyra threat model.

## 5. Origin-model web security (observation, standards-anchored)

The web security surface Veyra must implement — each anchored to its spec:

- **Same-origin policy** and origin serialization ([ST2]/HTML [ST1]; URL
  parsing/canonicalization [ST3] — origin confusion class starts here).
- **CORS** ([ST4]): credentialed cross-origin reads require explicit
  allowance; preflight for non-simple requests.
- **CSP** [ST32], **mixed content** [ST33], **secure contexts** [ST35],
  **Permissions Policy** [ST34], **Fetch Metadata** (Sec-Fetch-*) [ST20],
  **SRI** [ST45], **PNA/LNA** (private-network access controls) [ST46],
  **CORP/COOP/COEP** (cross-origin isolation, documented in Fetch/HTML [K]).
- **Cookies**: RFC 6265 (+6265bis direction) with SameSite; partitioned
  cookies (CHIPS) [N5, verified].
- **Referrer policies**, **content sniffing rules** [ST5], **MIME handling**.

**Assessment:** none of this is optional for "real-site compatible"; it is
also the most spec-testable security area (WPT has deep coverage). Phase 5–6
tests draw directly from WPT's webappsec suites.

## 6. Vulnerability classes a new implementation must prevent

Derived from the documented architecture and published analyses [S1][S2][D4][M1][M2][K]:

| Class | Typical mechanism | Veyra mitigation |
|---|---|---|
| Memory corruption / UAF | Unsafe C++ engine code | Rust + `unsafe_code=forbid` [workspace lints]; fuzzing parsers |
| JIT type confusion | Optimizing-compiler miscompilation | No JIT in early phases; WASM runtime with proven sandbox (wasmtime) when JIT lands; W^X |
| Origin confusion | Misattributed document/origin across IPC | Origin-tagged messages; browser-side origin checks at every brokered API |
| Sandbox escape | Direct syscalls from renderer | seccomp/landlock budgets; minimal renderer syscall surface |
| Confused deputy | Renderer tricks a privileged service into acting for the wrong origin | Capability-scoped requests (origin bound at request creation, checked at service) |
| IPC injection/protocol confusion | Malformed cross-process messages | Typed, length-bounded, versioned message encoding; rejection-by-default parsers |
| Race conditions / TOCTOU | Broker checks then acts on changed state | Snapshot-then-validate patterns; no shared mutable state across boundaries |
| Resource exhaustion | Parser bombs, tab bombs, unbounded caches | Limits tables per subsystem (see engineering standards), scheduler quotas |
| Supply chain | Malicious dependency | cargo-deny, dependency review, minimal dependency count |

## 7. What Veyra needs (mapping)

| Capability | Veyra subsystem | Phase |
|---|---|---|
| Threat model & security invariants | `process-and-security-model.md` (done at Phase 0) | 0 |
| URL/origin canon (single authority) | `veyra-url` wrapper over `url` crate with origin rules | 2 |
| Brokered, capability-scoped internal APIs | `veyra-ipc` message design | 2 (design), 6 (enforcement) |
| Sandbox (Linux) | `veyra-sandbox` (landlock+seccomp+namespaces) | 6 |
| Sandbox (Windows/macOS) | restricted-token/AppContainer & Seatbelt-equivalent paths | 9+ (risk-recorded) |
| WebAppSec enforcement (CSP/mixed-content/etc.) | `veyra-net` policy layer | 5–6 |
| Fuzzing infrastructure | cargo-fuzz targets per parser | 2 onward |
| Security regression tests | WPT webappsec + custom suites | 5–6 |

## 8. Open questions

1. **[U] Renderer syscall budget on Linux** — the concrete seccomp filter set
   Veyra needs depends on the renderer's runtime behavior (threading, memory
   growth, IPC transport); design lands with Phase 6 experiments.
2. **[U] macOS sandbox feasibility without C glue** — Seatbelt profile
   compilation is historically C-adjacent; feasibility spike required before
   Phase 9 planning counts on it.
3. **[U] Current Chromium PNA → Local Network Access status** [ST46 is a
   draft explainer] — spec is in flux; Veyra tracks it rather than
   committing early.
4. **[U] Precise bfcache/COOP/COEP interaction semantics** for
   cross-origin-isolated browsing contexts — needs a dedicated standards pass.

## Confidence and gaps

- High: threat-model structure, broker/sandbox architecture, memory-safety
  statistics [M1][M2], the webAppSec spec obligations.
- Medium: platform sandbox mechanism details (documented but not
  source-verified); exact current Chrome flag/state of PNA.
- Absent by design: CVE case studies (not re-verified this pass), and any
  claim that Veyra's design is "as secure as Chrome's" — no such claim is
  supportable before audits exist.

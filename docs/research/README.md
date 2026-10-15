# Chromium Research Dossier — Index and Method

This directory contains Veyra's investigation of Chromium as a complete browser
system: architecture, web platform, execution, security, networking, storage,
product surface, testing, and the dependency landscape. It is the evidence base
from which Veyra's architecture (see `../architecture/`) and roadmap (see
`../roadmap/`) are derived.

## Status and scope of this pass

This is a **first, evidence-backed pass completed on 2026-10-15**, not a
completed study. It maps the major subsystems, records what the publicly
available documentation and live-verified sources establish, and names the
gaps. Chromium is a codebase of tens of millions of lines; a genuine
source-level study is a continuing program, tracked in
[`coverage-matrix.md`](coverage-matrix.md). Nothing here should be read as
"Chromium fully studied" — the matrix is explicit about what was and was not
examined.

## Evidence notation

Every research document tags its claims. The notation appears in each
document's *Evidence basis* section and inline where useful:

| Tag | Meaning |
|---|---|
| `[D]` | Chromium/Google **design documentation** — URL verified reachable on the access date recorded in the [source register](source-register.md) |
| `[S]` | **Source-level observation** — a Chromium source file or directory was actually read for this claim (rare in this pass; see coverage matrix) |
| `[T]` | **Standards text** — WHATWG/W3C/TC39/ISO/Unicode/RFC specification |
| `[E]` | **Ecosystem data** — retrieved live from crates.io / GitHub APIs on 2026-10-15 (licenses, versions, activity) |
| `[K]` | **Established knowledge** — well-documented behavior widely corroborated across the listed primary sources; cross-check pending source-level study |
| `[U]` | **Unverified** — plausible, unconfirmed, or explicitly pending; never load-bearing for a decision |

Documents distinguish three registers of writing, marked inline:

- **Observation** — what Chromium does (supported by the tags above).
- **Assessment** — our analysis of what it implies for Veyra.
- **Assumption** — a working belief we act on until evidence changes it;
  assumptions are listed in *Open questions* and revisited per the roadmap.

## Documents

| Document | Covers |
|---|---|
| [chromium-architecture.md](chromium-architecture.md) | Process model, IPC, navigation, lifecycle, scheduling |
| [web-platform-and-rendering.md](web-platform-and-rendering.md) | HTML/DOM lifecycle, CSS, layout, painting, compositing, a11y |
| [javascript-and-wasm.md](javascript-and-wasm.md) | V8, event-loop semantics, bindings, workers, WebAssembly, Rust engine landscape |
| [security-model.md](security-model.md) | Threat model, sandboxing, site isolation, vulnerability classes |
| [networking-and-storage.md](networking-and-storage.md) | HTTP stack, cookies/cache, fetch pipeline, storage, profiles |
| [browser-product-features.md](browser-product-features.md) | Tabs, omnibox, extensions, DevTools, platform integrations, proprietary-service dependencies |
| [testing-and-performance.md](testing-and-performance.md) | WPT/Test262, fuzzing, benchmarks, reliability engineering |
| [dependency-and-license-audit.md](dependency-and-license-audit.md) | Candidate Rust crates with live license/version evidence |
| [source-register.md](source-register.md) | Every source consulted, access dates, what it supports |
| [coverage-matrix.md](coverage-matrix.md) | What was investigated, what was not, priorities |

## Maintenance rules

1. Research documents describe **Chromium and standards**; proposals for Veyra
   live in `../architecture/`. A claim without a source tag or with tag `[U]`
   must not be the sole justification for an ADR.
2. Corrections update the document *and* the source register entry; the
   decision log (`../architecture/decision-log.md`) records when new evidence
   invalidates a decision.
3. The register records access dates. Sources whose URLs 403 automated checks
   (e.g., `w3.org/TR`, IETF Datatracker) are marked as bot-blocked canonical
   registries — cited where they are the canonical home, with a live mirror
   recorded when one was verified.
4. Coverage matrix entries move forward (and occasionally backward) with dates
   and notes; regressions are recorded, not hidden.

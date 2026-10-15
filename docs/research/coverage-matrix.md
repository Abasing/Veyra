# Research Coverage Matrix

What has actually been investigated, how, and what remains. This matrix is
the honest boundary of the 2026-10-15 first pass: it prevents both overclaim
("Chromium fully studied") and underclaim (the dossier does cover these areas
with cited evidence). Entries move with dated notes; regressions are recorded.

Method tags: **docs** = design/spec documentation pass (URLs in the source
register); **data** = live API/ecosystem pull; **source** = Chromium source
reading (none completed this pass); **spike** = hands-on integration
experiment (none completed this pass).

| # | Area (assignment §4) | Status | Method | Evidence | Remaining work | Priority |
|---|---|---|---|---|---|---|
| 4.1 | Browser process architecture | Investigated | docs | D3, D4, D6, D7(503), N3 | Source-level pass: navigation/content layer, process reuse thresholds | High (pre-Phase 6) |
| 4.2 | HTML, DOM, document lifecycle | Investigated | docs | ST1, ST2, B2 | Parser fuzzing corpus design; bfcache semantics deep-dive | Medium (pre-Phase 5) |
| 4.3 | CSS, style, layout, rendering | Investigated | docs | ST12–ST25, ST37, ST49, B3, B4, D8, D13 | stylo integration spike; inline-layout gap analysis; invalidation internals | **High (pre-Phase 3)** |
| 4.4 | JavaScript, WebAssembly, execution | Investigated | docs+data | J1–J8, ST15, ST7, E1–E4 | Boa embedding spike; Test262 harness dry-run; GC-interop design | **High (pre-Phase 4)** |
| 4.5 | Networking and web security | Investigated | docs | N1, N6–N10, ST3–ST5, ST20, ST32–ST35, ST44–ST46 | Protocol fuzz corpus; PNA/LNA current status; proxy/PAC depth | Medium (pre-Phase 5–6) |
| 4.6 | Storage, profiles, persistence | Investigated | docs | ST26, ST27, ST42, N2, N5 | Chromium quota/eviction source pass; IndexedDB edge semantics | Medium (pre-Phase 6) |
| 4.7 | Graphics, media, device APIs | Investigated (first pass) | docs+data | D13, ST28–ST31; dependency audit §5/§8 | GPU process design detail; video-decode gap decision (ADR-0007); WebRTC hardening survey | Medium (pre-Phase 7–9) |
| 4.8 | Browser product features | Investigated (first pass) | docs | P1–P5 + product inventory in dossier | Omnibox/autofill behavior depth; settings/policy surface detail | Low (pre-Phase 9) |
| 4.9 | Extensions and developer tools | Investigated (first pass) | docs | P1, P2, P4, P5 | chrome.* API inventory matrix; DevTools frontend license review | Low (pre-Phase 9) |
| 4.10 | Standards compliance & real-world compatibility | Investigated | docs+data | T1–T3, T6, T7, J8 | wptrunner hands-on; expectation-file strategy; real-site corpus curation | **High (pre-Phase 5)** |
| 4.11 | Performance and reliability | Investigated (first pass) | docs | T4, T5 + method in dossier/testing | Baseline harness (Phase 1); profiling integration; no targets set by design | Medium (Phase 1) |

Cross-cutting research obligations embedded in the roadmap (not separate
matrix rows): threat-model formalization (Phase 0, done at proposal level in
`../architecture/process-and-security-model.md`), sandbox feasibility spikes
(Phase 6 entry), stylo/Boa spikes (Phase 3/4 entries).

**Explicit non-goals of this pass** (to avoid scope creep per the working
method): enumerating Chromium's full IPC interface inventory, reading
specific Blink/V8 source files, reproducing Chromium performance numbers.
Each is scheduled in the roadmap phase where it becomes decision-relevant.

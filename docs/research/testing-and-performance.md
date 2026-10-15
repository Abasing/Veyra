# Research: Testing, Conformance, and Performance

status: Investigated (first pass) | last-updated: 2026-10-15 | maintainer: foundation work

## Evidence basis

WPT documentation and organization [T1], wptrunner [T2], wpt.fyi dashboards
[T6], live server [T7], Test262 repository and interpretation guide [T3][J8],
Speedometer 3 [T4][T5] (license present in repo but reported as
NOASSERTION by the API — manual license review required before any use [E,
recorded]). Fuzzing/benchmark methodology is `[K]` established practice
cross-referenced with the engineering standards. **No performance numbers
for Veyra or Chromium appear in this document** — targets are defined as
"baseline first, then budget", not invented.

## 1. Why testing strategy is the load-bearing wall

A new engine cannot converge on web compatibility by ad-hoc testing: the
platform is too large, regressions are constant, and "works on the pages I
tried" is worthless as evidence. Chromium/Servo/WebKit all converge through
mass conformance suites plus continuous regression detection. Veyra's
program (formalized in [../roadmap/compatibility-program.md](../roadmap/compatibility-program.md))
rests on four legs:

1. **Conformance suites** (spec truth): WPT, Test262, WASM spec tests.
2. **Regression testing** (don't re-break): unit, integration, golden-image
   rendering, and re-run conformance subsets per PR.
3. **Fuzzing** (find what attackers will): structure-aware fuzzing of every
   parser and untrusted-input boundary.
4. **Performance measurement** (proves feasibility): benchmarks with
   baselines and budgets, run on stable infrastructure.

## 2. Web Platform Tests (observation)

WPT [T1] is the cross-browser conformance corpus: `testharness.js` tests
(assert-based, spec-anchored), **reftests** (render X and Y must match),
`wdspec` (WebDriver), and per-directory spec mapping. The `wptrunner`
harness [T2] executes suites against a browser via WebDriver/CDP; browser-
specific expectation files record known-fail states; `wpt.fyi` [T6]
aggregates pass rates across engines; `wpt.live`/`w3c-test.org` [T7] serve
the suite live.

**Integration facts that matter for Veyra planning [K][T2]:**

- wptrunner supports multiple browser products; adding a new product means
  implementing a browser abstraction (process control, protocol bridge).
  Servo demonstrates a Rust-engine integration path.
- A minimal adoption path exists that avoids harness work initially: run
  WPT's *testharness* logic inside Veyra's own JS engine once Phase 4 JS
  exists, then graduate to wptrunner as protocol coverage grows.
- Licensing: WPT is distributed under the W3C 3-clause BSD + W3C Test Suite
  licenses (the GitHub license API reports NOASSERTION due to the dual
  license files [E]); **attribution and license texts must be vendored with
  any imported tests** — recorded as a hard rule in the compatibility
  program.

## 3. Test262 (observation)

Test262 [T3] is ECMAScript's conformance suite: thousands of spec-anchored
tests with frontmatter (features, flags, includes), harness files, and an
interpretation guide [J8]. Engines embed the `$262` host object; results are
reported per-test with subtest detail. A runner needs: file enumeration,
frontmatter parsing, harness injection, async completion detection, and
result normalization [J8]. **Veyra approach:** a custom Rust runner over the
embedded engine (Boa ships `$262` support hooks per its docs [U — verify at
Phase 4 entry]); results land in a tracked JSON with per-feature rollups.
WASM spec tests follow the same pattern against the WASM runtime
(official spec test suite [J5]).

## 4. Regression testing and determinism (assessment)

- **Deterministic rendering harness (Phase 1 exit):** render a fixture page
  to a fixed-size buffer with a pinned font set and fixed device scale;
  hash the output. Golden images + hashes make visual regressions reviewable
  diffs, not surprises. Font stack pinning is mandatory — platform font
  variance is the classic source of flaky visual tests.
- **Injectable time and RNG:** the event loop and all engine randomness take
  injected sources (see [../architecture/execution-and-web-apis.md](../architecture/execution-and-web-apis.md));
  no wall-clock in engine logic.
- **Failure reproduction:** every conformance failure gets a minimized
  fixture in `tests/fixtures/` with the spec citation — the triage process
  in the compatibility program turns flakes into permanent regression tests.
- **Real-world site corpus:** a curated set of pages rendered periodically
  (not per-PR) with *diffed* screenshots and *reported* — not gated —
  results, since third-party content drifts independently of Veyra.

## 5. Fuzzing (assessment; requirements formalized in engineering standards)

Untrusted-input boundaries and their required fuzz targets:

| Boundary | Input | Target |
|---|---|---|
| HTML tokenizer/tree builder | bytes | corpus: html5lib-tests + real-page seeds |
| CSS tokenizer/parser | bytes | corpus: WPT css fixtures |
| URL parser | strings | spec tests + mutation corpus |
| HTTP/1.1 responses, headers | bytes | synthetic + recorded-response corpus |
| Image decoders | bytes | per-format seed corpora |
| WASM modules | bytes | spec suite + mutation |
| IPC message decoding | bytes | per-message-type generators |

Tooling: `cargo-fuzz` (libFuzzer) for in-process targets with
`-runs`-bounded CI smoke budgets and scheduled long runs; structured
generators (arbitrary-based) for typed inputs. Crash policy: every fuzzer
crash = P1 issue with a minimized reproducer added to the regression suite.
This is written into
[../engineering/engineering-standards.md](../engineering/engineering-standards.md).

## 6. Benchmarks and performance method (observation + assessment)

Established browser benchmarks: **Speedometer 3** [T4][T5] (the industry
cross-browser workload — note license needs manual review before adoption
[E]); microbenchmarks for parser/style/layout stages; memory (RSS + heap
profiles); energy/CPU on long sessions [K].

**Method (no invented numbers):**

1. **Baseline first.** Each phase establishes baselines on the repeatable
   environment (fixed runner class, N-run median with outlier trim,
   reported variance). See engineering standards for the exact protocol.
2. **Budgets from baselines.** A regression budget (e.g., ±10% on a phase's
   key metric) is set *from the measured baseline*, recorded in the phase's
   acceptance criteria, and enforced on the scheduled benchmark workflow.
3. **Profiling discipline.** `tracing` spans across subsystems; flamegraphs
   on demand; per-phase profiling notes stored with results.
4. **No cross-phase comparisons** (feature sets differ); comparisons are
   against the same phase's baseline.

## 7. Reliability engineering (assessment)

- **Crash-free rate** as the top-line health metric once a shell exists
  (renderer deaths expected and handled; browser-process deaths are bugs).
- **OOM discipline:** every untrusted-size allocation path has a documented
  limit (engineering standards' limits table); fuzzers assert limits.
- **Malformed-input robustness:** conformance suites *and* fuzz corpora both
  feed the "must not panic, must not hang, must not OOM" property tests on
  parsers.

## 8. What Veyra needs (mapping)

| Capability | Phase | Notes |
|---|---|---|
| Deterministic screenshot harness | 1 | Golden images; pinned fonts |
| veyra-doctor repo invariants | 0–1 | Done/CI-enforced |
| HTML parser conformance (html5lib-tests) | 2 | BSD/MIT-attributed corpus [E] |
| URL conformance (WPT url subset) | 2 | via `url` crate adoption |
| Test262 runner + tracked results | 4 | JSON artifacts in CI (scheduled) |
| WPT testharness-in-engine run | 4–5 | Bootstrap before wptrunner |
| WPT reftests via screenshot compare | 5+ | Requires stable paint |
| Fuzz targets + CI smoke | 2 onward | cargo-fuzz |
| Benchmark suite + baselines | 1–3 | Criterion micro + startup macro |
| Speedometer 3 evaluation | 10 | After license review [E] |

## 9. Open questions

1. **[U] wptrunner product-integration effort** — estimate after Phase 4 JS
   exists (WebDriver layer is the main cost).
2. **[U] Speedometer 3 license terms** — manual review before any use [E:
   repo reports NOASSERTION].
3. **[U] Boa `$262`/module support depth** for Test262 — verify at Phase 4.
4. **[U] Reference-vs-golden-image testing** — reftests need *two* rendering
   paths; golden images need frozen fonts. Decide the mix after Phase 3
   paint stability data exists.

## Confidence and gaps

- High: WPT/Test262 structure and licensing posture [T1][T2][T3][E];
  fuzzing boundaries; determinism requirements.
- Medium: wptrunner integration specifics; benchmark environment variance
  controls (to be calibrated in Phase 1).
- Absent: any pass-rate claims for Veyra (none exists), any performance
  target numbers (baselines first).

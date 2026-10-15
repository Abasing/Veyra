# Performance Baselines — Phase 1 (2026-10-15)

status: Measured | measured: 2026-10-15 | scope: Phase 1 (shell, CPU raster,
screenshot pipeline) | method owner: project lead

The first measured baselines for Veyra, per roadmap Phase 1 acceptance
criterion 3 ("baselines exist with variance data; regression budgets set
from them — documented numbers, not guesses") and engineering standards
§11 (benchmark methodology). **Every number below was measured on the
dated environment described here; none is extrapolated or inherited.**

## 1. Environment

| Property | Value |
|---|---|
| Date | 2026-10-15 (UTC+8 session) |
| Host CPU | Intel Xeon (server-class, model string genericized by the hypervisor), 2 vCPU visible (`nproc` = 2) |
| Memory | ~4.06 GiB |
| GPU | None — no `/dev/dri`; pure CPU rendering is the only path (consistent with ADR-0005's CPU-first CI posture) |
| OS | Debian GNU/Linux 13 (trixie), kernel 5.10.134-013.15.kangaroo.al8.x86_64, containerized |
| Rust | 1.99.0 (b940084d7 2026-09-28), pinned in `rust-toolchain.toml` |
| Build profiles | `bench` (inherits `release`: `lto = "thin"`, `codegen-units = 1`, opt-level 3) for criterion; `--release` for the process-level runs |
| Display | Xvfb 2:21.1.16 (headless X server) for the window smoke test only; screenshot benchmarks never touch a display |
| Load conditions | Shared containerized host; no other Veyra processes; standard background noise applies |

## 2. Methodology

Two complementary protocols:

**A. Criterion micro-benchmarks** (`cargo bench -p veyra-shell`,
`crates/veyra-shell/benches/phase1.rs`): the crate's default protocol —
3.000 s warm-up, 100 samples per benchmark, median + median absolute
deviation (MAD) reported, no outlier trimming beyond criterion's
built-in slope estimation. Each benchmark renders or hashes a
deterministic input (the diagnostic scene, scene version 1), so the
measured work is identical run-to-run.

**B. Process-level timing** (`tools/bench_process.py --runs 10`): wall
time around 10 full invocations of `target/release/veyra-shell
screenshot` (process spawn → config defaults → scene render → PNG
encode → atomic file write), reporting median, MAD, min, max, and
verifying the canonical hash is identical across all runs. Per §11 this
is the "shell startup" class measurement; criterion cannot spawn
processes, so this is the honest way to measure it. No warm-up: process
spawns are cold by nature; the spread captures that.

Which numbers are **regression-trackable**: the criterion medians (A)
are — fixed machine-independent inputs, tight MADs. The process-level
figure (B) is **preliminary**: process spawn on a shared container is
noisy across runners, so it is recorded for orientation, not for
automated budget gates. Regression budgets (±10% default per §11)
apply to the criterion numbers from this note.

## 3. Criterion results (median [MAD], µs)

| Benchmark | 800×600 | 1920×1080 |
|---|---|---|
| `scene_render` (full diagnostic scene on `CpuRaster`) | 98.06 [2.90] | 619.26 [2.82] |
| `clear` (full-buffer fill, lower bound of the raster path) | 43.32 [0.73] | 387.11 [2.49] |
| `canonical_hash` (canonical serialization + SHA-256) | 1 263.85 [5.06] | 5 467.50 [23.69] |
| `png_encode` (RGBA8 → PNG, in memory) | 12 594.76 [58.52] | 52 995.97 [228.81] |
| `config_parse` (TOML document → validated config) | 2.58 [0.02] (default document) | — |
| `screenshot_e2e` (render + encode + hash, no file I/O) | 13 908.59 [51.02] | 60 263.69 [339.32] |

Reading notes:

- `scene_render` at 800×600 is ~99 µs (~0.1 ms/frame): the diagnostic
  scene is rectangle fills only; real content rendering (Phase 3) will
  dominate this by orders of magnitude. These are *infrastructure*
  baselines, not engine performance claims.
- `png_encode` is the pipeline's dominant step (~91% of
  `screenshot_e2e` at 800×600). Deflate compression cost is expected;
  if screenshot latency ever matters for automation, a
  compression-level knob is the documented first lever.
- `canonical_hash` is dominated by the SHA-256 over ~1.92 MB of pixel
  data at 800×600 — the identity check we run on every golden
  comparison.

## 4. Process-level result (protocol B)

Command: `python3 tools/bench_process.py --bin target/release/veyra-shell --runs 10`
(viewport 800×600, defaults otherwise).

| Statistic | Value |
|---|---|
| Median | 17.40 ms |
| MAD | 0.04 ms |
| Min / Max | 17.31 / 18.63 ms |
| Canonical hash across all 10 runs | identical (`9e473fb9…8696d9`, matching the committed golden fixture) |

Two honest observations: the spread is tight for a process-spawn
measurement on this host, and the canonical hash produced by the
release binary is **bit-identical to the debug-profile golden fixture**
— cross-profile determinism holds, which is what the CI hash checks
actually depend on.

## 5. Not measured (and why)

- **Window presentation** (buffer → softbuffer → Xvfb): requires a live
  display; Xvfb timing on shared CI runners is too noisy to publish as
  a baseline. The path is exercised by the smoke tests; presentation
  cost is bounded by a full-buffer blit and becomes a recorded number
  when a stable benchmarking display exists.
- **Interactive frame pacing**: Phase 1 has no animation or rAF; there
  is nothing meaningful to measure. Phase 4's executor baselines will
  use the injected frame clock.
- **Regression budgets**: deliberately not asserted as CI gates yet —
  per the task rules ("do not establish arbitrary regression thresholds
  before collecting data"), the scheduled benchmark workflow that
  compares against this note lands when the harness has run across
  several independent commits.

## 6. Reproduction

```sh
cargo bench -p veyra-shell                          # protocol A (criterion)
cargo build --release -p veyra-shell
python3 tools/bench_process.py --bin target/release/veyra-shell --runs 10   # protocol B
```

Criterion's raw distributions persist under `target/criterion/`.
Re-running on different hardware will produce different numbers; the
regression-tracking rule is "same runner class, same profile, compare
against the newest dated note" (§11).

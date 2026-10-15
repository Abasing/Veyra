#!/usr/bin/env python3
"""Process-level timing for the Phase 1 shell (complements criterion).

Measures full-process invocations of the built `veyra-shell` binary —
process spawn + config defaults + scene render + PNG encode + atomic
write — the "shell initialization where measurable" item from the
roadmap. Protocol per engineering standards §11: N runs, median +
spread (min/max and MAD), no outlier trimming (N is small and reported
honestly). Wall-clock timing via time.perf_counter around subprocess.

Usage: python3 tools/bench_process.py --bin target/release/veyra-shell --runs 10
Output: JSON on stdout (machine-readable), summary on stderr.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", required=True, help="path to the veyra-shell binary")
    parser.add_argument("--runs", type=int, default=10, help="number of measured runs")
    parser.add_argument("--width", type=int, default=800)
    parser.add_argument("--height", type=int, default=600)
    args = parser.parse_args()

    binary = Path(args.bin)
    if not binary.is_file():
        print(f"binary not found: {binary}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="veyra-bench-") as td:
        out = Path(td) / "bench.png"
        times_ms: list[float] = []
        hashes: set[str] = set()
        for _ in range(args.runs):
            start = time.perf_counter()
            proc = subprocess.run(
                [str(binary), "screenshot", "--output", str(out),
                 "--width", str(args.width), "--height", str(args.height)],
                capture_output=True, text=True, check=True,
            )
            elapsed_ms = (time.perf_counter() - start) * 1000.0
            times_ms.append(elapsed_ms)
            for line in proc.stdout.splitlines():
                if line.startswith("pixels-hash=sha256:"):
                    hashes.add(line.removeprefix("pixels-hash=sha256:"))
            out.unlink(missing_ok=True)

    median = statistics.median(times_ms)
    mad = statistics.median([abs(t - median) for t in times_ms])
    result = {
        "binary": str(binary),
        "runs": args.runs,
        "viewport": f"{args.width}x{args.height}",
        "times_ms": [round(t, 3) for t in times_ms],
        "median_ms": round(median, 3),
        "mad_ms": round(mad, 3),
        "min_ms": round(min(times_ms), 3),
        "max_ms": round(max(times_ms), 3),
        "hash_stable_across_runs": len(hashes) == 1,
        "canonical_hash": next(iter(hashes)) if len(hashes) == 1 else None,
    }
    json.dump(result, sys.stdout, indent=2)
    print(file=sys.stderr)
    print(
        f"median={median:.2f} ms  MAD={mad:.2f} ms  "
        f"min={min(times_ms):.2f}  max={max(times_ms):.2f}  "
        f"hash-stable={result['hash_stable_across_runs']}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

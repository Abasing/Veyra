#!/usr/bin/env python3
"""Verify that the sources recorded in docs/research/source-register.md are live.

Behavior (documented in CONTRIBUTING.md and linkutil.py):
- P1 (required-tier) rows: a dead link (404/410/other-4xx) fails the run.
  403/429 (bot-blocked canonical registries) and 5xx/unreachable after
  retries are reported as warnings — adversarial-to-bots infrastructure
  is not the same as a dead reference.
- P2 rows: all outcomes are warnings; the run records them for review.

Usage:
  python3 tools/verify_sources.py [--fail-on-required] [--register PATH]

The scheduled CI workflow runs this weekly with --fail-on-required.
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

from linkutil import UrlResult, parse_register

DEFAULT_REGISTER = "docs/research/source-register.md"
POLITENESS_DELAY = 0.4


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--register", default=DEFAULT_REGISTER)
    parser.add_argument(
        "--fail-on-required",
        action="store_true",
        help="exit non-zero when a required (P1) source is dead",
    )
    parser.add_argument("--only-tier", default=None, help="restrict to P1 or P2")
    args = parser.parse_args(argv)

    register = Path(args.register)
    if not register.exists():
        print(f"ERROR: register not found: {register}", file=sys.stderr)
        return 2

    rows = parse_register(register.read_text(encoding="utf-8"))
    if args.only_tier:
        rows = [r for r in rows if r[2] == args.only_tier]
    if not rows:
        print("ERROR: no register rows parsed — parser or file problem", file=sys.stderr)
        return 2

    failures: list[str] = []
    warnings: list[str] = []
    ok_count = 0

    for sid, url, tier in rows:
        result: UrlResult
        try:
            from linkutil import check_url

            result = check_url(url)
        except Exception as exc:  # defensive: one bad row must not abort the sweep
            result = UrlResult(url, "unreachable", None, None, f"crash: {exc}")
        time.sleep(POLITENESS_DELAY)

        line = f"[{tier}] {sid} {result.outcome} {url} {result.detail}".rstrip()
        if result.outcome == "ok":
            ok_count += 1
            print(f"ok       {line}")
            continue
        if result.outcome == "dead":
            if tier == "P1" and args.fail_on_required:
                failures.append(line)
            else:
                warnings.append(line)
            print(f"DEAD     {line}")
        else:  # bot-blocked / transient / unreachable
            warnings.append(line)
            print(f"warn     {line}")

    print()
    print(f"summary: {ok_count} ok, {len(warnings)} warning(s), {len(failures)} failure(s)")
    for f in failures:
        print(f"FAILURE: {f}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())

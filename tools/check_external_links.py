#!/usr/bin/env python3
"""Check external http(s) links across markdown files.

By default a dead link (404-class) fails the run; bot-blocked (403/429),
transient (5xx), and unreachable outcomes are warnings. `--warn-only`
downgrades failures to warnings (used for local pre-flight runs).

Usage:
  python3 tools/check_external_links.py docs/ [more paths...] [--warn-only]
"""

from __future__ import annotations

import argparse
import time
from pathlib import Path

from linkutil import check_url, extract_links, is_external

POLITENESS_DELAY = 0.5


def collect_markdown(paths: list[Path]) -> list[Path]:
    files: list[Path] = []
    for p in paths:
        if p.is_file() and p.suffix == ".md":
            files.append(p)
        elif p.is_dir():
            files.extend(sorted(p.rglob("*.md")))
    return files


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", help="files or directories to scan")
    parser.add_argument("--warn-only", action="store_true")
    args = parser.parse_args(argv)

    files = collect_markdown([Path(p) for p in args.paths])
    seen: dict[str, str] = {}
    failures: list[str] = []
    warnings: list[str] = []
    ok_count = 0

    for f in files:
        text = f.read_text(encoding="utf-8", errors="replace")
        for target in extract_links(text):
            if not is_external(target):
                continue
            if target in seen:
                continue
            seen[target] = str(f)
            result = check_url(target)
            time.sleep(POLITENESS_DELAY)
            line = f"{result.outcome} {target} (via {f}) {result.detail}".rstrip()
            if result.outcome == "ok":
                ok_count += 1
                print(f"ok   {target}")
            elif result.outcome == "dead":
                if args.warn_only:
                    warnings.append(line)
                    print(f"warn {target} (warn-only)")
                else:
                    failures.append(line)
                    print(f"DEAD {target}")
            else:
                warnings.append(line)
                print(f"warn {target}")

    print()
    print(f"summary: {ok_count} ok, {len(warnings)} warning(s), {len(failures)} failure(s)")
    for line in failures:
        print(f"FAILURE: {line}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())

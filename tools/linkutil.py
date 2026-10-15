"""Shared utilities for Veyra repository link/source tooling.

Design constraints: Python standard library only (test orchestration
tooling per the project language policy); polite fetching (browser UA,
timeouts, bounded retries); conservative classification of HTTP statuses
so bot-blocked registries are not mistaken for dead links.

Status classification:
  200                     -> ok
  403 / 429               -> bot-blocked (warning; canonical registries do this)
  404 / 410               -> dead (failure)
  other 4xx               -> dead (failure)
  5xx                     -> transient after retries (warning)
  DNS/connection errors   -> unreachable (warning; environment-dependent)
"""

from __future__ import annotations

import re
import time
import urllib.error
import urllib.request
from dataclasses import dataclass

USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/126.0 Safari/537.36 "
    "(veyra-repo-checks; github.com/Abasing/Veyra)"
)

# Classifies a final status code into a check outcome.
BOT_BLOCKED = {"bot-blocked"}
DEAD = {"dead"}
TRANSIENT = {"transient"}
UNREACHABLE = {"unreachable"}
OK = {"ok"}


@dataclass
class UrlResult:
    url: str
    outcome: str  # one of: ok, bot-blocked, dead, transient, unreachable
    status: int | None
    final_url: str | None
    detail: str = ""


def classify(status: int) -> str:
    if status == 200:
        return "ok"
    if status in (403, 429):
        return "bot-blocked"
    if status in (404, 410):
        return "dead"
    if 400 <= status < 500:
        return "dead"
    return "transient"


def check_url(
    url: str,
    timeout: float = 25.0,
    retries: int = 2,
    backoff: float = 1.5,
) -> UrlResult:
    """Fetches *url* (HEAD, falling back to GET) and classifies the result."""
    methods = ("HEAD", "GET")
    last = UrlResult(url, "unreachable", None, None, "not attempted")
    for method in methods:
        for attempt in range(retries + 1):
            req = urllib.request.Request(url, method=method, headers={"User-Agent": USER_AGENT})
            try:
                with urllib.request.urlopen(req, timeout=timeout) as resp:
                    return UrlResult(url, "ok", resp.status, resp.geturl())
            except urllib.error.HTTPError as exc:
                outcome = classify(exc.code)
                last = UrlResult(url, outcome, exc.code, None, f"HTTP {exc.code}")
                if outcome in {"bot-blocked", "dead"}:
                    return last
                if attempt < retries:
                    time.sleep(backoff * (attempt + 1))
                continue
            except (urllib.error.URLError, TimeoutError, ConnectionError, OSError) as exc:
                last = UrlResult(url, "unreachable", None, None, f"{type(exc).__name__}: {exc}")
                if attempt < retries:
                    time.sleep(backoff * (attempt + 1))
                continue
        # fall through to GET for HEAD-specific failures (405 etc.)
        if last.outcome in {"bot-blocked", "dead"}:
            return last
    return last


MD_LINK_RE = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)\)")


def extract_links(md_text: str) -> list[str]:
    """Extracts inline markdown link targets (relative or absolute)."""
    return MD_LINK_RE.findall(md_text)


def is_external(target: str) -> bool:
    return target.startswith("http://") or target.startswith("https://")


# Register row parsing: | ID | URL | Status | Tier | Supports | Accessed |
REGISTER_ROW_RE = re.compile(r"^\|\s*([A-Z]+\d+\w*)\s*\|\s*(https?://[^|\s]+)\s*\|")


def parse_register(text: str) -> list[tuple[str, str, str]]:
    """Returns (id, url, tier) rows from source-register.md table sections."""
    rows: list[tuple[str, str, str]] = []
    current_tier = "P2"
    for line in text.splitlines():
        tier_match = re.match(r"^##\s+.*Tier:\s*\*\*(P\d)\*\*", line)
        if tier_match:
            current_tier = tier_match.group(1)
            continue
        m = REGISTER_ROW_RE.match(line)
        if m:
            rows.append((m.group(1), m.group(2), current_tier))
    return rows

"""Tests for Veyra repository tooling (standard library only)."""

from __future__ import annotations

import linkutil
from linkutil import classify, extract_links, is_external, parse_register

SAMPLE_REGISTER = """\
# Source Register

Tier: **P1** = required.

## Chromium: architecture

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| D3 | https://www.chromium.org/developers/design-documents/multi-process-architecture/ | verified | P1 | Process roles | 2026-10-15 |

## Product

| ID | URL | Status | Tier | Supports | Accessed |
|---|---|---|---|---|---|
| P2 | https://chromedevtools.github.io/devtools-protocol/ | verified | P2 | CDP shape | 2026-10-15 |
"""


def test_classify_statuses():
    assert classify(200) == "ok"
    assert classify(403) == "bot-blocked"
    assert classify(429) == "bot-blocked"
    assert classify(404) == "dead"
    assert classify(410) == "dead"
    assert classify(400) == "dead"
    assert classify(503) == "transient"
    assert classify(500) == "transient"


def test_extract_links_skips_images_and_anchors():
    md = (
        "[docs](./docs/README.md) "
        "[web](https://example.com/a?b=c) "
        "![pic](img.png) "
        "[anchor](#section) "
        "[titled](https://example.org/x \"title\")"
    )
    links = extract_links(md)
    assert "./docs/README.md" in links
    assert "https://example.com/a?b=c" in links
    assert "img.png" not in links
    assert "#section" not in links


def test_is_external():
    assert is_external("https://x")
    assert is_external("http://x")
    assert not is_external("./relative.md")
    assert not is_external("#anchor")
    assert not is_external("mailto:a@b")


def test_parse_register_rows_and_tiers():
    rows = parse_register(SAMPLE_REGISTER)
    assert ("D3", "https://www.chromium.org/developers/design-documents/multi-process-architecture/", "P1") in rows
    assert ("P2", "https://chromedevtools.github.io/devtools-protocol/", "P2") in rows


def test_check_url_dead_status_classification():
    """classify() drives outcomes; verify the mapping used by both tools."""
    # 404 must be 'dead' (failure-class), 403 must be 'bot-blocked' (warning-class)
    assert linkutil.classify(404) == "dead"
    assert linkutil.classify(403) == "bot-blocked"

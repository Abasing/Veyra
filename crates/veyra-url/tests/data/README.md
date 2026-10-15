# Vendored test corpora — `veyra-url`

## `urltestdata.json`

- **Source:** web-platform-tests, `url/resources/urltestdata.json`
  (https://github.com/web-platform-tests/wpt/blob/master/url/resources/urltestdata.json).
- **License:** W3C 3-clause BSD (WPT repository license; see
  https://github.com/web-platform-tests/wpt/blob/master/LICENSE).
- **Vendored revision:** WPT master, fetched 2026-10-15 (1011 entries).
- **Purpose:** WHATWG basic URL parser conformance for `veyra-url`
  (roadmap Phase 2, acceptance criterion 2: "WPT url subset passes via
  `url` crate adoption (documented deltas)").
- **Runner:** `tests/wpt_url_parsing.rs`. Excluded entries must be listed
  in `deltas.md` (input string → documented reason); the runner's
  `DOCUMENTED_DELTAS` table is the only skip mechanism.

## `deltas.md`

Append a section per excluded entry when a corpus entry is not satisfied:

```
## <input string>
- <why: spec section + behavior observed + tracking reference>
```

Current deltas: none for the vendored revision.

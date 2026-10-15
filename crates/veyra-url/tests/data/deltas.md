# Documented WPT url deltas

Corpus: `urltestdata.json` (provenance in `README.md`), vendored 2026-10-15.
This file documents every corpus entry the engine does not satisfy, as
required by the runner (`tests/wpt_url_parsing.rs`) — `DOCUMENTED_DELTAS`
there is the only skip mechanism, keyed by `(input, base)`.

Result with `url` crate 2.5.8 + `idna` 1.x at adoption time: of the 896
corpus entries with assertions, **827 pass and 69 are documented deltas
(7.7%)** across five categories. Each category cites the divergence and
the tracking signal; a dependency bump that resolves a category shrinks
this file.

## 1. Special-scheme extra leading slashes (7 entries)

The URL standard's "special authority slashes state" tolerates any number
of `/`/`\` before the authority: relative input `///test` against
`http://example.org/` yields `http://test/`. The parser reports
`EmptyHost` instead. Upstream signal: rust-url tracks this in its WPT
runner's ignore list (extra-slash handling).

Entries (base `http://example.org/` unless noted): `///test`,
`///\//\//test`, `///example.org/path`, `///example.org/../path`,
`///example.org/../../`, `///example.org/../path/../../`,
`///example.org/../path/../../path`.

## 2. IDNA: lenient domain-to-ASCII on empty/invalid punycode (8 entries)

The WHATWG domain-to-ASCII algorithm (beStrict=false, VerifyDnsLength=false)
accepts `xn--` labels that UTS-46 processing in `idna` 1.x rejects. The
parser fails with `IdnaError` where browsers parse successfully.

Entries: `https://xn--/`, `file://xn--/p`,
`http://a.b.c.xn--pokxncvks`, `http://a.b.c.Xn--pokxncvks`,
`http://a.b.c.XN--pokxncvks`, `http://10.0.0.xn--pokxncvks`,
`http://10.0.0.xN--pokxncvks`, `http://10.0.0.XN--pokxncvks`.

## 3. `file:` URL host/path edge cases (31 entries)

Windows drive-letter normalization (`C|` → `C:` visible in the
serialization, e.g. `file://host/C:/` where the parser emits
`file:///C:/`), empty-host path-slash preservation
(`file://spider///` keeps three slashes), and multi-slash path collapse
(`file:////foo`, `////one/two`, `/..//localhost//pig`) differ from the
standard's file-slash states. This is the largest category; it is
confined to `file:` URLs, which Phase 2 navigation resolves locally — no
network policy depends on these serializations.

Entries: see the `file:`-schemed rows and drive-letter rows (`C|`,
`C|/`, `C|?`, `C|#`, `C|\`, `C|\n/`, `/C:/`, `/c:/foo/bar`, `..` against
`file://x/C:/` and `abc://x/y/z/C:/`, `/`, `/////mouse`, `//localhost//pig`,
`\/localhost//pig`, `/..//localhost//pig`, `/\//\/a/../`, `file:///w|/m`,
`file:////one/two`, `file:///.//`, `file:.//p`, `file:/.//p`,
`file:\\//`, `file:\\\\`, `file:\\\\?fox`, `file:\\\\#guppy`,
`file:\\localhost//`, `file://\/localhost//cat`, and the
`file://localhost//...` family) in `DOCUMENTED_DELTAS`.

## 4. `blob:` origin derivation (4 entries)

The current URL standard derives a blob URL's origin from its path URL
only for `http`/`https`; everything else is opaque ("null"). The parser
derives tuple origins for `ws`/`wss`/`ftp` and resolves nested
`blob:blob:...`. Security-relevant in principle, but `blob:` URLs are not
navigable document URLs in Phase 2 (no blob store exists yet); revisit at
the blob-store phase.

Entries: `blob:blob:https://example.org/`, `blob:ftp://host/path`,
`blob:ws://example.org/`, `blob:wss://example.org/`.

## 5. Path percent-encoding of U+0020 and backslash handling (19 entries)

The 2023+ standard percent-encodes U+0020 in paths
(`non-special:opaque %20?hi`) and treats `\` as a literal (to be
percent-encoded) character in *non-special* scheme paths
(`foo://foo/\a`); the parser serializes the space literally and folds
`\` into `/` even for non-special schemes.

Entries: the `non-special:opaque` family (5), `foo://host/ !"…`,
`wss://host/ !"…`, and the `foo://foo/\a` family
(`\a`, `\/a`, `\\a` against `foo://foo/a`), plus the file-URL
backslash rows counted in category 3.

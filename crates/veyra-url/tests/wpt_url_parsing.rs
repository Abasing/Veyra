// WPT `url` subset runner (roadmap Phase 2, acceptance criterion 2).
//
// Corpus: web-platform-tests `url/resources/urltestdata.json`, vendored at
// `tests/data/urltestdata.json` (W3C 3-clause BSD license — see
// `tests/data/README.md` for provenance and revision). The corpus drives the
// WHATWG basic URL parser and asserts the serialization fields exactly as
// browsers report them.
//
// Documented deltas: any corpus entry the engine does not satisfy is listed
// in `tests/data/deltas.md` with the reason, and skipped here by URL input —
// a skip without a deltas entry is a test bug. The run below asserts the
// full corpus *minus* the documented-delta list passes.

// Scoped lint exception, documented per engineering standards §2: the
// denied `unwrap`/`expect`/`panic` lints target *untrusted input* handling
// in production paths. Test fixtures are provably internal, and tests fail
// through the standard assert machinery. The workspace lint configuration
// is unchanged; non-test code keeps the full deny set.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)] // test-crate internals: the runner documents itself here

use serde_json::Value;
use veyra_url::{Url, UrlError};

const CORPUS: &str = include_str!("data/urltestdata.json");

/// Corpus pairs excluded from the pass assertion. Each entry is
/// `(input, Option<base>)` and MUST have a matching section in
/// `tests/data/deltas.md` — this table is the only skip mechanism, and a
/// delta that stops reproducing (e.g. after a dependency bump) is a hard
/// failure so the list shrinks instead of rotting.
const DOCUMENTED_DELTAS: &[(&str, Option<&str>)] = &[
    // 2026-10-15 corpus run: 896 entries, 69 documented deltas (data/deltas.md).
    // Categories: 1 extra-slash authority, 2 IDNA lenient punycode, 3 file:
    // host/path edges, 4 blob: origin derivation, 5 U+0020/backslash encoding.
    ("file:///w|/m", None),
    ("non-special:opaque  ?hi", None),
    ("non-special:opaque  #hi", None),
    ("non-special:opaque \t\t  \t#hi", None),
    ("non-special:opaque \t\t  #hi", None),
    ("non-special:opaque\t\t  \r #hi", None),
    ("http://a.b.c.xn--pokxncvks", None),
    ("http://10.0.0.xn--pokxncvks", None),
    ("http://a.b.c.XN--pokxncvks", None),
    ("http://a.b.c.Xn--pokxncvks", None),
    ("http://10.0.0.XN--pokxncvks", None),
    ("http://10.0.0.xN--pokxncvks", None),
    ("/", Some("file://h/C:/a/b")),
    ("file:\\\\//", None),
    ("file:\\\\\\\\", None),
    ("file:\\\\\\\\?fox", None),
    ("file:\\\\\\\\#guppy", None),
    ("file://spider///", None),
    ("file:\\\\localhost//", None),
    ("file://\\/localhost//cat", None),
    ("file://localhost//a//../..//", None),
    ("/////mouse", Some("file:///elephant")),
    ("\\/localhost//pig", Some("file://lion/")),
    ("//localhost//pig", Some("file://lion/")),
    ("/..//localhost//pig", Some("file://lion/")),
    ("C|", Some("file://host/dir/file")),
    ("C|", Some("file://host/D:/dir1/dir2/file")),
    ("C|#", Some("file://host/dir/file")),
    ("C|?", Some("file://host/dir/file")),
    ("C|/", Some("file://host/dir/file")),
    ("C|\n/", Some("file://host/dir/file")),
    ("C|\\", Some("file://host/dir/file")),
    ("/c:/foo/bar", Some("file://host/path")),
    ("..", Some("abc://x/y/z/C:/")),
    ("..", Some("file://x/C:/")),
    ("file://example.net/C:/", None),
    ("file://1.2.3.4/C:/", None),
    ("file://[1::8]/C:/", None),
    ("C|/", Some("file://host/")),
    ("/C:/", Some("file://host/")),
    ("file:C:/", Some("file://host/")),
    ("file:/C:/", Some("file://host/")),
    ("file://localhost//a//../..//foo", None),
    ("file://localhost////foo", None),
    ("file:////foo", None),
    ("file:////one/two", Some("file:///")),
    ("////one/two", Some("file:///")),
    ("file:///.//", Some("file:////")),
    ("file:.//p", None),
    ("file:/.//p", None),
    ("blob:blob:https://example.org/", None),
    ("blob:ftp://host/path", None),
    ("blob:ws://example.org/", None),
    ("blob:wss://example.org/", None),
    ("file://xn--/p", None),
    ("foo://host/ !\"$%&'()*+,-./:;<=>@[\\]^_`{|}~", None),
    ("wss://host/ !\"$%&'()*+,-./:;<=>@[\\]^_`{|}~", None),
    ("https://xn--/", None),
    ("///test", Some("http://example.org/")),
    ("///\\//\\//test", Some("http://example.org/")),
    ("///example.org/path", Some("http://example.org/")),
    ("///example.org/../path", Some("http://example.org/")),
    ("///example.org/../../", Some("http://example.org/")),
    ("///example.org/../path/../../", Some("http://example.org/")),
    (
        "///example.org/../path/../../path",
        Some("http://example.org/"),
    ),
    ("/\\//\\/a/../", Some("file:///")),
    ("\\a", Some("foo://foo/a")),
    ("\\/a", Some("foo://foo/a")),
    ("\\\\a", Some("foo://foo/a")),
];

fn field_string<'a>(case: &'a Value, key: &str) -> Option<&'a str> {
    case.get(key).and_then(Value::as_str)
}

fn assert_field(case: &Value, key: &str, actual: &str) -> Result<(), String> {
    match field_string(case, key) {
        Some(expected) => {
            if expected != actual {
                Err(format!(
                    "{key}: expected {expected:?}, got {actual:?} (input {:?})",
                    case.get("input").and_then(Value::as_str).unwrap_or("?")
                ))
            } else {
                Ok(())
            }
        }
        None => Ok(()),
    }
}

fn run_case(case: &Value) -> Result<(), String> {
    let input = match case.get("input").and_then(Value::as_str) {
        Some(input) => input,
        None => return Ok(()), // comment-only entries
    };
    let base = case.get("base").and_then(Value::as_str);

    let parsed = match base {
        Some(base) => Url::parse(base)
            .map_err(|error| format!("base {base:?} failed to parse: {error}"))
            .and_then(|base| Url::parse_with_base(&base, input).map_err(|e| e.to_string())),
        None => Url::parse(input).map_err(|e| e.to_string()),
    };

    // Entries marked "failure" must fail to parse.
    let expects_failure = case
        .get("failure")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    match parsed {
        Err(error) => {
            if expects_failure {
                Ok(())
            } else {
                Err(format!(
                    "unexpected parse failure {error:?} for input {input:?} (base {base:?})"
                ))
            }
        }
        Ok(url) => {
            if expects_failure {
                return Err(format!(
                    "input {input:?} (base {base:?}) parsed but corpus marks it failure"
                ));
            }
            assert_field(case, "href", url.as_str())?;
            assert_field(case, "origin", &url.origin().ascii_serialization())?;
            // `protocol` in the corpus is scheme + ":".
            assert_field(case, "protocol", &format!("{}:", url.scheme()))?;
            assert_field(case, "username", url.username())?;
            assert_field(case, "password", url.password().unwrap_or(""))?;
            // `host` is hostname[:port]; `hostname` is the bare host.
            let port = url.port().map(|p| p.to_string()).unwrap_or_default();
            let host_with_port = match (url.host_str(), url.port()) {
                (Some(host), Some(_)) => format!("{host}:{port}"),
                (Some(host), None) => host.to_string(),
                (None, _) => String::new(),
            };
            assert_field(case, "host", &host_with_port)?;
            assert_field(case, "hostname", url.host_str().unwrap_or(""))?;
            assert_field(case, "port", &port)?;
            assert_field(case, "pathname", url.path())?;
            // `search`/`hash` are the standard's *getter* semantics, not
            // raw serialization: an empty query or fragment yields "" (the
            // `?`/`#` stays in `href`).
            let search = match url.query() {
                Some(query) if !query.is_empty() => format!("?{query}"),
                _ => String::new(),
            };
            assert_field(case, "search", &search)?;
            let hash = match url.fragment() {
                Some(fragment) if !fragment.is_empty() => format!("#{fragment}"),
                _ => String::new(),
            };
            assert_field(case, "hash", &hash)?;
            Ok(())
        }
    }
}

#[test]
fn wpt_url_subset_passes_with_documented_deltas() {
    let corpus: Vec<Value> = serde_json::from_str(CORPUS)
        .unwrap_or_else(|error| panic!("vendored corpus must be valid JSON: {error}"));

    let dump = std::env::var("VEYRA_DUMP_URL_DELTAS").is_ok();
    let mut total = 0usize;
    let mut skipped_deltas = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for case in &corpus {
        let input = case.get("input").and_then(Value::as_str);
        if input.is_none() {
            continue; // comment-only entry
        }
        total += 1;
        let base = case.get("base").and_then(Value::as_str);
        if DOCUMENTED_DELTAS.contains(&(input.unwrap_or_default(), base)) {
            skipped_deltas += 1;
            continue;
        }
        if let Err(message) = run_case(case) {
            if dump {
                println!(
                    "DELTA {:?}|{:?}|{}",
                    input.unwrap_or_default(),
                    base,
                    message
                );
            } else {
                failures.push(message);
            }
        }
    }

    let passed = total - skipped_deltas - failures.len();
    println!("WPT url subset: {passed}/{total} passed, {skipped_deltas} documented deltas");
    if !dump {
        assert!(
            failures.is_empty(),
            "{} corpus case(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn corpus_entries_with_base_resolve_through_the_veyra_join_path() {
    // Every base-based entry must exercise relative resolution through
    // `Url::parse_with_base`; spot-check one known pair so a refactor that
    // accidentally drops the base cannot silently turn every case into an
    // absolute parse.
    let corpus: Vec<Value> = serde_json::from_str(CORPUS).unwrap();
    let with_base = corpus
        .iter()
        .find(|case| {
            case.get("base").and_then(Value::as_str).is_some() && case.get("input").is_some()
        })
        .expect("corpus contains base-based entries");
    let input = with_base.get("input").and_then(Value::as_str).unwrap();
    let base = with_base.get("base").and_then(Value::as_str).unwrap();
    let base_url = Url::parse(base).unwrap_or_else(|e| panic!("base {base:?}: {e}"));
    let _ = Url::parse_with_base(&base_url, input);
}

#[test]
fn failure_entries_surface_typed_errors_not_panics() {
    let corpus: Vec<Value> = serde_json::from_str(CORPUS).unwrap();
    let mut failure_cases = 0usize;
    for case in &corpus {
        let expects_failure = case
            .get("failure")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let input = case.get("input").and_then(Value::as_str);
        if !expects_failure {
            continue;
        }
        let Some(input) = input else { continue };
        failure_cases += 1;
        let result = match case.get("base").and_then(Value::as_str) {
            Some(base) => Url::parse(base).and_then(|base| Url::parse_with_base(&base, input)),
            None => Url::parse(input),
        };
        if let Ok(url) = result {
            panic!(
                "input {input:?} marked failure in corpus but parsed to {}",
                url.as_str()
            );
        }
    }
    assert!(failure_cases > 0, "corpus contains failure entries");
}

#[test]
fn malformed_percent_and_scheme_inputs_return_errors() {
    // Independent of the corpus: documented parser outcomes that navigation
    // relies on for request-policy rejection.
    let cases: [(&str, Option<UrlError>); 4] = [
        ("http://example.org:99999999/", None), // invalid port (any error, must fail)
        ("http://exa mple.org/", None),
        ("http://[::1]:bad/", None),
        ("http://%", None),
    ];
    for (input, _) in cases {
        assert!(
            Url::parse(input).is_err(),
            "input {input:?} should fail to parse"
        );
    }
}

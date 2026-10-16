//! End-to-end style-loading tests through the real binary
//! (`CARGO_BIN_EXE_veyra-shell`), per engineering standards §8. The
//! primary fixture is `file://` (hermetic, no network); one HTTP case
//! reuses the hand-rolled responder pattern (IP literal — DNS-free).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::sync::Arc;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_veyra-shell")
}

fn run_cli(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn veyra-shell")
}

/// Writes the fixture tree and returns the index.html file:// URL.
fn write_fixture(files: &[(&str, Vec<u8>)]) -> String {
    let dir = std::env::temp_dir().join(format!(
        "veyra-styles-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write fixture");
    }
    let index = dir.join("index.html");
    veyra_file_url(&index)
}

/// A `file://` URL for a path (no external URL crate in the test — the
/// shell's own URL parser is the authority; this only needs the trivial
/// POSIX form).
fn veyra_file_url(path: &std::path::Path) -> String {
    format!("file://{}", path.to_str().expect("utf8 path"))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn open_styles_collect_link_style_import_and_inline_sources() {
    let index = write_fixture(&[
        (
            "index.html",
            r#"<!DOCTYPE html>
<html><head>
<link rel="stylesheet" href="site.css">
<style>body { color: green; }</style>
<style media="print">body { color: magenta; }</style>
</head><body>
<p id="lead" class="tip" style="color: purple">hello</p>
<span>text</span>
</body></html>
"#
            .as_bytes()
            .to_vec(),
        ),
        (
            "site.css",
            "@import url(\"extra.css\");\n@import \"extra.css\";\np { color: red; }\np.tip { color: blue; }\n"
                .as_bytes()
                .to_vec(),
        ),
        (
            "extra.css",
            "p { font-size: 20px; }\n@import \"extra.css\";\n"
                .as_bytes()
                .to_vec(),
        ),
    ]);

    let output = run_cli(&["open", &index, "--styles"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    // The UA + author sheets: body is block with the <style> green.
    let body_line = out
        .lines()
        .find(|line| line.starts_with("style html>body["))
        .expect("body style line");
    assert!(
        body_line.contains("display=block"),
        "body line: {body_line}"
    );
    assert!(
        body_line.contains("color=#008000ff"),
        "body line: {body_line}"
    );
    assert!(
        body_line.contains("margin=8px/8px/8px/8px"),
        "UA body margin: {body_line}"
    );

    // The print-scoped <style> was skipped: body is green, not magenta.
    assert!(
        !out.contains("color=#ff00ffff"),
        "print sheet leaked: {out}"
    );

    // The lead paragraph: inline style beats the author rules; the
    // imported sheet's font-size applied; the class rule beat the type
    // rule (blue vs red) before the inline override.
    let lead_line = out
        .lines()
        .find(|line| line.starts_with("style html>body>p["))
        .expect("p style line");
    assert!(
        lead_line.contains("color=#800080ff"),
        "lead line: {lead_line}"
    );
    assert!(
        lead_line.contains("font-size=20px"),
        "import font-size missing: {lead_line}"
    );

    // Shorthand expansion through the pipeline: `p { margin: 1px 2px 3px 4px }`
    // was not in this fixture — the UA p margin is 1em top/bottom at 20px
    // font-size... the p inherits 20px from extra.css? No: font-size on p
    // only. The UA p margin is 1em of the p's own font size (20px).
    assert!(
        lead_line.contains("margin=20px/0px/20px/0px"),
        "p UA margin at own font size: {lead_line}"
    );

    // Color inheritance: the span inherits the body's green.
    let span_line = out
        .lines()
        .find(|line| line.starts_with("style html>body>span["))
        .expect("span style line");
    assert!(
        span_line.contains("color=#008000ff"),
        "span line: {span_line}"
    );

    // The load report: 3 sheets (site, extra via import, inline style),
    // zero failures; imports attempted 3 times (dup + self-cycle are
    // visited-guarded).
    let err = stderr(&output);
    assert!(err.contains("sheets=3"), "sheet count: {err}");
    assert!(err.contains("sheet-failures=0"), "failures: {err}");
    assert!(err.contains("imports=3"), "import count: {err}");
}

#[test]
fn open_styles_survives_missing_and_unparsable_sheets() {
    let index = write_fixture(&[
        (
            "index.html",
            r#"<!DOCTYPE html>
<html><head>
<link rel="stylesheet" href="missing.css">
<link rel="stylesheet" href="binary.css">
<style>p { color: teal; }</style>
</head><body><p>x</p></body></html>
"#
            .as_bytes()
            .to_vec(),
        ),
        // Invalid UTF-8: the loader's typed NonUtf8 rejection (the only
        // whole-sheet parse failure in the batch-1 discipline; malformed
        // *rules* are error-recovered, not rejected).
        ("binary.css", vec![0xFF, 0xFE, b'p']),
    ]);

    let output = run_cli(&["open", &index, "--styles"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    let p_line = out
        .lines()
        .find(|line| line.starts_with("style html>body>p["))
        .expect("p style line");
    // The surviving <style> still applies.
    assert!(p_line.contains("color=#008080ff"), "p line: {p_line}");
    // Two sheet failures counted (missing fetch, broken parse).
    let err = stderr(&output);
    assert!(err.contains("sheet-failures=2"), "failures: {err}");
    assert!(err.contains("sheets=1"), "loaded count: {err}");
}

#[test]
fn open_styles_specificity_and_source_order_across_sheets() {
    let index = write_fixture(&[
        (
            "index.html",
            r#"<!DOCTYPE html>
<html><head>
<link rel="stylesheet" href="a.css">
<style>#target { color: yellow; }</style>
</head><body>
<p id="target" class="item">x</p>
</body></html>
"#
            .as_bytes()
            .to_vec(),
        ),
        (
            "a.css",
            "p.item { color: red; }\np { color: blue; }\n"
                .as_bytes()
                .to_vec(),
        ),
    ]);

    let output = run_cli(&["open", &index, "--styles"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    let p_line = out
        .lines()
        .find(|line| line.starts_with("style html>body>p["))
        .expect("p style line");
    // The inline <style> sheet comes after the link (document order) —
    // but specificity decides: `#target` (1,0,0) beats `p.item` (0,1,1).
    assert!(p_line.contains("color=#ffff00ff"), "p line: {p_line}");
}

#[test]
fn open_styles_over_http_fetches_link_sheets() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let routes: Arc<HashMap<String, (u16, Vec<u8>)>> = Arc::new(HashMap::from([
        (
            "/doc".to_string(),
            (
                200,
                b"<html><head><link rel=\"stylesheet\" href=\"site.css\"></head><body><p>x</p></body></html>"
                    .to_vec(),
            ),
        ),
        ("/site.css".to_string(), (200, b"p { color: navy; }".to_vec())),
    ]));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let routes = Arc::clone(&routes);
            std::thread::spawn(move || {
                let Ok(mut writer) = stream.try_clone() else {
                    return;
                };
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    return;
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                for _ in 0..64 {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                        break;
                    }
                }
                let default = (404u16, b"missing".to_vec());
                let (status, body) = routes.get(&path).unwrap_or(&default);
                let response = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status,
                    if *status == 200 { "OK" } else { "Not Found" },
                    body.len()
                );
                let _ = writer.write_all(response.as_bytes());
                let _ = writer.write_all(body);
                let _ = writer.flush();
            });
        }
    });

    let output = run_cli(&["open", &format!("http://127.0.0.1:{port}/doc"), "--styles"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    let p_line = out
        .lines()
        .find(|line| line.starts_with("style html>body>p["))
        .expect("p style line");
    assert!(p_line.contains("color=#000080ff"), "navy p: {p_line}");
    let err = stderr(&output);
    assert!(err.contains("sheets=1"), "css sheet loaded: {err}");
}

/// Regression: documents without any sheets still compute (UA defaults
/// only) and the flag is off by default.
#[test]
fn open_without_sheets_and_without_flag() {
    let index = write_fixture(&[(
        "index.html",
        "<!DOCTYPE html><html><body><p>x</p></body></html>"
            .as_bytes()
            .to_vec(),
    )]);

    // Without --styles: no style lines on stdout.
    let output = run_cli(&["open", &index]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!stdout(&output).contains("style "), "leaked style lines");

    // With --styles: UA defaults only.
    let output = run_cli(&["open", &index, "--styles"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    let p_line = out
        .lines()
        .find(|line| line.starts_with("style html>body>p["))
        .expect("p style line");
    assert!(p_line.contains("display=block"), "UA p: {p_line}");
    assert!(p_line.contains("color=#000000ff"), "UA color: {p_line}");
}

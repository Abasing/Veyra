// End-to-end navigation tests through the real binary
// (`CARGO_BIN_EXE_veyra-shell`), per engineering standards §8: the shell
// is exercised exactly as CI and users invoke it. The HTTP fixture is a
// hand-rolled responder on 127.0.0.1 (IP literal — DNS-free, hermetic;
// the full fixture machinery lives in veyra-net's tests).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::Arc;

/// One canned route: (status, content type, body, extra headers).
type Route = (u16, &'static str, &'static [u8], Vec<String>);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_veyra-shell")
}

struct Fixture {
    port: u16,
}

impl Fixture {
    fn start(routes: HashMap<String, Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let routes = Arc::new(routes);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let routes = Arc::clone(&routes);
                std::thread::spawn(move || {
                    let _ = serve(stream, &routes);
                });
            }
        });
        Self { port }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{port}{path}", port = self.port)
    }
}

fn serve(mut stream: TcpStream, routes: &HashMap<String, Route>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    for _ in 0..64 {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
    }
    static FALLBACK: Route = (404, "text/plain", b"missing", Vec::new());
    let (status, content_type, body, extra) = routes.get(&path).unwrap_or(&FALLBACK);
    let status_text = if *status == 302 { "Found" } else { "OK" };
    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        status,
        status_text,
        content_type,
        body.len()
    );
    for header in extra {
        response.push_str(header);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    stream.write_all(response.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn run_cli(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn veyra-shell")
}

#[test]
fn open_loads_http_documents_and_traverses_history() {
    let fixture = Fixture::start(HashMap::from([
        (
            "/one".to_string(),
            (
                200,
                "text/html",
                b"<p>page one</p>" as &'static [u8],
                Vec::new(),
            ),
        ),
        (
            "/two".to_string(),
            (
                200,
                "text/html",
                b"<p>page two</p>" as &'static [u8],
                Vec::new(),
            ),
        ),
    ]));

    let session =
        std::env::temp_dir().join(format!("veyra-cli-session-{}.json", std::process::id()));
    let output = run_cli(&[
        "open",
        &fixture.url("/one"),
        &fixture.url("/two"),
        "--dump-dom",
        "--session-file",
        session.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("== {}", fixture.url("/one"))));
    assert!(stdout.contains(&format!("== {}", fixture.url("/two"))));
    assert!(stdout.contains("\"page one\""));
    assert!(stdout.contains("\"page two\""));
    // History traversal happened through the state machine.
    assert!(stdout.contains(&format!("== back: {}", fixture.url("/one"))));
    assert!(stdout.contains(&format!("== forward: {}", fixture.url("/two"))));

    // The session file is valid and holds both entries.
    let raw = std::fs::read_to_string(&session).unwrap();
    assert!(raw.contains(&fixture.url("/one")));
    assert!(raw.contains(&fixture.url("/two")));
    let _ = std::fs::remove_file(&session);
}

#[test]
fn open_reports_load_failures_with_exit_code_1() {
    // An HTTP 404 is a *loaded resource* (the navigation layer decides
    // what to show), so the failure path needs a connection-level error:
    // a port that was bound and is now closed.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let output = run_cli(&[
        "open",
        &format!("http://127.0.0.1:{port}/gone"),
        "--timeout-ms",
        "2000",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("load failed") || stderr.contains("network"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn open_rejects_unsupported_schemes_as_usage_or_operation_error() {
    let output = run_cli(&["open", "ftp://example.org/x"]);
    assert_eq!(output.status.code().unwrap_or(0), 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ftp"), "unexpected stderr: {stderr}");
}

#[test]
fn open_requires_at_least_one_url() {
    let output = run_cli(&["open"]);
    assert_eq!(output.status.code(), Some(2), "usage error exits 2");
}

// Integration tests over a local HTTP fixture server (engineering
// standards §8: "Services (net/storage/ipc): Integration tests against
// local fixtures"). No external network: the server is a hand-rolled
// HTTP/1.1 responder on 127.0.0.1 (an IP literal, so the DNS path is not
// involved), and every response is canned. TLS termination is not part of
// the fixture (the rustls path is exercised by the manual HTTPS smoke
// recorded in the completion report; CI stays hermetic).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use veyra_net::{Client, NetworkConfig, blocking};
use veyra_url::Url;

/// One canned route.
struct Route {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    extra_headers: Vec<String>,
    delay_ms: u64,
    repeat_headers: usize,
}

/// A tiny threaded HTTP/1.1 fixture server with a request log.
struct FixtureServer {
    addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FixtureServer {
    fn start(routes: HashMap<String, Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shared_routes = Arc::new(routes);
        {
            let requests = Arc::clone(&requests);
            let routes = Arc::clone(&shared_routes);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let requests = Arc::clone(&requests);
                    let routes = Arc::clone(&routes);
                    std::thread::spawn(move || handle(stream, &requests, &routes));
                }
            });
        }
        drop(shared_routes);
        Self { addr, requests }
    }

    fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{path}", self.addr.port())).unwrap()
    }

    fn cookie_headers_seen(&self, path: &str) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.starts_with(path))
            .filter_map(|entry| entry.split('|').nth(1).map(str::to_string))
            .collect()
    }
}

fn handle(stream: TcpStream, requests: &Mutex<Vec<String>>, routes: &HashMap<String, Route>) {
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

    // Drain headers (bounded).
    let mut cookie = String::new();
    for _ in 0..128 {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if line == "\r\n" || line.is_empty() {
            break;
        }
        // hyper may write the header name in any case; match loosely.
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("cookie")
        {
            cookie = value.trim().to_string();
        }
    }
    requests.lock().unwrap().push(format!("{path}|{cookie}"));

    let default = Route {
        status: 404,
        content_type: "text/plain",
        body: b"not found".to_vec(),
        extra_headers: Vec::new(),
        delay_ms: 0,
        repeat_headers: 0,
    };
    let route = routes.get(&path).unwrap_or(&default);
    if route.delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(route.delay_ms));
    }
    let status_text = match route.status {
        200 => "OK",
        302 => "Found",
        303 => "See Other",
        307 => "Temporary Redirect",
        _ => "OK",
    };
    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        route.status,
        status_text,
        route.content_type,
        route.body.len()
    );
    for header in &route.extra_headers {
        response.push_str(header);
        response.push_str("\r\n");
    }
    for _ in 0..route.repeat_headers {
        response.push_str("X-Filler: 0123456789abcdef\r\n");
    }
    response.push_str("\r\n");
    if let Ok(mut stream) = reader.get_ref().try_clone() {
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(&route.body);
        let _ = stream.flush();
    }
}

fn route(status: u16, content_type: &'static str, body: &[u8]) -> Route {
    Route {
        status,
        content_type,
        body: body.to_vec(),
        extra_headers: Vec::new(),
        delay_ms: 0,
        repeat_headers: 0,
    }
}

fn config() -> NetworkConfig {
    NetworkConfig::default()
}

#[test]
fn loads_a_document_from_the_fixture_server() {
    let server = FixtureServer::start(HashMap::from([(
        "/".to_string(),
        route(200, "text/html", b"<p>fixture</p>"),
    )]));
    let client = Client::new(config()).unwrap();
    let resource = blocking::fetch(&client, &server.url("/"), None).unwrap();
    assert_eq!(resource.status, 200);
    assert_eq!(resource.body, b"<p>fixture</p>");
    assert_eq!(resource.content_type().as_deref(), Some("text/html"));
    assert_eq!(resource.redirect_chain.len(), 0);
    // http origins are tuple origins carrying the authority.
    assert_eq!(
        resource.origin().ascii_serialization(),
        format!("http://127.0.0.1:{}", server.addr.port())
    );
}

#[test]
fn follows_redirect_chain_and_collects_it() {
    let server = FixtureServer::start(HashMap::from([
        (
            "/start".to_string(),
            Route {
                status: 302,
                content_type: "text/plain",
                body: Vec::new(),
                extra_headers: vec!["Location: /hop".to_string()],
                delay_ms: 0,
                repeat_headers: 0,
            },
        ),
        (
            "/hop".to_string(),
            Route {
                status: 303,
                content_type: "text/plain",
                body: Vec::new(),
                extra_headers: vec!["Location: /end".to_string()],
                delay_ms: 0,
                repeat_headers: 0,
            },
        ),
        ("/end".to_string(), route(200, "text/html", b"end")),
    ]));
    let client = Client::new(config()).unwrap();
    let resource = blocking::fetch(&client, &server.url("/start"), None).unwrap();
    assert_eq!(resource.status, 200);
    assert_eq!(resource.final_url, server.url("/end"));
    assert_eq!(
        resource
            .redirect_chain
            .iter()
            .map(Url::path)
            .collect::<Vec<_>>(),
        vec!["/start", "/hop"]
    );
    assert_eq!(resource.body, b"end");
}

#[test]
fn cookies_are_stored_and_sent_on_later_hops() {
    let server = FixtureServer::start(HashMap::from([
        (
            "/login".to_string(),
            Route {
                status: 302,
                content_type: "text/plain",
                body: Vec::new(),
                extra_headers: vec![
                    "Location: /home".to_string(),
                    "Set-Cookie: session=abc; Path=/".to_string(),
                ],
                delay_ms: 0,
                repeat_headers: 0,
            },
        ),
        ("/home".to_string(), route(200, "text/html", b"home")),
    ]));
    let client = Client::new(config()).unwrap();
    let jar = Client::new_cookie_jar();
    let resource = blocking::fetch(&client, &server.url("/login"), Some(&jar)).unwrap();
    assert_eq!(resource.final_url, server.url("/home"));
    // The redirect hop to /home must carry the session cookie.
    let seen = server.cookie_headers_seen("/home");
    assert_eq!(seen, vec!["session=abc".to_string()]);
}

#[test]
fn too_many_redirects_is_typed() {
    let routes: HashMap<String, Route> = (0..40)
        .map(|i| {
            (
                format!("/loop{i}"),
                Route {
                    status: 302,
                    content_type: "text/plain",
                    body: Vec::new(),
                    extra_headers: vec![format!("Location: /loop{}", i + 1)],
                    delay_ms: 0,
                    repeat_headers: 0,
                },
            )
        })
        .collect();
    let server = FixtureServer::start(routes);
    let client = Client::new(config()).unwrap();
    let error = blocking::fetch(&client, &server.url("/loop0"), None).unwrap_err();
    assert!(matches!(
        error,
        veyra_net::NetError::TooManyRedirects { max: 20 }
    ));
}

#[test]
fn response_body_cap_is_enforced() {
    let big = vec![b'x'; 2_000_000];
    let server = FixtureServer::start(HashMap::from([(
        "/big".to_string(),
        route(200, "application/octet-stream", &big),
    )]));
    let tight = NetworkConfig {
        max_response_bytes: 1_000_000,
        ..NetworkConfig::default()
    };
    let client = Client::new(tight).unwrap();
    let error = blocking::fetch(&client, &server.url("/big"), None).unwrap_err();
    assert!(matches!(
        error,
        veyra_net::NetError::ResponseTooLarge { max: 1_000_000 }
    ));
}

#[test]
fn response_deadline_is_enforced() {
    let server = FixtureServer::start(HashMap::from([(
        "/slow".to_string(),
        Route {
            delay_ms: 3_000,
            ..route(200, "text/plain", b"late")
        },
    )]));
    let fast = NetworkConfig {
        response_deadline: std::time::Duration::from_millis(300),
        ..NetworkConfig::default()
    };
    let client = Client::new(fast).unwrap();
    let error = blocking::fetch(&client, &server.url("/slow"), None).unwrap_err();
    assert!(matches!(error, veyra_net::NetError::Timeout));
}

#[test]
fn header_count_cap_is_enforced() {
    let server = FixtureServer::start(HashMap::from([(
        "/many".to_string(),
        Route {
            repeat_headers: 200,
            ..route(200, "text/plain", b"many")
        },
    )]));
    let client = Client::new(config()).unwrap();
    let error = blocking::fetch(&client, &server.url("/many"), None).unwrap_err();
    assert!(matches!(error, veyra_net::NetError::HeaderLimitExceeded));
}

#[test]
fn policy_rejects_schemes_and_ports_without_touching_the_network() {
    let client = Client::new(config()).unwrap();
    let ftp = Url::parse("ftp://example.org/x").unwrap();
    assert!(matches!(
        blocking::fetch(&client, &ftp, None),
        Err(veyra_net::NetError::SchemeNotAllowed(_))
    ));
    let file = Url::parse("file:///etc/hostname").unwrap();
    assert!(blocking::fetch(&client, &file, None).is_err());
    let ssh = Url::parse("http://example.org:22/").unwrap();
    assert!(matches!(
        blocking::fetch(&client, &ssh, None),
        Err(veyra_net::NetError::PortBlocked(22))
    ));
}

#[test]
fn async_surface_matches_blocking_surface() {
    let server = FixtureServer::start(HashMap::from([(
        "/".to_string(),
        route(200, "text/html", b"async"),
    )]));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let client = Client::new(config()).unwrap();
    let resource = runtime
        .block_on(async { client.fetch(&server.url("/"), None).await })
        .unwrap();
    assert_eq!(resource.body, b"async");
}

#[test]
fn set_cookie_on_terminal_response_is_stored() {
    let server = FixtureServer::start(HashMap::from([(
        "/set".to_string(),
        Route {
            status: 200,
            content_type: "text/html",
            body: b"ok".to_vec(),
            extra_headers: vec!["Set-Cookie: kept=1; Max-Age=100".to_string()],
            delay_ms: 0,
            repeat_headers: 0,
        },
    )]));
    let client = Client::new(config()).unwrap();
    let jar = Client::new_cookie_jar();
    blocking::fetch(&client, &server.url("/set"), Some(&jar)).unwrap();
    let page = server.url("/set");
    let header = jar.cookie_header(&page, 1_000_000_000).unwrap();
    assert_eq!(header, "kept=1");
}

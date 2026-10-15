//! Process-level CLI tests: exit codes, stdout contract, file creation,
//! repeatability, and headless independence (no display server needed).
//!
//! These tests spawn the real binary (`CARGO_BIN_EXE_veyra-shell`), so
//! they exercise exactly what CI and scripts invoke. They never require a
//! window server — that separation is the point of the screenshot path.

// Scoped lint exception, documented per engineering standards §2: the
// denied `unwrap`/`expect`/`panic` lints target *untrusted input* handling
// in production paths. Test/bench fixtures are provably internal, and
// tests fail through the standard assert machinery. The workspace lint
// configuration is unchanged; non-test code keeps the full deny set.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_veyra-shell")
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("binary spawns")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("veyra-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn screenshot_report(output: &Output) -> Vec<(String, String)> {
    stdout_text(output)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn help_and_version_exit_zero() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    let text = stdout_text(&help);
    // The honesty gate tracks the current phase claim: Phase 2 loads and
    // dumps documents, but the window still does not render web content.
    assert!(
        text.contains("does not render web content"),
        "help must be honest: {text}"
    );
    assert!(text.contains("open"), "help must document the open command");
    assert!(text.contains("EXIT CODES"));

    let version = run(&["--version"]);
    assert!(version.status.success());
    assert!(stdout_text(&version).starts_with("veyra-shell "));
}

#[test]
fn no_arguments_shows_help_with_zero_exit() {
    let output = run(&[]);
    assert!(output.status.success());
    assert!(stdout_text(&output).contains("USAGE"));
}

#[test]
fn screenshot_creates_file_reports_hash_and_is_repeatable() {
    let dir = temp_dir("shot");
    let out1 = dir.join("one.png");
    let out2 = dir.join("two.png");

    let first = run(&["screenshot", "--output", out1.to_str().unwrap()]);
    assert!(first.status.success(), "stderr: {}", stderr_text(&first));
    assert!(out1.is_file(), "output file must exist");

    let report = screenshot_report(&first);
    let get = |key: &str| {
        report
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(get("width"), "800");
    assert_eq!(get("height"), "600");
    assert_eq!(get("stride"), "3200");
    assert_eq!(get("scene"), "diagnostic-v1");
    let hash1 = get("pixels-hash");
    assert!(hash1.starts_with("sha256:"), "hash line: {hash1}");
    assert_eq!(&hash1["sha256:".len()..].len(), &64);
    assert!(
        hash1["sha256:".len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "hash must be 64 lowercase hex chars"
    );
    assert_eq!(
        stdout_text(&first).lines().next(),
        Some("veyra-screenshot v1")
    );

    let second = run(&["screenshot", "--output", out2.to_str().unwrap()]);
    assert!(second.status.success());
    let hash2 = screenshot_report(&second)
        .iter()
        .find(|(k, _)| k == "pixels-hash")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert_eq!(
        hash1, hash2,
        "repeated captures must be byte-identical in hash"
    );

    // The PNG on disk decodes to the expected dimensions.
    let bytes = std::fs::read(&out1).expect("png bytes");
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes[..]));
    let reader = decoder.read_info().expect("valid png");
    assert_eq!((reader.info().width, reader.info().height), (800, 600));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn screenshot_expect_hash_enforces_expectations() {
    let dir = temp_dir("expect");
    let out = dir.join("probe.png");
    let probe = run(&["screenshot", "--output", out.to_str().unwrap()]);
    assert!(probe.status.success());
    let hash = screenshot_report(&probe)
        .iter()
        .find(|(k, _)| k == "pixels-hash")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let bare = hash.trim_start_matches("sha256:");

    let matching = run(&[
        "screenshot",
        "--output",
        dir.join("ok.png").to_str().unwrap(),
        "--expect-hash",
        bare.to_uppercase().as_str(),
    ]);
    assert!(matching.status.success(), "uppercase hex accepted");

    let wrong: String = bare[..62].to_string() + "00";
    let mismatching = run(&[
        "screenshot",
        "--output",
        dir.join("bad.png").to_str().unwrap(),
        "--expect-hash",
        &wrong,
    ]);
    assert_eq!(
        mismatching.status.code(),
        Some(1),
        "hash mismatch is an operation failure"
    );
    assert!(stderr_text(&mismatching).contains("mismatch"));

    let malformed = run(&[
        "screenshot",
        "--output",
        dir.join("m.png").to_str().unwrap(),
        "--expect-hash",
        "nothex",
    ]);
    assert_eq!(malformed.status.code(), Some(1));
    assert!(stderr_text(&malformed).contains("SHA-256"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn screenshot_honors_cli_dimensions() {
    let dir = temp_dir("dims");
    let out = dir.join("small.png");
    let output = run(&[
        "screenshot",
        "--output",
        out.to_str().unwrap(),
        "--width",
        "640",
        "--height",
        "480",
    ]);
    assert!(output.status.success());
    let report = screenshot_report(&output);
    let get = |key: &str| {
        report
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(get("width"), "640");
    assert_eq!(get("height"), "480");
    assert_eq!(get("stride"), "2560");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn screenshot_honors_config_file() {
    let dir = temp_dir("config");
    let config = dir.join("veyra.toml");
    std::fs::write(
        &config,
        "schema_version = 1\n\n[window]\nwidth = 400\nheight = 300\n",
    )
    .expect("config write");
    let out = dir.join("from-config.png");
    let output = run(&[
        "screenshot",
        "--output",
        out.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr_text(&output));
    let report = screenshot_report(&output);
    let width = report
        .iter()
        .find(|(k, _)| k == "width")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert_eq!(width, "400", "config dimensions must be honored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cli_width_overrides_config() {
    let dir = temp_dir("override");
    let config = dir.join("veyra.toml");
    std::fs::write(&config, "[window]\nwidth = 400\nheight = 300\n").expect("config write");
    let out = dir.join("override.png");
    let output = run(&[
        "screenshot",
        "--output",
        out.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--width",
        "200",
    ]);
    assert!(output.status.success());
    let width = screenshot_report(&output)
        .iter()
        .find(|(k, _)| k == "width")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert_eq!(width, "200");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_output_directory_is_exit_one() {
    let dir = temp_dir("missing");
    let missing = dir.join("no-such-dir").join("out.png");
    let output = run(&["screenshot", "--output", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_text(&output).contains("parent directory does not exist"));
    assert!(!missing.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_config_file_is_exit_one() {
    let dir = temp_dir("noconfig");
    let output = run(&[
        "screenshot",
        "--output",
        dir.join("x.png").to_str().unwrap(),
        "--config",
        dir.join("missing.toml").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_text(&output).contains("cannot read configuration"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_config_is_exit_one_with_field_context() {
    let dir = temp_dir("badconfig");
    let config = dir.join("bad.toml");
    std::fs::write(&config, "[window]\nwidth = 0\n").expect("config write");
    let output = run(&[
        "screenshot",
        "--output",
        dir.join("x.png").to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr_text(&output);
    assert!(err.contains("configuration"), "message: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn usage_errors_exit_two() {
    let cases: [&[&str]; 5] = [
        &["screenshot"],                                           // missing --output
        &["window", "--width"],                                    // missing value
        &["window", "--width", "abc"],                             // bad number
        &["browse"],                                               // unknown command
        &["screenshot", "--output", "/tmp/x.png", "--speed", "5"], // unknown flag
    ];
    for case in cases {
        let output = run(case);
        assert_eq!(
            output.status.code(),
            Some(2),
            "case {case:?} must be a usage error; stderr: {}",
            stderr_text(&output)
        );
        assert!(stderr_text(&output).contains("usage error"));
    }
}

#[test]
fn screenshot_runs_without_a_display_server() {
    // The screenshot path must not require (or consult) a display. Run
    // with display environment stripped: success proves headless purity.
    let dir = temp_dir("headless");
    let out = dir.join("headless.png");
    let output = Command::new(bin())
        .args(["screenshot", "--output", out.to_str().unwrap()])
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("binary spawns");
    assert!(
        output.status.success(),
        "screenshot must be display-independent; stderr: {}",
        stderr_text(&output)
    );
    assert!(out.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

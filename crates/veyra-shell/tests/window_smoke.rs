//! Window smoke tests: real event loop + real presentation, executed
//! under an actual display server.
//!
//! These tests are **display-gated**: they run when a display server is
//! reachable (CI executes them under Xvfb — see `.github/workflows/ci.yml`,
//! `headless-shell` job; locally under any desktop session or
//! `Xvfb :99`). When no display exists they report clearly instead of
//! pretending to pass. The CI job always provides the display, so this
//! code path genuinely executes on every push.
//!
//! Process-level spawning is used deliberately: it also verifies repeated
//! initialization and clean shutdown across separate sessions (edge case
//! from the Phase 1 checklist) and the exact CLI contract.

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

/// Whether a display server is reachable via the standard environment.
fn display_available() -> bool {
    let x11 = std::env::var("DISPLAY").is_ok_and(|v| !v.trim().is_empty());
    let wayland = std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.trim().is_empty());
    x11 || wayland
}

fn run_window(exit_after_frames: u32, width: u32, height: u32) -> Output {
    Command::new(bin())
        .args([
            "window",
            "--exit-after-frames",
            &exit_after_frames.to_string(),
            "--width",
            &width.to_string(),
            "--height",
            &height.to_string(),
        ])
        .output()
        .expect("binary spawns")
}

#[test]
fn window_opens_presents_and_exits_cleanly() {
    if !display_available() {
        panic!(
            "no display server available (DISPLAY/WAYLAND_DISPLAY unset). \
             This test runs for real under Xvfb in CI (`headless-shell` job); \
             locally run it under `xvfb-run` or a desktop session."
        );
    }
    let output = run_window(2, 320, 240);
    assert!(
        output.status.success(),
        "window session must exit cleanly; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("veyra-window v1"),
        "stdout contract: {stdout}"
    );
    let frames = stdout
        .lines()
        .find_map(|l| l.strip_prefix("frames="))
        .unwrap_or("0")
        .parse::<u64>()
        .expect("frames= value");
    assert_eq!(frames, 2, "automation hook must present exactly N frames");
    let micros = stdout
        .lines()
        .find_map(|l| l.strip_prefix("last-frame-micros="))
        .and_then(|v| v.parse::<u64>().ok());
    assert!(micros.unwrap_or(0) > 0, "frame stamp must be present");
}

#[test]
fn repeated_window_sessions_initialize_and_shut_down_cleanly() {
    if !display_available() {
        panic!(
            "no display server available (DISPLAY/WAYLAND_DISPLAY unset). \
             This test runs for real under Xvfb in CI (`headless-shell` job)."
        );
    }
    for session in 0..3 {
        let output = run_window(1, 200, 150);
        assert!(
            output.status.success(),
            "session {session} must exit cleanly; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn window_command_fails_cleanly_without_a_display() {
    // With all display discovery paths removed on Linux, winit cannot
    // create an event loop; the shell must report a typed error (exit 1),
    // not crash and not hang. `--exit-after-frames 1` guarantees the
    // process terminates even if an exotic backend is still discovered
    // (in which case this test fails with a clear message instead of
    // hanging the suite).
    let empty_runtime =
        std::env::temp_dir().join(format!("veyra-no-display-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&empty_runtime);
    let output = Command::new(bin())
        .args(["window", "--exit-after-frames", "1"])
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env("XDG_RUNTIME_DIR", &empty_runtime)
        .output()
        .expect("binary spawns");
    assert_eq!(
        output.status.code(),
        Some(1),
        "missing display is an operation failure, not a crash; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("display server"),
        "error must explain the display situation: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&empty_runtime);
}

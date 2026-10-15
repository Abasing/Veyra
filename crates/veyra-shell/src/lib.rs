//! `veyra-shell` — the Veyra application shell (Phase 1).
//!
//! The first real Veyra executable: a native desktop window rendering the
//! deterministic diagnostic scene, plus the headless screenshot harness,
//! configuration, diagnostics, and the injectable frame clock that later
//! engine phases build on.
//!
//! **This is not a web browser.** There is no HTML, CSS, JavaScript,
//! networking, or web-content rendering here; the window shows a test
//! pattern and the CLI documents exactly what it does. Capability claims
//! live only in `docs/roadmap/compatibility-matrix.md`.
//!
//! Module map:
//!
//! - [`cli`] — argument parsing, exit codes, help/version text.
//! - [`config`] — the TOML configuration schema (strict, reserved
//!   `[policy]` table, `schema_version` gate).
//! - [`diag`] — `tracing` initialization with documented precedence.
//! - [`clock`] — [`FrameClock`] trait: [`clock::SystemFrameClock`]
//!   (production) and [`clock::FixedFrameClock`] (deterministic tests).
//! - [`input`] — input-event plumbing counters (no dispatch yet).
//! - [`screenshot`] — headless render → PNG + canonical pixel hash.
//! - [`window`] — winit event loop + softbuffer presentation.
//! - [`hashing`] — the canonical SHA-256 definition used by golden tests.
//! - [`golden`] — golden-fixture mechanics shared by the integration tests.
//!
//! [`FrameClock`]: clock::FrameClock
//! [`clock::SystemFrameClock`]: clock::SystemFrameClock
//! [`clock::FixedFrameClock`]: clock::FixedFrameClock

#![forbid(unsafe_code)]

pub mod cli;
pub mod clock;
pub mod config;
pub mod diag;
pub mod error;
pub mod golden;
pub mod hashing;
pub mod input;
pub mod screenshot;
pub mod window;

/// Executes a parsed command end-to-end.
///
/// This is the library entry point used by `main` (and reachable from
/// integration tests). It loads configuration, resolves dimensions, and
/// runs the command.
///
/// # Errors
/// Propagates every failure with full context; `main` renders the error
/// chain and maps it to exit code 1.
pub fn execute(command: cli::Command) -> Result<(), error::ShellError> {
    match command {
        cli::Command::Window {
            config,
            width,
            height,
            exit_after_frames,
        } => run_window_command(config, width, height, exit_after_frames),
        cli::Command::Screenshot {
            config,
            width,
            height,
            output,
            expect_hash,
        } => run_screenshot_command(config, width, height, output, expect_hash),
    }
}

fn load_config(path: &Option<std::path::PathBuf>) -> Result<config::Config, error::ShellError> {
    match path {
        Some(path) => Ok(config::Config::load(path)?),
        None => Ok(config::Config::default()),
    }
}

fn run_window_command(
    config_path: Option<std::path::PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    exit_after_frames: Option<u64>,
) -> Result<(), error::ShellError> {
    let config = load_config(&config_path)?;
    let (width, height) = cli::resolve_dimensions(width, height, &config)?;
    diag::init(config.diagnostics.level.level())?;
    tracing::info!(
        width,
        height,
        scene = config.render.scene.kind().as_str(),
        "veyra-shell starting (window mode)"
    );

    let mut clock = clock::SystemFrameClock::new();
    let request = window::WindowRequest {
        width,
        height,
        scene: config.render.scene.kind(),
        title: "Veyra — Phase 1 diagnostics".to_string(),
        exit_after_frames,
    };
    let stats = window::run_window(request, &mut clock)?;
    tracing::info!(
        frames = stats.frames_presented,
        input = %stats.input,
        "window session complete"
    );
    println!(
        "veyra-window v1\nframes={}\nlast-frame-micros={}\n{}",
        stats.frames_presented,
        stats.last_frame_micros.unwrap_or(0),
        stats.input
    );
    Ok(())
}

fn run_screenshot_command(
    config_path: Option<std::path::PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    output: std::path::PathBuf,
    expect_hash: Option<String>,
) -> Result<(), error::ShellError> {
    let config = load_config(&config_path)?;
    let (width, height) = cli::resolve_dimensions(width, height, &config)?;
    diag::init(config.diagnostics.level.level())?;
    tracing::info!(width, height, "veyra-shell starting (screenshot mode)");

    let request = screenshot::ScreenshotRequest {
        width,
        height,
        scene: config.render.scene.kind(),
        output: output.clone(),
        expect_hash,
    };
    let outcome = screenshot::capture(&request)?;
    print!(
        "{}",
        screenshot::format_report(&outcome, config.render.scene.kind())
    );
    Ok(())
}

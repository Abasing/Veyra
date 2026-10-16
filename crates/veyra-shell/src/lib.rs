//! `veyra-shell` — the Veyra application shell (Phase 1).
//!
//! The first real Veyra executable: a native desktop window rendering the
//! deterministic diagnostic scene, plus the headless screenshot harness,
//! configuration, diagnostics, and the injectable frame clock that later
//! engine phases build on.
//!
//! Phase 2 adds the first real document pipeline — `veyra-shell open`
//! loads URLs (or `file://` paths) through the navigation pipeline and
//! prints DOM dumps; session history and crash-safe session persistence
//! work. Phase 3 (batches 1-3) puts the CSS cascade behind it: every
//! loaded document collects its `<style>`/`<link>` sheets (with guarded
//! `@import` following), builds the style set, and computes per-element
//! styles (`open --styles` prints the report). **The window still shows
//! the diagnostic pattern**: web-content rendering (layout, text, paint)
//! is the remaining Phase 3 work. Capability claims live only in
//! `docs/roadmap/compatibility-matrix.md`.
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
//! - [`browser`] — the Phase 2 browser layer: navigation/session state
//!   machine, document loading (network via `veyra-net`, `file://` local)
//!   with the Phase 3 style pipeline (sheet collection, guarded
//!   `@import` following, computed styles), crash-safe session files.
//!
//! [`FrameClock`]: clock::FrameClock
//! [`clock::SystemFrameClock`]: clock::SystemFrameClock
//! [`clock::FixedFrameClock`]: clock::FixedFrameClock

#![forbid(unsafe_code)]

pub mod browser;
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
        cli::Command::Open {
            urls,
            dump_dom,
            dump_styles,
            timeout_ms,
            session_file,
        } => run_open_command(&urls, dump_dom, dump_styles, timeout_ms, session_file),
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

/// The Phase 2 `open` command: drive the navigation pipeline end-to-end.
///
/// Loads each URL in order through the browser layer (network via
/// `veyra-net`, `file://` via the filesystem), commits into session
/// history, then demonstrates history traversal (back once, forward once)
/// when history allows — the roadmap's "navigation back/forward within
/// session history works in the shell" acceptance item. Each visited
/// document prints an `== <url>` marker (plus its html5lib-format DOM
/// dump with `--dump-dom`) on stdout; per-load diagnostics go to stderr.
/// URLs are printed only by this explicit user command, never by
/// `tracing` diagnostics (engineering standards §10).
///
/// # Errors
///
/// Bad URL input and session-write failures are typed errors; individual
/// load failures are reported to stderr and remembered — the command
/// exits with the first one after the run (remaining URLs still load).
fn run_open_command(
    urls: &[String],
    dump_dom: bool,
    dump_styles: bool,
    timeout_ms: Option<u64>,
    session_file: Option<std::path::PathBuf>,
) -> Result<(), error::ShellError> {
    diag::init(config::LogLevel::Info)?;
    let mut config = veyra_net::NetworkConfig::default();
    if let Some(ms) = timeout_ms {
        config.response_deadline = std::time::Duration::from_millis(ms);
    }
    let client = veyra_net::Client::new(config)
        .map_err(|error| error::ShellError::Load(browser::LoadError::Network(error)))?;

    let mut nav = browser::NavigationController::new();
    let jar = veyra_net::Client::new_cookie_jar();
    let mut first_failure: Option<String> = None;

    let visit = |nav: &mut browser::NavigationController, url: &str| match veyra_url::Url::parse(
        url,
    ) {
        Ok(parsed) => {
            if let Err(error) = nav.navigate(parsed.clone()) {
                return Err(error.to_string());
            }
            match browser::load_document(&parsed, &client, Some(&jar)) {
                Ok(document) => {
                    let status_text = document
                        .status
                        .map_or_else(|| "none".to_string(), |status| status.to_string());
                    eprintln!(
                        "loaded status={status_text} bytes={} sheets={} sheet-failures={} imports={}",
                        document.byte_len,
                        document.styles.report.sheets_loaded,
                        document.styles.report.sheets_failed,
                        document.styles.report.imports_followed,
                    );
                    println!("== {}", document.url.as_str());
                    if dump_dom {
                        print!("{}", veyra_html::dump_tree(&document.tree));
                    }
                    if dump_styles {
                        print!("{}", format_style_report(&document));
                    }
                    let entry = browser::SessionEntry::from_document(&document);
                    match nav.commit(entry) {
                        Ok(()) => Ok(()),
                        Err(error) => Err(error.to_string()),
                    }
                }
                Err(error) => {
                    let _ = nav.fail();
                    Err(error.to_string())
                }
            }
        }
        Err(error) => Err(error.to_string()),
    };

    for url in urls {
        if let Err(message) = visit(&mut nav, url) {
            eprintln!("error: {message}");
            first_failure.get_or_insert(message);
        }
    }

    // Session-history traversal: back once, forward once, when possible.
    if nav.can_back() {
        match nav.go_back() {
            Ok(target) => match browser::load_document(&target, &client, Some(&jar)) {
                Ok(document) => {
                    println!("== back: {}", document.url.as_str());
                    if dump_dom {
                        print!("{}", veyra_html::dump_tree(&document.tree));
                    }
                    nav.commit(browser::SessionEntry::from_document(&document))
                        .map_err(error::ShellError::Navigation)?;
                }
                Err(error) => {
                    let _ = nav.fail();
                    eprintln!("error: back navigation load failed: {error}");
                }
            },
            Err(error) => eprintln!("error: back navigation refused: {error}"),
        }
    }
    if nav.can_forward() {
        match nav.go_forward() {
            Ok(target) => match browser::load_document(&target, &client, Some(&jar)) {
                Ok(document) => {
                    println!("== forward: {}", document.url.as_str());
                    if dump_dom {
                        print!("{}", veyra_html::dump_tree(&document.tree));
                    }
                    nav.commit(browser::SessionEntry::from_document(&document))
                        .map_err(error::ShellError::Navigation)?;
                }
                Err(error) => {
                    let _ = nav.fail();
                    eprintln!("error: forward navigation load failed: {error}");
                }
            },
            Err(error) => eprintln!("error: forward navigation refused: {error}"),
        }
    }

    if let Some(path) = session_file {
        let index = nav.current_index().unwrap_or(0);
        let state = browser::state_from_history(nav.history(), index);
        browser::write_session(&path, &state).map_err(error::ShellError::Session)?;
        eprintln!("session written: {}", path.display());
    }

    if let Some(message) = first_failure {
        return Err(error::ShellError::LoadFailed(message));
    }
    Ok(())
}

/// Formats the per-element computed-style report (`open --styles`): one
/// deterministic line per element in document order, carrying the
/// working-set properties the later layout/paint batches consume. The
/// element path is the pre-order chain of local names; every value is
/// the computed (cascade-resolved) form.
fn format_style_report(document: &browser::loader::LoadedDocument) -> String {
    use veyra_dom::{NodeData, NodeId};
    use veyra_style::{Display, LengthPercent, MarginSizing, Sizing};

    let mut out = String::new();
    let Some(document_node) = document.tree.document() else {
        return out;
    };

    fn color_hex(color: veyra_style::Color) -> String {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }

    fn length_text(value: &LengthPercent) -> String {
        match value {
            LengthPercent::Px(px) => format!("{px}px"),
            LengthPercent::Percent(p) => format!("{p}%"),
            // Computed values resolve em/rem to px; these remain only
            // for font-size-relative spec forms the cascade keeps.
            LengthPercent::Em(factor) => format!("{factor}em"),
            LengthPercent::Rem(rem) => format!("{rem}rem"),
        }
    }

    fn margin_text(value: &MarginSizing) -> String {
        match value {
            MarginSizing::Auto => "auto".to_string(),
            MarginSizing::LengthPercent(length) => length_text(length),
        }
    }

    fn sizing_text(value: &Sizing) -> String {
        match value {
            Sizing::Auto => "auto".to_string(),
            Sizing::LengthPercent(length) => length_text(length),
        }
    }

    fn display_text(display: Display) -> &'static str {
        match display {
            Display::None => "none",
            Display::Block => "block",
            Display::Inline => "inline",
            Display::InlineBlock => "inline-block",
        }
    }

    // Iterative pre-order walk with element paths (no recursion on
    // tree depth — the same discipline as the DOM traversals).
    let mut ordinal = 0usize;
    let mut stack: Vec<(NodeId, String)> = vec![(document_node, String::new())];
    while let Some((id, parent_path)) = stack.pop() {
        let children = document.tree.children(id).unwrap_or(&[]);
        for child in children.iter().rev() {
            let name = match document.tree.data(*child) {
                Some(NodeData::Element { name, .. }) => name.local.clone(),
                Some(_) => {
                    // Text/comment/doctype nodes carry no styles; still
                    // walk their children (none) — nothing to do.
                    continue;
                }
                None => continue,
            };
            ordinal += 1;
            let path = if parent_path.is_empty() {
                name.clone()
            } else {
                format!("{parent_path}>{name}")
            };
            let Some(style) = document.styles.computed.get(*child) else {
                continue;
            };
            out.push_str(&format!(
                "style {path}[{ordinal}]: display={} color={} background={} font-size={}px weight={} text-align={:?} width={} height={} margin={}/{}/{}/{} padding={}/{}/{}/{}\n",
                display_text(style.display),
                color_hex(style.color),
                color_hex(style.background_color),
                style.font_size,
                style.font_weight,
                style.text_align,
                sizing_text(&style.width),
                sizing_text(&style.height),
                margin_text(&style.margin.top),
                margin_text(&style.margin.right),
                margin_text(&style.margin.bottom),
                margin_text(&style.margin.left),
                length_text(&style.padding.top),
                length_text(&style.padding.right),
                length_text(&style.padding.bottom),
                length_text(&style.padding.left),
            ));
            let child_path = path;
            stack.push((*child, child_path));
        }
    }
    out
}

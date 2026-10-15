//! Command-line interface: argument parsing and command dispatch.
//!
//! The parser is hand-rolled on purpose (ADR-0012): the surface is three
//! subcommands and a handful of flags, and staying dependency-free here
//! keeps the CLI's error messages under Veyra's control and the
//! dependency tree thin. Exit-code contract:
//!
//! - `0` — success (including `--help` / `--version`).
//! - `1` — operation failure (I/O, rendering, hash mismatch, windowing).
//! - `2` — usage error (unknown flag, missing value, bad number).
//!
//! Dimension resolution order (documented, deterministic): explicit CLI
//! flag > configuration file value > built-in default.

use std::fmt;
use std::path::PathBuf;

use crate::config::{Config, DEFAULT_HEIGHT, DEFAULT_WIDTH};
use crate::error::ConfigError;

/// A parsed top-level command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open the native window and render the scene.
    Window {
        /// Optional configuration file path.
        config: Option<PathBuf>,
        /// CLI width override.
        width: Option<u32>,
        /// CLI height override.
        height: Option<u32>,
        /// Automation hook: exit after N presented frames.
        exit_after_frames: Option<u64>,
    },
    /// Load documents through the navigation pipeline (Phase 2): fetch
    /// (or read file://), parse to a DOM, dump it; then exercise session
    /// history (back/forward) and persist the session when requested.
    Open {
        /// URLs to visit, in order (one or more).
        urls: Vec<String>,
        /// Print the html5lib-format DOM dump for each visited document.
        dump_dom: bool,
        /// Response deadline override, milliseconds.
        timeout_ms: Option<u64>,
        /// Write the final session history to this file.
        session_file: Option<PathBuf>,
    },
    /// Headless screenshot: render → PNG + canonical hash.
    Screenshot {
        /// Optional configuration file path.
        config: Option<PathBuf>,
        /// CLI width override.
        width: Option<u32>,
        /// CLI height override.
        height: Option<u32>,
        /// Output PNG path (required).
        output: PathBuf,
        /// Optional expected canonical hash (hex, case-insensitive).
        expect_hash: Option<String>,
    },
}

/// Result of parsing the argument list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    /// A command to execute.
    Command(Command),
    /// `--help` output (print to stdout, exit 0).
    Help(String),
    /// `--version` output (print to stdout, exit 0).
    Version(String),
}

/// A usage error (exit code 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// Parses command-line arguments (excluding the program name).
///
/// # Errors
/// [`UsageError`] for any malformed invocation, including unknown flags,
/// missing values, and non-numeric sizes. Unknown *subcommand* is also a
/// usage error.
pub fn parse(args: &[String]) -> Result<ParseOutcome, UsageError> {
    if args.is_empty() {
        return Ok(ParseOutcome::Help(help_text()));
    }
    match args[0].as_str() {
        "-h" | "--help" => Ok(ParseOutcome::Help(help_text())),
        "-V" | "--version" => Ok(ParseOutcome::Version(version_text())),
        "window" => parse_window(&args[1..]),
        "screenshot" => parse_screenshot(&args[1..]),
        "open" => parse_open(&args[1..]),
        other => Err(UsageError(format!(
            "unknown command `{other}` (expected `window` or `screenshot`); \
             run `veyra-shell --help`"
        ))),
    }
}

struct Flags {
    config: Option<PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    output: Option<PathBuf>,
    expect_hash: Option<String>,
    exit_after_frames: Option<u64>,
}

impl Flags {
    fn new() -> Self {
        Self {
            config: None,
            width: None,
            height: None,
            output: None,
            expect_hash: None,
            exit_after_frames: None,
        }
    }
}

/// Recognized flag names that take a value.
const VALUE_FLAGS: [&str; 6] = [
    "--config",
    "--width",
    "--height",
    "--output",
    "--expect-hash",
    "--exit-after-frames",
];

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, UsageError> {
    args.get(index)
        .cloned()
        .ok_or_else(|| UsageError(format!("{flag} requires a value (none was supplied)")))
}

fn parse_flag_value(args: &[String], index: usize, flags: &mut Flags) -> Result<usize, UsageError> {
    // Returns the number of extra tokens consumed (always 1: flag value).
    let flag = args[index].as_str();
    if !VALUE_FLAGS.contains(&flag) {
        return Err(UsageError(format!(
            "unknown flag `{flag}`; run `veyra-shell --help`"
        )));
    }
    let value = take_value(args, index + 1, flag)?;
    match flag {
        "--config" => set_once(&mut flags.config, PathBuf::from(value), flag)?,
        "--output" => set_once(&mut flags.output, PathBuf::from(value), flag)?,
        "--expect-hash" => set_once(&mut flags.expect_hash, value, flag)?,
        "--width" => set_once(&mut flags.width, parse_dimension(&value, flag)?, flag)?,
        "--height" => set_once(&mut flags.height, parse_dimension(&value, flag)?, flag)?,
        "--exit-after-frames" => {
            let parsed = value.parse::<u64>().map_err(|_| {
                UsageError(format!(
                    "--exit-after-frames expects a whole number, got `{value}`"
                ))
            })?;
            set_once(&mut flags.exit_after_frames, parsed, flag)?;
        }
        // Unreachable: the VALUE_FLAGS membership check above guarantees a
        // known flag and every known flag has an arm. Kept for exhaustiveness.
        _ => {}
    }
    Ok(1)
}

/// Stores a value into a single-assignment slot, rejecting duplicates.
fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError(format!("{flag} was supplied twice")));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_dimension(value: &str, flag: &str) -> Result<u32, UsageError> {
    value.parse::<u32>().map_err(|_| {
        UsageError(format!(
            "{flag} expects a non-negative whole number, got `{value}`"
        ))
    })
}

fn parse_window(args: &[String]) -> Result<ParseOutcome, UsageError> {
    let mut flags = Flags::new();
    let mut i = 0;
    while i < args.len() {
        let consumed = parse_flag_value(args, i, &mut flags)?;
        i += 1 + consumed;
    }
    if flags.output.is_some() || flags.expect_hash.is_some() {
        return Err(UsageError(
            "--output/--expect-hash belong to the `screenshot` command".to_string(),
        ));
    }
    Ok(ParseOutcome::Command(Command::Window {
        config: flags.config,
        width: flags.width,
        height: flags.height,
        exit_after_frames: flags.exit_after_frames,
    }))
}

/// Parses the `open` command: positional URLs plus its flags.
fn parse_open(args: &[String]) -> Result<ParseOutcome, UsageError> {
    let mut urls: Vec<String> = Vec::new();
    let mut dump_dom = false;
    let mut timeout_ms: Option<u64> = None;
    let mut session_file: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dump-dom" => {
                if dump_dom {
                    return Err(UsageError("--dump-dom given twice".to_string()));
                }
                dump_dom = true;
            }
            "--timeout-ms" => {
                let value = take_value(args, i + 1, "--timeout-ms")?;
                let parsed = value.parse::<u64>().map_err(|_| {
                    UsageError(format!(
                        "--timeout-ms expects a whole number, got `{value}`"
                    ))
                })?;
                set_once(&mut timeout_ms, parsed, "--timeout-ms")?;
                i += 1;
            }
            "--session-file" => {
                let value = take_value(args, i + 1, "--session-file")?;
                set_once(&mut session_file, PathBuf::from(value), "--session-file")?;
                i += 1;
            }
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(UsageError(format!(
                    "unknown flag `{other}` for `open`; run `veyra-shell --help`"
                )));
            }
            other => urls.push(other.to_string()),
        }
        i += 1;
    }
    if urls.is_empty() {
        return Err(UsageError(
            "open requires at least one URL (http/https/file)".to_string(),
        ));
    }
    Ok(ParseOutcome::Command(Command::Open {
        urls,
        dump_dom,
        timeout_ms,
        session_file,
    }))
}

fn parse_screenshot(args: &[String]) -> Result<ParseOutcome, UsageError> {
    let mut flags = Flags::new();
    let mut i = 0;
    while i < args.len() {
        let consumed = parse_flag_value(args, i, &mut flags)?;
        i += 1 + consumed;
    }
    if flags.exit_after_frames.is_some() {
        return Err(UsageError(
            "--exit-after-frames belongs to the `window` command".to_string(),
        ));
    }
    let Some(output) = flags.output else {
        return Err(UsageError(
            "screenshot requires --output PATH (where the PNG is written)".to_string(),
        ));
    };
    Ok(ParseOutcome::Command(Command::Screenshot {
        config: flags.config,
        width: flags.width,
        height: flags.height,
        output,
        expect_hash: flags.expect_hash,
    }))
}

/// Merges CLI overrides with a loaded configuration into final dimensions,
/// applying the same validation as the config schema.
///
/// # Errors
/// [`ConfigError::InvalidValue`] when the resolved dimensions violate the
/// documented limits.
pub fn resolve_dimensions(
    cli_width: Option<u32>,
    cli_height: Option<u32>,
    config: &Config,
) -> Result<(u32, u32), ConfigError> {
    let width = cli_width.unwrap_or(config.window.width);
    let height = cli_height.unwrap_or(config.window.height);
    for (name, value) in [("width", width), ("height", height)] {
        if value == 0 {
            return Err(ConfigError::InvalidValue {
                field: name,
                value: value.to_string(),
                reason: "must be at least 1".to_string(),
            });
        }
        if value > veyra_gfx::MAX_DIMENSION {
            return Err(ConfigError::InvalidValue {
                field: name,
                value: value.to_string(),
                reason: format!("exceeds the maximum of {}", veyra_gfx::MAX_DIMENSION),
            });
        }
    }
    Ok((width, height))
}

/// The rendered `--version` text.
pub fn version_text() -> String {
    format!("veyra-shell {} (Phase 1)", env!("CARGO_PKG_VERSION"))
}

/// The rendered `--help` text (also the no-argument output).
pub fn help_text() -> String {
    format!(
        "\
veyra-shell — the Veyra application shell (Phases 1-2)

Phase 1: a native window with a deterministic diagnostic test pattern and
a headless screenshot harness. Phase 2: the first real document pipeline —
`open` fetches a URL (or reads file://), parses the HTML into a DOM, and
dumps it; session history (back/forward) and crash-safe session files
work. The window does not render web content yet (that is Phase 3).

USAGE:
    veyra-shell <COMMAND> [OPTIONS]

COMMANDS:
    window        Open a native window rendering the diagnostic scene.
    screenshot    Render headlessly and write a PNG + canonical hash.
    open          Load URL(s) through the navigation pipeline and print
                  document dumps.

OPTIONS (both commands):
    --config PATH       Load a TOML configuration file (schema documented
                        in `veyra_shell::config`; unknown keys rejected).
    --width N           Viewport width override (1..=16384).
    --height N          Viewport height override (1..=16384).

OPTIONS (window only):
    --exit-after-frames N   Automation hook: exit cleanly after N frames.

OPTIONS (screenshot only):
    --output PATH       Output PNG path (required; written atomically).
    --expect-hash HEX   Fail unless the canonical pixel hash matches.

OPTIONS (open only):
    --dump-dom          Print the html5lib-format DOM dump of every
                        visited document (stdout).
    --timeout-ms N      Response deadline override in milliseconds.
    --session-file PATH Write the final session history (JSON, written
                        atomically: temp + fsync + rename).

INFO:
    -h, --help          This text.
    -V, --version       Version line.

OUTPUT (screenshot stdout, one key=value pair per line):
    veyra-screenshot v1
    file=<path>
    width=<w>
    height=<h>
    stride=<bytes-per-row>
    scene=diagnostic-v1
    pixels-hash=sha256:<64 hex chars>

EXIT CODES:
    0  success        1  operation failure      2  usage error

DEFAULTS (no --config): width {DEFAULT_WIDTH}, height {DEFAULT_HEIGHT},
log level info (RUST_LOG overrides when valid).
"
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_args_show_help() {
        let outcome = parse(&[]).expect("empty is help");
        assert!(matches!(outcome, ParseOutcome::Help(_)));
    }

    #[test]
    fn help_and_version_flags() {
        assert!(matches!(
            parse(&args(&["--help"])),
            Ok(ParseOutcome::Help(_))
        ));
        assert!(matches!(parse(&args(&["-h"])), Ok(ParseOutcome::Help(_))));
        assert!(matches!(
            parse(&args(&["--version"])),
            Ok(ParseOutcome::Version(_))
        ));
        assert!(matches!(
            parse(&args(&["-V"])),
            Ok(ParseOutcome::Version(_))
        ));
    }

    #[test]
    fn window_command_with_all_flags() {
        let outcome = parse(&args(&[
            "window",
            "--config",
            "/tmp/veyra.toml",
            "--width",
            "640",
            "--height",
            "480",
            "--exit-after-frames",
            "3",
        ]))
        .expect("parses");
        match outcome {
            ParseOutcome::Command(Command::Window {
                config,
                width,
                height,
                exit_after_frames,
            }) => {
                assert_eq!(config, Some(PathBuf::from("/tmp/veyra.toml")));
                assert_eq!(width, Some(640));
                assert_eq!(height, Some(480));
                assert_eq!(exit_after_frames, Some(3));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn screenshot_command_requires_output() {
        let err = parse(&args(&["screenshot"])).expect_err("output required");
        assert!(err.to_string().contains("--output"));
        let outcome = parse(&args(&["screenshot", "--output", "/tmp/x.png"])).expect("ok");
        match outcome {
            ParseOutcome::Command(Command::Screenshot { output, .. }) => {
                assert_eq!(output, PathBuf::from("/tmp/x.png"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn cross_command_flags_are_rejected() {
        let err = parse(&args(&["window", "--output", "/tmp/x.png"])).expect_err("misplaced flag");
        assert!(err.to_string().contains("screenshot"), "message: {err}");
        let err =
            parse(&args(&["screenshot", "--exit-after-frames", "1"])).expect_err("misplaced flag");
        assert!(err.to_string().contains("window"), "message: {err}");
    }

    #[test]
    fn unknown_flag_and_missing_value_and_duplicates() {
        assert!(parse(&args(&["window", "--speed", "4"])).is_err());
        assert!(parse(&args(&["window", "--width"])).is_err());
        assert!(parse(&args(&["window", "--width", "1", "--width", "2"])).is_err());
        assert!(parse(&args(&["screenshot", "--output", "a", "--output", "b"])).is_err());
    }

    #[test]
    fn unknown_command_is_usage_error() {
        let err = parse(&args(&["browse"])).expect_err("no such command");
        assert!(err.to_string().contains("browse"));
    }

    #[test]
    fn bad_numbers_are_usage_errors() {
        assert!(parse(&args(&["window", "--width", "-3"])).is_err());
        assert!(parse(&args(&["window", "--width", "abc"])).is_err());
        assert!(parse(&args(&["window", "--width", "4294967296"])).is_err());
        assert!(parse(&args(&["window", "--exit-after-frames", "x"])).is_err());
    }

    #[test]
    fn resolve_dimensions_precedence_and_limits() {
        let config = Config::default();
        assert_eq!(
            resolve_dimensions(None, None, &config).expect("defaults"),
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        );
        assert_eq!(
            resolve_dimensions(Some(100), Some(50), &config).expect("overrides"),
            (100, 50)
        );
        let zero = resolve_dimensions(Some(0), None, &config).expect_err("zero rejected");
        assert!(zero.to_string().contains("width"));
        let huge = resolve_dimensions(Some(veyra_gfx::MAX_DIMENSION + 1), None, &config)
            .expect_err("huge rejected");
        assert!(huge.to_string().contains("16384"));
    }
}

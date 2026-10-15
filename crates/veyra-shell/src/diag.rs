//! Structured diagnostics: `tracing` initialization for the shell.
//!
//! Policy (system-overview §4, engineering standards §10):
//!
//! - Levels: `error`/`warn`/`info` on by default; `debug`/`trace` opt-in.
//! - Precedence: the `RUST_LOG` environment variable wins when it is set
//!   **and valid**; otherwise the configured level (config
//!   `diagnostics.level`, default `info`) applies. An invalid `RUST_LOG`
//!   value is a hard error — silently falling back would hide a
//!   misconfiguration from the person who set it.
//! - No user data is logged: Phase 1 logs configuration summaries and
//!   lifecycle events only. There is no page content, URL, credential, or
//!   storage value in existence yet, and the discipline starts now.

use tracing_subscriber::EnvFilter;

use crate::config::LogLevel;
pub use crate::error::DiagError;

/// Initializes the global `tracing` subscriber exactly once.
///
/// # Errors
/// [`DiagError::InvalidRustLog`] when `RUST_LOG` is set but invalid;
/// [`DiagError::AlreadyInstalled`] when a subscriber already exists.
pub fn init(level: LogLevel) -> Result<(), DiagError> {
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) if !value.trim().is_empty() => {
            EnvFilter::try_new(&value).map_err(|error| DiagError::InvalidRustLog {
                value,
                message: error.to_string(),
            })?
        }
        _ => EnvFilter::new(level.as_str()),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        // Diagnostics belong on stderr: stdout is the machine-readable
        // output contract (screenshot report / window summary), and log
        // lines must never interleave with it.
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .try_init()
        .map_err(|error| DiagError::AlreadyInstalled {
            message: error.to_string(),
        })
}

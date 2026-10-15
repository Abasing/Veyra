//! Typed errors for `veyra-shell`.
//!
//! The workspace denies `panic`/`unwrap`/`expect` (engineering standards
//! §2): every recoverable failure — bad configuration, refused allocation,
//! failed file write, windowing error, hash mismatch — is a value with a
//! human-readable message and, where applicable, a source error preserved
//! for diagnostics. `main` renders the full error chain to stderr.

use std::fmt;
use std::path::PathBuf;

use veyra_gfx::GfxError;

/// Errors from configuration loading and validation.
#[derive(Debug)]
pub enum ConfigError {
    /// The configuration file could not be read.
    FileRead {
        /// Path that was attempted.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// The configuration file is not valid TOML or violates the schema.
    Parse {
        /// Path of the offending file (or `<inline>` for string sources).
        path: PathBuf,
        /// Deserializer error, including TOML line/column information.
        source: toml::de::Error,
    },
    /// The document's `schema_version` is not supported by this build.
    SchemaVersion {
        /// The version value found in the document.
        found: u32,
        /// The only version this build accepts.
        expected: u32,
    },
    /// A value parsed successfully but failed range/enum validation.
    InvalidValue {
        /// Dotted field name (e.g. `window.width`).
        field: &'static str,
        /// The rejected value, as written.
        value: String,
        /// Why it was rejected.
        reason: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::FileRead { path, source } => {
                write!(
                    f,
                    "cannot read configuration file {}: {source}",
                    path.display()
                )
            }
            ConfigError::Parse { path, source } => {
                write!(f, "invalid configuration in {}: {source}", path.display())
            }
            ConfigError::SchemaVersion { found, expected } => {
                write!(
                    f,
                    "configuration schema_version is {found}, but this build understands \
                     only {expected}; upgrade Veyra or fix the file"
                )
            }
            ConfigError::InvalidValue {
                field,
                value,
                reason,
            } => {
                write!(
                    f,
                    "configuration field {field} has invalid value `{value}`: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::FileRead { source, .. } => Some(source),
            ConfigError::Parse { source, .. } => Some(source),
            ConfigError::SchemaVersion { .. } | ConfigError::InvalidValue { .. } => None,
        }
    }
}

/// Errors from the window/event-loop path (winit + softbuffer).
#[derive(Debug)]
pub enum WindowError {
    /// No usable display/event-loop backend was found (headless machine,
    /// missing display server, or missing platform libraries).
    EventLoopUnavailable {
        /// Underlying winit error.
        source: winit::error::EventLoopError,
    },
    /// The OS refused window creation.
    WindowCreation {
        /// Underlying winit OS error.
        source: winit::error::OsError,
    },
    /// The presentation backend (softbuffer) failed during setup.
    Presentation {
        /// Context/surface creation or resize failure.
        source: softbuffer::SoftBufferError,
    },
    /// Frame-buffer access or present failed.
    Present {
        /// Buffer access or present failure.
        source: softbuffer::SoftBufferError,
    },
}

impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowError::EventLoopUnavailable { source } => {
                write!(
                    f,
                    "no usable display server is available for the window event loop: {source}"
                )
            }
            WindowError::WindowCreation { source } => {
                write!(f, "the operating system refused window creation: {source}")
            }
            WindowError::Presentation { source } => {
                write!(f, "window presentation backend failed: {source}")
            }
            WindowError::Present { source } => {
                write!(f, "writing the frame to the window failed: {source}")
            }
        }
    }
}

impl std::error::Error for WindowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WindowError::EventLoopUnavailable { source } => Some(source),
            WindowError::WindowCreation { source } => Some(source),
            WindowError::Presentation { source } => Some(source),
            WindowError::Present { source } => Some(source),
        }
    }
}

/// Errors from the screenshot harness.
#[derive(Debug)]
pub enum ScreenshotError {
    /// Rendering failed.
    Render {
        /// Underlying gfx error.
        source: GfxError,
    },
    /// The output location could not be written (missing/unwritable parent,
    /// permission denied, disk full).
    OutputWrite {
        /// The final output path that was requested.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// PNG encoding failed.
    PngEncode {
        /// Underlying encoder error.
        source: png::EncodingError,
    },
    /// PNG decoding failed (fixture comparison path).
    PngDecode {
        /// Underlying decoder error.
        source: png::DecodingError,
    },
    /// `--expect-hash` did not match the rendered canonical hash.
    HashMismatch {
        /// The expected hash (lowercase hex).
        expected: String,
        /// The actual hash (lowercase hex).
        actual: String,
        /// Rendered width, for diagnostics.
        width: u32,
        /// Rendered height, for diagnostics.
        height: u32,
    },
    /// The `--expect-hash` value was not a 64-character hex string.
    InvalidHashSpec {
        /// The rejected value.
        value: String,
    },
}

impl fmt::Display for ScreenshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScreenshotError::Render { source } => {
                write!(f, "rendering the scene failed: {source}")
            }
            ScreenshotError::OutputWrite { path, source } => {
                write!(
                    f,
                    "cannot write screenshot to {}: {source} (check that the parent \
                     directory exists and is writable)",
                    path.display()
                )
            }
            ScreenshotError::PngEncode { source } => {
                write!(f, "PNG encoding failed: {source}")
            }
            ScreenshotError::PngDecode { source } => {
                write!(
                    f,
                    "PNG decoding failed (golden fixture comparison): {source}"
                )
            }
            ScreenshotError::HashMismatch {
                expected,
                actual,
                width,
                height,
            } => {
                write!(
                    f,
                    "canonical pixel hash mismatch at {width}x{height}:\n  expected {expected}\n  \
                     actual   {actual}\nThe rendered image differs from the expected content."
                )
            }
            ScreenshotError::InvalidHashSpec { value } => {
                write!(
                    f,
                    "`{value}` is not a valid SHA-256 hash (expected 64 hexadecimal characters)"
                )
            }
        }
    }
}

impl std::error::Error for ScreenshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScreenshotError::Render { source } => Some(source),
            ScreenshotError::OutputWrite { source, .. } => Some(source),
            ScreenshotError::PngEncode { source } => Some(source),
            ScreenshotError::PngDecode { source } => Some(source),
            ScreenshotError::HashMismatch { .. } | ScreenshotError::InvalidHashSpec { .. } => None,
        }
    }
}

/// Errors from frame-clock construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    /// A zero step would freeze time; it is rejected at construction.
    ZeroStep,
    /// Advancing the clock overflowed its 64-bit microsecond counter.
    Overflow,
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClockError::ZeroStep => write!(f, "frame-clock step must be at least 1 microsecond"),
            ClockError::Overflow => write!(f, "frame clock overflowed its microsecond counter"),
        }
    }
}

impl std::error::Error for ClockError {}

/// Errors from diagnostics initialization.
#[derive(Debug)]
pub enum DiagError {
    /// `RUST_LOG` was set but is not a valid filter expression.
    InvalidRustLog {
        /// The rejected environment value.
        value: String,
        /// The parser's complaint, verbatim.
        message: String,
    },
    /// A global subscriber was already installed.
    AlreadyInstalled {
        /// The `try_init` failure message (this failure is purely
        /// human-facing; it is never matched programmatically).
        message: String,
    },
}

impl fmt::Display for DiagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagError::InvalidRustLog { value, message } => {
                write!(
                    f,
                    "RUST_LOG is set to `{value}` but that is not a valid filter \
                     expression: {message} (examples: `info`, `veyra_shell=debug`)"
                )
            }
            DiagError::AlreadyInstalled { message } => {
                write!(f, "diagnostics already initialized: {message}")
            }
        }
    }
}

impl std::error::Error for DiagError {}

/// Top-level shell error, rendered by `main` with its source chain.
#[derive(Debug)]
pub enum ShellError {
    /// Configuration loading/validation failed.
    Config(ConfigError),
    /// Window/event-loop/presentation failed.
    Window(WindowError),
    /// Screenshot harness failed.
    Screenshot(ScreenshotError),
    /// Frame-clock construction failed.
    Clock(ClockError),
    /// Diagnostics initialization failed.
    Diagnostics(DiagError),
    /// Rendering failed outside the screenshot path.
    Gfx(GfxError),
    /// Navigation/document loading failed.
    Navigation(crate::browser::navigation::NavigationError),
    /// A document could not be loaded.
    Load(crate::browser::loader::LoadError),
    /// A load failed during the `open` run (message for the first failure).
    LoadFailed(String),
    /// Session persistence failed.
    Session(crate::browser::session::SessionError),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellError::Config(source) => write!(f, "configuration error: {source}"),
            ShellError::Window(source) => write!(f, "window error: {source}"),
            ShellError::Screenshot(source) => write!(f, "screenshot error: {source}"),
            ShellError::Clock(source) => write!(f, "frame-clock error: {source}"),
            ShellError::Diagnostics(source) => write!(f, "diagnostics error: {source}"),
            ShellError::Gfx(source) => write!(f, "rendering error: {source}"),
            ShellError::Navigation(source) => write!(f, "navigation error: {source}"),
            ShellError::Load(source) => write!(f, "document load failed: {source}"),
            ShellError::LoadFailed(message) => write!(f, "document load failed: {message}"),
            ShellError::Session(source) => write!(f, "session error: {source}"),
        }
    }
}

impl std::error::Error for ShellError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ShellError::Config(source) => Some(source),
            ShellError::Window(source) => Some(source),
            ShellError::Screenshot(source) => Some(source),
            ShellError::Clock(source) => Some(source),
            ShellError::Diagnostics(source) => Some(source),
            ShellError::Gfx(source) => Some(source),
            ShellError::Navigation(source) => Some(source),
            ShellError::Load(source) => Some(source),
            ShellError::LoadFailed(_) => None,
            ShellError::Session(source) => Some(source),
        }
    }
}

impl From<ConfigError> for ShellError {
    fn from(source: ConfigError) -> Self {
        ShellError::Config(source)
    }
}

impl From<WindowError> for ShellError {
    fn from(source: WindowError) -> Self {
        ShellError::Window(source)
    }
}

impl From<ScreenshotError> for ShellError {
    fn from(source: ScreenshotError) -> Self {
        ShellError::Screenshot(source)
    }
}

impl From<ClockError> for ShellError {
    fn from(source: ClockError) -> Self {
        ShellError::Clock(source)
    }
}

impl From<DiagError> for ShellError {
    fn from(source: DiagError) -> Self {
        ShellError::Diagnostics(source)
    }
}

impl From<GfxError> for ShellError {
    fn from(source: GfxError) -> Self {
        ShellError::Gfx(source)
    }
}

//! Configuration schema for the Veyra shell (Phase 1).
//!
//! One schema, owned by `veyra-shell` (system-overview §4), extended by
//! later phases. Contract:
//!
//! - **Format:** TOML. Loading is explicit: `veyra-shell` never searches
//!   for a file unless `--config PATH` says so; without it, documented
//!   defaults apply (all values below).
//! - **Strictness:** unknown keys are rejected (`deny_unknown_fields` on
//!   every table) and out-of-range/enum values are rejected with the field
//!   name and reason — nothing is silently coerced or ignored.
//! - **Reserved fields:** the `[policy]` table is *reserved* for the
//!   Phase 6+ enterprise-policy work. This build accepts an **empty**
//!   `[policy]` table (so deployments can pre-create it) and rejects any
//!   keys inside it, because no policy semantics exist to interpret them.
//!   `schema_version` gates future schema evolution.
//!
//! A sample document lives at `tests/fixtures/sample-config.toml` (the
//! benchmark fixture parses it).

use std::fmt;
use std::path::Path;

use serde::Deserialize;

use crate::error::ConfigError;

/// The only configuration schema version this build understands.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Default window width when no configuration is supplied.
pub const DEFAULT_WIDTH: u32 = 800;

/// Default window height when no configuration is supplied.
pub const DEFAULT_HEIGHT: u32 = 600;

/// Log level for shell diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    /// `error` — failures only.
    Error,
    /// `warn` — failures + unusual-but-recoverable conditions.
    Warn,
    /// `info` — the default: lifecycle events (startup, config, shutdown).
    #[default]
    Info,
    /// `debug` — per-frame/per-event details, opt-in.
    Debug,
    /// `trace` — everything, opt-in.
    Trace,
}

impl LogLevel {
    /// The canonical lowercase name, as written in configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }

    fn parse(value: &str, field: &'static str) -> Result<Self, ConfigError> {
        match value {
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            other => Err(ConfigError::InvalidValue {
                field,
                value: other.to_string(),
                reason: "expected one of: error, warn, info, debug, trace".to_string(),
            }),
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The diagnostic scene to render. Phase 1 ships exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SceneKind {
    /// The deterministic diagnostic test pattern
    /// (`veyra_gfx::fixture`, version-tagged for golden images).
    #[default]
    Diagnostic,
}

impl SceneKind {
    /// The canonical name, as written in configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            SceneKind::Diagnostic => "diagnostic",
        }
    }

    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "diagnostic" => Ok(SceneKind::Diagnostic),
            other => Err(ConfigError::InvalidValue {
                field: "render.scene",
                value: other.to_string(),
                reason: "expected `diagnostic` (the only scene in this phase)".to_string(),
            }),
        }
    }
}

/// Window settings. Defaults are the golden/CI viewport.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WindowConfig {
    /// Window width in pixels (1..=`veyra_gfx::MAX_DIMENSION`).
    pub width: u32,
    /// Window height in pixels (1..=`veyra_gfx::MAX_DIMENSION`).
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }
}

/// Rendering settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RenderConfig {
    /// Which deterministic scene to render.
    pub scene: SceneChoice,
}

/// Newtype so serde can surface the exact written value in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneChoice(SceneKind);

impl SceneChoice {
    /// The parsed scene kind.
    pub fn kind(self) -> SceneKind {
        self.0
    }
}

impl<'de> Deserialize<'de> for SceneChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        SceneKind::parse(&value)
            .map_err(serde::de::Error::custom)
            .map(SceneChoice)
    }
}

/// Diagnostics settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DiagnosticsConfig {
    /// Default log level when `RUST_LOG` is unset (see `diag`).
    pub level: LevelChoice,
}

/// Newtype carrying the written value through to typed errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelChoice(LogLevel);

impl LevelChoice {
    /// The parsed level.
    pub fn level(self) -> LogLevel {
        self.0
    }
}

impl<'de> Deserialize<'de> for LevelChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        LogLevel::parse(&value, "diagnostics.level")
            .map_err(serde::de::Error::custom)
            .map(LevelChoice)
    }
}

/// Reserved for Phase 6+ enterprise policy. Any key inside this table is
/// rejected: no policy semantics exist to interpret it today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {}

/// The parsed, validated shell configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Schema gate; must be [`CONFIG_SCHEMA_VERSION`] if present.
    pub schema_version: Option<u32>,
    /// Window settings.
    pub window: WindowConfig,
    /// Rendering settings.
    pub render: RenderConfig,
    /// Diagnostics settings.
    pub diagnostics: DiagnosticsConfig,
    /// Reserved policy table (must be empty in this phase).
    pub policy: PolicyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema_version: Some(CONFIG_SCHEMA_VERSION),
            window: WindowConfig::default(),
            render: RenderConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
            policy: PolicyConfig {},
        }
    }
}

impl Config {
    /// Parses and validates a configuration document from a string.
    ///
    /// # Errors
    /// [`ConfigError::Parse`] for TOML/schema violations,
    /// [`ConfigError::SchemaVersion`] for an unsupported version,
    /// [`ConfigError::InvalidValue`] for out-of-range values.
    pub fn parse_document(text: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(text).map_err(|source| ConfigError::Parse {
            path: "<inline>".into(),
            source,
        })?;
        config.validate()
    }

    /// Loads and validates a configuration file.
    ///
    /// # Errors
    /// [`ConfigError::FileRead`] when the file cannot be read; parse and
    /// validation errors as [`Config::from_str`].
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::FileRead {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        config.validate()
    }

    /// Range/schema validation beyond what serde expresses.
    fn validate(self) -> Result<Self, ConfigError> {
        if let Some(version) = self.schema_version
            && version != CONFIG_SCHEMA_VERSION
        {
            return Err(ConfigError::SchemaVersion {
                found: version,
                expected: CONFIG_SCHEMA_VERSION,
            });
        }
        for (name, value) in [
            ("window.width", self.window.width),
            ("window.height", self.window.height),
        ] {
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
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    // Scoped exception, documented as in veyra-gfx: test fixtures are
    // internal; tests fail through the standard assert machinery.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/sample-config.toml");

    #[test]
    fn defaults_parse_and_match_documented_values() {
        let config = Config::parse_document("").expect("empty document = defaults");
        assert_eq!(config, Config::default());
        assert_eq!(config.window.width, DEFAULT_WIDTH);
        assert_eq!(config.window.height, DEFAULT_HEIGHT);
        assert_eq!(config.render.scene.kind(), SceneKind::Diagnostic);
        assert_eq!(config.diagnostics.level.level(), LogLevel::Info);
    }

    #[test]
    fn sample_fixture_parses() {
        let config = Config::parse_document(SAMPLE).expect("fixture must parse");
        assert_eq!(config.window.width, 800);
        assert_eq!(config.window.height, 600);
        assert_eq!(config.render.scene.kind(), SceneKind::Diagnostic);
        assert_eq!(config.diagnostics.level.level(), LogLevel::Debug);
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        let err = Config::parse_document("[mystery]\nkey = 1\n").expect_err("must reject");
        assert!(err.to_string().contains("mystery"), "message: {err}");
    }

    #[test]
    fn unknown_nested_key_is_rejected() {
        let err = Config::parse_document("[window]\nwidth = 100\nextra = true\n")
            .expect_err("must reject");
        assert!(err.to_string().contains("extra"), "message: {err}");
    }

    #[test]
    fn policy_table_rejects_any_keys() {
        let err = Config::parse_document("[policy]\nsome_future_flag = true\n")
            .expect_err("reserved policy must reject keys");
        assert!(
            err.to_string().contains("some_future_flag"),
            "message: {err}"
        );
        // Empty policy table is accepted (documented behavior).
        assert!(Config::parse_document("[policy]\n").is_ok());
    }

    #[test]
    fn schema_version_gate() {
        let wrong = Config::parse_document("schema_version = 2\n").expect_err("must reject v2");
        assert!(matches!(
            wrong,
            ConfigError::SchemaVersion {
                found: 2,
                expected: 1
            }
        ));
        assert!(Config::parse_document("schema_version = 1\n").is_ok());
    }

    #[test]
    fn dimension_validation() {
        let zero = Config::parse_document("[window]\nwidth = 0\n").expect_err("must reject 0");
        assert!(
            zero.to_string().contains("window.width"),
            "message should name the field: {zero}"
        );
        let huge =
            Config::parse_document("[window]\nwidth = 99999\n").expect_err("must reject huge");
        assert!(huge.to_string().contains("16384"), "message: {huge}");
        assert!(Config::parse_document("[window]\nwidth = 16384\nheight = 16384\n").is_ok());
    }

    #[test]
    fn enum_validation() {
        let bad_scene =
            Config::parse_document("[render]\nscene = \"fancy\"\n").expect_err("must reject");
        assert!(
            bad_scene.to_string().contains("diagnostic"),
            "message: {bad_scene}"
        );
        let bad_level =
            Config::parse_document("[diagnostics]\nlevel = \"loud\"\n").expect_err("must reject");
        assert!(
            bad_level.to_string().contains("error, warn"),
            "message: {bad_level}"
        );
    }

    #[test]
    fn invalid_toml_reports_parse_error() {
        let err = Config::parse_document("[window\nwidth = = 3\n").expect_err("must reject");
        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn missing_file_is_file_read_error() {
        let err = Config::load(Path::new("/nonexistent/veyra-definitely-missing.toml"))
            .expect_err("missing file");
        assert!(matches!(err, ConfigError::FileRead { .. }));
    }
}

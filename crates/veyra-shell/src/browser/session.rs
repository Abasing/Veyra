//! Crash-safe session persistence (roadmap Phase 2 deliverable):
//! temp-file write, fsync, atomic rename.
//!
//! The write path is: serialize → write `<path>.tmp` → flush + fsync →
//! rename over `<path>`. A crash before the rename leaves the previous
//! session intact and a stale `.tmp` behind (cleaned on next write); a
//! crash after it leaves the new session. The reader accepts only
//! well-formed files and reports everything else as typed errors.

use std::fs::File;
use std::io::Write as IoWrite;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::browser::navigation::SessionEntry;
use veyra_url::Url;

/// On-disk session state (JSON).
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SessionFileState {
    /// Schema version (bumped on incompatible changes; readers reject
    /// unknown versions).
    pub version: u32,
    /// History URLs, oldest first.
    pub urls: Vec<String>,
    /// Index of the current entry.
    pub index: usize,
}

/// The schema version this build writes and accepts.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// Session persistence failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum SessionError {
    /// The file could not be written (temp create, write, fsync, rename).
    Write {
        /// The target path.
        path: std::path::PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file could not be read.
    Read {
        /// The path attempted.
        path: std::path::PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file is not a valid session (bad JSON, wrong schema version,
    /// out-of-bounds index).
    Corrupt {
        /// The path attempted.
        path: std::path::PathBuf,
        /// Why it was rejected.
        reason: String,
    },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Write { path, source } => {
                write!(f, "session write to {} failed: {source}", path.display())
            }
            Self::Read { path, source } => {
                write!(f, "session read from {} failed: {source}", path.display())
            }
            Self::Corrupt { path, reason } => {
                write!(f, "session file {} is corrupt: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for SessionError {}

/// Writes the session atomically: temp file → fsync → rename.
///
/// # Errors
///
/// [`SessionError::Write`] for any filesystem failure on the way.
pub fn write_session(path: &Path, state: &SessionFileState) -> Result<(), SessionError> {
    let temp_path = temp_path_for(path);
    let serialized = serde_json::to_vec(state).map_err(|error| SessionError::Write {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()),
    })?;
    let mut file = File::create(&temp_path).map_err(|source| SessionError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(&serialized)
        .map_err(|source| SessionError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| SessionError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    drop(file);
    std::fs::rename(&temp_path, path).map_err(|source| SessionError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Reads and validates a session file.
///
/// # Errors
///
/// [`SessionError::Read`] for I/O failures; [`SessionError::Corrupt`] for
/// malformed content (bad JSON, unknown schema version, index out of
/// bounds, non-URL strings).
pub fn load_session(path: &Path) -> Result<SessionFileState, SessionError> {
    let bytes = std::fs::read(path).map_err(|source| SessionError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let state: SessionFileState =
        serde_json::from_slice(&bytes).map_err(|error| SessionError::Corrupt {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
    if state.version != SESSION_SCHEMA_VERSION {
        return Err(SessionError::Corrupt {
            path: path.to_path_buf(),
            reason: format!(
                "schema version {} unsupported (expected {})",
                state.version, SESSION_SCHEMA_VERSION
            ),
        });
    }
    if state.index >= state.urls.len() && !state.urls.is_empty() {
        return Err(SessionError::Corrupt {
            path: path.to_path_buf(),
            reason: format!(
                "index {} out of bounds for {} entries",
                state.index,
                state.urls.len()
            ),
        });
    }
    for url in &state.urls {
        if Url::parse(url).is_err() {
            return Err(SessionError::Corrupt {
                path: path.to_path_buf(),
                reason: format!("entry is not a valid URL: {url:?}"),
            });
        }
    }
    Ok(state)
}

/// Converts a controller's history into the file state.
#[must_use]
pub fn state_from_history(history: &[SessionEntry], index: usize) -> SessionFileState {
    SessionFileState {
        version: SESSION_SCHEMA_VERSION,
        urls: history.iter().map(|entry| entry.url.to_string()).collect(),
        index,
    }
}

/// Rebuilds session entries from file state (URLs re-parsed; origins
/// derived — the file stores no origin data).
///
/// # Errors
///
/// [`SessionError::Corrupt`] when any URL fails to re-parse.
pub fn entries_from_state(state: &SessionFileState) -> Result<Vec<SessionEntry>, SessionError> {
    state
        .urls
        .iter()
        .map(|url| {
            Url::parse(url).map(|parsed| {
                let origin = parsed.origin();
                SessionEntry {
                    url: parsed,
                    origin,
                    status: None,
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SessionError::Corrupt {
            path: std::path::PathBuf::new(),
            reason: error.to_string(),
        })
}

fn temp_path_for(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().map_or_else(
        || "veyra-session".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    path.with_file_name(format!("{name}.tmp"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use veyra_url::Origin;

    fn entry(url: &str) -> SessionEntry {
        let parsed = Url::parse(url).unwrap();
        SessionEntry {
            url: parsed.clone(),
            origin: parsed.origin(),
            status: Some(200),
        }
    }

    #[test]
    fn roundtrip_preserves_urls_and_index() {
        let dir = std::env::temp_dir().join(format!("veyra-session-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");

        let history = vec![entry("https://a.example/"), entry("https://b.example/x")];
        let state = state_from_history(&history, 1);
        write_session(&path, &state).unwrap();

        let loaded = load_session(&path).unwrap();
        assert_eq!(loaded, state);
        assert_eq!(
            loaded.urls,
            vec!["https://a.example/", "https://b.example/x"]
        );
        assert_eq!(loaded.index, 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failed_write_leaves_previous_session_intact() {
        // Crash-safety property: the target is only ever replaced by a
        // rename of a fully-written, fsynced temp file. A write that fails
        // before that point cannot damage the previous session.
        let dir = std::env::temp_dir().join(format!("veyra-session-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");

        let state = state_from_history(&[entry("https://ok.example/")], 0);
        write_session(&path, &state).unwrap();
        let before = std::fs::read(&path).unwrap();

        // A write whose temp file cannot be created (missing directory)
        // fails with a typed error and leaves the original untouched.
        let doomed = dir.join("missing-dir").join("session.json");
        assert!(matches!(
            write_session(&doomed, &state),
            Err(SessionError::Write { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);

        // A completed write leaves no temp file behind.
        assert!(!dir.join("session.json.tmp").exists());

        // Invalid states are rejected on read, never half-accepted.
        let bad = SessionFileState {
            version: 1,
            urls: vec!["https://ok.example/".to_string()],
            index: 9,
        };
        write_session(&path, &bad).unwrap();
        assert!(matches!(
            load_session(&path),
            Err(SessionError::Corrupt { .. })
        ));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_inputs_are_typed_errors() {
        let dir = std::env::temp_dir().join(format!("veyra-session-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");

        std::fs::write(&path, b"not json").unwrap();
        assert!(matches!(
            load_session(&path),
            Err(SessionError::Corrupt { .. })
        ));

        std::fs::write(
            &path,
            br#"{"version": 99, "urls": ["https://a.example/"], "index": 0}"#,
        )
        .unwrap();
        assert!(matches!(
            load_session(&path),
            Err(SessionError::Corrupt { .. })
        ));

        std::fs::write(
            &path,
            br#"{"version": 1, "urls": ["https://a.example/"], "index": 5}"#,
        )
        .unwrap();
        assert!(matches!(
            load_session(&path),
            Err(SessionError::Corrupt { .. })
        ));

        std::fs::write(&path, br#"{"version": 1, "urls": ["::::"], "index": 0}"#).unwrap();
        assert!(matches!(
            load_session(&path),
            Err(SessionError::Corrupt { .. })
        ));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn entries_rebuild_from_state() {
        let state = SessionFileState {
            version: 1,
            urls: vec!["https://a.example/".to_string()],
            index: 0,
        };
        let entries = entries_from_state(&state).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url.as_str(), "https://a.example/");
        assert!(matches!(entries[0].origin, Origin::Tuple { .. }));
    }
}

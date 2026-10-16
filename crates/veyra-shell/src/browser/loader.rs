//! Document loading for navigation: URL → bytes → parsed, origin-tagged
//! tree.
//!
//! The pipeline per the roadmap's Phase 2 scope: request policy and HTTP
//! via `veyra-net` (the policy layer — the shell never touches sockets),
//! `file://` via the local filesystem (same parse path), HTML parsing via
//! `veyra-html`, and a `veyra-dom::Tree` out. Everything else is a typed
//! [`LoadError`].

use veyra_dom::Tree;
use veyra_url::{Origin, Url};

use crate::browser::navigation::SessionEntry;
use crate::browser::style_loader::{self, DocumentStyles};

/// A loaded document: parsed tree plus the metadata navigation records.
pub struct LoadedDocument {
    /// The final URL (post-redirect).
    pub url: Url,
    /// The origin the document carries (parse-derived).
    pub origin: Origin,
    /// The HTTP status, or `None` for non-HTTP loads.
    pub status: Option<u16>,
    /// The parsed document tree.
    pub tree: Tree,
    /// The body size in bytes (post-decode for files, network size for
    /// HTTP).
    pub byte_len: usize,
    /// The document's computed styles (batch 3): the UA sheet plus the
    /// document's `<style>`/`<link>` sheets (and their `@import`
    /// closure), cascaded over the tree. Sheet-level failures are
    /// diagnostics (see [`DocumentStyles::report`]), never load errors.
    pub styles: DocumentStyles,
}

/// Load failure (typed; transport details ride along for diagnostics).
#[derive(Debug)]
#[non_exhaustive]
pub enum LoadError {
    /// The URL scheme is not loadable by the shell (not http/https/file).
    UnsupportedScheme(String),
    /// Network failure (policy, transport, limits — see `veyra-net`).
    Network(veyra_net::NetError),
    /// The HTML parse rejected the document (limits) — the document is
    /// not loaded partially.
    Parse(veyra_html::HtmlError),
    /// A file:// load failed I/O.
    FileIo {
        /// The path attempted.
        path: std::path::PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A file:// URL did not convert to a local path.
    NotALocalFile,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedScheme(scheme) => write!(f, "scheme {scheme:?} cannot be loaded"),
            Self::Network(error) => write!(f, "network load failed: {error}"),
            Self::Parse(error) => write!(f, "document rejected: {error}"),
            Self::FileIo { path, source } => {
                write!(f, "file {} could not be read: {source}", path.display())
            }
            Self::NotALocalFile => write!(f, "file URL does not name a local path"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Loads `url` into a parsed document (network or filesystem).
///
/// # Errors
///
/// [`LoadError`] variants for every failure mode; a rejected parse
/// yields no tree.
pub fn load_document(
    url: &Url,
    client: &veyra_net::Client,
    jar: Option<&veyra_net::cookie::CookieJar>,
) -> Result<LoadedDocument, LoadError> {
    match url.scheme() {
        "http" | "https" => {
            let resource =
                veyra_net::blocking::fetch(client, url, jar).map_err(LoadError::Network)?;
            let origin = resource.origin();
            let tree = veyra_html::parse_document(&resource.body, origin.clone())
                .map_err(LoadError::Parse)?;
            let styles =
                style_loader::load_document_styles(&tree, &resource.final_url, client, jar);
            Ok(LoadedDocument {
                url: resource.final_url,
                origin,
                status: Some(resource.status),
                tree,
                byte_len: resource.body.len(),
                styles,
            })
        }
        "file" => {
            let path = url.to_file_path().map_err(|_| LoadError::NotALocalFile)?;
            let bytes = std::fs::read(&path).map_err(|source| LoadError::FileIo {
                path: path.clone(),
                source,
            })?;
            let origin = url.origin();
            let tree =
                veyra_html::parse_document(&bytes, origin.clone()).map_err(LoadError::Parse)?;
            let styles = style_loader::load_document_styles(&tree, url, client, jar);
            Ok(LoadedDocument {
                url: url.clone(),
                origin,
                status: None,
                tree,
                byte_len: bytes.len(),
                styles,
            })
        }
        other => Err(LoadError::UnsupportedScheme(other.to_string())),
    }
}

/// Builds the session-history entry for a loaded document.
#[must_use]
pub fn entry_for(document: &LoadedDocument) -> SessionEntry {
    SessionEntry::from_document(document)
}

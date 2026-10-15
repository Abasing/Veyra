//! The synchronous bridge for non-async callers (the shell's navigation
//! worker threads): one current-thread runtime per fetch.
//!
//! The engine core stays synchronous (engineering standards §5); this
//! module is the documented seam. Spawning a runtime per call is not free
//! — the async [`Client::fetch`](crate::Client::fetch) surface remains the
//! path for runtime-hosted callers.

use veyra_url::Url;

use crate::cookie::CookieJar;
use crate::{Client, LoadedResource, NetError};

/// Fetches `url` synchronously (navigation GET) with `client`.
///
/// # Errors
///
/// As [`Client::fetch`](crate::Client::fetch).
pub fn fetch(
    client: &Client,
    url: &Url,
    jar: Option<&CookieJar>,
) -> Result<LoadedResource, NetError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| NetError::Transport(error.to_string()))?;
    runtime.block_on(client.fetch(url, jar))
}

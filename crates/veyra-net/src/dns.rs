//! DNS resolution: hickory stub resolver behind hyper-util's connector
//! interface (ADR-0008's DNS/DoH adoption, transport isolation §5).
//!
//! The wrapper is a tower `Service<Name>` — the shape hyper-util's legacy
//! client consumes — backed by the system resolver configuration
//! (`/etc/resolv.conf`). IP-literal hosts never reach this service
//! (hyper's HTTP connector short-circuits them), so fixture-server tests
//! against `127.0.0.1` are DNS-free.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use hickory_resolver::TokioResolver;
use hyper_util::client::legacy::connect::dns::Name;
use tower_service::Service as TowerService;

/// A hyper-compatible DNS resolver over hickory's Tokio stub resolver.
#[derive(Clone)]
pub struct HickoryDns {
    resolver: TokioResolver,
}

impl HickoryDns {
    /// Builds the resolver from the system configuration.
    ///
    /// # Errors
    ///
    /// [`DnsSetupError`] when the system resolver configuration cannot be
    /// read or parsed (e.g. no resolv.conf in a container).
    pub fn from_system_config() -> Result<Self, DnsSetupError> {
        let builder = TokioResolver::builder_tokio()?;
        // hickory 0.26: `build` reports resolver-construction failures
        // (e.g. malformed system configuration) as `NetError`.
        let resolver = builder.build().map_err(DnsSetupError::from)?;
        Ok(Self { resolver })
    }
}

/// Failure to construct the DNS resolver.
#[derive(Debug)]
pub struct DnsSetupError {
    message: String,
}

impl std::fmt::Display for DnsSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DNS resolver setup failed: {}", self.message)
    }
}

impl std::error::Error for DnsSetupError {}

impl From<hickory_resolver::net::NetError> for DnsSetupError {
    fn from(error: hickory_resolver::net::NetError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

/// The resolved-address iterator the connector expects.
pub type ResolvedAddrs = std::vec::IntoIter<SocketAddr>;

/// The per-lookup error (boxed: the connector erases it anyway).
pub type ResolveError = Box<dyn std::error::Error + Send + Sync>;

/// The lookup future.
pub type ResolveFuture = Pin<Box<dyn Future<Output = Result<ResolvedAddrs, ResolveError>> + Send>>;

impl TowerService<Name> for HickoryDns {
    type Response = ResolvedAddrs;
    type Error = ResolveError;
    type Future = ResolveFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The stub resolver is always ready; connections spawn lazily.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let resolver = self.resolver.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let lookup = resolver
                .lookup_ip(host.as_str())
                .await
                .map_err(|error| -> ResolveError { Box::new(error) })?;
            let addrs: Vec<SocketAddr> = lookup.iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            if addrs.is_empty() {
                return Err("no addresses resolved".into());
            }
            Ok(addrs.into_iter())
        })
    }
}

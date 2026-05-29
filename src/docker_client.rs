//! Docker transport layer for synapse2 (`flux` domain).
//!
//! This is the Docker spine: a [`DockerClient`] trait (segregated into
//! [`ContainerOps`], [`ImageOps`], [`NetworkOps`], [`VolumeOps`], [`SystemOps`]
//! sub-traits) plus the [`BollardClient`] implementation and a per-host
//! [`DockerClientCache`].
//!
//! Design (locked decisions — see bead rmcp-template-3tt.2):
//!
//! - **bollard everywhere.** No `docker` CLI subprocess for any operation. The
//!   client returns bollard's typed structs (`ContainerSummary`, `SystemInfo`,
//!   …) — the input to B4's formatters. No stdout parsing.
//! - **`bollard::Docker` is cheap to Clone** (internally `Arc<ClientType>` +
//!   `Arc<hyper>`), so each cache entry holds it **by value** inside a
//!   [`BollardClient`] bundle. For remote hosts the bundle also owns the
//!   [`ForwardedSocket`] + `Arc<PooledSession>` that keep the unix socket alive;
//!   bollard's `Docker` is only valid while that forward lives, so the bundle is
//!   the unit of caching (handed out as `Arc<BollardClient>`).
//! - **Per-host cache keyed by `HostConfig.name`.** One `BollardClient` per
//!   host, reused across calls. Concurrent creation for the same host is
//!   deduplicated through a per-key `OnceCell` — this also gives the
//!   "same instance on repeated lookup" property and prevents two racing callers
//!   from binding the *same* deterministic forward socket path.
//! - **Transport selection** mirrors synapse-mcp's `client-factory.ts`:
//!   - Local (`HostProtocol::Local` / `localhost`): an explicit
//!     `docker_socket_path` wins, else `connect_with_unix_defaults()`.
//!   - Remote (SSH): `pool.checkout(host)` → `ForwardedSocket::open(session,
//!     forward_socket_path(host))` → `connect_with_socket(path, …)`.
//! - **API version negotiation:** use bollard's `API_DEFAULT_VERSION` — do not
//!   hardcode a version string (locked decision).
//! - **BrokenPipe eviction** (HIGH, perf-oracle): a cached client whose SSH
//!   tunnel died returns IO `BrokenPipe` / `ConnectionRefused`. [`is_transport_dead`]
//!   classifies those; [`DockerClientCache::invalidate`] evicts the cache entry
//!   *and* the underlying SSH session so the next checkout rebuilds. B8/B9 do
//!   evict-then-retry.
//! - **Generous bollard timeout** (`CLIENT_TIMEOUT_SECS`, 120s): must exceed
//!   `SERVER_ALIVE_INTERVAL × count` so the SSH layer detects a dead connection
//!   first (perf-oracle).

// ---------------------------------------------------------------------------
// Submodule declarations
// ---------------------------------------------------------------------------

pub mod bollard_client;
pub mod cache;
pub mod traits;

#[cfg(any(test, feature = "test-support"))]
pub mod mock;

// ---------------------------------------------------------------------------
// Public re-exports — callers use `crate::docker_client::Foo`, unchanged.
// ---------------------------------------------------------------------------

pub use bollard_client::BollardClient;
pub use cache::DockerClientCache;
pub use traits::{
    BoxStream, ContainerAction, ContainerOps, DockerClient, ImageOps, NetworkOps, SystemOps,
    VolumeOps,
};

#[cfg(any(test, feature = "test-support"))]
pub use mock::{MockDockerClient, MutatingOp};

// ---------------------------------------------------------------------------
// Shared constants and helpers that sub-modules depend on.
// ---------------------------------------------------------------------------

/// bollard request timeout (seconds).
///
/// Deliberately generous: it must exceed `ssh::SERVER_ALIVE_INTERVAL` ×
/// ServerAliveCountMax so the SSH layer detects a dead control connection before
/// bollard times out the HTTP request (perf-oracle, see module docs).
pub const CLIENT_TIMEOUT_SECS: u64 = 120;

/// Classify a `bollard::errors::Error` as a dead-transport condition that should
/// trigger cache eviction + rebuild (HIGH, perf-oracle).
///
/// Matches IO `BrokenPipe` / `ConnectionRefused` / `ConnectionReset` plus
/// hyper/HTTP-client failures (the surface a severed SSH-forwarded socket
/// produces).
pub fn is_transport_dead(err: &bollard::errors::Error) -> bool {
    use bollard::errors::Error as E;
    use std::io::ErrorKind;
    match err {
        E::IOError { err } => matches!(
            err.kind(),
            ErrorKind::BrokenPipe
                | ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::NotConnected
                | ErrorKind::UnexpectedEof
        ),
        // hyper / lower-level HTTP client errors over a severed socket.
        E::HyperResponseError { .. } | E::HttpClientError { .. } | E::HyperLegacyError { .. } => {
            true
        }
        E::RequestTimeoutError => true,
        _ => false,
    }
}

#[cfg(test)]
#[path = "docker_client_tests.rs"]
mod tests;

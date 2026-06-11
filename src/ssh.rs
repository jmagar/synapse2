//! SSH transport layer for synapse2.
//!
//! Provides an [`SshSession`] abstraction over the `openssh` crate covering
//! connection lifecycle, command execution, and unix-socket forwarding. This is
//! the bedrock for every remote operation: `scout` (remote exec/peek) and
//! `flux` (remote docker via forwarded socket).
//!
//! Design (locked decisions — see bead rmcp-template-3tt.1):
//!
//! - **openssh crate, process-mux backend.** Requires the `ssh` binary at
//!   runtime. Reuses `~/.ssh/config` and `~/.ssh/known_hosts` — no custom TOFU
//!   store. `KnownHosts::Strict` rejects unknown/changed host keys (MITM).
//! - **5s connect timeout** via `tokio::time::timeout` wrapping the connect.
//!   This is the authoritative guard against a black-holed host stalling fanout
//!   for the 75s TCP RTO (the builder's `ConnectTimeout` does not cover all
//!   hang modes).
//! - **One `Arc<Session>` per host.** openssh ControlMaster multiplexes, so a
//!   pool of N control sockets gives no concurrency benefit. A single session
//!   is shared by all callers; a per-host [`Semaphore`] (default 8) caps
//!   concurrent `command()` invocations.
//! - **Passive health.** Sessions are marked dead on command failure and lazily
//!   reconnected on next checkout. A background task evicts sessions idle > 5
//!   minutes. No active liveness probe.
//! - **execvp-style exec only.** Commands run as `session.command(prog).arg(..)`
//!   — never `sh -c`. LOCKED INVARIANT.
//! - **Forwarded unix sockets** are created at `/tmp/synapse2-{host}-{pid}.sock`,
//!   chmod 0600 immediately, and removed on drop. A startup sweep removes stale
//!   sockets whose owning pid is dead.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use crate::synapse::HostConfig;

pub mod forward;
pub mod known_hosts;
pub mod pool;

pub use forward::{forward_socket_path, ForwardedSocket};
pub use known_hosts::{
    scan_known_hosts_wildcards, sweep_stale_sockets, sweep_stale_sockets_in,
    warn_on_known_hosts_wildcards,
};
pub use pool::{PooledSession, SshPool};

// Re-export internal helpers needed by ssh_tests.rs via `use super::*`.
#[cfg(test)]
pub(crate) use known_hosts::{parse_socket_pid, pid_is_alive};
#[cfg(test)]
pub(crate) use pool::connect;

/// Hard ceiling on how long a single SSH connect may take.
///
/// HARD BLOCKER (perf-oracle, HIGH): without this, one unreachable host stalls
/// the whole fanout for the 75s TCP retransmission timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// `ServerAliveInterval` — keeps NAT/VPN paths from idling out the control
/// connection.
pub const SERVER_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// Default per-host concurrent-exec cap.
pub const DEFAULT_EXEC_PERMITS: usize = 8;

/// Sessions untouched for longer than this are evicted by the background task.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Background eviction sweep interval.
pub const EVICTION_INTERVAL: Duration = Duration::from_secs(60);

/// Remote docker socket that forwarded sockets connect to.
pub const REMOTE_DOCKER_SOCKET: &str = "/var/run/docker.sock";

/// The output of a single remote command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Object-safe SSH executor — the seam downstream beads (scout, flux) depend on.
///
/// Tests provide a mock impl so they never need a live `ssh` server.
#[async_trait]
pub trait SshExecutor: Send + Sync {
    /// Run `program` with `args` on the host (execvp-style — no shell).
    async fn exec(&self, host: &HostConfig, program: &str, args: &[&str]) -> Result<CommandOutput>;
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;

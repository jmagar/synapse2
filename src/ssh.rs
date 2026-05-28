//! SSH transport layer for synapse2.
//!
//! Provides an [`SshSession`] abstraction over the `openssh` crate covering
//! connection lifecycle, command execution, and unix-socket forwarding. This is
//! the bedrock for every remote operation: `scout` (remote exec/peek) and
//! `flux` (remote docker via forwarded socket).
//!
//! Design (locked decisions — see bead rmcp-template-3tt.1):
//!
//! - **openssh crate, native-mux backend.** Requires the `ssh` binary at
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

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use openssh::{ForwardType, KnownHosts, Session, SessionBuilder, Socket};
use tokio::sync::Semaphore;

use crate::synapse::HostConfig;

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

/// A pooled SSH session: one multiplexed `openssh::Session` plus the per-host
/// exec semaphore and a last-activity timestamp for idle eviction.
pub struct PooledSession {
    session: Arc<Session>,
    permits: Arc<Semaphore>,
    last_used: std::sync::Mutex<Instant>,
}

impl PooledSession {
    fn touch(&self) {
        if let Ok(mut guard) = self.last_used.lock() {
            *guard = Instant::now();
        }
    }

    fn idle_for(&self, now: Instant) -> Duration {
        self.last_used
            .lock()
            .map(|t| now.saturating_duration_since(*t))
            .unwrap_or_default()
    }

    /// Shared session handle for concurrent multiplexed exec / port forwarding.
    pub fn session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }
}

/// Build the SSH destination string (`[user@]host`) and apply config to the
/// builder. `ssh_port`/`ssh_config_path` override `~/.ssh/config` defaults.
fn configure_builder(host: &HostConfig) -> (SessionBuilder, String) {
    let mut builder = SessionBuilder::default();
    builder
        .known_hosts_check(KnownHosts::Strict)
        .connect_timeout(CONNECT_TIMEOUT)
        .server_alive_interval(SERVER_ALIVE_INTERVAL);

    if let Some(user) = &host.ssh_user {
        builder.user(user.clone());
    }
    // Prefer an explicit ssh_port; fall back to the generic `port` field.
    if let Some(port) = host.ssh_port.or(host.port) {
        builder.port(port);
    }
    if let Some(key) = &host.ssh_key_path {
        builder.keyfile(key);
    }
    if let Some(cfg) = &host.ssh_config_path {
        builder.config_file(cfg);
    }

    (builder, host.host.clone())
}

/// Connect to `host` with the locked 5s outer timeout. Honors the builder
/// `ConnectTimeout` too, but the outer `tokio::time::timeout` is authoritative.
async fn connect(host: &HostConfig) -> Result<Session> {
    let (builder, destination) = configure_builder(host);
    let fut = builder.connect_mux(&destination);
    match tokio::time::timeout(CONNECT_TIMEOUT, fut).await {
        Ok(Ok(session)) => Ok(session),
        Ok(Err(e)) => Err(anyhow!("ssh connect to {} failed: {e}", host.name)),
        Err(_) => bail!(
            "ssh connect to {} timed out after {}s",
            host.name,
            CONNECT_TIMEOUT.as_secs()
        ),
    }
}

/// Per-host SSH session pool. One `Arc<Session>` per host, multiplexed.
pub struct SshPool {
    sessions: DashMap<String, Arc<PooledSession>>,
    exec_permits: usize,
}

impl SshPool {
    pub fn new() -> Self {
        Self::with_permits(DEFAULT_EXEC_PERMITS)
    }

    pub fn with_permits(exec_permits: usize) -> Self {
        Self {
            sessions: DashMap::new(),
            exec_permits: exec_permits.max(1),
        }
    }

    /// Pool key: stable per host identity (name disambiguates same-hostname
    /// entries; port distinguishes alternate ports).
    fn key(host: &HostConfig) -> String {
        let port = host.ssh_port.or(host.port).unwrap_or(22);
        format!("{}:{port}", host.name)
    }

    /// Hand out the shared session for `host`, connecting (and caching) on
    /// first use or after a passive-health eviction.
    ///
    /// NOTE: never holds a `DashMap` guard across `.await` — the entry guard is
    /// dropped before connecting, then the connected session is inserted.
    pub async fn checkout(&self, host: &HostConfig) -> Result<Arc<PooledSession>> {
        let key = Self::key(host);

        // Fast path: existing live session. Clone the Arc out and drop the
        // guard immediately so we never await while holding a DashMap ref.
        if let Some(existing) = self.sessions.get(&key) {
            let pooled = Arc::clone(&existing);
            drop(existing);
            pooled.touch();
            return Ok(pooled);
        }

        // Slow path: connect without holding any guard.
        let session = connect(host).await?;
        let pooled = Arc::new(PooledSession {
            session: Arc::new(session),
            permits: Arc::new(Semaphore::new(self.exec_permits)),
            last_used: std::sync::Mutex::new(Instant::now()),
        });

        // Insert; if another task raced us, prefer the already-cached entry to
        // avoid leaking a second control connection.
        use dashmap::mapref::entry::Entry;
        match self.sessions.entry(key) {
            Entry::Occupied(occupied) => {
                let winner = Arc::clone(occupied.get());
                // Best-effort close of our now-redundant session.
                let loser = Arc::clone(&pooled.session);
                tokio::spawn(async move {
                    if let Ok(session) = Arc::try_unwrap(loser) {
                        let _ = session.close().await;
                    }
                });
                winner.touch();
                Ok(winner)
            }
            Entry::Vacant(vacant) => {
                vacant.insert(Arc::clone(&pooled));
                Ok(pooled)
            }
        }
    }

    /// Drop a host's session from the pool (passive health: called on command
    /// failure so the next checkout reconnects).
    pub fn invalidate(&self, host: &HostConfig) {
        let key = Self::key(host);
        if let Some((_, pooled)) = self.sessions.remove(&key) {
            spawn_close(pooled);
        }
    }

    /// Number of cached sessions (pool stats / test assertions).
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Evict sessions idle longer than [`IDLE_TIMEOUT`] as of `now`.
    ///
    /// Directly callable so the idle-eviction unit test can drive it after
    /// `tokio::time::advance` instead of waiting on the spawned interval.
    pub fn evict_idle(&self, now: Instant) {
        let stale: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| entry.value().idle_for(now) >= IDLE_TIMEOUT)
            .map(|entry| entry.key().clone())
            .collect();
        for key in stale {
            if let Some((_, pooled)) = self.sessions.remove(&key) {
                spawn_close(pooled);
            }
        }
    }

    /// Close every cached session. Call from `main.rs` on shutdown.
    pub async fn shutdown(&self) {
        let keys: Vec<String> = self.sessions.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            if let Some((_, pooled)) = self.sessions.remove(&key) {
                if let Ok(session) = Arc::try_unwrap(pooled) {
                    if let Ok(session) = Arc::try_unwrap(session.session) {
                        let _ = session.close().await;
                    }
                }
            }
        }
    }

    /// Spawn the background idle-eviction task. Returns the join handle so the
    /// caller can abort it on shutdown.
    pub fn spawn_eviction_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(EVICTION_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                pool.evict_idle(Instant::now());
            }
        })
    }
}

impl Default for SshPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort detached close of a pooled session (used when we can't await).
fn spawn_close(pooled: Arc<PooledSession>) {
    tokio::spawn(async move {
        if let Ok(session) = Arc::try_unwrap(pooled) {
            if let Ok(session) = Arc::try_unwrap(session.session) {
                let _ = session.close().await;
            }
        }
    });
}

#[async_trait]
impl SshExecutor for SshPool {
    async fn exec(&self, host: &HostConfig, program: &str, args: &[&str]) -> Result<CommandOutput> {
        let pooled = self.checkout(host).await?;

        // LOCKED: acquire the permit INSIDE this call (never before a spawn) to
        // avoid the deadlock where a permit is held across a task boundary.
        let _permit = pooled
            .permits
            .acquire()
            .await
            .context("ssh exec semaphore closed")?;

        // LOCKED INVARIANT: execvp-style, no `sh -c`. `arc_command` shell-escapes
        // the program name and each arg individually — args are passed as
        // discrete argv entries, never concatenated into a shell command line.
        let mut command = pooled.session().arc_command(program.to_string());
        for arg in args {
            command.arg(*arg);
        }

        match command.output().await {
            Ok(output) => {
                pooled.touch();
                Ok(CommandOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    exit_code: output.status.code(),
                })
            }
            Err(e) => {
                // Passive health: mark the session dead so the next checkout
                // reconnects.
                drop(_permit);
                self.invalidate(host);
                Err(anyhow!("ssh exec `{program}` on {} failed: {e}", host.name))
            }
        }
    }
}

// ── Forwarded unix socket (the bollard bridge B2 consumes) ──────────────────

/// Local path for a host's forwarded docker socket: `/tmp/synapse2-{host}-{pid}.sock`.
///
/// Host names may contain hyphens, so the startup sweep parses the pid from the
/// RIGHT (`rsplit_once('-')`). Keep this format in sync with [`sweep_stale_sockets`].
pub fn forward_socket_path(host: &HostConfig) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/synapse2-{}-{}.sock",
        host.name,
        std::process::id()
    ))
}

/// An RAII guard over a forwarded unix socket. On drop it removes the socket
/// file (sync, best-effort) and detaches an async `close_port_forward`. Prefer
/// the explicit [`ForwardedSocket::close`] for the happy path.
pub struct ForwardedSocket {
    session: Arc<Session>,
    local_path: PathBuf,
    closed: bool,
}

impl ForwardedSocket {
    /// Open a local→remote unix-socket forward bridging `local_path` to the
    /// remote docker socket. The socket is chmod 0600 before this returns so it
    /// is never world-connectable.
    pub async fn open(session: Arc<Session>, local_path: PathBuf) -> Result<Self> {
        // Remove any leftover socket at this path (a prior run with our pid that
        // crashed) so the bind does not silently fail.
        if local_path.exists() {
            let _ = std::fs::remove_file(&local_path);
        }

        session
            .request_port_forward(
                ForwardType::Local,
                Socket::UnixSocket {
                    path: local_path.as_path().into(),
                },
                Socket::UnixSocket {
                    path: Path::new(REMOTE_DOCKER_SOCKET).into(),
                },
            )
            .await
            .with_context(|| format!("request unix-socket forward at {}", local_path.display()))?;

        // native-mux may create the listener slightly after the request returns.
        // Poll briefly, then lock it down to 0600 BEFORE handing the path out.
        Self::secure_socket(&local_path).await?;

        Ok(Self {
            session,
            local_path,
            closed: false,
        })
    }

    /// Wait (briefly) for the socket to appear, then chmod 0600.
    ///
    /// SECURITY (security-sentinel, MEDIUM): a world-readable socket would let
    /// other local users connect to the remote docker daemon. The 0600 must
    /// land before the path is exposed to bollard.
    async fn secure_socket(local_path: &Path) -> Result<()> {
        const MAX_WAIT: Duration = Duration::from_secs(2);
        const POLL: Duration = Duration::from_millis(20);
        let deadline = Instant::now() + MAX_WAIT;
        loop {
            if local_path.exists() {
                std::fs::set_permissions(local_path, std::fs::Permissions::from_mode(0o600))
                    .with_context(|| {
                        format!("chmod 0600 forwarded socket {}", local_path.display())
                    })?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!(
                    "forwarded socket {} did not appear within {}ms",
                    local_path.display(),
                    MAX_WAIT.as_millis()
                );
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Local socket path to hand to a bollard client (`unix://{path}`).
    pub fn path(&self) -> &Path {
        &self.local_path
    }

    /// Explicit async teardown — preferred over relying on `Drop`.
    pub async fn close(mut self) -> Result<()> {
        self.closed = true;
        let result = self
            .session
            .close_port_forward(
                ForwardType::Local,
                Socket::UnixSocket {
                    path: self.local_path.as_path().into(),
                },
                Socket::UnixSocket {
                    path: Path::new(REMOTE_DOCKER_SOCKET).into(),
                },
            )
            .await;
        let _ = std::fs::remove_file(&self.local_path);
        result.with_context(|| format!("close port forward {}", self.local_path.display()))
    }
}

impl Drop for ForwardedSocket {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        // Sync best-effort file removal — Drop cannot await.
        let _ = std::fs::remove_file(&self.local_path);

        // Detach the async port-forward teardown if a runtime is available.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let session = Arc::clone(&self.session);
            let path = self.local_path.clone();
            handle.spawn(async move {
                let _ = session
                    .close_port_forward(
                        ForwardType::Local,
                        Socket::UnixSocket {
                            path: path.as_path().into(),
                        },
                        Socket::UnixSocket {
                            path: Path::new(REMOTE_DOCKER_SOCKET).into(),
                        },
                    )
                    .await;
            });
        }
    }
}

// ── known_hosts wildcard warning ────────────────────────────────────────────

/// Scan `~/.ssh/known_hosts` and WARN if any host pattern contains a wildcard
/// (`*` or `?`). A wildcard entry trusts any host key, defeating the MITM
/// protection of `KnownHosts::Strict`. Called once at startup.
///
/// SECURITY (security-sentinel, MEDIUM): documents the assumption that the
/// user's known_hosts is wildcard-free; see docs/SECURITY.md.
pub fn warn_on_known_hosts_wildcards() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let path = Path::new(&home).join(".ssh").join("known_hosts");
    if let Some(patterns) = scan_known_hosts_wildcards(&path) {
        if !patterns.is_empty() {
            tracing::warn!(
                count = patterns.len(),
                "~/.ssh/known_hosts contains wildcard host patterns ({}); \
                 these trust ANY host key and undermine StrictHostKeyChecking — \
                 see docs/SECURITY.md",
                patterns.join(", ")
            );
        }
    }
}

/// Return the wildcard host patterns found in a known_hosts file, or `None` if
/// the file can't be read. Extracted for unit testing.
pub fn scan_known_hosts_wildcards(path: &Path) -> Option<Vec<String>> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut found = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // First whitespace-delimited field is the comma-separated host list.
        let Some(hosts) = line.split_whitespace().next() else {
            continue;
        };
        for host in hosts.split(',') {
            if host.contains('*') || host.contains('?') {
                found.push(host.to_string());
            }
        }
    }
    Some(found)
}

// ── Startup sweep ───────────────────────────────────────────────────────────

/// Remove stale `/tmp/synapse2-*-*.sock` files whose owning pid is no longer
/// running. Called once from `main.rs` before pool init to stop accumulation
/// across crashes (the socket persists on SIGKILL/panic).
pub fn sweep_stale_sockets() {
    sweep_stale_sockets_in(Path::new("/tmp"));
}

/// Sweep a specific directory (extracted so the unit test can point at a tmp
/// dir without touching the real `/tmp`). Returns the paths removed.
pub fn sweep_stale_sockets_in(dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return removed,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(pid) = parse_socket_pid(name) else {
            continue;
        };
        if !pid_is_alive(pid) && std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// Parse the pid out of `synapse2-{host}-{pid}.sock`. Host names contain
/// hyphens, so strip the fixed prefix/suffix and split from the RIGHT.
fn parse_socket_pid(name: &str) -> Option<u32> {
    let inner = name.strip_prefix("synapse2-")?.strip_suffix(".sock")?;
    let (_host, pid) = inner.rsplit_once('-')?;
    pid.parse::<u32>().ok()
}

/// Linux-only liveness check via `/proc/{pid}` — avoids pulling in `libc`/`nix`.
fn pid_is_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;

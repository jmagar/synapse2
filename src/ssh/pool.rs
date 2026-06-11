//! SSH session pool — one multiplexed `openssh::Session` per host.
//!
//! `SshPool` hands out `Arc<PooledSession>` handles, connecting (and caching)
//! on first use and lazily reconnecting after passive-health eviction.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use openssh::{KnownHosts, Session, SessionBuilder};
use tokio::sync::Semaphore;

use crate::synapse::HostConfig;

use super::{
    CommandOutput, SshExecutor, CONNECT_TIMEOUT, DEFAULT_EXEC_PERMITS, EVICTION_INTERVAL,
    IDLE_TIMEOUT, SERVER_ALIVE_INTERVAL,
};

/// A pooled SSH session: one multiplexed `openssh::Session` plus the per-host
/// exec semaphore and a last-activity timestamp for idle eviction.
pub struct PooledSession {
    pub(super) session: Arc<Session>,
    pub(super) permits: Arc<Semaphore>,
    pub(super) last_used: std::sync::Mutex<Instant>,
}

impl PooledSession {
    pub(super) fn touch(&self) {
        if let Ok(mut guard) = self.last_used.lock() {
            *guard = Instant::now();
        }
    }

    pub(super) fn idle_for(&self, now: Instant) -> Duration {
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
        .control_directory("/tmp")
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
        let known_hosts = std::path::Path::new(cfg)
            .parent()
            .map(|dir| dir.join("known_hosts"));
        if let Some(path) = known_hosts {
            builder.user_known_hosts_file(path);
        }
    }

    let destination = if host.ssh_config_path.is_some() {
        host.name.clone()
    } else {
        host.host.clone()
    };

    (builder, destination)
}

/// Connect to `host` with the locked 5s outer timeout. Honors the builder
/// `ConnectTimeout` too, but the outer `tokio::time::timeout` is authoritative.
pub(crate) async fn connect(host: &HostConfig) -> Result<Session> {
    let (builder, destination) = configure_builder(host);
    let fut = builder.connect(&destination);
    match tokio::time::timeout(CONNECT_TIMEOUT, fut).await {
        Ok(Ok(session)) => Ok(session),
        Ok(Err(e)) => Err(anyhow!("ssh connect to {} failed: {e:?}", host.name)),
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

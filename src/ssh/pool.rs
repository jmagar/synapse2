//! SSH session pool — one multiplexed `openssh::Session` per host.
//!
//! `SshPool` hands out `Arc<PooledSession>` handles, connecting (and caching)
//! on first use and lazily reconnecting after passive-health eviction.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use dashmap::DashMap;
use openssh::{KnownHosts, Session, SessionBuilder};
use tokio::sync::{OnceCell, Semaphore};

use crate::synapse::HostConfig;

use super::{
    CONNECT_TIMEOUT, CommandOutput, DEFAULT_EXEC_PERMITS, EVICTION_INTERVAL, IDLE_TIMEOUT,
    SERVER_ALIVE_INTERVAL, SshExecutor,
};

/// A fixed epoch for converting `Instant` durations to `u64` nanoseconds.
///
/// `Instant` is not representable as a number directly, so we measure all
/// timestamps as nanoseconds elapsed *since this anchor*. The anchor is
/// initialised lazily on first use (via `std::sync::OnceLock`) and remains
/// stable for the rest of the process lifetime.
fn instant_epoch() -> Instant {
    use std::sync::OnceLock;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Convert an `Instant` to nanoseconds since [`instant_epoch`], saturating to
/// `u64::MAX` if the instant is implausibly far in the future.
fn instant_to_nanos(t: Instant) -> u64 {
    t.saturating_duration_since(instant_epoch())
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// A pooled SSH session: one multiplexed `openssh::Session` plus the per-host
/// exec semaphore and a last-activity timestamp for idle eviction.
///
/// `last_used_nanos` stores nanoseconds since [`instant_epoch`] as an
/// [`AtomicU64`], replacing the former `std::sync::Mutex<Instant>`.  This is
/// lock-free and avoids mutex contention in async contexts (A-M4 / P-M5).
pub struct PooledSession {
    pub(super) session: Arc<Session>,
    pub(super) permits: Arc<Semaphore>,
    pub(super) last_used_nanos: AtomicU64,
}

impl PooledSession {
    pub(super) fn touch(&self) {
        self.last_used_nanos
            .store(instant_to_nanos(Instant::now()), Ordering::Relaxed);
    }

    pub(super) fn idle_for(&self, now: Instant) -> Duration {
        let last_nanos = self.last_used_nanos.load(Ordering::Relaxed);
        let now_nanos = instant_to_nanos(now);
        let age_nanos = now_nanos.saturating_sub(last_nanos);
        Duration::from_nanos(age_nanos)
    }

    /// Shared session handle for concurrent multiplexed exec / port forwarding.
    pub fn session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }
}

/// Return a process-private directory for SSH ControlMaster sockets with mode
/// 0700. Preference order:
///
/// 1. `$XDG_RUNTIME_DIR/synapse2/` (already 0700, owned by the current user)
/// 2. `<temp_dir>/synapse2-<pid>/` created with mode 0700
///
/// The directory is created on first call and reused thereafter. Using a
/// process-private path prevents other local users from connecting through our
/// ControlMaster socket.
fn control_dir() -> Result<std::path::PathBuf> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let dir = if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        std::path::PathBuf::from(xdg).join("synapse2")
    } else {
        std::env::temp_dir().join(format!("synapse2-{}", std::process::id()))
    };

    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("create ControlMaster dir {}", dir.display()))?;
    }
    // Always enforce 0700 even if the directory already existed.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod 0700 ControlMaster dir {}", dir.display()))?;

    Ok(dir)
}

/// Build the SSH destination string (`[user@]host`) and apply config to the
/// builder. `ssh_port`/`ssh_config_path` override `~/.ssh/config` defaults.
fn configure_builder(host: &HostConfig) -> Result<(SessionBuilder, String)> {
    let ctrl_dir = control_dir()?;

    let mut builder = SessionBuilder::default();
    builder
        .known_hosts_check(KnownHosts::Strict)
        .control_directory(ctrl_dir)
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

    Ok((builder, destination))
}

/// Connect to `host` with the locked 5s outer timeout. Honors the builder
/// `ConnectTimeout` too, but the outer `tokio::time::timeout` is authoritative.
pub(crate) async fn connect(host: &HostConfig) -> Result<Session> {
    let (builder, destination) = configure_builder(host)?;
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
///
/// The inner map value is `Arc<OnceCell<Arc<PooledSession>>>`. The `OnceCell`
/// acts as a per-key init guard: concurrent tasks that miss the fast path all
/// share the *same* cell and only one of them runs `connect()`; the rest wait
/// on the already-in-flight future instead of each opening a 5s connect race.
///
/// Invalidation replaces the cell entirely (removes the key), so the next
/// checkout inserts a fresh cell and reconnects cleanly.
pub struct SshPool {
    /// Map from pool key → init cell. A cell is present ↔ a connect is in
    /// flight or already succeeded. A missing key means "not yet connected or
    /// evicted."
    sessions: DashMap<String, Arc<OnceCell<Arc<PooledSession>>>>,
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
    /// Concurrent calls for the same key share a single `OnceCell` — only one
    /// initiates a `connect()` and the others wait for it, eliminating the
    /// K×5s duplicate-connect race on a cold cache miss.
    ///
    /// NOTE: DashMap guards are never held across `.await`. The cell Arc is
    /// cloned out and the guard dropped before any async work begins.
    pub async fn checkout(&self, host: &HostConfig) -> Result<Arc<PooledSession>> {
        let key = Self::key(host);

        // Get-or-insert the cell for this key. Drop the DashMap guard
        // immediately after cloning the Arc — never hold it across await.
        let cell: Arc<OnceCell<Arc<PooledSession>>> = {
            use dashmap::mapref::entry::Entry;
            match self.sessions.entry(key.clone()) {
                Entry::Occupied(o) => Arc::clone(o.get()),
                Entry::Vacant(v) => {
                    let cell = Arc::new(OnceCell::new());
                    v.insert(Arc::clone(&cell));
                    cell
                }
            }
        };

        // `get_or_try_init` guarantees exactly one `connect()` per cell across
        // concurrent callers. Additional callers await the shared future.
        let exec_permits = self.exec_permits;
        let pooled = match cell
            .get_or_try_init(|| async move {
                let session = connect(host).await?;
                Ok::<Arc<PooledSession>, anyhow::Error>(Arc::new(PooledSession {
                    session: Arc::new(session),
                    permits: Arc::new(Semaphore::new(exec_permits)),
                    last_used_nanos: AtomicU64::new(instant_to_nanos(Instant::now())),
                }))
            })
            .await
        {
            Ok(pooled) => pooled,
            Err(e) => {
                // Connect failed: `OnceCell` resets itself, but the empty cell
                // would linger in the map (`evict_idle` skips uninitialised
                // cells). Drop it so `len()`/metrics don't count a ghost entry
                // and the next checkout starts fresh — but only if it's still
                // our uninitialised cell (another task may have raced a success).
                self.sessions
                    .remove_if(&key, |_, c| c.get().is_none() && Arc::ptr_eq(c, &cell));
                return Err(e);
            }
        };

        pooled.touch();
        Ok(Arc::clone(pooled))
    }

    /// Drop a host's session from the pool (passive health: called on command
    /// failure so the next checkout reconnects).
    ///
    /// Removes the cell entirely so the next `checkout` inserts a fresh one
    /// and triggers a new `connect()`.
    pub fn invalidate(&self, host: &HostConfig) {
        let key = Self::key(host);
        if let Some((_, cell)) = self.sessions.remove(&key) {
            // Best-effort close of the old session, if it successfully
            // initialised before the failure that triggered invalidation.
            if let Some(pooled) = cell.get() {
                spawn_close(Arc::clone(pooled));
            }
        }
    }

    /// Number of cached session cells (pool stats / test assertions).
    ///
    /// A cell is counted even while the connect is still in flight; the count
    /// drops to zero only after `invalidate` / `evict_idle` / `shutdown`.
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
            .filter(|entry| {
                // Only evict cells whose session has successfully initialised
                // and has been idle long enough. Cells still connecting are
                // skipped so we don't interrupt an in-flight connect.
                entry
                    .value()
                    .get()
                    .map(|pooled| pooled.idle_for(now) >= IDLE_TIMEOUT)
                    .unwrap_or(false)
            })
            .map(|entry| entry.key().clone())
            .collect();
        for key in stale {
            if let Some((_, cell)) = self.sessions.remove(&key)
                && let Some(pooled) = cell.get()
            {
                spawn_close(Arc::clone(pooled));
            }
        }
    }

    /// Close every cached session. Call from `main.rs` on shutdown.
    pub async fn shutdown(&self) {
        let keys: Vec<String> = self.sessions.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            if let Some((_, cell)) = self.sessions.remove(&key)
                && let Some(pooled) = cell.get()
                && let Ok(session) = Arc::try_unwrap(Arc::clone(&pooled.session))
            {
                let _ = session.close().await;
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
        if let Ok(session) = Arc::try_unwrap(pooled)
            && let Ok(session) = Arc::try_unwrap(session.session)
        {
            let _ = session.close().await;
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

        match crate::runtime_budget::with_operation_deadline(
            &format!("ssh command `{program}` on {}", host.name),
            command.output(),
        )
        .await
        {
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

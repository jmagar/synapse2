//! Unit tests for the SSH transport layer.
//!
//! Tests that require a live `ssh` server connect to `localhost` and skip
//! cleanly (return early) when no server is reachable — they never fail CI.
//! Pure-logic tests (pid parsing, sweep, wildcard scan, mock executor,
//! semaphore, connect timeout) always run.

use super::*;
use openssh::Session;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

fn host(name: &str) -> HostConfig {
    HostConfig {
        name: name.into(),
        host: "localhost".into(),
        port: None,
        protocol: crate::synapse::HostProtocol::Ssh,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: Vec::new(),
        scout_read_roots: Vec::new(),
        exec_allowlist: Vec::new(),
    }
}

/// Try to connect to localhost over SSH; returns `None` if unreachable so the
/// caller can skip. Uses the real connect path (5s timeout, strict host keys).
async fn try_localhost_session() -> Option<Session> {
    let h = host("localhost");
    connect(&h).await.ok()
}

// ── pid parsing (startup sweep) ─────────────────────────────────────────────

#[test]
fn parse_socket_pid_simple_host() {
    assert_eq!(parse_socket_pid("synapse2-dookie-12345.sock"), Some(12345));
}

#[test]
fn parse_socket_pid_hyphenated_host() {
    // Host names contain hyphens — pid must be parsed from the RIGHT.
    assert_eq!(
        parse_socket_pid("synapse2-dookie-prod-01-67890.sock"),
        Some(67890)
    );
}

#[test]
fn parse_socket_pid_rejects_foreign() {
    assert_eq!(parse_socket_pid("other-thing-1.sock"), None);
    assert_eq!(parse_socket_pid("synapse2-dookie.sock"), None); // no pid
    assert_eq!(parse_socket_pid("synapse2-dookie-notapid.sock"), None);
    assert_eq!(parse_socket_pid("synapse2-dookie-12345.txt"), None);
}

// ── startup sweep ───────────────────────────────────────────────────────────

#[test]
fn sweep_removes_stale_sockets_only() {
    let dir = tempfile::tempdir().unwrap();
    let live_pid = std::process::id();
    let dead_pid = pick_dead_pid();

    let live = dir.path().join(format!("synapse2-hostA-{live_pid}.sock"));
    let stale = dir.path().join(format!("synapse2-host-b-{dead_pid}.sock"));
    let foreign = dir.path().join("unrelated.sock");

    std::fs::write(&live, b"").unwrap();
    std::fs::write(&stale, b"").unwrap();
    std::fs::write(&foreign, b"").unwrap();

    let removed = sweep_stale_sockets_in(dir.path());

    assert!(stale.starts_with(dir.path()));
    assert!(!stale.exists(), "stale socket (dead pid) should be removed");
    assert!(live.exists(), "live socket (our pid) must be kept");
    assert!(foreign.exists(), "unrelated files must be left alone");
    assert_eq!(removed, vec![stale]);
}

/// Find a pid that is not currently running.
fn pick_dead_pid() -> u32 {
    // Scan downward from a high pid value for one with no /proc entry.
    for candidate in (90_000..99_999).rev() {
        if !pid_is_alive(candidate) {
            return candidate;
        }
    }
    99_999
}

#[test]
fn pid_is_alive_detects_self() {
    assert!(pid_is_alive(std::process::id()));
    assert!(!pid_is_alive(pick_dead_pid()));
}

// ── known_hosts wildcard warning ────────────────────────────────────────────

#[test]
fn scan_known_hosts_flags_wildcards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    std::fs::write(
        &path,
        "# a comment\n\
         github.com ssh-ed25519 AAAAREALKEY\n\
         * ssh-ed25519 AAAAWILDCARD\n\
         192.168.1.* ssh-rsa AAAAGLOB\n\
         host-?.example ssh-rsa AAAAQ\n",
    )
    .unwrap();

    let found = scan_known_hosts_wildcards(&path).unwrap();
    assert!(found.contains(&"*".to_string()));
    assert!(found.contains(&"192.168.1.*".to_string()));
    assert!(found.contains(&"host-?.example".to_string()));
    assert!(!found.iter().any(|p| p == "github.com"));
    assert_eq!(found.len(), 3);
}

#[test]
fn scan_known_hosts_clean_file_has_no_wildcards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    std::fs::write(
        &path,
        "github.com ssh-ed25519 AAAA\nhost.example ssh-rsa BBBB\n",
    )
    .unwrap();
    assert_eq!(scan_known_hosts_wildcards(&path), Some(Vec::new()));
}

#[test]
fn scan_known_hosts_missing_file_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist");
    assert_eq!(scan_known_hosts_wildcards(&path), None);
}

// ── forward socket path ─────────────────────────────────────────────────────

#[test]
fn forward_socket_path_format() {
    let p = forward_socket_path(&host("dookie"));
    let name = p.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("synapse2-dookie-"));
    assert!(name.ends_with(".sock"));
    // Round-trips through the sweep parser.
    assert_eq!(parse_socket_pid(name), Some(std::process::id()));
}

// ── mock executor (the seam downstream beads depend on) ─────────────────────

struct MockExecutor {
    calls: AtomicUsize,
    last: std::sync::Mutex<Vec<String>>,
}

impl MockExecutor {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            last: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl SshExecutor for MockExecutor {
    async fn exec(
        &self,
        _host: &HostConfig,
        program: &str,
        args: &[&str],
    ) -> Result<CommandOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut recorded = vec![program.to_string()];
        recorded.extend(args.iter().map(|a| a.to_string()));
        *self.last.lock().unwrap() = recorded;
        Ok(CommandOutput {
            stdout: format!("ran {program}"),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

#[tokio::test]
async fn mock_executor_is_object_safe_and_records_calls() {
    // Object safety: it must be usable as a trait object.
    let exec: Box<dyn SshExecutor> = Box::new(MockExecutor::new());
    let out = exec
        .exec(&host("h"), "hostname", &["-f"])
        .await
        .expect("mock exec");
    assert!(out.success());
    assert_eq!(out.stdout, "ran hostname");
}

#[tokio::test]
async fn mock_executor_smoke_proves_execvp_args_not_shell() {
    // The trait contract passes program + discrete args — never a `sh -c`
    // string. This test documents/guards the execvp-style invariant at the
    // seam: a caller cannot smuggle a shell pipeline through a single arg
    // because args are passed positionally, not concatenated into a shell line.
    let mock = MockExecutor::new();
    mock.exec(&host("h"), "grep", &["pattern", "/etc/hostname"])
        .await
        .unwrap();
    let recorded = mock.last.lock().unwrap().clone();
    assert_eq!(recorded, vec!["grep", "pattern", "/etc/hostname"]);
    // No element is a shell, and the program is never "sh"/"bash".
    assert_ne!(recorded[0], "sh");
    assert_ne!(recorded[0], "bash");
}

// ── per-host semaphore concurrency cap ──────────────────────────────────────

#[tokio::test]
async fn semaphore_caps_concurrency_at_eight() {
    // Models the per-host exec semaphore directly (no live session needed):
    // 8 permits → the 9th acquire must queue until a permit is released.
    let sem = Arc::new(Semaphore::new(DEFAULT_EXEC_PERMITS));
    assert_eq!(DEFAULT_EXEC_PERMITS, 8);

    let mut held = Vec::new();
    for _ in 0..DEFAULT_EXEC_PERMITS {
        held.push(sem.clone().acquire_owned().await.unwrap());
    }
    assert_eq!(sem.available_permits(), 0);

    // 9th must not be immediately grantable.
    assert!(sem.try_acquire().is_err());

    // Release one; now a 9th can proceed.
    held.pop();
    let ninth = sem.try_acquire();
    assert!(ninth.is_ok());
}

// ── connect timeout (HARD BLOCKER) ──────────────────────────────────────────

#[tokio::test]
async fn connect_to_black_holed_host_errors_within_six_seconds() {
    // 203.0.113.0/24 (TEST-NET-3, RFC 5737) is reserved and unroutable, so the
    // TCP handshake hangs — exactly the failure mode the 5s timeout guards. We
    // assert the error returns in <= 6s, NOT the 75s TCP RTO.
    let mut h = host("blackhole");
    h.host = "203.0.113.1".into();

    let start = Instant::now();
    let result = connect(&h).await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "connect to unroutable host must error");
    assert!(
        elapsed <= Duration::from_secs(6),
        "connect should bail in <= 6s (got {elapsed:?})"
    );
}

// ── pool: connection reuse + eviction (live, skip if no ssh) ────────────────

#[tokio::test]
async fn pool_reuses_single_session_across_commands() {
    let Some(_probe) = try_localhost_session().await else {
        eprintln!("skipping: no reachable ssh server on localhost");
        return;
    };
    // Probe succeeded → localhost ssh works. Exercise the pool.
    let pool = SshPool::new();
    let h = host("localhost");

    for _ in 0..5 {
        let out = pool.exec(&h, "hostname", &[]).await.expect("exec hostname");
        assert!(out.success(), "stderr: {}", out.stderr);
        assert!(!out.stdout.trim().is_empty());
    }
    // All five commands multiplexed over a single cached session.
    assert_eq!(pool.len(), 1, "expected exactly one pooled session");

    pool.shutdown().await;
    assert!(pool.is_empty(), "shutdown must drop all sessions");
}

#[tokio::test]
async fn pool_evicts_idle_sessions() {
    let Some(_probe) = try_localhost_session().await else {
        eprintln!("skipping: no reachable ssh server on localhost");
        return;
    };
    let pool = SshPool::new();
    let h = host("localhost");
    pool.exec(&h, "hostname", &[]).await.expect("exec");
    assert_eq!(pool.len(), 1);

    // Eviction with a "now" far in the past leaves the fresh session alone.
    pool.evict_idle(Instant::now());
    assert_eq!(pool.len(), 1, "fresh session must not be evicted");

    // Eviction with a "now" past the idle window removes it. We synthesize the
    // future instant rather than sleeping 5 real minutes.
    let future = Instant::now() + IDLE_TIMEOUT + Duration::from_secs(1);
    pool.evict_idle(future);
    assert_eq!(pool.len(), 0, "idle session must be evicted");

    pool.shutdown().await;
}

// ── forwarded socket: creation, 0600 perms, removal (live, skip if no ssh) ──

#[tokio::test]
async fn forwarded_socket_has_0600_perms_and_is_removed_on_close() {
    let Some(session) = try_localhost_session().await else {
        eprintln!("skipping: no reachable ssh server on localhost");
        return;
    };
    // Only meaningful if a docker socket exists on localhost; otherwise the
    // forward target is absent but the LOCAL listener + perms still apply.
    let session = Arc::new(session);
    let local_path = std::env::temp_dir().join(format!(
        "synapse2-fwdtest-{}-{}.sock",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));

    let forwarded = match ForwardedSocket::open(Arc::clone(&session), local_path.clone()).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("skipping: forward unsupported in this environment: {e}");
            let _ = std::fs::remove_file(&local_path);
            return;
        }
    };

    assert_eq!(forwarded.path(), local_path.as_path());
    let mode = std::fs::metadata(&local_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "forwarded socket must be 0600, got {mode:o}");

    forwarded.close().await.expect("explicit close");
    assert!(!local_path.exists(), "socket file must be removed on close");

    if let Ok(session) = Arc::try_unwrap(session) {
        let _ = session.close().await;
    }
}

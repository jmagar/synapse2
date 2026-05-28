//! Integration test for the SSH pool against `localhost`.
//!
//! This test requires a reachable SSH server on localhost AND that the current
//! user's key is in `~/.ssh/known_hosts` for localhost (KnownHosts::Strict).
//! When SSH is unavailable or the host is unknown, the test SKIPS CLEANLY
//! (returns early) so it never breaks CI on machines without sshd.
//!
//! Validates the bead's locked behaviour:
//!   - connect, exec `hostname`, reconnect, reuse (1 connect, N commands)
//!   - shutdown drops all sessions (no orphaned `ssh` processes)

use synapse2::ssh::{SshExecutor, SshPool};
use synapse2::synapse::{HostConfig, HostProtocol};

fn localhost() -> HostConfig {
    HostConfig {
        name: "localhost".into(),
        host: "localhost".into(),
        port: None,
        protocol: HostProtocol::Ssh,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: Vec::new(),
        exec_allowlist: Vec::new(),
    }
}

#[tokio::test]
async fn pool_connects_execs_and_reuses_against_localhost() {
    let pool = SshPool::new();
    let host = localhost();

    // Probe: a single exec. If it fails, SSH to localhost isn't available in
    // this environment — skip cleanly rather than failing CI.
    let probe = pool.exec(&host, "hostname", &[]).await;
    let Ok(first) = probe else {
        eprintln!(
            "skipping ssh_pool integration test: localhost ssh unavailable ({:?})",
            probe.err()
        );
        return;
    };
    assert!(first.success(), "hostname stderr: {}", first.stderr);
    assert!(!first.stdout.trim().is_empty());

    // N more commands should reuse the same cached session (multiplexed).
    for _ in 0..10 {
        let out = pool.exec(&host, "hostname", &[]).await.expect("reuse exec");
        assert!(out.success());
    }
    assert_eq!(
        pool.len(),
        1,
        "10+ commands must reuse exactly one SSH session"
    );

    // Exec with args (execvp-style, no shell).
    let echoed = pool
        .exec(&host, "echo", &["synapse2-ssh-ok"])
        .await
        .expect("echo exec");
    assert_eq!(echoed.stdout.trim(), "synapse2-ssh-ok");

    // Shutdown drops the session.
    pool.shutdown().await;
    assert!(pool.is_empty(), "shutdown must close all pooled sessions");

    // Reconnect after shutdown: a fresh exec re-establishes the session.
    let reconnected = pool.exec(&host, "hostname", &[]).await.expect("reconnect");
    assert!(reconnected.success());
    assert_eq!(pool.len(), 1, "reconnect creates one new session");
    pool.shutdown().await;
}

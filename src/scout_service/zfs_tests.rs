//! Unit tests for scout ZFS operations.

use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::HostConfig;
use async_trait::async_trait;

// ─── Mock executor ────────────────────────────────────────────────────────────

/// A mock SSH executor that returns a fixed response.
struct FixedExec {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

impl FixedExec {
    fn ok(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
        }
    }

    fn fail(stderr: &str) -> Self {
        Self {
            stdout: String::new(),
            stderr: stderr.to_owned(),
            exit_code: Some(1),
        }
    }
}

#[async_trait]
impl SshExecutor for FixedExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        _program: &str,
        _args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: self.exit_code,
        })
    }
}

/// A mock that records what program + args were called.
struct RecordingExec {
    stdout: String,
}

impl RecordingExec {
    fn with_output(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_owned(),
        }
    }
}

#[async_trait]
impl SshExecutor for RecordingExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        _program: &str,
        _args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            stdout: self.stdout.clone(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

// We use an SSH host so is_local_host = false, routing through RemoteExec.
fn ssh_host() -> HostConfig {
    HostConfig {
        name: "test-host".to_owned(),
        host: "192.168.1.1".to_owned(),
        port: None,
        protocol: crate::synapse::HostProtocol::Ssh,
        ssh_user: Some("root".to_owned()),
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

const ZPOOL_LIST_OUTPUT: &str = "\
NAME    SIZE  ALLOC   FREE  CKPOINT  EXPANDSZ   FRAG    CAP  DEDUP    HEALTH  ALTROOT
data   9.09T  3.62T  5.47T        -         -    11%    39%  1.00x    ONLINE  -
";

const ZFS_LIST_OUTPUT: &str = "\
NAME                                 USED  AVAIL     REFER  MOUNTPOINT
data                                3.62T  5.47T      192K  /data
data/appdata                        1.23T  5.47T     1.23T  /mnt/user/appdata
data/media                          2.39T  5.47T     2.39T  /mnt/user/media
";

const ZFS_SNAPSHOT_OUTPUT: &str = "\
NAME                                     USED  AVAIL     REFER  MOUNTPOINT
data/appdata@daily-2026-05-01              0B      -      512M  -
data/appdata@daily-2026-05-02            12K      -      513M  -
data/media@weekly-2026-05-01               0B      -     1.00T  -
";

// ─── pools ───────────────────────────────────────────────────────────────────

#[test]
fn pools_parses_tabular_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(ZPOOL_LIST_OUTPUT);

    let result = rt.block_on(super::pools(&host, &exec, None)).unwrap();
    assert_eq!(result["subaction"], "pools");
    assert_eq!(result["host"], "test-host");
    let header = result["header"].as_str().unwrap();
    assert!(
        header.contains("NAME"),
        "header should contain NAME: {header}"
    );
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "should have one pool row");
    assert!(rows[0].as_str().unwrap().contains("data"));
}

#[test]
fn pools_returns_error_when_zpool_not_found() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::fail("zpool: not found");

    let err = rt.block_on(super::pools(&host, &exec, None)).unwrap_err();
    assert!(
        err.to_string().contains("ZFS may not be installed"),
        "expected install hint: {err}"
    );
}

#[test]
fn pools_with_pool_filter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = RecordingExec::with_output(ZPOOL_LIST_OUTPUT);

    let result = rt
        .block_on(super::pools(&host, &exec, Some("data")))
        .unwrap();
    // Just verify it ran without error and returned expected structure
    assert_eq!(result["subaction"], "pools");
}

// ─── datasets ────────────────────────────────────────────────────────────────

#[test]
fn datasets_parses_tabular_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(ZFS_LIST_OUTPUT);

    let result = rt
        .block_on(super::datasets(&host, &exec, None, None, false))
        .unwrap();
    assert_eq!(result["subaction"], "datasets");
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "should have 3 dataset rows");
}

#[test]
fn datasets_rejects_invalid_type() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok("");

    let err = rt
        .block_on(super::datasets(
            &host,
            &exec,
            None,
            Some("invalid-type"),
            false,
        ))
        .unwrap_err();
    assert!(
        err.to_string().contains("invalid dataset type"),
        "expected type error: {err}"
    );
}

#[test]
fn datasets_accepts_valid_type_filesystem() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(ZFS_LIST_OUTPUT);

    let result = rt
        .block_on(super::datasets(
            &host,
            &exec,
            None,
            Some("filesystem"),
            false,
        ))
        .unwrap();
    assert_eq!(result["subaction"], "datasets");
}

#[test]
fn datasets_returns_error_when_zfs_not_found() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::fail("zfs: not found");

    let err = rt
        .block_on(super::datasets(&host, &exec, None, None, false))
        .unwrap_err();
    assert!(
        err.to_string().contains("ZFS may not be installed"),
        "expected install hint: {err}"
    );
}

// ─── snapshots ───────────────────────────────────────────────────────────────

#[test]
fn snapshots_parses_tabular_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(ZFS_SNAPSHOT_OUTPUT);

    let result = rt
        .block_on(super::snapshots(&host, &exec, None, None, None))
        .unwrap();
    assert_eq!(result["subaction"], "snapshots");
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "should have 3 snapshot rows");
    assert!(!result["truncated"].as_bool().unwrap());
}

#[test]
fn snapshots_applies_limit() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(ZFS_SNAPSHOT_OUTPUT);

    let result = rt
        .block_on(super::snapshots(&host, &exec, None, None, Some(2)))
        .unwrap();
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "limit=2 should truncate to 2 rows");
    assert!(
        result["truncated"].as_bool().unwrap(),
        "truncated flag should be true"
    );
}

#[test]
fn snapshots_with_dataset_filter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(ZFS_SNAPSHOT_OUTPUT);

    let result = rt
        .block_on(super::snapshots(
            &host,
            &exec,
            None,
            Some("data/appdata"),
            None,
        ))
        .unwrap();
    assert_eq!(result["subaction"], "snapshots");
}

#[test]
fn snapshots_dataset_takes_priority_over_pool() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    // Both pool and dataset supplied — dataset wins (no error expected)
    let exec = FixedExec::ok(ZFS_SNAPSHOT_OUTPUT);

    let result = rt
        .block_on(super::snapshots(
            &host,
            &exec,
            Some("data"),
            Some("data/appdata"),
            None,
        ))
        .unwrap();
    assert_eq!(result["subaction"], "snapshots");
}

// ─── parse_tabular ───────────────────────────────────────────────────────────

#[test]
fn parse_tabular_empty_input() {
    let result = super::parse_tabular("");
    assert_eq!(result.header, "");
    assert!(result.rows.is_empty());
}

#[test]
fn parse_tabular_header_only() {
    let result = super::parse_tabular("NAME   SIZE   ALLOC\n");
    assert_eq!(result.header, "NAME   SIZE   ALLOC");
    assert!(result.rows.is_empty());
}

#[test]
fn parse_tabular_ignores_blank_lines() {
    let raw = "NAME   SIZE\ndata   10G\n\n";
    let result = super::parse_tabular(raw);
    assert_eq!(result.rows.len(), 1);
}

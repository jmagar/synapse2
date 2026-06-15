//! Scout ZFS operations: `pools`, `datasets`, `snapshots`.
//!
//! All operations are READ-ONLY. Commands are developer-hardcoded (not
//! user-supplied), so the `EXEC_ALLOWLIST` guard does NOT gate these calls —
//! but user-supplied filter values (pool/dataset/type) are passed as typed
//! argv arguments, never interpolated into a shell string.
//!
//! # Command shapes (mirrors synapse-mcp scout-zfs.ts)
//!
//! - `pools`     → `zpool list [<pool>]`
//! - `datasets`  → `zfs list [-t <type>] [-r] [<pool>]`
//! - `snapshots` → `zfs list -t snapshot [-r <dataset|pool>]`
//!
//! Output is parsed from the tabular `zpool list` / `zfs list` format into
//! structured JSON (header + rows + truncated flag), mirroring `proc::ps`.

use anyhow::{Result, bail};
use serde_json::{Value, json};

#[cfg(test)]
#[path = "zfs_tests.rs"]
mod tests;

use crate::flux_service::host::{HostExec, LocalExec, RemoteExec, is_local_host};
use crate::ssh::SshExecutor;
use crate::synapse::HostConfig;

// ─── Valid dataset types ──────────────────────────────────────────────────────

/// Allowlist for the `type` filter on `zfs list -t <type>`.
const VALID_ZFS_TYPES: &[&str] = &["filesystem", "volume", "snapshot", "bookmark", "all"];

// ─── pools ───────────────────────────────────────────────────────────────────

/// List ZFS storage pools via `zpool list [<pool>]`.
///
/// - `pool` — optional pool name filter (exact).
///
/// Returns structured JSON: `{ host, subaction, header, rows, truncated }`.
/// If `zpool` is not installed the error propagates as a clear anyhow error.
pub async fn pools(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    pool: Option<&str>,
) -> Result<Value> {
    let mut args: Vec<String> = vec!["list".to_owned()];
    if let Some(p) = pool {
        args.push(p.to_owned());
    }

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = if is_local_host(host) {
        LocalExec.run("zpool", &argv).await?
    } else {
        RemoteExec { executor, host }.run("zpool", &argv).await?
    };

    if output.exit_code == Some(1) && output.stderr.contains("not found") {
        bail!(
            "zpool not found on host `{}`; ZFS may not be installed",
            host.name
        );
    }

    let parsed = parse_tabular(&output.stdout);
    Ok(json!({
        "host": host.name,
        "subaction": "pools",
        "header": parsed.header,
        "rows": parsed.rows,
        "truncated": false,
    }))
}

// ─── datasets ────────────────────────────────────────────────────────────────

/// List ZFS datasets via `zfs list [-t <type>] [-r] [<pool>]`.
///
/// - `pool`      — optional pool filter (positional arg).
/// - `type`      — optional dataset type (`filesystem`, `volume`, etc.).
/// - `recursive` — pass `-r` to list recursively.
pub async fn datasets(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    pool: Option<&str>,
    dataset_type: Option<&str>,
    recursive: bool,
) -> Result<Value> {
    if let Some(t) = dataset_type
        && !VALID_ZFS_TYPES.contains(&t)
    {
        bail!(
            "invalid dataset type `{t}`; must be one of: {}",
            VALID_ZFS_TYPES.join(", ")
        );
    }

    let mut args: Vec<String> = vec!["list".to_owned()];

    if let Some(t) = dataset_type {
        args.push("-t".to_owned());
        args.push(t.to_owned());
    }

    if recursive || pool.is_some() {
        args.push("-r".to_owned());
    }

    if let Some(p) = pool {
        args.push(p.to_owned());
    }

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = if is_local_host(host) {
        LocalExec.run("zfs", &argv).await?
    } else {
        RemoteExec { executor, host }.run("zfs", &argv).await?
    };

    if output.exit_code == Some(1) && output.stderr.contains("not found") {
        bail!(
            "zfs not found on host `{}`; ZFS may not be installed",
            host.name
        );
    }

    let parsed = parse_tabular(&output.stdout);
    Ok(json!({
        "host": host.name,
        "subaction": "datasets",
        "header": parsed.header,
        "rows": parsed.rows,
        "truncated": false,
    }))
}

// ─── snapshots ───────────────────────────────────────────────────────────────

/// List ZFS snapshots via `zfs list -t snapshot [-r <dataset|pool>]`.
///
/// - `pool`    — optional pool filter (used if `dataset` not given).
/// - `dataset` — optional dataset filter (takes priority over `pool`).
/// - `limit`   — max snapshot rows (applied after fetch, header preserved).
pub async fn snapshots(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    pool: Option<&str>,
    dataset: Option<&str>,
    limit: Option<u32>,
) -> Result<Value> {
    let mut args: Vec<String> = vec!["list".to_owned(), "-t".to_owned(), "snapshot".to_owned()];

    // dataset takes priority over pool for the recursive filter target
    if let Some(ds) = dataset {
        args.push("-r".to_owned());
        args.push(ds.to_owned());
    } else if let Some(p) = pool {
        args.push("-r".to_owned());
        args.push(p.to_owned());
    }

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = if is_local_host(host) {
        LocalExec.run("zfs", &argv).await?
    } else {
        RemoteExec { executor, host }.run("zfs", &argv).await?
    };

    if output.exit_code == Some(1) && output.stderr.contains("not found") {
        bail!(
            "zfs not found on host `{}`; ZFS may not be installed",
            host.name
        );
    }

    let parsed = parse_tabular(&output.stdout);
    let limit_n = limit.map(|l| l as usize);
    let truncated = limit_n.map(|l| parsed.rows.len() > l).unwrap_or(false);
    let rows = match limit_n {
        Some(l) => parsed.rows.into_iter().take(l).collect::<Vec<_>>(),
        None => parsed.rows,
    };

    Ok(json!({
        "host": host.name,
        "subaction": "snapshots",
        "header": parsed.header,
        "rows": rows,
        "truncated": truncated,
    }))
}

// ─── tabular parser ──────────────────────────────────────────────────────────

struct TabularOutput {
    header: String,
    rows: Vec<String>,
}

/// Parse `zpool list` / `zfs list` tabular output into header + data rows.
/// The first line is the header; subsequent non-empty lines are data rows.
fn parse_tabular(raw: &str) -> TabularOutput {
    let mut lines = raw.lines();
    let header = lines.next().unwrap_or("").to_owned();
    let rows: Vec<String> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_owned())
        .collect();
    TabularOutput { header, rows }
}

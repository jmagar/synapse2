//! Compose operations (B13): `list`, `status`, `up`, `down`, `restart`,
//! `recreate`, `logs`, `build`, `pull`, `refresh`.
//!
//! # Architecture seam
//!
//! Pure per-host functions here take `&dyn HostExec` (reused from B11) and
//! a project name + config file path resolved from B12's discovery. The service
//! layer (`FluxService`) owns host resolution, discovery lookup, and the
//! destructive confirmation gate (B5).
//!
//! # Gating (locked bead decisions)
//!
//! - **Gated** (destructive — `confirmer.require` before exec): `down`, `restart`, `recreate`.
//! - **Not gated**: `up`, `build`, `pull`, `list`, `status`, `logs`, `refresh`.
//!
//! # `down --remove-volumes`
//!
//! `remove_volumes=true` is only accepted when `force=true`. Validated at the
//! **service layer** (not the shim) so both CLI and MCP inherit the check.
//! Order: validate → confirmer.require → exec.
//!
//! # Command strategy
//!
//! All ops invoke `docker compose -f <config_file> <subcommand>` via the
//! `HostExec` seam. Compose resolves the project from `config_file` so `--project-name`
//! is unnecessary; the `-f` flag is authoritative.

use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::host::HostExec;

#[cfg(test)]
#[path = "compose_ops_tests.rs"]
mod tests;

// ─────────────────────────────── helpers ──────────────────────────────────────

/// Build the base `docker compose -f <file>` argv prefix.
fn compose_base(config_file: &str) -> Vec<&str> {
    vec!["compose", "-f", config_file]
}

// ─────────────────────────────── list ─────────────────────────────────────────

/// Run `docker compose ls --format json` and return the structured project list.
/// (Thin delegation to HostExec; the discovery cache path lives in `FluxService`.)
pub async fn list_on_host(exec: &dyn HostExec, host_name: &str) -> Result<Value> {
    let out = exec
        .run("docker", &["compose", "ls", "--format", "json"])
        .await?;
    Ok(json!({
        "host": host_name,
        "raw": out.stdout.trim(),
    }))
}

// ─────────────────────────────── status ───────────────────────────────────────

/// Run `docker compose -f <file> ps --format json` for the given project.
/// `service_filter` restricts to a single service name when present.
pub async fn status_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
    service_filter: Option<&str>,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.extend_from_slice(&["ps", "--format", "json"]);
    if let Some(svc) = service_filter {
        args.push(svc);
    }
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "output": out.stdout.trim(),
        "exit_code": out.exit_code,
    }))
}

// ─────────────────────────────── up ───────────────────────────────────────────

/// Run `docker compose -f <file> up -d`. Non-gated: `up` creates resources
/// but does not destroy them. Not destructive per bead classification.
pub async fn up_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.extend_from_slice(&["up", "-d"]);
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "action": "up",
        "succeeded": out.exit_code == Some(0),
        "exit_code": out.exit_code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

// ─────────────────────────────── down ─────────────────────────────────────────

/// Parsed + validated arguments for `compose down`.
///
/// `remove_volumes=true` requires `force=true` — validated at the service
/// layer before the confirmer is invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownArgs {
    pub remove_volumes: bool,
    pub force: bool,
}

/// Validate `DownArgs`: `remove_volumes=true` without `force=true` is rejected.
///
/// This is the sole place the cross-field check lives. Returns
/// `ValidationError`-convertible `anyhow::Error` so the MCP boundary maps it
/// to `invalid_params`.
pub fn validate_down_args(args: &DownArgs) -> Result<()> {
    if args.remove_volumes && !args.force {
        bail!(
            "`remove_volumes=true` requires `force=true` to prevent accidental data loss; \
             set both or omit remove_volumes"
        );
    }
    Ok(())
}

/// Run `docker compose -f <file> down [--volumes]`.
/// DESTRUCTIVE: the confirmer gate runs **before** this is called.
pub async fn down_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
    remove_volumes: bool,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.push("down");
    if remove_volumes {
        args.push("--volumes");
    }
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "action": "down",
        "remove_volumes": remove_volumes,
        "succeeded": out.exit_code == Some(0),
        "exit_code": out.exit_code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

// ─────────────────────────────── restart ──────────────────────────────────────

/// Run `docker compose -f <file> restart`.
/// DESTRUCTIVE: the confirmer gate runs **before** this is called.
pub async fn restart_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.push("restart");
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "action": "restart",
        "succeeded": out.exit_code == Some(0),
        "exit_code": out.exit_code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

// ─────────────────────────────── recreate ─────────────────────────────────────

/// Run `docker compose -f <file> up -d --force-recreate`.
/// DESTRUCTIVE: the confirmer gate runs **before** this is called.
pub async fn recreate_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.extend_from_slice(&["up", "-d", "--force-recreate"]);
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "action": "recreate",
        "succeeded": out.exit_code == Some(0),
        "exit_code": out.exit_code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

// ─────────────────────────────── logs ─────────────────────────────────────────

/// Parsed log options for `compose logs`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposeLogOptions {
    /// Tail N lines. None = docker compose default (all).
    pub lines: Option<u32>,
    /// Since: duration string, RFC3339 timestamp, or unix seconds.
    /// Passed through to `docker compose logs --since` unchanged.
    pub since: Option<String>,
    /// Specific service to fetch logs from. None = all services.
    pub service: Option<String>,
}

/// Run `docker compose -f <file> logs [--tail N] [--since T] [<service>]`.
/// Read-only: not gated.
pub async fn logs_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
    opts: &ComposeLogOptions,
) -> Result<Value> {
    let tail_str;
    let mut args = compose_base(config_file);
    args.push("logs");
    args.push("--no-color");
    if let Some(n) = opts.lines {
        tail_str = n.to_string();
        args.push("--tail");
        args.push(&tail_str);
    }
    if let Some(ref since) = opts.since {
        args.push("--since");
        args.push(since.as_str());
    }
    if let Some(ref svc) = opts.service {
        args.push(svc.as_str());
    }
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "logs": out.stdout.trim(),
        "exit_code": out.exit_code,
    }))
}

// ─────────────────────────────── build ────────────────────────────────────────

/// Run `docker compose -f <file> build [<service>]`. Not gated (compose build
/// builds images but does not destroy state — parity with synapse-mcp).
pub async fn build_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
    service: Option<&str>,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.push("build");
    if let Some(svc) = service {
        args.push(svc);
    }
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "action": "build",
        "succeeded": out.exit_code == Some(0),
        "exit_code": out.exit_code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

// ─────────────────────────────── pull ─────────────────────────────────────────

/// Run `docker compose -f <file> pull [<service>]`. Not gated (parity with
/// synapse-mcp; pull brings images down, doesn't destroy existing state).
pub async fn pull_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    project_name: &str,
    config_file: &str,
    service: Option<&str>,
) -> Result<Value> {
    let mut args = compose_base(config_file);
    args.push("pull");
    if let Some(svc) = service {
        args.push(svc);
    }
    let out = exec.run("docker", &args).await?;
    Ok(json!({
        "host": host_name,
        "project": project_name,
        "action": "pull",
        "succeeded": out.exit_code == Some(0),
        "exit_code": out.exit_code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

//! Host inspection operations (B11): `status`, `info`, `uptime`, `resources`,
//! `services`, `network`, `mounts`, `ports`, `doctor`.
//!
//! # Architecture seam
//!
//! Pure per-host functions here take `&dyn HostExec` — a thin seam over either
//! `std::process::Command` (local) or `SshExecutor` (remote). [`FluxService`]
//! routes local vs. SSH, resolves hosts, drives fanout, and calls these fns.
//!
//! # Command strategy
//!
//! Host ops run developer-hardcoded commands — NOT user-supplied strings from
//! `scout exec`. The validate_command / EXEC_ALLOWLIST guard exists only for
//! user-supplied `scout exec` input and does NOT gate these calls.
//!
//! Commands chosen to match synapse-mcp output shapes:
//! - `status`    — bollard docker info + container list (via FluxService)
//! - `info`      — `uname -a`
//! - `uptime`    — `uptime`
//! - `resources` — `cat /proc/meminfo`, `cat /proc/stat`, `df -h`
//! - `services`  — `systemctl list-units --type=service --no-pager`
//! - `network`   — `ip addr show` (fallback: `cat /proc/net/dev`)
//! - `mounts`    — `df -h` (matches synapse-mcp which uses df, not mount)
//! - `ports`     — bollard container list + port mapping (via FluxService)
//! - `doctor`    — aggregates: docker, containers, resources, network, services, logs, processes

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::{HostConfig, HostProtocol};

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;

// ─── HostExec seam ───────────────────────────────────────────────────────────

/// Thin execution seam: one method, execvp-style (program + args, no shell).
/// Local impl runs `std::process::Command`; remote impl delegates to `SshExecutor`.
#[async_trait]
pub trait HostExec: Send + Sync {
    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput>;
}

/// Local executor — spawns `std::process::Command` in-process.
pub struct LocalExec;

#[async_trait]
impl HostExec for LocalExec {
    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        crate::runtime_budget::run_local_command(program, args, None).await
    }
}

/// SSH executor adapter — wraps `Arc<dyn SshExecutor>` bound to a single host.
pub struct RemoteExec<'a> {
    pub executor: &'a dyn SshExecutor,
    pub host: &'a HostConfig,
}

#[async_trait]
impl HostExec for RemoteExec<'_> {
    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        self.executor.exec(self.host, program, args).await
    }
}

/// Determine whether `host` should use local execution.
pub fn is_local_host(host: &HostConfig) -> bool {
    host.protocol == HostProtocol::Local || host.host == "localhost"
}

// ─── host:info ────────────────────────────────────────────────────────────────

/// Run `uname -a` and return a structured info payload.
pub async fn info_on_host(exec: &dyn HostExec, host_name: &str) -> Result<Value> {
    let out = exec.run("uname", &["-a"]).await?;
    Ok(json!({
        "host": host_name,
        "info": out.stdout.trim(),
    }))
}

// ─── host:uptime ─────────────────────────────────────────────────────────────

/// Run `uptime` and return a structured uptime payload.
pub async fn uptime_on_host(exec: &dyn HostExec, host_name: &str) -> Result<Value> {
    let out = exec.run("uptime", &[]).await?;
    Ok(json!({
        "host": host_name,
        "uptime": out.stdout.trim(),
    }))
}

// ─── host:resources ──────────────────────────────────────────────────────────

/// Parse key=value pairs from `/proc/meminfo`.
pub fn parse_meminfo(raw: &str) -> Value {
    let mut total_kb: u64 = 0;
    let mut available_kb: u64 = 0;
    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let key = parts[0].trim_end_matches(':');
        let val: u64 = parts[1].parse().unwrap_or(0);
        match key {
            "MemTotal" => total_kb = val,
            "MemAvailable" => available_kb = val,
            _ => {}
        }
    }
    let used_kb = total_kb.saturating_sub(available_kb);
    let usage_percent = if total_kb > 0 {
        (used_kb as f64 / total_kb as f64 * 100.0).round() as u64
    } else {
        0
    };
    json!({
        "totalKb": total_kb,
        "availableKb": available_kb,
        "usedKb": used_kb,
        "usagePercent": usage_percent,
    })
}

/// Parse `/proc/loadavg` (first three space-separated values).
pub fn parse_loadavg(raw: &str) -> Value {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    let load1: f64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let load5: f64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let load15: f64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    json!({ "load1": load1, "load5": load5, "load15": load15 })
}

/// Collect CPU/memory/disk metrics for one host.
pub async fn resources_on_host(exec: &dyn HostExec, host_name: &str) -> Result<Value> {
    let meminfo_out = exec.run("cat", &["/proc/meminfo"]).await?;
    let loadavg_out = exec.run("cat", &["/proc/loadavg"]).await?;
    let df_out = exec.run("df", &["-h"]).await?;

    let memory = parse_meminfo(&meminfo_out.stdout);
    let load = parse_loadavg(&loadavg_out.stdout);

    Ok(json!({
        "host": host_name,
        "memory": memory,
        "cpu": { "loadAvg": load },
        "disk": df_out.stdout.trim(),
    }))
}

// ─── host:services ────────────────────────────────────────────────────────────

/// Strip the systemctl legend/footer from `systemctl list-units` output.
/// Mirrors the TypeScript `stripSystemctlFooter` in synapse-mcp host.ts.
pub fn strip_systemctl_footer(raw: &str) -> String {
    let mut filtered: Vec<&str> = Vec::new();
    let mut units_listed_line: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        // Capture the "N loaded units listed" summary
        if trimmed
            .to_lowercase()
            .starts_with(|c: char| c.is_ascii_digit())
            && trimmed.to_lowercase().contains("loaded units listed")
        {
            let condensed = trimmed.split('.').next().unwrap_or(trimmed);
            units_listed_line = Some(condensed.to_owned());
            continue;
        }
        // Drop "To show all installed unit files" hint
        if trimmed
            .to_lowercase()
            .starts_with("to show all installed unit files")
        {
            continue;
        }
        // Drop "Legend:" and continuation lines
        if trimmed.to_lowercase().starts_with("legend:") {
            continue;
        }
        if line.len() >= 2 && line.starts_with("  ") {
            let upper = trimmed.to_uppercase();
            if upper.starts_with("LOAD ")
                || upper.starts_with("ACTIVE ")
                || upper.starts_with("SUB ")
            {
                continue;
            }
        }
        filtered.push(line);
    }

    let joined = filtered.join("\n").trim_end().to_owned();
    match units_listed_line {
        Some(summary) => format!("{joined}\n\n{summary}"),
        None => joined,
    }
}

/// List systemd services with optional state/service name filters.
pub async fn services_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    state: Option<&str>,
    service: Option<&str>,
) -> Result<Value> {
    let mut args: Vec<&str> = vec!["list-units", "--type=service", "--no-pager"];

    // Build state arg; we can't use a format!() string and hold a ref,
    // so produce a String and push its slice into a separate slot.
    let state_arg_owned: String;
    if let Some(s) = state {
        if s != "all" {
            state_arg_owned = format!("--state={s}");
            args.push(&state_arg_owned);
        }
    } else {
        // avoid unused-variable; state is None, no arg needed
        let _ = state;
    }

    let service_owned: Option<String> = service.map(str::to_owned);
    if let Some(ref svc) = service_owned {
        args.push(svc.as_str());
    }

    let out = exec.run("systemctl", &args).await?;
    let cleaned = strip_systemctl_footer(&out.stdout);
    Ok(json!({
        "host": host_name,
        "services": cleaned,
    }))
}

// ─── host:network ─────────────────────────────────────────────────────────────

/// Collect network interface info via `ip addr show`; falls back to
/// `cat /proc/net/dev` if ip is unavailable.
pub async fn network_on_host(exec: &dyn HostExec, host_name: &str) -> Result<Value> {
    let output = match exec.run("ip", &["addr", "show"]).await {
        Ok(out) if out.exit_code == Some(0) => out.stdout,
        _ => {
            // Fallback: /proc/net/dev — always available on Linux
            let out = exec.run("cat", &["/proc/net/dev"]).await?;
            out.stdout
        }
    };
    Ok(json!({
        "host": host_name,
        "network": output.trim(),
    }))
}

// ─── host:mounts ──────────────────────────────────────────────────────────────

/// Show mounted filesystems via `df -h` (matches synapse-mcp which uses df).
pub async fn mounts_on_host(exec: &dyn HostExec, host_name: &str) -> Result<Value> {
    let out = exec.run("df", &["-h"]).await?;
    Ok(json!({
        "host": host_name,
        "mounts": out.stdout.trim(),
    }))
}

// ─── host:doctor ─────────────────────────────────────────────────────────────

/// A single doctor check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub check: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
}

/// Run the `resources` check sub-probe for doctor.
pub async fn doctor_check_resources(exec: &dyn HostExec, host_name: &str) -> CheckResult {
    match resources_on_host(exec, host_name).await {
        Ok(r) => {
            let usage = r
                .get("memory")
                .and_then(|m| m.get("usagePercent"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let load1 = r
                .get("cpu")
                .and_then(|c| c.get("loadAvg"))
                .and_then(|l| l.get("load1"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let status = if usage > 90 {
                CheckStatus::Warn
            } else {
                CheckStatus::Pass
            };
            CheckResult {
                check: "resources".into(),
                status,
                detail: format!("Mem {usage}% · Load {load1:.1}"),
            }
        }
        Err(e) => CheckResult {
            check: "resources".into(),
            status: CheckStatus::Fail,
            detail: e.to_string(),
        },
    }
}

/// Run the `network` check sub-probe for doctor.
pub async fn doctor_check_network(exec: &dyn HostExec, host_name: &str) -> CheckResult {
    match network_on_host(exec, host_name).await {
        Ok(r) => {
            let net = r.get("network").and_then(Value::as_str).unwrap_or("");
            // Count interface blocks in `ip addr` output (lines like "1: lo:")
            let iface_count = net
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    t.starts_with(|c: char| c.is_ascii_digit()) && t.contains(':')
                })
                .count();
            CheckResult {
                check: "network".into(),
                status: CheckStatus::Pass,
                detail: format!("{iface_count} network interface(s)"),
            }
        }
        Err(e) => CheckResult {
            check: "network".into(),
            status: CheckStatus::Fail,
            detail: e.to_string(),
        },
    }
}

/// Run the `logs` check sub-probe (journald accessible).
pub async fn doctor_check_logs(exec: &dyn HostExec) -> CheckResult {
    match exec.run("journalctl", &["-n", "1", "--no-pager"]).await {
        Ok(_) => CheckResult {
            check: "logs".into(),
            status: CheckStatus::Pass,
            detail: "journald accessible".into(),
        },
        Err(e) => CheckResult {
            check: "logs".into(),
            status: CheckStatus::Fail,
            detail: e.to_string(),
        },
    }
}

/// Run the `processes` check sub-probe.
pub async fn doctor_check_processes(exec: &dyn HostExec) -> CheckResult {
    match exec.run("ps", &["--no-header", "-e"]).await {
        Ok(out) => {
            let count = out.stdout.lines().filter(|l| !l.trim().is_empty()).count();
            CheckResult {
                check: "processes".into(),
                status: CheckStatus::Pass,
                detail: format!("{count} process(es) running"),
            }
        }
        Err(e) => CheckResult {
            check: "processes".into(),
            status: CheckStatus::Fail,
            detail: e.to_string(),
        },
    }
}

/// Run the `services` check sub-probe (systemd accessible).
pub async fn doctor_check_services(exec: &dyn HostExec, host_name: &str) -> CheckResult {
    match services_on_host(exec, host_name, Some("failed"), None).await {
        Ok(r) => {
            let text = r.get("services").and_then(Value::as_str).unwrap_or("");
            let failed_count = text
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && t.contains(".service")
                })
                .count();
            if failed_count > 0 {
                CheckResult {
                    check: "services".into(),
                    status: CheckStatus::Warn,
                    detail: format!("{failed_count} failed service(s)"),
                }
            } else {
                CheckResult {
                    check: "services".into(),
                    status: CheckStatus::Pass,
                    detail: "no failed services".into(),
                }
            }
        }
        Err(_) => CheckResult {
            check: "services".into(),
            status: CheckStatus::Warn,
            detail: "systemd not available (non-systemd host?)".into(),
        },
    }
}

/// Run the requested doctor checks against a host using its exec seam.
/// `docker` and `containers` checks are handled at the FluxService layer
/// (they use bollard, not the exec seam) and are pre-injected as results.
pub async fn doctor_on_host(
    exec: &dyn HostExec,
    host_name: &str,
    checks: &[String],
    pre_results: Vec<CheckResult>,
) -> Value {
    let mut results = pre_results;

    for check in checks {
        let result = match check.as_str() {
            "resources" => doctor_check_resources(exec, host_name).await,
            "network" => doctor_check_network(exec, host_name).await,
            "logs" => doctor_check_logs(exec).await,
            "processes" => doctor_check_processes(exec).await,
            "services" => doctor_check_services(exec, host_name).await,
            other => CheckResult {
                check: other.to_owned(),
                status: CheckStatus::Fail,
                detail: format!("unknown check: {other}"),
            },
        };
        results.push(result);
    }

    let pass = results
        .iter()
        .filter(|r| r.status == CheckStatus::Pass)
        .count();
    let warn = results
        .iter()
        .filter(|r| r.status == CheckStatus::Warn)
        .count();
    let fail = results
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .count();
    let summary = format!("{pass} pass · {warn} warn · {fail} fail");

    let checks_json: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "check": r.check,
                "status": r.status.as_str(),
                "detail": r.detail,
            })
        })
        .collect();

    json!({
        "host": host_name,
        "checks": checks_json,
        "summary": summary,
    })
}

/// Default set of doctor checks (mirrors synapse-mcp).
pub const DEFAULT_DOCTOR_CHECKS: &[&str] = &[
    "docker",
    "containers",
    "resources",
    "network",
    "services",
    "logs",
    "processes",
];

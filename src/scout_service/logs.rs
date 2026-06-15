//! Scout log operations: `syslog`, `journal`, `dmesg`, `auth`.
//!
//! All operations are READ-ONLY, bounded reads (max 500 lines). Commands are
//! developer-hardcoded — NOT user-supplied. Filter values (unit, priority,
//! since, until) are passed as typed argv arguments, never shell-interpolated.
//! Grep filtering is applied **locally** after remote execution (injection-safe).
//!
//! # Command shapes (mirrors synapse-mcp scout-logs.ts)
//!
//! - `syslog`  → `tail -n <lines> /var/log/syslog` (fallback: `/var/log/messages`)
//! - `journal` → `journalctl -n <lines> --no-pager [-u <unit>] [-p <priority>]
//!                             [--since <since>] [--until <until>]`
//! - `dmesg`   → `dmesg --color=never --lines <n>` (permission errors → helpful message)
//! - `auth`    → `tail -n <lines> /var/log/auth.log` (fallback: `/var/log/secure`)
//!
//! # Security (S-H3)
//!
//! `journal` filter parameters (`unit`, `priority`, `since`, `until`) are
//! validated BEFORE being inserted into the argv. Validators:
//!
//! - `unit`     : no leading `-`, no NUL, max 256 chars.
//! - `priority` : must be one of the eight syslog level names or digits 0-7.
//! - `since`/`until` : no option-like leading `-` (relative times like `-1h`
//!   are allowed), no NUL, max 64 chars.
//!
//! This prevents flag-smuggling (e.g. `unit = "-M container"`) even though the
//! args are passed via execvp (not a shell) — argv position matters to
//! journalctl because `-u -M container` would still be parsed as two separate
//! flags.

use anyhow::{Result, bail};
use serde_json::{Value, json};

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;

use crate::flux_service::host::{HostExec, LocalExec, RemoteExec, is_local_host};
use crate::ssh::SshExecutor;
use crate::synapse::HostConfig;

// ─── journalctl arg validators (S-H3) ────────────────────────────────────────

/// Known syslog priority names accepted by journalctl (RFC 5424 + aliases).
const PRIORITY_ALLOWLIST: &[&str] = &[
    "emerg", "alert", "crit", "err", "warning", "notice", "info", "debug", "0", "1", "2", "3", "4",
    "5", "6", "7",
];

/// Maximum length for `unit` values.
const UNIT_MAX_LEN: usize = 256;

/// Maximum length for `since`/`until` time expressions.
const TIME_FILTER_MAX_LEN: usize = 64;

/// Validate a `journalctl -u <unit>` argument.
///
/// Rejects:
/// - leading `-` (flag smuggling, e.g. `-M container`)
/// - NUL bytes
/// - values longer than `UNIT_MAX_LEN`
fn validate_journal_unit(unit: &str) -> Result<()> {
    if unit.starts_with('-') {
        bail!("journal unit must not start with `-` (got: {unit:?})");
    }
    if unit.contains('\0') {
        bail!("journal unit must not contain NUL bytes");
    }
    if unit.len() > UNIT_MAX_LEN {
        bail!(
            "journal unit too long: {} chars (max {UNIT_MAX_LEN})",
            unit.len()
        );
    }
    Ok(())
}

/// Validate a `journalctl -p <priority>` argument.
///
/// Must be one of the eight syslog level names (emerg … debug) or the
/// numeric equivalents 0–7. All other values, including ranges like
/// `err..warning`, are rejected.
fn validate_journal_priority(priority: &str) -> Result<()> {
    if PRIORITY_ALLOWLIST.contains(&priority) {
        return Ok(());
    }
    bail!(
        "journal priority must be one of {:?}, got: {priority:?}",
        PRIORITY_ALLOWLIST
    );
}

/// Validate a `journalctl --since`/`--until` time filter argument.
///
/// Rejects:
/// - leading `--` (option injection, e.g. `--output=json`)
/// - NUL bytes
/// - values longer than `TIME_FILTER_MAX_LEN`
fn validate_journal_time_filter(value: &str) -> Result<()> {
    // Reject option-like values (`-b`, `--boot`) so a filter can never smuggle a
    // journalctl flag, while still allowing systemd relative times like `-1h`
    // (leading `-` followed by a digit).
    let option_like = value.starts_with("--")
        || (value.starts_with('-')
            && !value[1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit()));
    if option_like {
        bail!("journal time filter must not start with an option-like `-` (got: {value:?})");
    }
    if value.contains('\0') {
        bail!("journal time filter must not contain NUL bytes");
    }
    if value.len() > TIME_FILTER_MAX_LEN {
        bail!(
            "journal time filter too long: {} chars (max {TIME_FILTER_MAX_LEN})",
            value.len()
        );
    }
    Ok(())
}

/// Default line count when no `lines` param is supplied.
pub const DEFAULT_LINES: u32 = 100;
/// Maximum lines that can be requested.
pub const MAX_LINES: u32 = 500;

// ─── syslog ──────────────────────────────────────────────────────────────────

/// Tail the system log via `tail -n <lines> /var/log/syslog`.
/// Falls back to `/var/log/messages` when syslog is absent (e.g. RHEL/CentOS).
/// `grep` is applied locally after retrieval (injection-safe).
pub async fn syslog(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    lines: u32,
    grep: Option<&str>,
) -> Result<Value> {
    let lines = lines.clamp(1, MAX_LINES);
    let lines_s = lines.to_string();

    // Try /var/log/syslog first, fall back to /var/log/messages.
    let output = run_tail_with_fallback(
        host,
        executor,
        &lines_s,
        "/var/log/syslog",
        "/var/log/messages",
    )
    .await?;

    let filtered = apply_grep(output, grep);
    Ok(json!({
        "host": host.name,
        "subaction": "syslog",
        "lines": lines,
        "grep": grep,
        "output": filtered.trim(),
    }))
}

// ─── journal ─────────────────────────────────────────────────────────────────

/// Query the systemd journal via `journalctl -n <lines> --no-pager`.
///
/// Optional filters (passed as argv, not shell-interpolated):
/// - `unit`     → `-u <unit>`
/// - `priority` → `-p <priority>`
/// - `since`    → `--since <since>`
/// - `until`    → `--until <until>`
///
/// `grep` is applied locally after retrieval.
#[allow(clippy::too_many_arguments)]
pub async fn journal(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    lines: u32,
    unit: Option<&str>,
    priority: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    grep: Option<&str>,
) -> Result<Value> {
    let lines = lines.clamp(1, MAX_LINES);
    let lines_s = lines.to_string();

    // Validate all filter parameters BEFORE building argv (S-H3).
    if let Some(u) = unit {
        validate_journal_unit(u)?;
    }
    if let Some(p) = priority {
        validate_journal_priority(p)?;
    }
    if let Some(s) = since {
        validate_journal_time_filter(s)?;
    }
    if let Some(u) = until {
        validate_journal_time_filter(u)?;
    }

    // Build argv with owned strings to avoid lifetime issues.
    let mut args: Vec<String> = vec!["-n".to_owned(), lines_s, "--no-pager".to_owned()];

    if let Some(u) = unit {
        args.push("-u".to_owned());
        args.push(u.to_owned());
    }
    if let Some(p) = priority {
        args.push("-p".to_owned());
        args.push(p.to_owned());
    }
    if let Some(s) = since {
        args.push("--since".to_owned());
        args.push(s.to_owned());
    }
    if let Some(u) = until {
        args.push("--until".to_owned());
        args.push(u.to_owned());
    }

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = if is_local_host(host) {
        LocalExec.run("journalctl", &argv).await?
    } else {
        RemoteExec { executor, host }
            .run("journalctl", &argv)
            .await?
    };

    let filtered = apply_grep(output.stdout, grep);
    Ok(json!({
        "host": host.name,
        "subaction": "journal",
        "lines": lines,
        "unit": unit,
        "priority": priority,
        "since": since,
        "until": until,
        "grep": grep,
        "output": filtered.trim(),
    }))
}

// ─── dmesg ───────────────────────────────────────────────────────────────────

/// Read the kernel ring buffer via `dmesg --color=never`.
///
/// Permission errors (kernel 3.5+ restriction) are caught and returned as a
/// structured help message rather than hard-failing.
/// Grep + tail are applied locally after retrieval.
///
/// # Volume reduction (P-M7)
///
/// `dmesg` can produce ~512 KB over SSH. We reduce transfer volume by passing
/// a generous line cap through the `--lines` flag (supported on util-linux
/// 2.21+); older `dmesg` versions ignore `--lines` gracefully (they produce the
/// full buffer) and the local tail still applies. `dmesg | tail -n` would
/// require shell composition, which violates the execvp invariant, so this is a
/// best-effort size reduction, not a hard cap at the exec layer.
pub async fn dmesg(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    lines: u32,
    grep: Option<&str>,
) -> Result<Value> {
    let lines = lines.clamp(1, MAX_LINES);
    // Fetch slightly more than requested so local grep-then-tail semantics
    // match what the caller expects. Multiply by 4 as a generous margin for
    // grep pre-filtering; cap at 2 × MAX_LINES to avoid unbounded over-fetch.
    let fetch_lines = (lines.saturating_mul(4)).min(MAX_LINES * 2);
    let fetch_lines_s = fetch_lines.to_string();

    // --lines caps the ring-buffer entries at the source when available
    // (util-linux 2.21+). Passed as argv arguments (execvp-safe, no shell).
    let args: Vec<&str> = vec!["--color=never", "--lines", &fetch_lines_s];

    let run_result = if is_local_host(host) {
        LocalExec.run("dmesg", &args).await
    } else {
        RemoteExec { executor, host }.run("dmesg", &args).await
    };

    match run_result {
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            let is_permission = msg.contains("operation not permitted")
                || msg.contains("permission denied")
                || msg.contains("read kernel buffer failed");
            if is_permission {
                return Ok(permission_error_response(host, &e.to_string()));
            }
            Err(e)
        }
        Ok(ref out)
            if out.exit_code == Some(1)
                && (out
                    .stderr
                    .to_lowercase()
                    .contains("operation not permitted")
                    || out.stderr.to_lowercase().contains("permission denied")) =>
        {
            Ok(permission_error_response(host, &out.stderr))
        }
        Ok(out) => {
            let filtered = apply_grep(out.stdout, grep);
            // Tail lines locally.
            let output_lines: Vec<&str> = filtered.trim().lines().collect();
            let tail = output_lines
                .iter()
                .rev()
                .take(lines as usize)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            Ok(json!({
                "host": host.name,
                "subaction": "dmesg",
                "lines": lines,
                "grep": grep,
                "output": tail,
            }))
        }
    }
}

fn permission_error_response(host: &HostConfig, raw_detail: &str) -> Value {
    json!({
        "host": host.name,
        "subaction": "dmesg",
        "error": "permission_required",
        "message": raw_detail,
        "help": "dmesg requires root or CAP_SYSLOG (restricted since Linux kernel 3.5+). Options: run as root, 'sudo sysctl kernel.dmesg_restrict=0', or use 'scout exec' with sudo.",
    })
}

// ─── auth ────────────────────────────────────────────────────────────────────

/// Tail the auth log via `tail -n <lines> /var/log/auth.log`.
/// Falls back to `/var/log/secure` (RHEL/CentOS).
/// `grep` is applied locally after retrieval.
pub async fn auth(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    lines: u32,
    grep: Option<&str>,
) -> Result<Value> {
    let lines = lines.clamp(1, MAX_LINES);
    let lines_s = lines.to_string();

    let output = run_tail_with_fallback(
        host,
        executor,
        &lines_s,
        "/var/log/auth.log",
        "/var/log/secure",
    )
    .await?;

    let filtered = apply_grep(output, grep);
    Ok(json!({
        "host": host.name,
        "subaction": "auth",
        "lines": lines,
        "grep": grep,
        "output": filtered.trim(),
    }))
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Run `tail -n <lines> <primary>`, falling back to `tail -n <lines> <fallback>`
/// if the primary path fails with a "not found" or "no such file" error.
async fn run_tail_with_fallback(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    lines: &str,
    primary: &str,
    fallback: &str,
) -> Result<String> {
    let args_primary = &["-n", lines, primary];
    let primary_result = if is_local_host(host) {
        LocalExec.run("tail", args_primary).await
    } else {
        RemoteExec { executor, host }
            .run("tail", args_primary)
            .await
    };

    match primary_result {
        Ok(out) if out.exit_code == Some(0) => return Ok(out.stdout),
        Ok(out)
            if out
                .stderr
                .to_lowercase()
                .contains("no such file or directory") =>
        {
            // fall through to fallback
            let _ = out;
        }
        Err(ref e) if e.to_string().to_lowercase().contains("no such file") => {
            // fall through to fallback
        }
        Ok(out) => return Ok(out.stdout), // non-zero but not a missing-file error
        Err(e) => return Err(e),
    }

    // Fallback path
    let args_fallback = &["-n", lines, fallback];
    let out = if is_local_host(host) {
        LocalExec.run("tail", args_fallback).await?
    } else {
        RemoteExec { executor, host }
            .run("tail", args_fallback)
            .await?
    };
    Ok(out.stdout)
}

/// Apply an optional grep filter locally (injection-safe).
fn apply_grep(text: String, grep: Option<&str>) -> String {
    match grep {
        None | Some("") => text,
        Some(pattern) => text
            .lines()
            .filter(|line| line.contains(pattern))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

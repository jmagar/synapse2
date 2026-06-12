//! Scout (SSH/local file system) domain formatters.
//!
//! All functions take `&serde_json::Value` and return `String` markdown.
//!
//! Shapes correspond to the output of [`crate::scout`] module functions
//! (`peek`, `exec`, `nodes`).
//!
//! ## STYLE.md compliance
//! - §3.1  Plain text titles (no `##` prefix)
//! - §3.2  Summary lines with pipe separators
//! - §3.6  Freshness timestamps for volatile operations (exec, ps, df)
//! - §4.1  Symbols: ✓ success, ✗ error, ⚠ warning

use serde_json::Value;

use crate::formatters::{format_bytes, format_timestamp, str_field};

// ──────────────────────────────────────────────
// Scout nodes
// ──────────────────────────────────────────────

/// Format a list of scout nodes as markdown.
///
/// Expects an object with `hosts` (array of host config objects or strings).
///
/// # Example output
///
/// ```text
/// Scout Nodes
/// Hosts: 3
///
/// | Host |
/// |------|
/// | squirts |
/// | boops |
/// | nicks |
/// ```
pub fn render_scout_nodes_markdown(data: &Value) -> String {
    let hosts: Vec<String> = if let Some(arr) = data.get("hosts").and_then(|v| v.as_array()) {
        arr.iter()
            .map(|h| {
                h.get("name")
                    .and_then(|v| v.as_str())
                    .or_else(|| h.as_str())
                    .unwrap_or("—")
                    .to_owned()
            })
            .collect()
    } else {
        vec![]
    };

    if hosts.is_empty() {
        return "Scout Nodes\nHosts: 0\n\nNo hosts configured.".to_owned();
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("Scout Nodes".to_owned());
    lines.push(format!("Hosts: {}", hosts.len()));
    lines.push(String::new());
    lines.push("| Host |".to_owned());
    lines.push("|------|".to_owned());
    for host in &hosts {
        lines.push(format!("| {host} |"));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Scout peek (file/directory read)
// ──────────────────────────────────────────────

/// Format a scout peek (file or directory read) result as markdown.
///
/// Expects an object with `host`, `path`, `kind` (`"file"` or `"directory"`),
/// and either `content` (string) or `entries` (string array).
///
/// # Example output (file)
///
/// ```text
/// File Read: squirts:/etc/hostname
/// Size: 8 B | truncated: no
///
/// ```
/// squirts
/// ```
/// ```
pub fn render_scout_peek_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let path = str_field(data, "path");
    let kind = str_field(data, "kind");

    match kind {
        "file" => {
            let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let size_bytes = content.len() as u64;
            let title = format!("File Read: {host}:{path}");
            let summary = format!("Size: {} | truncated: no", format_bytes(size_bytes));
            format!("{title}\n{summary}\n\n```\n{content}\n```")
        }
        "directory" => {
            let entries: Vec<String> = data
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.as_str().map(|s| s.to_owned()))
                        .collect()
                })
                .unwrap_or_default();
            let item_count = entries.len();
            let listing = entries.join("\n");
            let title = format!("Directory Listing: {host}:{path}");
            let summary = format!("Items: {item_count}");
            format!("{title}\n{summary}\n\n```\n{listing}\n```")
        }
        _ => {
            format!("Scout Peek: {host}:{path}\n\nUnknown kind: {kind}")
        }
    }
}

// ──────────────────────────────────────────────
// Scout exec
// ──────────────────────────────────────────────

/// Format a scout exec (command execution) result as markdown.
///
/// Expects an object with `host`, `path`, `command`, `exit_code`, `stdout`, `stderr`.
///
/// # Example output
///
/// ```text
/// ✓ Command Execution: squirts:/tmp
/// Exit: 0
/// As of (UTC): 11:05:20 | 02/13/2026
///
/// **Command:** `uptime`
/// **Exit:** 0
///
/// **Output:**
/// ```
/// 15:23:45 up 3 days
/// ```
/// ```
pub fn render_scout_exec_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let path = str_field(data, "path");
    let command = str_field(data, "command");
    let exit_code = data.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let timestamp = format_timestamp();
    let status_symbol = if exit_code == 0 { '✓' } else { '✗' };

    format!(
        "{status_symbol} Command Execution: {host}:{path}\nExit: {exit_code}\n{timestamp}\n\n**Command:** `{command}`\n**Exit:** {exit_code}\n\n**Output:**\n```\n{stdout}\n```"
    )
}

// ──────────────────────────────────────────────
// Scout process list
// ──────────────────────────────────────────────

/// Format a process listing as markdown.
///
/// Expects `host` and `processes` (raw string from ps output).
pub fn render_scout_ps_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let processes = data.get("processes").and_then(|v| v.as_str()).unwrap_or("");
    let lines: Vec<&str> = processes.lines().filter(|l| !l.trim().is_empty()).collect();
    let process_count = if lines.len() > 1 { lines.len() - 1 } else { 0 };
    let timestamp = format_timestamp();

    format!(
        "Process Listing: {host}\nProcesses: {process_count}\n{timestamp}\n\n```\n{processes}\n```"
    )
}

// ──────────────────────────────────────────────
// Scout disk usage
// ──────────────────────────────────────────────

/// Format disk usage as markdown with warnings for high usage.
///
/// Expects `host` and `disk_usage` (raw string from df output).
pub fn render_scout_df_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let disk_usage = data
        .get("disk_usage")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let lines: Vec<&str> = disk_usage
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let fs_count = if lines.len() > 1 { lines.len() - 1 } else { 0 };
    let timestamp = format_timestamp();

    // Check for high usage (>85%)
    let has_warnings = lines.iter().any(|line| {
        line.split_whitespace()
            .find(|s| s.ends_with('%'))
            .and_then(|s| s.trim_end_matches('%').parse::<u64>().ok())
            .map(|p| p > 85)
            .unwrap_or(false)
    });

    let warning_suffix = if has_warnings { " ⚠" } else { "" };
    let mut output = format!(
        "Disk Usage: {host}\nFilesystems: {fs_count}{warning_suffix}\n{timestamp}\n\n```\n{disk_usage}\n```"
    );

    if has_warnings {
        output.push_str("\n\n⚠ *One or more filesystems exceed 85% usage*");
    }

    output
}

// ──────────────────────────────────────────────
// Scout file transfer
// ──────────────────────────────────────────────

/// Format a file transfer result as markdown.
pub fn render_scout_transfer_markdown(data: &Value) -> String {
    let source_host = str_field(data, "source_host");
    let source_path = str_field(data, "source_path");
    let target_host = str_field(data, "target_host");
    let target_path = str_field(data, "target_path");
    let bytes_transferred = data
        .get("bytes_transferred")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let warning = data.get("warning").and_then(|v| v.as_str());

    let mut lines: Vec<String> = Vec::new();
    lines.push("File Transfer Complete".to_owned());
    lines.push(format!(
        "From: {source_host} | To: {target_host} | Size: {}",
        format_bytes(bytes_transferred)
    ));
    lines.push(String::new());
    lines.push(format!("**From:** {source_host}:{source_path}"));
    lines.push(format!("**To:** {target_host}:{target_path}"));
    lines.push(format!("**Size:** {}", format_bytes(bytes_transferred)));

    if let Some(w) = warning {
        lines.push(String::new());
        lines.push(format!("⚠ {w}"));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Scout find
// ──────────────────────────────────────────────

/// Format find results as markdown.
pub fn render_scout_find_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let path = str_field(data, "path");
    let pattern = str_field(data, "pattern");
    let results = data.get("results").and_then(|v| v.as_str()).unwrap_or("");
    let result_lines: Vec<&str> = results.lines().filter(|l| !l.trim().is_empty()).collect();

    format!(
        "Find Results: {host}:{path}\nPattern: {pattern} | Results: {}\n\n**Pattern:** `{pattern}`\n**Results:** {} files\n\n```\n{results}\n```",
        result_lines.len(),
        result_lines.len()
    )
}

// ──────────────────────────────────────────────
// Scout diff
// ──────────────────────────────────────────────

/// Format a file diff result as markdown.
pub fn render_scout_diff_markdown(data: &Value) -> String {
    let host1 = str_field(data, "host1");
    let path1 = str_field(data, "path1");
    let host2 = str_field(data, "host2");
    let path2 = str_field(data, "path2");
    let diff = data.get("diff").and_then(|v| v.as_str()).unwrap_or("");

    format!(
        "File Diff\nFile 1: {host1}:{path1} | File 2: {host2}:{path2}\n\n**File 1:** {host1}:{path1}\n**File 2:** {host2}:{path2}\n\n```diff\n{diff}\n```"
    )
}

// ──────────────────────────────────────────────
// Scout log formatters
// ──────────────────────────────────────────────

fn render_log_markdown(title: &str, data: &Value) -> String {
    let host = str_field(data, "host");
    let lines_requested = data
        .get("lines_requested")
        .or_else(|| data.get("lines"))
        .and_then(|v| v.as_u64())
        .unwrap_or(50);
    let logs = data
        .get("logs")
        .or_else(|| data.get("output"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let grep_filter = data
        .get("grep_filter")
        .or_else(|| data.get("grep"))
        .and_then(|v| v.as_str());
    let timestamp = format_timestamp();
    let truncated = data
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let actual_lines = logs.lines().filter(|l| !l.trim().is_empty()).count();
    let filter_part = grep_filter
        .map(|f| format!(" | Filter: {f}"))
        .unwrap_or_default();

    format!(
        "{title}: {host}\nLines requested: {lines_requested} | Returned: {actual_lines} | truncated: {}{filter_part}\n{timestamp}\n\n```\n{logs}\n```",
        if truncated { "yes" } else { "no" }
    )
}

/// Format syslog output as markdown.
pub fn render_scout_syslog_markdown(data: &Value) -> String {
    render_log_markdown("Syslog", data)
}

/// Format journalctl output as markdown.
pub fn render_scout_journal_markdown(data: &Value) -> String {
    render_log_markdown("Journal", data)
}

/// Format dmesg output as markdown.
pub fn render_scout_dmesg_markdown(data: &Value) -> String {
    render_log_markdown("Dmesg", data)
}

/// Format auth log output as markdown.
pub fn render_scout_auth_markdown(data: &Value) -> String {
    render_log_markdown("Auth Logs", data)
}

// ──────────────────────────────────────────────
// Scout ZFS formatters
// ──────────────────────────────────────────────

/// Format ZFS pool list as markdown with health status symbols.
pub fn render_scout_zfs_pools_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let pools = data.get("pools").and_then(|v| v.as_str()).unwrap_or("");
    let lines: Vec<&str> = pools.lines().filter(|l| !l.trim().is_empty()).collect();
    let pool_lines = if lines.len() > 1 {
        &lines[1..]
    } else {
        &[] as &[&str]
    };
    let pool_count = pool_lines.len();

    // Annotate with health symbols
    let header = lines
        .first()
        .copied()
        .unwrap_or("NAME SIZE ALLOC FREE HEALTH");
    let mut annotated: Vec<String> = vec![format!("{header}    STATUS")];
    for line in pool_lines {
        let symbol = if line.contains("ONLINE") {
            '●'
        } else if line.contains("DEGRADED") || line.contains("UNAVAIL") {
            '⚠'
        } else if line.contains("FAULTED") || line.contains("OFFLINE") || line.contains("REMOVED") {
            '✗'
        } else {
            '—'
        };
        annotated.push(format!("{line}    {symbol}"));
    }

    format!(
        "ZFS Pools: {host}\nPools: {pool_count}\n\n```\n{}\n```",
        annotated.join("\n")
    )
}

/// Format ZFS datasets as markdown with usage warnings.
pub fn render_scout_zfs_datasets_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let datasets = data.get("datasets").and_then(|v| v.as_str()).unwrap_or("");
    let lines: Vec<&str> = datasets.lines().filter(|l| !l.trim().is_empty()).collect();
    let dataset_lines = if lines.len() > 1 {
        &lines[1..]
    } else {
        &[] as &[&str]
    };
    let dataset_count = dataset_lines.len();

    // Check for high usage (>85%) by parsing USED/AVAIL columns
    let has_warnings = dataset_lines.iter().any(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let used = parse_zfs_size(parts[1]);
            let avail = parse_zfs_size(parts[2]);
            let total = used + avail;
            total > 0.0 && (used / total) > 0.85
        } else {
            false
        }
    });

    let warning_suffix = if has_warnings { " ⚠" } else { "" };
    let mut output = format!(
        "ZFS Datasets: {host}\nDatasets: {dataset_count}{warning_suffix}\n\n```\n{datasets}\n```"
    );

    if has_warnings {
        output.push_str("\n\n⚠ *One or more datasets exceed 85% usage*");
    }

    output
}

/// Format ZFS snapshots as markdown.
pub fn render_scout_zfs_snapshots_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let snapshots = data.get("snapshots").and_then(|v| v.as_str()).unwrap_or("");
    let lines: Vec<&str> = snapshots.lines().filter(|l| !l.trim().is_empty()).collect();
    let snapshot_count = if lines.len() > 1 { lines.len() - 1 } else { 0 };

    format!("ZFS Snapshots: {host}\nSnapshots: {snapshot_count}\n\n```\n{snapshots}\n```")
}

/// Parse ZFS size string (e.g. "8.2T", "512M") to bytes as f64.
fn parse_zfs_size(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let s = s.trim();
    let (num_str, suffix) = if s.ends_with(|c: char| c.is_ascii_alphabetic()) {
        (&s[..s.len() - 1], &s[s.len() - 1..])
    } else {
        (s, "")
    };
    let num: f64 = num_str.parse().unwrap_or(0.0);
    let mult = match suffix.to_ascii_uppercase().as_str() {
        "K" => 1024.0,
        "M" => 1024.0_f64.powi(2),
        "G" => 1024.0_f64.powi(3),
        "T" => 1024.0_f64.powi(4),
        "P" => 1024.0_f64.powi(5),
        _ => 1.0,
    };
    num * mult
}

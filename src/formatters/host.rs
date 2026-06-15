//! Host and system domain formatters.
//!
//! All functions take `&serde_json::Value` and return `String` markdown.
//!
//! ## STYLE.md compliance
//! - §3.1  Plain text titles
//! - §3.2  Summary lines with pipe separators
//! - §3.3  Legend for mixed states
//! - §3.6  Freshness timestamp for volatile data (CPU, memory, disk)
//! - §4.1  Symbols: ● online, ⚠ degraded, ○ offline, ✗ errors
//! - §10.2 Warning thresholds: CPU >90%, MEM >90%, DISK >85%

use serde_json::Value;

use crate::formatters::{format_timestamp, str_field};

// ──────────────────────────────────────────────
// Host status
// ──────────────────────────────────────────────

/// Format a list of host status entries as a markdown table.
///
/// Expects an array of objects with fields: `name`, `connected` (bool),
/// `container_count`, `running_count`, `error` (optional).
///
/// Sorts offline hosts first (severity-first per STYLE.md §12).
///
/// # Example output
///
/// ```text
/// Homelab Host Status
/// Hosts: 2 | Online: 1 | Offline: 1
/// Legend: ● online  ○ offline
///
/// | Host    | Status              | Containers | Running |
/// |---------|---------------------|------------|---------|
/// | boops   | ○ Offline (Timeout) | 0          | 0       |
/// | squirts | ● Online            | 10         | 8       |
/// ```
pub fn render_host_status_markdown(data: &Value) -> String {
    // data may be a single host status object or an array
    let entries: Vec<Value> = if let Some(arr) = data.as_array() {
        arr.clone()
    } else if let Some(arr) = data.get("hosts").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        vec![data.clone()]
    };

    if entries.is_empty() {
        return "Homelab Host Status\n\nNo hosts found.".to_owned();
    }

    // Sort: offline first, then degraded, then online
    let mut sorted = entries.clone();
    sorted.sort_by_key(|h| {
        let connected = h
            .get("connected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let failed_services = h
            .get("failed_service_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if !connected {
            0u8 // offline — highest severity
        } else if failed_services > 0 {
            1 // degraded
        } else {
            2 // healthy online
        }
    });

    let total = sorted.len();
    let online_count = sorted
        .iter()
        .filter(|h| {
            h.get("connected")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count();
    let offline_count = total - online_count;

    // Build summary
    let summary = format!("Hosts: {total} | Online: {online_count} | Offline: {offline_count}");

    // Legend for mixed states
    let has_offline = offline_count > 0;
    let has_online = online_count > 0;
    let legend = if has_offline && has_online {
        Some("Legend: ● online  ○ offline")
    } else {
        None
    };

    let mut lines: Vec<String> = Vec::new();
    lines.push("Homelab Host Status".to_owned());
    lines.push(summary);
    if let Some(leg) = legend {
        lines.push(leg.to_owned());
    }
    lines.push(String::new());
    lines.push("| Host | Status | Containers | Running |".to_owned());
    lines.push("|------|--------|------------|---------|".to_owned());

    for h in &sorted {
        let name = str_field(h, "name");
        let connected = h
            .get("connected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let container_count = h
            .get("container_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let running_count = h.get("running_count").and_then(|v| v.as_u64()).unwrap_or(0);

        let (symbol, status_text) = if !connected {
            let error = h.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown");
            ('○', format!("Offline ({error})"))
        } else {
            ('●', "Online".to_owned())
        };

        lines.push(format!(
            "| {name} | {symbol} {status_text} | {container_count} | {running_count} |"
        ));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Host resources
// ──────────────────────────────────────────────

/// Format host resource usage as markdown.
///
/// Expects an object (or array of objects) with fields:
/// `host`, `cpu_percent`, `mem_used_mb`, `mem_total_mb`, `mem_percent`,
/// `load_1m`, `load_5m`, `load_15m`, `disk` (array of `{mount, used_gb, total_gb, percent}`).
///
/// # Example output
///
/// ```text
/// Host Resources
/// As of (UTC): 11:45:30 | 02/13/2026
///
/// ### squirts
/// - Uptime: 10 days
/// - Load: 1.0, 1.5, 2.0
/// - CPU: 8 cores @ 95% ⚠
/// - Memory: 15360 MB / 16384 MB (94% ⚠)
///
/// **Disks:**
/// - /: 90G / 100G (90% ⚠)
/// ```
pub fn render_host_resources_markdown(data: &Value) -> String {
    let timestamp = format_timestamp();

    // Accept single object or array
    let entries: Vec<Value> = if let Some(arr) = data.as_array() {
        arr.clone()
    } else {
        vec![data.clone()]
    };

    let mut lines: Vec<String> = Vec::new();
    lines.push("Host Resources".to_owned());
    lines.push(timestamp);
    lines.push(String::new());

    for entry in &entries {
        let host = str_field(entry, "host");

        if let Some(error) = entry.get("error").and_then(|v| v.as_str()) {
            lines.push(format!("### {host}"));
            lines.push(format!("✗ {error}"));
            lines.push(String::new());
            continue;
        }

        lines.push(format!("### {host}"));

        // Load average
        let load_1 = entry.get("load_1m").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let load_5 = entry.get("load_5m").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let load_15 = entry
            .get("load_15m")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        lines.push(format!(
            "- **Load:** {load_1:.1}, {load_5:.1}, {load_15:.1}"
        ));

        // CPU — warn if >90%
        let cpu_cores = entry.get("cpu_cores").and_then(|v| v.as_u64()).unwrap_or(0);
        let cpu_percent = entry
            .get("cpu_percent")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let cpu_warn = if cpu_percent > 90.0 { " ⚠" } else { "" };
        if cpu_cores > 0 {
            lines.push(format!(
                "- **CPU:** {cpu_cores} cores @ {cpu_percent:.1}%{cpu_warn}"
            ));
        } else {
            lines.push(format!("- **CPU:** {cpu_percent:.1}%{cpu_warn}"));
        }

        // Memory — warn if >90%
        let mem_used = entry
            .get("mem_used_mb")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mem_total = entry
            .get("mem_total_mb")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mem_percent = entry
            .get("mem_percent")
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| {
                if mem_total > 0 {
                    mem_used as f64 / mem_total as f64 * 100.0
                } else {
                    0.0
                }
            });
        let mem_warn = if mem_percent > 90.0 { " ⚠" } else { "" };
        lines.push(format!(
            "- **Memory:** {mem_used} MB / {mem_total} MB ({mem_percent:.1}%{mem_warn})"
        ));

        // Disks
        if let Some(disks) = entry.get("disk").and_then(|v| v.as_array())
            && !disks.is_empty()
        {
            lines.push(String::new());
            lines.push("**Disks:**".to_owned());
            for d in disks {
                let mount = str_field(d, "mount");
                let used_gb = d.get("used_gb").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let total_gb = d.get("total_gb").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let disk_percent = d
                    .get("percent")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| {
                        if total_gb > 0.0 {
                            used_gb / total_gb * 100.0
                        } else {
                            0.0
                        }
                    });
                let disk_warn = if disk_percent > 85.0 { " ⚠" } else { "" };
                lines.push(format!(
                    "- {mount}: {used_gb:.0}G / {total_gb:.0}G ({disk_percent:.0}%{disk_warn})"
                ));
            }
        }

        lines.push(String::new());
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Host ports
// ──────────────────────────────────────────────

/// Format host port mappings as markdown.
///
/// Expects an object with `host` and `ports` (array of port mapping objects).
///
/// # Example output
///
/// ```text
/// Port Mappings - squirts
///
/// | Port | Protocol | State | Source |
/// |------|----------|-------|--------|
/// | 8080 | tcp | listening | docker |
/// ```
pub fn render_host_ports_markdown(data: &Value) -> String {
    let host = str_field(data, "host");
    let ports = data
        .get("ports")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if ports.is_empty() {
        return format!("Port Mappings - {host}\n\nNo ports found.");
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Port Mappings - {host}"));
    lines.push(String::new());
    lines.push("| Port | Protocol | State | Source | Container | Mapping |".to_owned());
    lines.push("|------|----------|-------|--------|-----------|---------|".to_owned());

    for p in &ports {
        let port = p.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
        let protocol = str_field(p, "protocol");
        let state = str_field(p, "state");
        let source = str_field(p, "source");
        let container = p
            .get("container_name")
            .and_then(|v| v.as_str())
            .unwrap_or("—");
        let container_port = p.get("container_port").and_then(|v| v.as_u64());
        // STYLE.md §4.1: canonical → for port mapping notation
        let mapping = container_port
            .map(|cp| format!("{port} → {cp}"))
            .unwrap_or_else(|| "—".to_owned());
        lines.push(format!(
            "| {port} | {protocol} | {state} | {source} | {container} | {mapping} |"
        ));
    }

    lines.join("\n")
}

//! Docker Compose domain formatters.
//!
//! All functions take `&serde_json::Value` and return `String` markdown.
//!
//! Shapes correspond to `docker compose ls --format json` and
//! `docker compose ps --format json`.
//!
//! ## STYLE.md compliance
//! - §3.1  Plain text titles
//! - §3.2  Status breakdown summary
//! - §3.3  Legend for mixed states
//! - §3.6  Freshness timestamp
//! - §4.1  Symbols: ● running  ◐ partial  ○ stopped

use serde_json::Value;

use crate::formatters::{format_timestamp, str_field, truncate};

// ──────────────────────────────────────────────
// Symbols
// ──────────────────────────────────────────────

fn compose_status_symbol(status: &str) -> char {
    match status.to_ascii_lowercase().as_str() {
        "running" => '●',
        "partial" => '◐',
        _ => '○',
    }
}

// ──────────────────────────────────────────────
// Compose list
// ──────────────────────────────────────────────

/// Format a list of compose projects as markdown.
///
/// Expects an array of objects with `name`, `status`, `config_files` (optional),
/// `services` (optional array of service objects).
///
/// # Example output
///
/// ```text
/// Docker Compose Stacks
/// Status breakdown: running: 2, stopped: 1
/// Legend: ● running  ○ stopped
///
///   Stack                      Status     Services
///   -------------------------  ---------- -------------------------
/// ● myapp                      running    [3] web,db,redis
/// ○ oldapp                     exited     1
/// ```
pub fn render_compose_list_markdown(data: &Value) -> String {
    let projects: Vec<Value> = if let Some(arr) = data.as_array() {
        arr.clone()
    } else if let Some(arr) = data.get("projects").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        vec![data.clone()]
    };

    let host = data.get("host").and_then(|v| v.as_str()).unwrap_or("local");

    if projects.is_empty() {
        return format!("Docker Compose Stacks on {host}\n\nNo compose projects found.");
    }

    // Count by status
    let running_count = projects
        .iter()
        .filter(|p| str_field(p, "status").eq_ignore_ascii_case("running"))
        .count();
    let partial_count = projects
        .iter()
        .filter(|p| str_field(p, "status").eq_ignore_ascii_case("partial"))
        .count();
    let stopped_count = projects.len() - running_count - partial_count;

    let timestamp = format_timestamp();

    // Build status breakdown line
    let mut breakdown_parts: Vec<String> = Vec::new();
    if running_count > 0 {
        breakdown_parts.push(format!("running: {running_count}"));
    }
    if partial_count > 0 {
        breakdown_parts.push(format!("partial: {partial_count}"));
    }
    if stopped_count > 0 {
        breakdown_parts.push(format!("stopped: {stopped_count}"));
    }
    let status_breakdown = format!("Status breakdown: {}", breakdown_parts.join(", "));

    // Legend for mixed states
    let distinct_states: std::collections::HashSet<String> = projects
        .iter()
        .map(|p| str_field(p, "status").to_ascii_lowercase())
        .collect();
    let legend = if distinct_states.len() > 1 {
        Some("Legend: ● running  ◐ partial  ○ stopped")
    } else {
        None
    };

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Docker Compose Stacks on {host}"));
    lines.push(status_breakdown);
    lines.push(timestamp);
    if let Some(leg) = legend {
        lines.push(leg.to_owned());
    }
    lines.push(String::new());

    // Table header
    lines.push("  Stack                      Status     Services".to_owned());
    lines.push("  -------------------------  ---------- -------------------------".to_owned());

    // Sort: stopped first, then partial, then running (severity-first)
    let mut sorted = projects.clone();
    sorted.sort_by_key(
        |p| match str_field(p, "status").to_ascii_lowercase().as_str() {
            "running" => 2u8,
            "partial" => 1,
            _ => 0,
        },
    );
    sorted.reverse(); // highest severity first (running → partial → stopped for compose)
    // Actually compose is opposite: problems first. Let stopped be severity.
    // Re-sort: stopped (0) < partial (1) < running (2) means stopped has lowest numeric, running highest.
    // For severity-first (problems first): sort ascending (0=stopped first).
    sorted.sort_by_key(
        |p| match str_field(p, "status").to_ascii_lowercase().as_str() {
            "running" => 2u8,
            "partial" => 1,
            _ => 0,
        },
    );

    for p in &sorted {
        let name = str_field(p, "name");
        let status = str_field(p, "status").to_ascii_lowercase();
        let symbol = compose_status_symbol(&status);

        // Format services column
        let service_count = p
            .get("services")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .or_else(|| {
                p.get("service_count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
            })
            .unwrap_or(0);

        let service_names: Vec<String> = p
            .get("services")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.get("name").and_then(|v| v.as_str()).map(|s| s.to_owned()))
                    .collect()
            })
            .unwrap_or_default();

        let services_display = if service_count == 0 {
            "—".to_owned()
        } else if !service_names.is_empty() {
            let shown = service_names[..service_names.len().min(2)].join(",");
            let extra = if service_count > 2 { "…" } else { "" };
            format!("[{service_count}] {shown}{extra}")
        } else {
            service_count.to_string()
        };

        let name_col = truncate(name, 25);
        let name_col = format!("{name_col:<25}");
        let status_col = format!("{status:<10}");
        let services_col = truncate(&services_display, 25);

        lines.push(format!("{symbol} {name_col}  {status_col} {services_col}"));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Compose status (per-project service detail)
// ──────────────────────────────────────────────

/// Format a single compose project's service status as markdown.
///
/// Expects an object with `name`, `status`, `services` (array).
///
/// # Example output
///
/// ```text
/// Compose Stack: myapp (● running)
/// Services: 3
///
///   Service                    Status     Health     Ports
///   -------------------------  ---------- ---------- ---------------
/// ● web                        running    healthy    80→80
/// ● db                         running    healthy    —
/// ```
pub fn render_compose_status_markdown(data: &Value) -> String {
    let name = str_field(data, "name");
    let status = str_field(data, "status").to_ascii_lowercase();
    let symbol = compose_status_symbol(&status);
    let timestamp = format_timestamp();

    let services: Vec<Value> = data
        .get("services")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let title = format!("Compose Stack: {name} ({symbol} {status})");
    let summary = format!("Services: {}", services.len());

    let mut lines: Vec<String> = Vec::new();
    lines.push(title);
    lines.push(summary);
    lines.push(timestamp);

    if services.is_empty() {
        lines.push(String::new());
        lines.push("No services found.".to_owned());
        return lines.join("\n");
    }

    // Legend for mixed service states
    let service_states: std::collections::HashSet<String> = services
        .iter()
        .map(|s| str_field(s, "status").to_ascii_lowercase())
        .collect();
    if service_states.len() > 1 {
        lines.push("Legend: ● running  ○ stopped".to_owned());
    }
    lines.push(String::new());

    // Table header
    lines.push("  Service                    Status     Health     Ports".to_owned());
    lines.push("  -------------------------  ---------- ---------- ---------------".to_owned());

    // Sort: stopped first (severity-first)
    let mut sorted = services.clone();
    sorted.sort_by_key(
        |s| match str_field(s, "status").to_ascii_lowercase().as_str() {
            "running" => 1u8,
            _ => 0,
        },
    );

    for svc in &sorted {
        let svc_name = str_field(svc, "name");
        let svc_status = str_field(svc, "status").to_ascii_lowercase();
        let svc_symbol = if svc_status == "running" {
            '●'
        } else {
            '○'
        };
        let health = svc.get("health").and_then(|v| v.as_str()).unwrap_or("—");

        // Format ports
        let ports_display = svc
            .get("publishers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                let port_strs: Vec<String> = arr
                    .iter()
                    .filter_map(|p| {
                        let published = p
                            .get("PublishedPort")
                            .or_else(|| p.get("published_port"))
                            .and_then(|v| v.as_u64())?;
                        let target = p
                            .get("TargetPort")
                            .or_else(|| p.get("target_port"))
                            .and_then(|v| v.as_u64())?;
                        Some(format!("{published}→{target}"))
                    })
                    .collect();
                if port_strs.is_empty() {
                    "—".to_owned()
                } else {
                    port_strs[..port_strs.len().min(3)].join(",")
                }
            })
            .unwrap_or_else(|| "—".to_owned());

        let name_col = truncate(svc_name, 25);
        let name_col = format!("{name_col:<25}");
        let status_col = format!("{svc_status:<10}");
        let health_col = format!("{health:<10}");
        let ports_col = truncate(&ports_display, 15);

        lines.push(format!(
            "{svc_symbol} {name_col}  {status_col} {health_col} {ports_col}"
        ));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Compose lifecycle one-liners
// ──────────────────────────────────────────────

/// Format a compose up result.
pub fn render_compose_up_markdown(data: &Value) -> String {
    let project = str_field(data, "project");
    let host = str_field(data, "host");
    let services_started = data
        .get("services_started")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if services_started == 0 {
        return format!("Compose Up for {project} on {host}\nNo services started");
    }
    format!(
        "Compose Up for {project} on {host}\nServices started: {services_started}\n\n● {project} ({services_started} services)"
    )
}

/// Format a compose down result.
pub fn render_compose_down_markdown(data: &Value) -> String {
    let project = str_field(data, "project");
    let host = str_field(data, "host");
    let services_stopped = data
        .get("services_stopped")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if services_stopped == 0 {
        return format!("Compose Down for {project} on {host}\nNo services stopped");
    }
    format!(
        "Compose Down for {project} on {host}\nServices stopped: {services_stopped}\n\n○ {project} ({services_stopped} services stopped)"
    )
}

/// Format a compose restart result.
pub fn render_compose_restart_markdown(data: &Value) -> String {
    let project = str_field(data, "project");
    let host = str_field(data, "host");
    let services_restarted = data
        .get("services_restarted")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if services_restarted == 0 {
        return format!("Compose Restart for {project} on {host}\nNo services restarted");
    }
    format!(
        "Compose Restart for {project} on {host}\nServices restarted: {services_restarted}\n\n◐ {project} ({services_restarted} services restarted)"
    )
}

/// Format compose logs as markdown.
pub fn render_compose_logs_markdown(data: &Value) -> String {
    let project = str_field(data, "project");
    let host = str_field(data, "host");
    let logs_raw = data.get("logs").and_then(|v| v.as_str()).unwrap_or("");
    let log_lines: Vec<&str> = logs_raw.lines().collect();
    let timestamp = format_timestamp();
    let summary = format!(
        "Lines returned: {} | truncated: no | follow: no",
        log_lines.len()
    );

    const PREVIEW_THRESHOLD: usize = 10;
    let log_output = if log_lines.len() <= PREVIEW_THRESHOLD {
        log_lines.join("\n")
    } else {
        let first5 = log_lines[..5].join("\n");
        let last5 = log_lines[log_lines.len() - 5..].join("\n");
        format!("Preview (first 5):\n{first5}\n  ...\n\nPreview (last 5):\n{last5}\n  ...")
    };

    format!("Container Logs for {project} on {host}\n{summary}\n{timestamp}\n\n{log_output}")
}

//! Container domain formatters.
//!
//! All functions take `&serde_json::Value` (the shapes returned by
//! `SynapseService::flux_container_*` methods) and return `String` markdown.
//!
//! The JSON shapes mirror `docker container ls -a --format '{{json .}}'` and
//! `docker container inspect` output passed through [`crate::docker::docker_json`].
//!
//! ## STYLE.md compliance
//! - §3.1  Plain text titles (no `##` prefix)
//! - §3.2  Summary lines
//! - §3.3  Legend for mixed states
//! - §3.6  Freshness timestamp for volatile data
//! - §4.1  Canonical symbols only: ● running  ◐ restarting  ○ stopped

use serde_json::Value;

use crate::formatters::{format_timestamp, str_field, truncate};

// ──────────────────────────────────────────────
// Symbols
// ──────────────────────────────────────────────

fn container_status_symbol(state: &str) -> char {
    match state {
        "running" => '●',
        "paused" | "restarting" => '◐',
        _ => '○',
    }
}

// ──────────────────────────────────────────────
// Container list
// ──────────────────────────────────────────────

/// Format `docker container ls -a --format '{{json .}}'` output as markdown.
///
/// Accepts the `stdout` field value from [`crate::docker::docker_json`] which
/// contains one JSON object per line.
///
/// # Example output
///
/// ```text
/// Docker Containers
/// Showing 2 containers
/// Legend: ● running  ◐ restarting  ○ stopped
///
///   Container                  Status     Image
///   -------------------------  ---------  -------------------------
/// ● nginx                      running    nginx:latest
/// ○ myapp                      exited     myapp:v1.2.3
/// ```
pub fn render_container_list_markdown(data: &Value) -> String {
    // The docker_json wrapper stores stdout as a string with one JSON object per line.
    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Docker Containers\n\n✗ {error}");
    }

    // Parse NDJSON (one object per line)
    let containers: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if containers.is_empty() {
        return "Docker Containers\n\nNo containers found.".to_owned();
    }

    // Determine states for legend
    let states: std::collections::HashSet<String> = containers
        .iter()
        .map(|c| str_field(c, "State").to_ascii_lowercase())
        .collect();
    let has_multiple_states = states.len() > 1;

    let lines_count = containers.len();
    let mut lines: Vec<String> = Vec::new();
    lines.push("Docker Containers".to_owned());
    lines.push(format!("Showing {lines_count} containers"));
    if has_multiple_states {
        lines.push("Legend: ● running  ◐ restarting  ○ stopped".to_owned());
    }
    lines.push(String::new());

    // Table header (STYLE.md §5.1)
    lines.push("  Container                  Status     Image".to_owned());
    lines.push("  -------------------------  ---------  -------------------------".to_owned());

    // Sort: running first, then restarting, then stopped (severity-first §12)
    let mut sorted = containers.clone();
    sorted.sort_by_key(
        |c| match str_field(c, "State").to_ascii_lowercase().as_str() {
            "running" => 2u8,
            "paused" | "restarting" => 1,
            _ => 0,
        },
    );
    sorted.reverse(); // highest severity (running) first for containers

    for c in &sorted {
        // docker --format '{{json .}}' names: ID, Names, Image, Status, State, Ports
        let name = str_field(c, "Names");
        let name = name.trim_start_matches('/');
        let state = str_field(c, "State").to_ascii_lowercase();
        let image = str_field(c, "Image");
        let symbol = container_status_symbol(&state);

        let name_col = truncate(name, 25);
        let name_col = format!("{name_col:<25}");
        let state_col = truncate(&state, 9);
        let state_col = format!("{state_col:<9}");
        let image_col = truncate(image, 25);

        lines.push(format!("{symbol} {name_col}  {state_col}  {image_col}"));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Container inspect
// ──────────────────────────────────────────────

/// Format `docker container inspect <id>` output as markdown.
///
/// Accepts the `stdout` field which is a JSON array from inspect.
///
/// # Example output
///
/// ```text
/// Container: nginx (local)
///
/// **State**
/// - Status: running
/// - Running: true
/// ...
/// ```
pub fn render_container_inspect_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Container Inspect\n\n✗ {error}");
    }

    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("[]");
    let arr: Vec<Value> = serde_json::from_str(stdout).unwrap_or_default();
    let info = match arr.first() {
        Some(v) => v,
        None => return "Container Inspect\n\nNo data returned.".to_owned(),
    };

    let name = info.get("Name").and_then(|v| v.as_str()).unwrap_or("—");
    let name = name.trim_start_matches('/');

    let state = info.get("State").cloned().unwrap_or_default();
    let config = info.get("Config").cloned().unwrap_or_default();
    let mounts = info
        .get("Mounts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let network = info.get("NetworkSettings").cloned().unwrap_or_default();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Container: {name}"));
    lines.push(String::new());

    // State section
    lines.push("**State**".to_owned());
    lines.push(format!("- Status: {}", str_field(&state, "Status")));
    lines.push(format!(
        "- Running: {}",
        state
            .get("Running")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    ));
    lines.push(format!("- Started: {}", str_field(&state, "StartedAt")));
    lines.push(format!(
        "- Restart Count: {}",
        info.get("RestartCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    ));
    lines.push(String::new());

    // Config section
    lines.push("**Configuration**".to_owned());
    lines.push(format!("- Image: {}", str_field(&config, "Image")));
    let cmd_val = config.get("Cmd").cloned().unwrap_or_default();
    let cmd: Vec<&str> = cmd_val
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    lines.push(format!("- Command: {}", cmd.join(" ")));
    lines.push(format!(
        "- Working Dir: {}",
        config
            .get("WorkingDir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("/")
    ));
    lines.push(String::new());

    // Env section (redacted)
    if let Some(env_arr) = config.get("Env").and_then(|v| v.as_array())
        && !env_arr.is_empty()
    {
        lines.push("**Environment Variables**".to_owned());
        for env in env_arr.iter().take(20) {
            let env_str = env.as_str().unwrap_or("");
            let key = env_str.split('=').next().unwrap_or("");
            let is_sensitive = key.to_ascii_lowercase().contains("password")
                || key.to_ascii_lowercase().contains("secret")
                || key.to_ascii_lowercase().contains("token")
                || key.to_ascii_lowercase().contains("api_key")
                || key.to_ascii_lowercase().contains("apikey");
            if is_sensitive {
                lines.push(format!("- {key}=****"));
            } else {
                lines.push(format!("- {env_str}"));
            }
        }
        if env_arr.len() > 20 {
            lines.push(format!("- ... and {} more", env_arr.len() - 20));
        }
        lines.push(String::new());
    }

    // Mounts section
    if !mounts.is_empty() {
        lines.push("**Mounts**".to_owned());
        for m in &mounts {
            let src = str_field(m, "Source");
            let dst = str_field(m, "Destination");
            let mode = str_field(m, "Mode");
            let mode = if mode == "—" { "rw" } else { mode };
            // STYLE.md §4.1: canonical → for mapping notation
            lines.push(format!("- {src} → {dst} ({mode})"));
        }
        lines.push(String::new());
    }

    // Ports section
    if let Some(ports) = network.get("Ports").and_then(|v| v.as_object()) {
        let port_bindings: Vec<String> = ports
            .iter()
            .filter_map(|(container_port, bindings)| {
                let bindings = bindings.as_array()?;
                if bindings.is_empty() {
                    return None;
                }
                let binding_strs: Vec<String> = bindings
                    .iter()
                    .filter_map(|b| {
                        let host_ip = b
                            .get("HostIp")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0.0.0.0");
                        let host_port = b.get("HostPort").and_then(|v| v.as_str())?;
                        Some(format!("{host_ip}:{host_port} → {container_port}"))
                    })
                    .collect();
                Some(binding_strs)
            })
            .flatten()
            .collect();

        if !port_bindings.is_empty() {
            lines.push("**Ports**".to_owned());
            for p in &port_bindings {
                lines.push(format!("- {p}"));
            }
            lines.push(String::new());
        }
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Container logs
// ──────────────────────────────────────────────

const PREVIEW_THRESHOLD: usize = 10;

/// Format `docker container logs` output as markdown.
///
/// Uses preview format (first 5 + last 5) for outputs > 10 lines.
pub fn render_container_logs_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Container Logs\n\n✗ {error}");
    }

    let container = data
        .get("container")
        .and_then(|v| v.as_str())
        .unwrap_or("—");
    let host = data.get("host").and_then(|v| v.as_str());

    let log_lines: Vec<String> = if let Some(lines) = data.get("lines").and_then(|v| v.as_array()) {
        lines
            .iter()
            .filter_map(|line| line.as_str())
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    } else {
        data.get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .lines()
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    };

    if log_lines.is_empty() {
        return format!("Container Logs for {container}\n\nNo logs found.");
    }

    let title = match host {
        Some(host) => format!("Container Logs for {container} ({host})"),
        None => format!("Container Logs for {container}"),
    };
    let timestamp = format_timestamp();
    let truncated = data
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let summary = format!(
        "Lines returned: {} | truncated: {} | follow: no",
        data.get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(log_lines.len() as u64),
        if truncated { "yes" } else { "no" }
    );

    let log_output = if log_lines.len() <= PREVIEW_THRESHOLD {
        log_lines.join("\n")
    } else {
        let first5 = log_lines[..5].join("\n");
        let last5 = log_lines[log_lines.len() - 5..].join("\n");
        format!("Preview (first 5):\n{first5}\n...\n\nPreview (last 5):\n{last5}\n...")
    };

    format!("{title}\n{summary}\n{timestamp}\n\n{log_output}")
}

// ──────────────────────────────────────────────
// Container search
// ──────────────────────────────────────────────

/// Format a container search result as markdown.
pub fn render_container_search_markdown(data: &Value) -> String {
    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let query = data.get("query").and_then(|v| v.as_str()).unwrap_or("");

    let containers: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if containers.is_empty() {
        return format!("No containers found matching '{query}'.");
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "Search Results for '{}' ({} matches)",
        query,
        containers.len()
    ));
    lines.push(String::new());

    for c in &containers {
        let name = str_field(c, "Names").trim_start_matches('/').to_owned();
        let state = str_field(c, "State").to_ascii_lowercase();
        let image = str_field(c, "Image");
        let symbol = container_status_symbol(&state);
        lines.push(format!("{symbol} **{name}**"));
        lines.push(format!("   Image: {image} | State: {state}"));
        lines.push(String::new());
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Lifecycle one-liners
// ──────────────────────────────────────────────

/// Format a container start result.
pub fn render_container_start_markdown(data: &Value) -> String {
    let container = data
        .get("container")
        .and_then(|v| v.as_str())
        .unwrap_or("—");
    format!("Container {container} started")
}

/// Format a container stop result.
pub fn render_container_stop_markdown(data: &Value) -> String {
    let container = data
        .get("container")
        .and_then(|v| v.as_str())
        .unwrap_or("—");
    format!("Container {container} stopped")
}

/// Format a container restart result.
pub fn render_container_restart_markdown(data: &Value) -> String {
    let container = data
        .get("container")
        .and_then(|v| v.as_str())
        .unwrap_or("—");
    format!("Container {container} restarted")
}

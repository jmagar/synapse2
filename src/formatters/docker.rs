//! Docker system domain formatters.
//!
//! All functions take `&serde_json::Value` and return `String` markdown.
//!
//! Shapes correspond to `docker info --format '{{json .}}'`,
//! `docker images --format '{{json .}}'`, `docker network ls --format '{{json .}}'`,
//! and `docker volume ls --format '{{json .}}'` passed through
//! [`crate::docker::docker_json`].
//!
//! ## STYLE.md compliance
//! - §3.1  Plain text titles
//! - §3.2  Summary lines
//! - §4.1  ✗ for errors (no emoji)

use serde_json::Value;

use crate::formatters::{format_bytes, str_field, truncate};

// ──────────────────────────────────────────────
// Docker info
// ──────────────────────────────────────────────

/// Format `docker info --format '{{json .}}'` output as markdown.
///
/// # Example output
///
/// ```text
/// Docker System Info
///
/// - Docker: 24.0.5 (API 1.43)
/// - Containers: 12 running / 15 total
/// - Images: 28
/// - OS: linux (x86_64)
/// - Storage: overlay2
/// ```
pub fn render_docker_info_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Docker System Info\n\n✗ {error}");
    }

    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("{}");
    let info: Value = serde_json::from_str(stdout).unwrap_or_default();

    let docker_version = str_field(&info, "ServerVersion");
    let api_version = info
        .get("Server")
        .and_then(|s| s.get("Components"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("Details"))
        .and_then(|d| d.get("ApiVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or("—");
    let os_type = str_field(&info, "OSType");
    let arch = str_field(&info, "Architecture");
    let kernel = str_field(&info, "KernelVersion");
    let cpus = info.get("NCPU").and_then(|v| v.as_u64()).unwrap_or(0);
    let mem_bytes = info.get("MemTotal").and_then(|v| v.as_u64()).unwrap_or(0);
    let storage_driver = str_field(&info, "Driver");
    let containers_running = info
        .get("ContainersRunning")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let containers_total = info.get("Containers").and_then(|v| v.as_u64()).unwrap_or(0);
    let images = info.get("Images").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut lines: Vec<String> = Vec::new();
    lines.push("Docker System Info".to_owned());
    lines.push(String::new());
    lines.push(format!("- Docker: {docker_version} (API {api_version})"));
    lines.push(format!("- OS: {os_type} ({arch})"));
    lines.push(format!("- Kernel: {kernel}"));
    lines.push(format!(
        "- CPUs: {cpus} | Memory: {}",
        format_bytes(mem_bytes)
    ));
    lines.push(format!("- Storage: {storage_driver}"));
    lines.push(format!(
        "- Containers: {containers_running} running / {containers_total} total"
    ));
    lines.push(format!("- Images: {images}"));

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Docker disk usage (df)
// ──────────────────────────────────────────────

/// Format docker disk usage data as markdown.
///
/// Accepts a `Value` that may be an array of image/container/volume objects
/// (the shape from `docker system df --format '{{json .}}'`).
///
/// # Example output
///
/// ```text
/// Docker Disk Usage
///
/// | Type | Count | Size | Reclaimable |
/// |------|-------|------|-------------|
/// | Images | 5 | 1.2 GB | 800.0 MB |
/// | Containers | 3 | 10.0 MB | 5.0 MB |
/// | Volumes | 2 | 50.0 MB | 0 B |
/// ```
pub fn render_docker_df_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Docker Disk Usage\n\n✗ {error}");
    }

    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");

    // docker system df --format '{{json .}}' returns one JSON object per line
    // with keys: Type, Total, Active, Size, Reclaimable
    let entries: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if entries.is_empty() {
        // Try parsing as a single object
        let obj: Value = serde_json::from_str(stdout).unwrap_or_default();
        if obj.is_null() {
            return "Docker Disk Usage\n\nNo disk usage data available.".to_owned();
        }
    }

    let mut lines: Vec<String> = vec![
        "Docker Disk Usage".to_owned(),
        String::new(),
        "| Type | Count | Size | Reclaimable |".to_owned(),
        "|------|-------|------|-------------|".to_owned(),
    ];

    for entry in &entries {
        let ty = str_field(entry, "Type");
        let total = entry.get("Total").and_then(|v| v.as_u64()).unwrap_or(0);
        let size_str = str_field(entry, "Size");
        let reclaimable_str = str_field(entry, "Reclaimable");
        lines.push(format!(
            "| {ty} | {total} | {size_str} | {reclaimable_str} |"
        ));
    }

    if entries.is_empty() {
        lines.push("| — | — | — | — |".to_owned());
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Docker images
// ──────────────────────────────────────────────

/// Format `docker images --format '{{json .}}'` output as markdown.
///
/// # Example output
///
/// ```text
/// Docker Images
/// Showing 3 images
///
/// | ID | Repository:Tag | Size |
/// |----|----------------|------|
/// | abc123… | nginx:latest | 142.0 MB |
/// ```
pub fn render_docker_images_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Docker Images\n\n✗ {error}");
    }

    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");

    let images: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if images.is_empty() {
        return "Docker Images\n\nNo images found.".to_owned();
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("Docker Images".to_owned());
    lines.push(format!("Showing {} images", images.len()));
    lines.push(String::new());
    lines.push("| ID | Repository:Tag | Size |".to_owned());
    lines.push("|----|----------------|------|".to_owned());

    for img in &images {
        let id = str_field(img, "ID");
        // Truncate SHA to first 12 chars
        let id_short = if id.len() > 12 { &id[..12] } else { id };
        let repo = str_field(img, "Repository");
        let tag = str_field(img, "Tag");
        let size = str_field(img, "Size");
        let repo_tag = if repo == "—" && tag == "—" {
            "<none>".to_owned()
        } else {
            format!("{repo}:{tag}")
        };
        let repo_tag_col = truncate(&repo_tag, 30);
        lines.push(format!("| {id_short}… | {repo_tag_col} | {size} |"));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Docker networks
// ──────────────────────────────────────────────

/// Format `docker network ls --format '{{json .}}'` output as markdown.
///
/// # Example output
///
/// ```text
/// Docker Networks
/// Showing 3 networks
///
/// - bridge (bridge, local)
/// - host (host, local)
/// - myapp_net (overlay, swarm)
/// ```
pub fn render_docker_networks_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Docker Networks\n\n✗ {error}");
    }

    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");

    let networks: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if networks.is_empty() {
        return "Docker Networks\n\nNo networks found.".to_owned();
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("Docker Networks".to_owned());
    lines.push(format!("Showing {} networks", networks.len()));
    lines.push(String::new());

    for net in &networks {
        let name = str_field(net, "Name");
        let driver = str_field(net, "Driver");
        let scope = str_field(net, "Scope");
        lines.push(format!("- {name} ({driver}, {scope})"));
    }

    lines.join("\n")
}

// ──────────────────────────────────────────────
// Docker volumes
// ──────────────────────────────────────────────

/// Format `docker volume ls --format '{{json .}}'` output as markdown.
///
/// Named volumes are shown as-is; anonymous SHA256 hashes are truncated to
/// 16 characters with a `…` suffix to reduce visual noise.
///
/// # Example output
///
/// ```text
/// Docker Volumes
/// Showing 4 volumes — named: 2, anonymous: 2
///
/// - myapp_data (local)
/// - postgres_data (local)
/// - 1a2b3c4d5e6f7g8h… (anon) (local)
/// ```
pub fn render_docker_volumes_markdown(data: &Value) -> String {
    let available = data
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !available {
        let error = data
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("stderr").and_then(|v| v.as_str()))
            .unwrap_or("Docker unavailable");
        return format!("Docker Volumes\n\n✗ {error}");
    }

    let stdout = data.get("stdout").and_then(|v| v.as_str()).unwrap_or("");

    let volumes: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if volumes.is_empty() {
        return "Docker Volumes\n\nNo volumes found.".to_owned();
    }

    let named_count = volumes
        .iter()
        .filter(|v| !is_anonymous_volume(str_field(v, "Name")))
        .count();
    let anon_count = volumes.len() - named_count;

    let mut lines: Vec<String> = Vec::new();
    lines.push("Docker Volumes".to_owned());
    lines.push(format!(
        "Showing {} volumes — named: {named_count}, anonymous: {anon_count}",
        volumes.len()
    ));
    lines.push(String::new());

    for vol in &volumes {
        let name = str_field(vol, "Name");
        let driver = str_field(vol, "Driver");
        let display_name = format_volume_name(name);
        lines.push(format!("- {display_name} ({driver})"));
    }

    lines.join("\n")
}

/// Detect anonymous (SHA256 hash) volume names.
fn is_anonymous_volume(name: &str) -> bool {
    name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit())
}

/// Format a volume name: truncate anonymous hashes to 16 chars + `… (anon)`.
fn format_volume_name(name: &str) -> String {
    if is_anonymous_volume(name) {
        format!("{}… (anon)", &name[..16])
    } else {
        name.to_owned()
    }
}

// ──────────────────────────────────────────────
// Docker host status
// ──────────────────────────────────────────────

/// Format the combined host status (docker info + host) as markdown.
pub fn render_docker_host_status_markdown(data: &Value) -> String {
    let host = data.get("host").and_then(|v| v.as_str()).unwrap_or("local");
    let docker_data = data.get("docker").cloned().unwrap_or_default();

    let docker_section = render_docker_info_markdown(&docker_data);

    format!("Host Status — {host}\n\n{docker_section}")
}


//! Read-only container operations (B8): `list`, `inspect`, `logs`, `stats`,
//! `top`, `search`.
//!
//! # Architecture seam
//!
//! The **pure** per-host functions in this module operate on
//! `&dyn ContainerOps` so they are fully unit-testable with
//! [`MockDockerClient`](crate::docker_client::MockDockerClient) — no live docker
//! daemon required. [`FluxService`](super::FluxService) resolves hosts, acquires
//! the cached bollard client, and drives the fanout; it then calls these pure
//! functions per host.
//!
//! # JSON contract (B4 consumes / B17 verifies)
//!
//! - **list / search / stats(no id)** — fan out across all target hosts and
//!   return a **flat, host-tagged** collection plus a `partial`/`errors`
//!   section. Every container item carries a `host` field.
//! - **inspect / logs / top** — single-container ops. When `host` is specified
//!   they target it directly; when omitted they **fan out to find** the host
//!   owning the container, stop at the first match, and return a single
//!   host-tagged result. Not-found is reported only when no host has it.
//!
//! # One-shot streaming (locked decision)
//!
//! `logs` and `stats` are bollard **streams**. We drive them one-shot:
//! `LogsOptions.follow = false` and `StatsOptions { stream: false, one_shot:
//! true }`. A follow/stream read would never terminate against a live daemon.

use anyhow::Result;
use bollard::container::LogOutput;
use bollard::query_parameters::{ListContainersOptions, LogsOptions, StatsOptions, TopOptions};
use chrono::Utc;
use futures_util::StreamExt;
use serde_json::{Map, Value, json};

use crate::docker_client::ContainerOps;

/// Maximum log lines returned (parity with synapse-mcp `MAX_LOG_LINES`).
pub const MAX_LOG_LINES: u32 = 500;
/// Default log lines (parity with synapse-mcp `DEFAULT_LOG_LINES`).
pub const DEFAULT_LOG_LINES: u32 = 50;

// ───────────────────────────── filter params ─────────────────────────────

/// Client-side list filters. `state == "all"` (or `None`) disables state pruning.
#[derive(Debug, Clone, Default)]
pub struct ListFilters {
    /// `running` / `exited` / `paused` / `restarting` / `all`.
    pub state: Option<String>,
    /// Partial (case-insensitive substring) match on container name.
    pub name_filter: Option<String>,
    /// Partial (case-insensitive substring) match on image.
    pub image_filter: Option<String>,
    /// `key=value` label match (server-side via bollard `filters`).
    pub label_filter: Option<String>,
}

/// Log read options.
#[derive(Debug, Clone)]
pub struct LogOptions {
    /// Tail line count, clamped to `1..=MAX_LOG_LINES`.
    pub lines: u32,
    /// ISO 8601 timestamp, unix seconds, or relative (`"1h"`, `"30m"`).
    pub since: Option<String>,
    /// Same forms as `since`.
    pub until: Option<String>,
    /// Substring filter applied to each rendered log line.
    pub grep: Option<String>,
    /// `stdout` / `stderr` / `both`.
    pub stream: String,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            lines: DEFAULT_LOG_LINES,
            since: None,
            until: None,
            grep: None,
            stream: "both".to_owned(),
        }
    }
}

// ───────────────────────────── list ─────────────────────────────

/// List containers on a single host, applying client-side filters.
///
/// Returns a `Vec<Value>` of host-tagged container summaries. The `host` field
/// is injected by the caller-supplied `host_name`.
pub async fn list_on_host(
    client: &dyn ContainerOps,
    host_name: &str,
    filters: &ListFilters,
) -> Result<Vec<Value>, bollard::errors::Error> {
    let state = filters.state.as_deref().unwrap_or("all");
    let mut opts = ListContainersOptions {
        // `running` only → all=false is sufficient; anything else needs all=true.
        all: state != "running",
        ..Default::default()
    };
    if let Some(label) = filters.label_filter.as_deref().filter(|s| !s.is_empty()) {
        let mut map = std::collections::HashMap::new();
        map.insert("label".to_owned(), vec![label.to_owned()]);
        opts.filters = Some(map);
    }

    let containers = client.list_containers(Some(opts)).await?;
    let mut out = Vec::new();
    for c in &containers {
        if !state_matches(state, c.state.as_ref()) {
            continue;
        }
        let name = container_name(c);
        if let Some(nf) = filters.name_filter.as_deref().filter(|s| !s.is_empty())
            && !name.to_ascii_lowercase().contains(&nf.to_ascii_lowercase())
        {
            continue;
        }
        let image = c.image.clone().unwrap_or_default();
        if let Some(imf) = filters.image_filter.as_deref().filter(|s| !s.is_empty())
            && !image
                .to_ascii_lowercase()
                .contains(&imf.to_ascii_lowercase())
        {
            continue;
        }
        out.push(summary_to_value(c, host_name));
    }
    Ok(out)
}

/// True when `state` (filter) is `all` or matches the container's serialized state.
fn state_matches(
    state: &str,
    container_state: Option<&bollard::models::ContainerSummaryStateEnum>,
) -> bool {
    if state == "all" {
        return true;
    }
    let actual = container_state
        .and_then(|s| serde_json::to_value(s).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    actual == state
}

/// Container display name: first entry, leading `/` stripped, else short id.
fn container_name(c: &bollard::models::ContainerSummary) -> String {
    c.names
        .as_ref()
        .and_then(|n| n.first())
        .map(|n| n.trim_start_matches('/').to_owned())
        .unwrap_or_else(|| {
            c.id.as_deref()
                .map(|id| id.chars().take(12).collect())
                .unwrap_or_default()
        })
}

/// Render a [`bollard::models::ContainerSummary`] into the B8 host-tagged shape.
fn summary_to_value(c: &bollard::models::ContainerSummary, host_name: &str) -> Value {
    let state = c
        .state
        .as_ref()
        .and_then(|s| serde_json::to_value(s).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    json!({
        "id": c.id.clone().unwrap_or_default(),
        "name": container_name(c),
        "image": c.image.clone().unwrap_or_default(),
        "state": state,
        "status": c.status.clone().unwrap_or_default(),
        "labels": c.labels.clone().unwrap_or_default(),
        "host": host_name,
    })
}

// ───────────────────────────── search ─────────────────────────────

/// Full-text (substring, case-insensitive) match over name + image + labels.
///
/// This is the locked B8 semantics — a client-side grep, NOT a bollard
/// server-side `name` filter (which is what synapse-mcp happened to use).
pub fn search_matches(container: &Value, query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    let name = container.get("name").and_then(Value::as_str).unwrap_or("");
    let image = container.get("image").and_then(Value::as_str).unwrap_or("");
    if name.to_ascii_lowercase().contains(&q) || image.to_ascii_lowercase().contains(&q) {
        return true;
    }
    if let Some(labels) = container.get("labels").and_then(Value::as_object) {
        for (k, v) in labels {
            if k.to_ascii_lowercase().contains(&q) {
                return true;
            }
            if let Some(val) = v.as_str()
                && val.to_ascii_lowercase().contains(&q)
            {
                return true;
            }
        }
    }
    false
}

// ───────────────────────────── inspect ─────────────────────────────

/// Inspect a single container. `summary == true` returns an abbreviated form.
pub async fn inspect_on_host(
    client: &dyn ContainerOps,
    host_name: &str,
    container_id: &str,
    summary: bool,
) -> Result<Value, bollard::errors::Error> {
    let resp = client.inspect_container(container_id, None).await?;
    let full = serde_json::to_value(&resp).unwrap_or(Value::Null);
    let body = if summary {
        json!({
            "id": full.get("Id").cloned().unwrap_or(Value::Null),
            "name": full.get("Name").cloned().unwrap_or(Value::Null),
            "image": full.get("Config").and_then(|c| c.get("Image")).cloned().unwrap_or(Value::Null),
            "state": full.get("State").and_then(|s| s.get("Status")).cloned().unwrap_or(Value::Null),
            "created": full.get("Created").cloned().unwrap_or(Value::Null),
        })
    } else {
        full
    };
    Ok(json!({ "host": host_name, "container": body, "summary": summary }))
}

// ───────────────────────────── top ─────────────────────────────

/// Running processes inside a container (`docker top`, bollard-wrapped).
pub async fn top_on_host(
    client: &dyn ContainerOps,
    host_name: &str,
    container_id: &str,
) -> Result<Value, bollard::errors::Error> {
    let resp = client
        .top_processes(container_id, None::<TopOptions>)
        .await?;
    Ok(json!({
        "host": host_name,
        "container": container_id,
        "titles": resp.titles.unwrap_or_default(),
        "processes": resp.processes.unwrap_or_default(),
    }))
}

// ───────────────────────────── stats ─────────────────────────────

/// One-shot resource stats for a single container.
pub async fn stats_on_host(
    client: &dyn ContainerOps,
    host_name: &str,
    container_id: &str,
) -> Result<Value, bollard::errors::Error> {
    let opts = StatsOptions {
        stream: false,
        one_shot: true,
    };
    let mut stream = client.stats(container_id, Some(opts));
    // An empty stream means the daemon produced no stats frame (e.g. the
    // container is absent on this host). Surface it as an error so the
    // find-host caller advances rather than reporting empty stats for the
    // wrong host.
    let stat = match stream.next().await {
        Some(Ok(s)) => serde_json::to_value(&s).unwrap_or(Value::Null),
        Some(Err(e)) => return Err(e),
        None => {
            return Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                message: format!("no stats for container {container_id}"),
            });
        }
    };
    Ok(json!({ "host": host_name, "container": container_id, "stats": stat }))
}

// ───────────────────────────── logs ─────────────────────────────

/// Build the bollard [`LogsOptions`] for a one-shot tail read.
pub fn build_logs_options(opts: &LogOptions) -> Result<LogsOptions> {
    let lines = opts.lines.clamp(1, MAX_LOG_LINES);
    let (stdout, stderr) = match opts.stream.as_str() {
        "stdout" => (true, false),
        "stderr" => (false, true),
        _ => (true, true),
    };
    let since = match opts.since.as_deref() {
        Some(s) => parse_time_spec(s)?,
        None => 0,
    };
    let until = match opts.until.as_deref() {
        Some(s) => parse_time_spec(s)?,
        None => 0,
    };
    Ok(LogsOptions {
        follow: false, // one-shot: never block on a live daemon.
        stdout,
        stderr,
        since,
        until,
        timestamps: false,
        tail: lines.to_string(),
    })
}

/// Drain a bollard log stream one-shot, returning rendered text lines.
///
/// Propagates the **first** stream error (e.g. a 404 for a nonexistent
/// container) so the find-host caller advances to the next host instead of
/// silently returning empty logs for the wrong host. A container that exists
/// but has no logs ends the stream with no items → `Ok(vec![])`.
pub async fn collect_log_lines(
    client: &dyn ContainerOps,
    container_id: &str,
    options: LogsOptions,
) -> Result<Vec<String>, bollard::errors::Error> {
    let mut stream = client.logs(container_id, Some(options));
    let mut lines = Vec::new();
    while let Some(item) = stream.next().await {
        lines.extend(log_output_lines(&item?));
    }
    Ok(lines)
}

/// Split a single [`LogOutput`] frame into trimmed, non-empty text lines.
pub fn log_output_lines(output: &LogOutput) -> Vec<String> {
    let text = output.to_string();
    text.lines()
        .map(|l| l.trim_end_matches(['\r', '\n']).to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Apply an optional substring grep to collected log lines.
pub fn grep_lines(lines: Vec<String>, grep: Option<&str>) -> Vec<String> {
    match grep.filter(|s| !s.is_empty()) {
        None => lines,
        Some(pattern) => lines.into_iter().filter(|l| l.contains(pattern)).collect(),
    }
}

/// Assemble the logs JSON contract from already-collected (and grepped) lines.
pub fn logs_value(host_name: &str, container_id: &str, lines: Vec<String>) -> Value {
    let mut obj = Map::new();
    obj.insert("host".into(), json!(host_name));
    obj.insert("container".into(), json!(container_id));
    obj.insert("count".into(), json!(lines.len()));
    obj.insert("lines".into(), json!(lines));
    Value::Object(obj)
}

/// Parse a time spec into unix seconds.
///
/// Accepts:
/// - relative durations: `"30s"`, `"10m"`, `"2h"`, `"1d"` (subtracted from now)
/// - unix timestamps: a bare integer (`"1700000000"`)
/// - ISO 8601 / RFC 3339: `"2024-01-01T00:00:00Z"`
pub fn parse_time_spec(spec: &str) -> Result<i32> {
    let spec = spec.trim();
    // Relative form: <digits><unit>
    if let Some(unit) = spec.chars().last()
        && matches!(unit, 's' | 'm' | 'h' | 'd')
    {
        let digits = &spec[..spec.len() - 1];
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let value: i64 = digits.parse()?;
            let mult: i64 = match unit {
                's' => 1,
                'm' => 60,
                'h' => 3600,
                _ => 86400,
            };
            let now = Utc::now().timestamp();
            return Ok((now - value * mult) as i32);
        }
    }
    // Bare unix timestamp.
    if spec.chars().all(|c| c.is_ascii_digit()) && !spec.is_empty() {
        return Ok(spec.parse::<i64>()? as i32);
    }
    // Absolute RFC 3339 / ISO 8601.
    let dt = chrono::DateTime::parse_from_rfc3339(spec)
        .map_err(|e| anyhow::anyhow!("invalid time spec {spec:?}: {e}"))?;
    Ok(dt.timestamp() as i32)
}

#[cfg(test)]
#[path = "container_read_tests.rs"]
mod tests;

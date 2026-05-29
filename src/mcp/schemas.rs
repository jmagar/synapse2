//! Tool JSON schemas for the MCP synapse2 tool.
//!
//! This file defines the action list and input schema for the `synapse2` tool.
//! MCP clients inspect this schema to know what arguments are valid.
//!
//! **Template**: rename `synapse2` to your tool name. Add/remove actions and
//! parameters to match your service. Use `"required": [...]` for mandatory args.

use std::sync::OnceLock;

use serde_json::{json, Value};

/// Cached JSON schema definitions (static data, built once at first call).
static TOOL_DEFINITIONS: OnceLock<Vec<Value>> = OnceLock::new();

/// Return the JSON schema definitions for all tools (cached after first call).
///
/// Returns a `Vec<Value>` where each item is a tool definition object matching
/// the MCP `Tool` schema: `{ name, description, inputSchema }`.
///
/// This is also used by the schema resource (`synapse://schema/mcp-tool`).
pub(super) fn tool_definitions() -> &'static Vec<Value> {
    TOOL_DEFINITIONS.get_or_init(build_tool_definitions)
}

fn build_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "flux",
            "description": "Docker infrastructure management for synapse2. Supports docker (info/df/images/networks/volumes/pull/build/rmi/prune), container (list/inspect/logs/stats/top/search/start/stop/restart/pause/resume/pull/recreate/exec), host status, and compose (list/status/up/down/restart/recreate/logs/build/pull/refresh) actions across configured hosts. build/rmi/prune, compose down/restart/recreate, and container stop/recreate/exec are destructive and require confirmation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["help", "docker", "container", "host", "compose"] },
                    "subaction": {
                        "type": "string",
                        "description": "For action=container: list|inspect|logs|stats|top|search|start|stop|restart|pause|resume|pull|recreate|exec. For action=docker: info|df|images|networks|volumes|pull|build|rmi|prune. For action=host: status|info|uptime|resources|services|network|mounts|ports|doctor. For action=compose: list|status|up|down|restart|recreate|logs|build|pull|refresh."
                    },
                    "host": { "type": "string", "description": "Target host name; omit to fan out across all configured hosts for read-only docker/container ops. REQUIRED for docker pull/build/rmi/prune (single-host) and all compose ops." },
                    "project": { "type": "string", "description": "compose: project name (required for all compose subactions except list/refresh)." },
                    "remove_volumes": { "type": "boolean", "description": "compose down: also remove named volumes. Requires force=true to prevent accidental data loss." },
                    "force": { "type": "boolean", "description": "docker rmi/prune: must be true. compose down with remove_volumes=true: must be true." },
                    "service": { "type": "string", "description": "compose logs/status/build/pull: restrict to a single service name." },
                    "dangling_only": { "type": "boolean", "description": "docker images: only list dangling (untagged) images." },
                    "image": { "type": "string", "description": "docker pull/rmi: image reference (e.g. nginx:latest)." },
                    "context": { "type": "string", "description": "docker build: absolute build context path (no .., ~, or $ expansion)." },
                    "tag": { "type": "string", "description": "docker build: image tag (-t)." },
                    "dockerfile": { "type": "string", "description": "docker build: Dockerfile path relative to context (optional)." },
                    "no_cache": { "type": "boolean", "description": "docker build: pass --no-cache." },
                    "prune_target": { "type": "string", "enum": ["containers", "images", "volumes", "networks", "buildcache", "all"], "description": "docker prune: what to prune. 'all' prunes containers, images, volumes, networks, AND build cache." },
                    "container_id": { "type": "string", "description": "Container id or name (required for inspect/logs/top; optional for stats)." },
                    "lines": { "type": "integer", "minimum": 1, "maximum": 500, "description": "container logs / compose logs: tail line count (default 50 for container; all for compose)." },
                    "state": { "type": "string", "enum": ["running", "exited", "paused", "restarting", "all"], "description": "container list: filter by state (default all)." },
                    "name_filter": { "type": "string", "description": "container list: partial match on container name." },
                    "image_filter": { "type": "string", "description": "container list: partial match on image." },
                    "label_filter": { "type": "string", "description": "container list: label match in key=value form." },
                    "since": { "type": "string", "description": "container logs / compose logs: ISO 8601 timestamp, unix seconds, duration (e.g. \"30m\"), or RFC3339." },
                    "until": { "type": "string", "description": "container logs: same forms as since." },
                    "grep": { "type": "string", "description": "container logs: keep only lines containing this substring." },
                    "stream": { "type": "string", "enum": ["stdout", "stderr", "both"], "description": "container logs: which stream(s) to read (default both)." },
                    "summary": { "type": "boolean", "description": "container inspect: true = abbreviated info only." },
                    "query": { "type": "string", "description": "container search: full-text query over name + image + labels." },
                    "response_format": { "type": "string", "enum": ["markdown", "json"], "description": "Output format (default markdown)." },
                    // B9: lifecycle params
                    "command": { "type": "array", "items": { "type": "string" }, "description": "container exec: command argv — index 0 is the binary, rest are args. execvp semantics (no sh -c). Required for exec." },
                    "exec_user": { "type": "string", "description": "container exec: optional user to run as inside the container (e.g. \"root\")." },
                    "exec_workdir": { "type": "string", "description": "container exec: optional working directory inside the container." },
                    "exec_timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 300000, "description": "container exec: timeout in milliseconds [1000, 300000], default 30000." },
                    "pull": { "type": "boolean", "description": "container recreate: whether to pull the latest image before recreating (default true)." }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "scout",
            "description": "SSH/local host inspection for synapse2 (B14+B15). Supports: nodes (list hosts), peek (file/dir view), find (glob search), ps (processes), df (disk usage), delta (file diff), exec (allowlisted command, destructive), emit (multi-host exec, destructive), beam (file transfer, destructive), zfs (pools/datasets/snapshots, read-only), logs (syslog/journal/dmesg/auth, read-only).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["help", "nodes", "peek", "find", "ps", "df", "delta", "exec", "emit", "beam", "zfs", "logs"]
                    },
                    "subaction": {
                        "type": "string",
                        "description": "For action=zfs: pools|datasets|snapshots. For action=logs: syslog|journal|dmesg|auth."
                    },
                    // shared
                    "host": { "type": "string", "description": "Target host name (required for most actions)." },
                    "path": { "type": "string", "description": "Absolute path (required for peek/find/exec path; optional for df; inline content alternative for delta)." },
                    // peek
                    "tree": { "type": "boolean", "description": "peek: emit a depth-limited directory tree." },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 20, "description": "peek/find: tree depth (default 3 for peek, 10 for find)." },
                    // find
                    "pattern": { "type": "string", "description": "find: glob pattern for -name (must not start with -)." },
                    "limit": { "type": "integer", "minimum": 1, "description": "find/ps/zfs snapshots: max results (default 500 for find, 50 for ps, unlimited for snapshots)." },
                    // ps
                    "sort": { "type": "string", "enum": ["cpu", "mem", "pid", "time"], "description": "ps: sort field (default cpu)." },
                    "grep": { "type": "string", "description": "ps: substring filter on process lines. logs: filter applied locally after retrieval (injection-safe)." },
                    "user": { "type": "string", "description": "ps: prefix-match filter on user column." },
                    // delta
                    "source_host": { "type": "string", "description": "delta: source host name." },
                    "source_path": { "type": "string", "description": "delta: source absolute path." },
                    "target_host": { "type": "string", "description": "delta: target host name (mutually exclusive with content)." },
                    "target_path": { "type": "string", "description": "delta: target absolute path." },
                    "content": { "type": "string", "description": "delta: inline content to compare against source (≤1 MB; mutually exclusive with target_host/target_path)." },
                    // exec/emit
                    "command": { "type": "string", "description": "exec/emit: command name from allowlist (cat/head/tail/grep/rg/find/ls/tree/wc/sort/uniq/diff/stat/file/du/df/pwd/hostname/uptime/whoami). git is NOT allowlisted." },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "exec/emit: positional arguments (execvp-style, no shell)." },
                    "timeout_secs": { "type": "integer", "minimum": 1, "description": "exec/emit: per-host timeout in seconds (default 30)." },
                    // emit
                    "targets": {
                        "type": "array",
                        "description": "emit: list of {host, path} targets.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "host": { "type": "string" },
                                "path": { "type": "string" }
                            },
                            "required": ["host"]
                        }
                    },
                    // beam
                    "dest_host": { "type": "string", "description": "beam: destination host name." },
                    "dest_path": { "type": "string", "description": "beam: destination absolute path." },
                    // zfs (B15)
                    "pool": { "type": "string", "description": "zfs pools: exact pool name filter. zfs datasets: restrict to this pool (implies -r). zfs snapshots: restrict to this pool (if dataset not given)." },
                    "dataset_type": { "type": "string", "enum": ["filesystem", "volume", "snapshot", "bookmark", "all"], "description": "zfs datasets: filter by dataset type (-t)." },
                    "recursive": { "type": "boolean", "description": "zfs datasets: list recursively (-r). Default false." },
                    "dataset": { "type": "string", "description": "zfs snapshots: restrict snapshots to this dataset (takes priority over pool)." },
                    // logs (B15)
                    "lines": { "type": "integer", "minimum": 1, "maximum": 500, "description": "logs: number of lines to retrieve (default 100)." },
                    "unit": { "type": "string", "description": "logs journal: systemd unit filter (-u)." },
                    "priority": { "type": "string", "description": "logs journal: priority filter (-p). e.g. err, warning, info, debug." },
                    "since": { "type": "string", "description": "logs journal: start time (--since). e.g. '2026-05-29 00:00:00' or '-1h'." },
                    "until": { "type": "string", "description": "logs journal: end time (--until)." }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    ]
}

#[cfg(test)]
#[path = "schemas_tests.rs"]
mod tests;

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
            "description": "Docker infrastructure management for synapse2. Supports docker (info/df/images/networks/volumes/pull/build/rmi/prune), container (list/inspect/logs/stats/top/search), host status, and compose (list/status/up/down/restart/recreate/logs/build/pull/refresh) actions across configured hosts. build/rmi/prune and compose down/restart/recreate are destructive and require confirmation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["help", "docker", "container", "host", "compose"] },
                    "subaction": {
                        "type": "string",
                        "description": "For action=container: list|inspect|logs|stats|top|search. For action=docker: info|df|images|networks|volumes|pull|build|rmi|prune. For action=host: status|info|uptime|resources|services|network|mounts|ports|doctor. For action=compose: list|status|up|down|restart|recreate|logs|build|pull|refresh."
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
                    "response_format": { "type": "string", "enum": ["markdown", "json"], "description": "Output format (default markdown)." }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "scout",
            "description": "SSH/local host inspection for synapse2. First slice supports nodes, peek, and allowlisted exec.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["help", "nodes", "peek", "exec"] },
                    "host": { "type": "string" },
                    "path": { "type": "string" },
                    "command": { "type": "string" }
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

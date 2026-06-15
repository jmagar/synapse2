//! Topic-aware help text for flux and scout tools.
//!
//! **Source of truth**: a static `HashMap<&'static str, &'static str>` mapping
//! `"<domain>:<action>"` topic keys to markdown documentation strings.
//!
//! **Synchronization rule**: this map is NOT auto-generated from `ACTION_SPECS`.
//! When adding an action, you MUST add a help entry here. See the CLAUDE.md
//! "How to add an action" checklist, step 8.
//!
//! Public API:
//! - [`topic_markdown`] — look up a topic by key; returns `None` for unknown topics.
//! - [`topic_index`] — return a JSON index of all topics for a domain.
//! - [`full_domain_markdown`] — return the full help text for a domain as markdown.
//! - [`help_response`] — build the MCP `help` response (topic or index, markdown or JSON).

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::Result;
use serde_json::{Value, json};

use super::help_topics::*;

// ── Static topic map ──────────────────────────────────────────────────────────

static HELP_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn help_map() -> &'static HashMap<&'static str, &'static str> {
    HELP_MAP.get_or_init(build_help_map)
}

fn build_help_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();

    // ── flux: docker ──────────────────────────────────────────────────────────
    m.insert("docker:info", DOCKER_INFO);
    m.insert("docker:df", DOCKER_DF);
    m.insert("docker:images", DOCKER_IMAGES);
    m.insert("docker:networks", DOCKER_NETWORKS);
    m.insert("docker:volumes", DOCKER_VOLUMES);
    m.insert("docker:pull", DOCKER_PULL);
    m.insert("docker:build", DOCKER_BUILD);
    m.insert("docker:rmi", DOCKER_RMI);
    m.insert("docker:prune", DOCKER_PRUNE);

    // ── flux: container ───────────────────────────────────────────────────────
    m.insert("container:list", CONTAINER_LIST);
    m.insert("container:inspect", CONTAINER_INSPECT);
    m.insert("container:logs", CONTAINER_LOGS);
    m.insert("container:stats", CONTAINER_STATS);
    m.insert("container:top", CONTAINER_TOP);
    m.insert("container:search", CONTAINER_SEARCH);
    m.insert("container:start", CONTAINER_START);
    m.insert("container:stop", CONTAINER_STOP);
    m.insert("container:restart", CONTAINER_RESTART);
    m.insert("container:pause", CONTAINER_PAUSE);
    m.insert("container:resume", CONTAINER_RESUME);
    m.insert("container:pull", CONTAINER_PULL_IMG);
    m.insert("container:recreate", CONTAINER_RECREATE);
    m.insert("container:exec", CONTAINER_EXEC);

    // ── flux: host ────────────────────────────────────────────────────────────
    m.insert("host:status", HOST_STATUS);
    m.insert("host:info", HOST_INFO);
    m.insert("host:uptime", HOST_UPTIME);
    m.insert("host:resources", HOST_RESOURCES);
    m.insert("host:services", HOST_SERVICES);
    m.insert("host:network", HOST_NETWORK);
    m.insert("host:mounts", HOST_MOUNTS);
    m.insert("host:ports", HOST_PORTS);
    m.insert("host:doctor", HOST_DOCTOR);

    // ── flux: compose ─────────────────────────────────────────────────────────
    m.insert("compose:list", COMPOSE_LIST);
    m.insert("compose:status", COMPOSE_STATUS);
    m.insert("compose:up", COMPOSE_UP);
    m.insert("compose:down", COMPOSE_DOWN);
    m.insert("compose:restart", COMPOSE_RESTART);
    m.insert("compose:recreate", COMPOSE_RECREATE);
    m.insert("compose:logs", COMPOSE_LOGS);
    m.insert("compose:build", COMPOSE_BUILD);
    m.insert("compose:pull", COMPOSE_PULL);
    m.insert("compose:refresh", COMPOSE_REFRESH);

    // ── scout: simple actions ─────────────────────────────────────────────────
    m.insert("nodes", SCOUT_NODES);
    m.insert("peek", SCOUT_PEEK);
    m.insert("find", SCOUT_FIND);
    m.insert("ps", SCOUT_PS);
    m.insert("df", SCOUT_DF);
    m.insert("delta", SCOUT_DELTA);
    m.insert("exec", SCOUT_EXEC);
    m.insert("emit", SCOUT_EMIT);
    m.insert("beam", SCOUT_BEAM);

    // ── scout: zfs ────────────────────────────────────────────────────────────
    m.insert("zfs:pools", ZFS_POOLS);
    m.insert("zfs:datasets", ZFS_DATASETS);
    m.insert("zfs:snapshots", ZFS_SNAPSHOTS);

    // ── scout: logs ───────────────────────────────────────────────────────────
    m.insert("logs:syslog", LOGS_SYSLOG);
    m.insert("logs:journal", LOGS_JOURNAL);
    m.insert("logs:dmesg", LOGS_DMESG);
    m.insert("logs:auth", LOGS_AUTH);

    m
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Look up a topic by key. Returns `None` for unknown topics.
///
/// Topic keys are `"<domain>:<action>"` (e.g., `"container:list"`) for
/// namespaced topics, or just `"<action>"` for scout simple actions (e.g.,
/// `"exec"`, `"nodes"`).
pub fn topic_markdown(topic: &str) -> Option<&'static str> {
    help_map().get(topic).copied()
}

/// All flux topic keys (sorted).
pub fn flux_topic_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = help_map()
        .keys()
        .copied()
        .filter(|k| {
            k.starts_with("docker:")
                || k.starts_with("container:")
                || k.starts_with("host:")
                || k.starts_with("compose:")
        })
        .collect();
    keys.sort_unstable();
    keys
}

/// All scout topic keys (sorted).
pub fn scout_topic_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = help_map()
        .keys()
        .copied()
        .filter(|k| {
            k.starts_with("zfs:")
                || k.starts_with("logs:")
                || matches!(
                    *k,
                    "nodes" | "peek" | "find" | "ps" | "df" | "delta" | "exec" | "emit" | "beam"
                )
        })
        .collect();
    keys.sort_unstable();
    keys
}

/// Return a JSON index of all topics for a domain (`"flux"` or `"scout"`).
pub fn topic_index(domain: &str) -> Value {
    let keys: Vec<&'static str> = if domain == "flux" {
        flux_topic_keys()
    } else {
        scout_topic_keys()
    };
    json!({
        "tool": domain,
        "topics": keys,
        "hint": format!("Pass topic=\"<key>\" to get help for a specific topic. E.g. {{action:\"help\", topic:\"container:list\"}}"),
    })
}

/// Return full markdown help for a domain.
pub fn full_domain_markdown(domain: &str) -> String {
    let keys: Vec<&'static str> = if domain == "flux" {
        flux_topic_keys()
    } else {
        scout_topic_keys()
    };
    let mut out = format!("# {domain} tool help\n\n");
    for key in keys {
        if let Some(text) = topic_markdown(key) {
            out.push_str(&format!("## {key}\n\n{text}\n\n"));
        }
    }
    out
}

/// Build the MCP `help` response.
///
/// - `topic=None` → return the topic index (list of all topics).
/// - `topic=Some(t)` → look up `t`; return error if unknown.
/// - `format="json"` → wrap in `{topic, text}` JSON; otherwise markdown string.
pub fn help_response(domain: &str, topic: Option<&str>, format: Option<&str>) -> Result<Value> {
    let use_json = matches!(format, Some("json"));

    match topic {
        None => {
            // Return index
            let index = topic_index(domain);
            if use_json {
                Ok(json!({ "topic": null, "index": index }))
            } else {
                Ok(index)
            }
        }
        Some(t) => {
            let text = topic_markdown(t).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown help topic: \"{t}\"; use action=\"help\" (no topic) for the topic index"
                )
            })?;

            if use_json {
                Ok(json!({ "topic": t, "text": text }))
            } else {
                Ok(Value::String(format!("## {t}\n\n{text}")))
            }
        }
    }
}

/// Build the legacy (no-topic) help response for backwards compatibility.
///
/// Returns the same index shape as before B16, extended with `topics`.
/// Used when callers pass `action="help"` with no `topic` param.
pub fn legacy_flux_help() -> Value {
    json!({
        "tool": "flux",
        "actions": {
            "docker": ["info", "df", "images", "networks", "volumes", "pull", "build", "rmi", "prune"],
            "container": ["list", "inspect", "logs", "stats", "top", "search", "start", "stop", "restart", "pause", "resume", "pull", "recreate", "exec"],
            "host": ["status", "info", "uptime", "resources", "services", "network", "mounts", "ports", "doctor"],
            "compose": ["list", "status", "up", "down", "restart", "recreate", "logs", "build", "pull", "refresh"],
            "help": []
        },
        "destructive": ["docker build", "docker rmi", "docker prune", "compose down", "compose restart", "compose recreate", "container stop", "container recreate", "container exec"],
        "topics": flux_topic_keys(),
        "hint": "Pass topic=\"<key>\" (e.g. topic=\"container:list\") to get per-subaction documentation."
    })
}

/// Build the legacy (no-topic) help response for scout.
pub fn legacy_scout_help() -> Value {
    json!({
        "tool": "scout",
        "actions": ["nodes", "peek", "find", "ps", "df", "delta", "exec", "emit", "beam", "zfs", "logs", "help"],
        "destructive": ["exec", "emit", "beam"],
        "zfs_subactions": ["pools", "datasets", "snapshots"],
        "logs_subactions": ["syslog", "journal", "dmesg", "auth"],
        "topics": scout_topic_keys(),
        "hint": "Pass topic=\"<key>\" (e.g. topic=\"exec\") to get per-subaction documentation."
    })
}
#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;

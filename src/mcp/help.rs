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
use serde_json::{json, Value};

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

// ── Help text constants ───────────────────────────────────────────────────────

// docker: subactions

const DOCKER_INFO: &str = "\
Query Docker daemon info on one or all configured hosts.

**Parameters**
- `host` (optional): target host name. Omit to fan out across all configured hosts.

**Returns** daemon version, OS, kernel, memory, CPU count, and plugin info.";

const DOCKER_DF: &str = "\
Show Docker disk usage (images, containers, volumes, build cache) on one or all hosts.

**Parameters**
- `host` (optional): target host name.";

const DOCKER_IMAGES: &str = "\
List Docker images on one or all configured hosts.

**Parameters**
- `host` (optional): target host name.
- `dangling_only` (bool, optional): only list untagged (dangling) images.";

const DOCKER_NETWORKS: &str = "\
List Docker networks on one or all configured hosts.

**Parameters**
- `host` (optional): target host name.";

const DOCKER_VOLUMES: &str = "\
List Docker volumes on one or all configured hosts.

**Parameters**
- `host` (optional): target host name.";

const DOCKER_PULL: &str = "\
Pull a Docker image on a specific host. **Requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `image` (required): image reference, e.g. `nginx:latest`.";

const DOCKER_BUILD: &str = "\
Build a Docker image on a specific host. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `context` (required): absolute build context path (no `..`, `~`, or `$` expansion).
- `tag` (required): image tag (`-t`).
- `dockerfile` (optional): Dockerfile path relative to context.
- `no_cache` (bool, optional): pass `--no-cache`.";

const DOCKER_RMI: &str = "\
Remove a Docker image on a specific host. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `image` (required): image reference.
- `force` (required, must be `true`): safety guard.";

const DOCKER_PRUNE: &str = "\
Prune Docker resources on a specific host. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `prune_target` (required): one of `containers`, `images`, `volumes`, `networks`, `buildcache`, `all`.
- `force` (required, must be `true`): safety guard.";

// container: subactions

const CONTAINER_LIST: &str = "\
List containers on one or all configured hosts.

**Parameters**
- `host` (optional): target host name.
- `state` (optional): filter by `running` | `exited` | `paused` | `restarting` | `all` (default `all`).
- `name_filter` (optional): partial match on container name.
- `image_filter` (optional): partial match on image.
- `label_filter` (optional): label match in `key=value` form.";

const CONTAINER_INSPECT: &str = "\
Inspect a container on a host.

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.
- `summary` (bool, optional): abbreviated info only.";

const CONTAINER_LOGS: &str = "\
Retrieve container logs.

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.
- `lines` (int, optional): tail line count (default 50).
- `since` / `until` (optional): ISO 8601 / unix seconds / duration (e.g. `30m`).
- `grep` (optional): keep only lines containing this substring.
- `stream` (optional): `stdout` | `stderr` | `both` (default `both`).";

const CONTAINER_STATS: &str = "\
Show live CPU / memory / network / IO stats for containers on a host.

**Parameters**
- `host` (optional): target host name.
- `container_id` (optional): restrict to a single container.";

const CONTAINER_TOP: &str = "\
Show running processes inside a container (`docker top`).

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_SEARCH: &str = "\
Full-text search over container names, images, and labels.

**Parameters**
- `host` (optional): target host name.
- `query` (required): search query.";

const CONTAINER_START: &str = "\
Start a stopped container.

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_STOP: &str = "\
Stop a running container. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_RESTART: &str = "\
Restart a container.

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_PAUSE: &str = "\
Pause a container (freeze all processes).

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_RESUME: &str = "\
Resume a paused container.

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_PULL_IMG: &str = "\
Pull the latest image for a container without recreating it.

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.";

const CONTAINER_RECREATE: &str = "\
Recreate a container (stop → pull → start). **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.
- `pull` (bool, optional): pull latest image before recreating (default `true`).";

const CONTAINER_EXEC: &str = "\
Execute a command inside a running container. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `container_id` (required): container id or name.
- `command` (array of strings, required): argv — index 0 is the binary, rest are args. No shell.
- `exec_user` (optional): user to run as inside the container.
- `exec_workdir` (optional): working directory inside the container.
- `exec_timeout_ms` (int, optional): timeout in milliseconds [1000, 300000] (default 30000).";

// host: subactions

const HOST_STATUS: &str = "\
Quick health check for one or all hosts.

**Parameters**
- `host` (optional): target host name.";

const HOST_INFO: &str = "\
Detailed host information (OS, kernel, hardware).

**Parameters**
- `host` (optional): target host name.";

const HOST_UPTIME: &str = "\
Host uptime and load averages.

**Parameters**
- `host` (optional): target host name.";

const HOST_RESOURCES: &str = "\
CPU and memory usage summary.

**Parameters**
- `host` (optional): target host name.";

const HOST_SERVICES: &str = "\
List systemd services.

**Parameters**
- `host` (required): target host name.
- `state` (optional): filter by service state.
- `service` (optional): filter by service name substring.";

const HOST_NETWORK: &str = "\
Network interfaces and addresses.

**Parameters**
- `host` (optional): target host name.";

const HOST_MOUNTS: &str = "\
Mounted filesystems.

**Parameters**
- `host` (required): target host name.";

const HOST_PORTS: &str = "\
Listening TCP/UDP ports.

**Parameters**
- `host` (required): target host name.
- `protocol` (optional): `tcp` | `udp`.
- `limit` / `offset` (int, optional): pagination.";

const HOST_DOCTOR: &str = "\
Pre-flight connectivity checks for a host.

**Parameters**
- `host` (required): target host name.
- `checks` (optional): comma-separated check names to run.";

// compose: subactions

const COMPOSE_LIST: &str = "\
List discovered compose projects on a single host.

**Parameters**
- `host` (required): target host name.";

const COMPOSE_STATUS: &str = "\
Show status of a compose project.

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.
- `service` (optional): restrict to a single service.";

const COMPOSE_UP: &str = "\
Start a compose project.

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.";

const COMPOSE_DOWN: &str = "\
Stop and remove a compose project. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.
- `remove_volumes` (bool, optional): also remove named volumes. Requires `force=true`.
- `force` (bool): required when `remove_volumes=true`.";

const COMPOSE_RESTART: &str = "\
Restart a compose project. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.";

const COMPOSE_RECREATE: &str = "\
Recreate a compose project (pull + down + up). **Destructive — requires confirmation.**

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.";

const COMPOSE_LOGS: &str = "\
Retrieve logs for a compose project.

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.
- `service` (optional): restrict to a single service.
- `lines` (int, optional): tail line count.
- `since` (optional): start time (ISO 8601 / duration).";

const COMPOSE_BUILD: &str = "\
Build images for a compose project.

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.
- `service` (optional): restrict to a single service.";

const COMPOSE_PULL: &str = "\
Pull images for a compose project.

**Parameters**
- `host` (required): target host name.
- `project` (required): compose project name.
- `service` (optional): restrict to a single service.";

const COMPOSE_REFRESH: &str = "\
Invalidate the compose project discovery cache for a host.

**Parameters**
- `host` (optional): target host name. Omit to invalidate all hosts.";

// scout: simple actions

const SCOUT_NODES: &str = "\
List all configured hosts (nodes).

No parameters required.

**Returns** host name, protocol, address, port, and tags for each host.";

const SCOUT_PEEK: &str = "\
Peek at a file or directory on a host.

**Parameters**
- `host` (required): target host name.
- `path` (required): absolute path to peek at.
- `tree` (bool, optional): emit a depth-limited directory tree.
- `depth` (int, optional): tree depth [1, 20] (default 3).";

const SCOUT_FIND: &str = "\
Glob search for files on a host.

**Parameters**
- `host` (required): target host name.
- `path` (required): starting directory (absolute).
- `pattern` (required): glob pattern for `-name` (must not start with `-`).
- `depth` (int, optional): max depth [1, 20] (default 10).
- `limit` (int, optional): max results (default 500).";

const SCOUT_PS: &str = "\
List running processes on a host.

**Parameters**
- `host` (required): target host name.
- `sort` (optional): `cpu` | `mem` | `pid` | `time` (default `cpu`).
- `grep` (optional): substring filter on process lines.
- `user` (optional): prefix-match filter on user column.
- `limit` (int, optional): max results (default 50).";

const SCOUT_DF: &str = "\
Disk usage on a host.

**Parameters**
- `host` (required): target host name.
- `path` (optional): restrict to a specific mount point.";

const SCOUT_DELTA: &str = "\
Diff a file between two hosts (or against inline content).

**Parameters**
- `source_host` (required): source host name.
- `source_path` (required): source file path (absolute).
- `target_host` + `target_path` OR `content`: destination. Mutually exclusive.";

const SCOUT_EXEC: &str = "\
Execute an allowlisted command on a host. **Destructive — requires confirmation.**

**Allowlist**: `cat`, `head`, `tail`, `grep`, `rg`, `find`, `ls`, `tree`, `wc`, `sort`, `uniq`, `diff`, `stat`, `file`, `du`, `df`, `pwd`, `hostname`, `uptime`, `whoami`.

**Parameters**
- `host` (required): target host name.
- `command` (required): command name from allowlist.
- `args` (array, optional): positional arguments (execvp-style, no shell).
- `path` (optional): working directory (local hosts only).
- `timeout_secs` (int, optional): per-host timeout (default 30).";

const SCOUT_EMIT: &str = "\
Run an allowlisted command across multiple hosts. **Destructive — requires confirmation.**

**Parameters**
- `targets` (array of `{host, path?}`): target hosts.
- `command` (required): allowlisted command name.
- `args` (array, optional): positional arguments.
- `timeout_secs` (int, optional): per-host timeout (default 30).";

const SCOUT_BEAM: &str = "\
Transfer a file between hosts. **Destructive — requires confirmation.**

**Parameters**
- `host` (required): source host name.
- `path` (required): source file path (absolute).
- `dest_host` (required): destination host name.
- `dest_path` (required): destination path (absolute).";

// zfs: subactions

const ZFS_POOLS: &str = "\
List ZFS pools on a host.

**Parameters**
- `host` (required): target host name.
- `pool` (optional): exact pool name filter.";

const ZFS_DATASETS: &str = "\
List ZFS datasets on a host.

**Parameters**
- `host` (required): target host name.
- `pool` (optional): restrict to this pool (`-r`).
- `dataset_type` (optional): `filesystem` | `volume` | `snapshot` | `bookmark` | `all`.
- `recursive` (bool, optional): list recursively (default false).";

const ZFS_SNAPSHOTS: &str = "\
List ZFS snapshots on a host.

**Parameters**
- `host` (required): target host name.
- `pool` (optional): restrict to this pool.
- `dataset` (optional): restrict to this dataset (takes priority over pool).
- `limit` (int, optional): max results.";

// logs: subactions

const LOGS_SYSLOG: &str = "\
Retrieve `/var/log/syslog` (or `/var/log/messages`) on a host.

**Parameters**
- `host` (required): target host name.
- `lines` (int, optional): line count [1, 500] (default 100).
- `grep` (optional): substring filter (injection-safe, applied locally).";

const LOGS_JOURNAL: &str = "\
Query the systemd journal on a host.

**Parameters**
- `host` (required): target host name.
- `lines` (int, optional): line count [1, 500] (default 100).
- `unit` (optional): systemd unit filter (`-u`).
- `priority` (optional): priority filter (`-p`). E.g. `err`, `warning`, `info`.
- `since` (optional): start time (`--since`). E.g. `2026-05-29 00:00:00` or `-1h`.
- `until` (optional): end time (`--until`).
- `grep` (optional): substring filter.";

const LOGS_DMESG: &str = "\
Retrieve kernel ring buffer (`dmesg`) on a host.

**Parameters**
- `host` (required): target host name.
- `lines` (int, optional): line count [1, 500] (default 100).
- `grep` (optional): substring filter.";

const LOGS_AUTH: &str = "\
Retrieve auth log (`/var/log/auth.log` or `journalctl` auth facility).

**Parameters**
- `host` (required): target host name.
- `lines` (int, optional): line count [1, 500] (default 100).
- `grep` (optional): substring filter.";

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;

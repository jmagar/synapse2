---
name: synapse2
description: "Use for Synapse flux/scout Docker, Compose, SSH, logs, files, ZFS, host diagnostics, and remote allowlisted commands."
---

# synapse2

<!-- TIER 1: Quick-reference table and critical gotchas -->

Two MCP tools: **`flux`** for Docker/host inspection, **`scout`** for SSH/local
host operations. All read-only ops use `synapse:read`; destructive ops
(`stop`, `exec`, `rmi`, `prune`, compose `down/restart/recreate`, `emit`,
`beam`) require `synapse:write` and go through a confirmation gate.

Use these tools before falling back to direct SSH or Docker API calls.

## Quick Action Table

| Tool | Action | Key Params | Use When |
|---|---|---|---|
| `flux` | `docker`, `info` | `host?` | Check Docker availability / host info |
| `flux` | `docker`, `df` | `host?` | Check Docker disk usage |
| `flux` | `docker`, `images` | `host?`, `dangling_only?` | List Docker images |
| `flux` | `docker`, `networks` | `host?` | List Docker networks |
| `flux` | `docker`, `volumes` | `host?` | List Docker volumes |
| `flux` | `docker`, `pull` | `host`, `image` | Pull a Docker image |
| `flux` | `docker`, `build` | `host`, `context`, `tag` | Build a Docker image |
| `flux` | `docker`, `rmi` | `host`, `image`, `force=true` | Remove a Docker image |
| `flux` | `docker`, `prune` | `host`, `prune_target`, `force=true` | Remove unused resources |
| `flux` | `container`, `list` | `host?`, `state?`, `name_filter?` | List containers |
| `flux` | `container`, `inspect` | `container_id`, `summary?` | Inspect a container |
| `flux` | `container`, `logs` | `container_id`, `lines?`, `grep?` | Read container logs |
| `flux` | `container`, `stats` | `container_id?` | CPU/mem/network stats |
| `flux` | `container`, `top` | `container_id` | Show running processes |
| `flux` | `container`, `search` | `query` | Search containers by name/image/label |
| `flux` | `container`, `start` | `container_id` | Start a stopped container |
| `flux` | `container`, `stop` | `container_id` | Stop a container (destructive) |
| `flux` | `container`, `restart` | `container_id` | Restart a container |
| `flux` | `container`, `pause` | `container_id` | Pause a container |
| `flux` | `container`, `resume` | `container_id` | Resume a paused container |
| `flux` | `container`, `pull` | `container_id` | Pull container's image |
| `flux` | `container`, `recreate` | `container_id`, `pull?` | Recreate with image pull (destructive) |
| `flux` | `container`, `exec` | `container_id`, `command` (array) | Run command in container (destructive) |
| `flux` | `host`, `status` | `host?` | Check Docker connectivity |
| `flux` | `host`, `info` | `host?` | OS/kernel/arch info |
| `flux` | `host`, `uptime` | `host?` | System uptime |
| `flux` | `host`, `resources` | `host?` | CPU/mem/disk usage |
| `flux` | `host`, `services` | `host`, `state?`, `service?` | Systemd service status |
| `flux` | `host`, `network` | `host?` | Network interfaces |
| `flux` | `host`, `mounts` | `host` | Mounted filesystems |
| `flux` | `host`, `ports` | `host`, `protocol?`, `limit?` | Port mappings |
| `flux` | `host`, `doctor` | `host`, `checks?` | Run diagnostic checks |
| `flux` | `compose`, `list` | `host` | List Compose projects |
| `flux` | `compose`, `status` | `host`, `project`, `service?` | Compose project status |
| `flux` | `compose`, `up` | `host`, `project` | Start a Compose project |
| `flux` | `compose`, `down` | `host`, `project` | Stop a Compose project (destructive) |
| `flux` | `compose`, `restart` | `host`, `project` | Restart a Compose project (destructive) |
| `flux` | `compose`, `recreate` | `host`, `project` | Recreate Compose containers (destructive) |
| `flux` | `compose`, `logs` | `host`, `project`, `lines?`, `service?` | Compose project logs |
| `flux` | `compose`, `build` | `host`, `project`, `service?` | Build Compose images |
| `flux` | `compose`, `pull` | `host`, `project`, `service?` | Pull Compose images |
| `flux` | `compose`, `refresh` | `host` | Refresh compose project cache |
| `flux` | `help` | `topic?`, `format?` | Flux documentation |
| `scout` | `nodes` | — | List all configured SSH hosts |
| `scout` | `peek` | `host`, `path`, `tree?`, `depth?` | Read file or directory |
| `scout` | `find` | `host`, `path`, `pattern` | Find files by glob |
| `scout` | `ps` | `host`, `sort?`, `grep?`, `user?` | List processes |
| `scout` | `df` | `host`, `path?` | Disk usage |
| `scout` | `delta` | `source_host`, `source_path`, ... | Compare files/content |
| `scout` | `exec` | `host`, `command` | Run allowlisted command (destructive) |
| `scout` | `emit` | `targets`, `command` | Multi-host command (destructive) |
| `scout` | `beam` | `source_host`, `source_path`, `dest_host`, `dest_path` | File transfer (destructive) |
| `scout` | `zfs`, `pools` | `host`, `pool?` | List ZFS pools |
| `scout` | `zfs`, `datasets` | `host`, `pool?`, `dataset_type?`, `recursive?` | List ZFS datasets |
| `scout` | `zfs`, `snapshots` | `host`, `pool?`, `dataset?`, `limit?` | List ZFS snapshots |
| `scout` | `logs`, `syslog` | `host`, `lines?`, `grep?` | Read syslog |
| `scout` | `logs`, `journal` | `host`, `unit?`, `priority?`, `since?`, `until?` | Read systemd journal |
| `scout` | `logs`, `dmesg` | `host`, `grep?` | Read kernel ring buffer |
| `scout` | `logs`, `auth` | `host`, `lines?`, `grep?` | Read auth log |
| `scout` | `help` | `topic?`, `format?` | Scout documentation |

## Critical Gotchas

- **Read ops fan out** across all configured hosts when `host` is omitted;
  destructive ops always require an explicit `host`.
- **`container exec` vs `scout exec`**: `container exec` runs inside a Docker
  container (literal argv array, so `--command sh -c "..."` is valid when you
  explicitly want a shell inside the container); `scout exec` runs on the host
  via SSH (allowlisted commands only, no shell).
- **`scout exec` is allowlisted** — only:
  `cat`, `head`, `tail`, `grep`, `rg`, `find`, `ls`, `tree`, `wc`, `sort`,
  `uniq`, `diff`, `stat`, `file`, `du`, `df`, `pwd`, `hostname`, `uptime`,
  `whoami`. `git` is explicitly denied.
- **For `scout exec`, never pass shell metacharacters** (`|`, `>`, `&&`, `..`);
  host command execution is execvp-style and does not run through `sh -c`.
- **Destructive ops need confirmation** via the MCP elicitation gate; declining
  returns an error without performing any IO.
- **Responses are token-budgeted** — very long log tails or large directory
  trees may be truncated. Use `lines`/`limit`/`depth` params to control size.

---

<!-- TIER 2: Full action reference with parameters and response shapes -->

## Full Action Reference

### `flux docker` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"docker"` |
| `subaction` | string | yes | `info\|df\|images\|networks\|volumes\|pull\|build\|rmi\|prune` |
| `host` | string | for write ops | Target host name; omit to fan out for read ops |
| `dangling_only` | boolean | no | `images`: only list untagged images |
| `image` | string | for pull/rmi | Image reference, e.g. `nginx:latest` |
| `force` | boolean | for rmi/prune | Must be `true` to allow destructive ops |
| `context` | string | for build | Absolute build context path |
| `tag` | string | for build | Image tag (e.g. `myapp:latest`) |
| `dockerfile` | string | no | Dockerfile path relative to context |
| `no_cache` | boolean | no | Pass `--no-cache` to build |
| `prune_target` | string | for prune | `containers\|images\|volumes\|networks\|buildcache\|all` |

### `flux container` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"container"` |
| `subaction` | string | yes | `list\|inspect\|logs\|stats\|top\|search\|start\|stop\|restart\|pause\|resume\|pull\|recreate\|exec` |
| `host` | string | no | Target host; fan-out when omitted |
| `container_id` | string | most subactions | Container id or name |
| `state` | string | no | `list`: `running\|exited\|paused\|restarting\|all` |
| `name_filter` | string | no | `list`: partial match on container name |
| `image_filter` | string | no | `list`: partial match on image |
| `label_filter` | string | no | `list`: `key=value` label match |
| `lines` | integer | no | `logs`: tail line count (default 50) |
| `since` | string | no | `logs`: ISO8601, unix seconds, or duration (e.g. `"30m"`) |
| `until` | string | no | `logs`: same formats as `since` |
| `grep` | string | no | `logs`: keep only lines containing this string |
| `stream` | string | no | `logs`: `stdout\|stderr\|both` (default `both`) |
| `summary` | boolean | no | `inspect`: return abbreviated info only |
| `query` | string | for search | `search`: full-text query |
| `command` | array of strings | for exec | `exec`: argv (`["ls", "-la", "/var/log"]`) |
| `exec_user` | string | no | `exec`: run as this user inside the container |
| `exec_workdir` | string | no | `exec`: working directory inside the container |
| `exec_timeout_ms` | integer | no | `exec`: timeout in ms [1000–300000], default 30000 |
| `pull` | boolean | no | `recreate`: pull image before recreating (default true) |

### `flux host` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"host"` |
| `subaction` | string | yes | `status\|info\|uptime\|resources\|services\|network\|mounts\|ports\|doctor` |
| `host` | string | for services/mounts/ports/doctor | Target host name |
| `state` | string | no | `services`: filter by state |
| `service` | string | no | `services`: filter by service name |
| `protocol` | string | no | `ports`: `tcp\|udp` |
| `limit` | integer | no | `ports`: max results |
| `offset` | integer | no | `ports`: pagination offset |
| `checks` | string | no | `doctor`: comma-separated check names (default: all) |

### `flux compose` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"compose"` |
| `subaction` | string | yes | `list\|status\|up\|down\|restart\|recreate\|logs\|build\|pull\|refresh` |
| `host` | string | yes | Target host name |
| `project` | string | most subactions | Compose project name |
| `service` | string | no | `logs\|status\|build\|pull`: restrict to a single service |
| `lines` | integer | no | `logs`: tail line count |
| `since` | string | no | `logs`: start time filter |
| `remove_volumes` | boolean | no | `down`: also remove named volumes |
| `force` | boolean | for `down` with `remove_volumes` | Must be `true` when `remove_volumes=true` |

### `flux help` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"help"` |
| `topic` | string | no | Topic key, e.g. `"container:list"`, `"docker:prune"`. Omit for the index. |
| `format` | string | no | `markdown\|json` (default `markdown`) |

---

### `scout nodes` parameters

No parameters (besides `action`).

### `scout peek` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"peek"` |
| `host` | string | yes | Target host |
| `path` | string | yes | Absolute path to file or directory |
| `tree` | boolean | no | Emit a depth-limited directory tree |
| `depth` | integer | no | Tree depth [1–20], default 3 |

### `scout find` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"find"` |
| `host` | string | yes | Target host |
| `path` | string | yes | Search root (absolute) |
| `pattern` | string | yes | Glob pattern for `-name` (must not start with `-`) |
| `depth` | integer | no | Max depth [1–20], default 10 |
| `limit` | integer | no | Max results, default 500 |

### `scout ps` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"ps"` |
| `host` | string | yes | Target host |
| `sort` | string | no | Sort field: `cpu\|mem\|pid\|time` (default `cpu`) |
| `grep` | string | no | Substring filter on process lines |
| `user` | string | no | Prefix-match filter on user column |
| `limit` | integer | no | Max results, default 50 |

### `scout df` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"df"` |
| `host` | string | yes | Target host |
| `path` | string | no | Restrict to this path |

### `scout delta` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"delta"` |
| `source_host` | string | yes | Source host |
| `source_path` | string | yes | Source absolute path |
| `target_host` | string | mutually exclusive with `content` | Target host |
| `target_path` | string | with `target_host` | Target absolute path |
| `content` | string | mutually exclusive with `target_host` | Inline content to compare (≤1 MB) |

### `scout exec` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"exec"` |
| `host` | string | yes | Target host |
| `command` | string | yes | Command name from allowlist |
| `args` | array of strings | no | Positional arguments (execvp-style) |
| `path` | string | no | Working directory (local hosts only) |
| `timeout_secs` | integer | no | Per-host timeout in seconds, default 30 |

### `scout emit` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"emit"` |
| `targets` | array | yes | `[{"host": "h1"}, {"host": "h2", "path": "/srv"}]` |
| `command` | string | yes | Command name from allowlist |
| `args` | array of strings | no | Positional arguments |
| `timeout_secs` | integer | no | Per-host timeout in seconds, default 30 |

### `scout beam` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"beam"` |
| `source_host` | string | yes | Source host |
| `source_path` | string | yes | Source absolute path |
| `dest_host` | string | yes | Destination host |
| `dest_path` | string | yes | Destination absolute path |

### `scout zfs` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"zfs"` |
| `subaction` | string | yes | `pools\|datasets\|snapshots` |
| `host` | string | yes | Target host |
| `pool` | string | no | `pools`: exact pool name filter. `datasets`: restrict to pool. `snapshots`: restrict to pool if `dataset` not given. |
| `dataset_type` | string | no | `datasets`: `filesystem\|volume\|snapshot\|bookmark\|all` |
| `recursive` | boolean | no | `datasets`: list recursively (default false) |
| `dataset` | string | no | `snapshots`: restrict to this dataset |
| `limit` | integer | no | `snapshots`: max results |

### `scout logs` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"logs"` |
| `subaction` | string | yes | `syslog\|journal\|dmesg\|auth` |
| `host` | string | yes | Target host |
| `lines` | integer | no | Lines to retrieve [1–500], default 100 |
| `grep` | string | no | Local filter (applied after retrieval, injection-safe) |
| `unit` | string | no | `journal`: systemd unit filter |
| `priority` | string | no | `journal`: priority filter (`err\|warning\|info\|debug`) |
| `since` | string | no | `journal`: start time, e.g. `"2026-05-29 00:00:00"` or `"-1h"` |
| `until` | string | no | `journal`: end time |

### `scout help` parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"help"` |
| `topic` | string | no | Topic key, e.g. `"exec"`, `"zfs:pools"`. Omit for the index. |
| `format` | string | no | `markdown\|json` (default `markdown`) |

---

<!-- TIER 3: Workflows, HTTP fallback, error handling -->

## Common Workflows

### Investigate a host's Docker stack

```text
# 1. Check connectivity
mcp__synapse2__flux(action="host", subaction="status", host="myhost")

# 2. List running containers
mcp__synapse2__flux(action="container", subaction="list", host="myhost", state="running")

# 3. Inspect a container
mcp__synapse2__flux(action="container", subaction="inspect", host="myhost", container_id="abc123")

# 4. Read recent logs
mcp__synapse2__flux(action="container", subaction="logs", host="myhost", container_id="abc123", lines=100, grep="ERROR")

# 5. Check overall disk usage
mcp__synapse2__flux(action="docker", subaction="df", host="myhost")
```

### Diagnose a failing Compose stack

```text
# 1. List all compose projects
mcp__synapse2__flux(action="compose", subaction="list", host="myhost")

# 2. Get project status
mcp__synapse2__flux(action="compose", subaction="status", host="myhost", project="mystack")

# 3. Read logs
mcp__synapse2__flux(action="compose", subaction="logs", host="myhost", project="mystack", lines=200)

# 4. Restart the stack
mcp__synapse2__flux(action="compose", subaction="restart", host="myhost", project="mystack")
```

### Investigate a remote host via SSH

```text
# 1. List all nodes
mcp__synapse2__scout(action="nodes")

# 2. Check processes
mcp__synapse2__scout(action="ps", host="myhost", sort="cpu", limit=20)

# 3. Read a config file
mcp__synapse2__scout(action="peek", host="myhost", path="/etc/nginx/nginx.conf")

# 4. Read system logs
mcp__synapse2__scout(action="logs", subaction="journal", host="myhost", since="-30m", priority="err")
```

### Check ZFS pool health (Unraid / TrueNAS)

```text
mcp__synapse2__scout(action="zfs", subaction="pools", host="myhost")
mcp__synapse2__scout(action="zfs", subaction="datasets", host="myhost", pool="tank")
mcp__synapse2__scout(action="zfs", subaction="snapshots", host="myhost", dataset="tank/data")
```

## HTTP Fallback

When the MCP transport is unavailable, use `POST /v1/synapse2` with a bearer token:

```bash
curl -sX POST http://localhost:3100/v1/synapse2 \
  -H "Authorization: Bearer $SYNAPSE_MCP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"action":"docker","params":{"subaction":"info"}}'
```

## Error Handling

| Error type | Cause | Recovery |
|---|---|---|
| `invalid_params` | Missing required param or unknown action/subaction | Check param names; use `help` action |
| `invalid_request` | Destructive op denied at confirmation gate | User declined; no state changed |
| `internal_error` | Service error (Docker unavailable, SSH timeout) | Check host connectivity, retry |

If you get `unknown action`, run `flux(action="help")` or `scout(action="help")`
for the current action list.

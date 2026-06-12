---
name: synapse2
description: "Use when the user needs to inspect or manage Synapse2-managed Docker/Compose infrastructure via flux, or SSH/local host files, processes, logs, ZFS, and allowlisted commands via scout. Prefer MCP tools first, CLI second, REST last; use write or confirmation-gated actions only when explicitly requested or necessary."
---

# synapse2

<!-- TIER 1: Quick-reference table and critical gotchas -->

Two MCP tools: **`flux`** for Docker/host inspection, **`scout`** for SSH/local
host operations. Start with read-only inspection. Use write-scope or
confirmation-gated actions only when explicitly requested or clearly necessary.

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

For the full parameter table, use `references/action-reference.md` in this skill
package or the live MCP help topics (`flux(action="help")`,
`scout(action="help")`).

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
  returns an error without performing any IO. `SYNAPSE_MCP_ALLOW_DESTRUCTIVE`
  defaults to false. If enabled on a non-loopback bind, startup refuses to run.
- **Responses are token-budgeted** — very long log tails or large directory
  trees may be truncated. Use `lines`/`limit`/`depth` params to control size.

## Safety Matrix

| Action family | Write scope | Confirmation gated | Operator risk |
|---|---:|---:|---|
| `flux docker info/df/images/networks/volumes` | no | no | Read-only Docker inventory. |
| `flux docker pull` | yes | no | Downloads an image; can consume bandwidth/disk. |
| `flux docker build/rmi/prune` | yes | yes | Builds, removes, or prunes Docker resources. |
| `flux container list/inspect/logs/stats/top/search` | no | no | Read-only container inspection. |
| `flux container start/restart/pause/resume/pull` | yes | no | Changes container runtime state or image cache. |
| `flux container stop/recreate/exec` | yes | yes | Stops/replaces containers or executes inside them. |
| `flux host status/info/uptime/resources/services/network/mounts/ports/doctor` | no | no | Read-only host diagnostics. |
| `flux compose list/status/logs` | no | no | Read-only Compose inspection. |
| `flux compose up/build/pull` | yes | no | Starts services or changes image/build state. |
| `flux compose down/restart/recreate` | yes | yes | Stops, restarts, or replaces services. |
| `scout nodes/peek/find/ps/df/delta/zfs/logs` | no | no | Read-only SSH/local inspection. |
| `scout exec/emit/beam` | yes | yes | Host command execution or file transfer. |

## Response Shapes

Representative response keys:

```json
{"hosts":[{"name":"local","protocol":"local"}]}
{"containers":[{"host":"local","id":"...","name":"..."}],"partial":false}
{"tool":"flux","topics":["container:list"],"actions":{"docker":["info"]}}
```

---

<!-- TIER 2: Workflows, fallback tiers, error handling -->

## Common Workflows

Use neutral tool-call examples in shared skill docs:
`flux(action="...", subaction="...")` and `scout(action="...")`.
Codex may expose these as `mcp__synapse2__flux` and `mcp__synapse2__scout`.

### Investigate a host's Docker stack

```text
# 1. Check connectivity
flux(action="host", subaction="status", host="myhost")

# 2. List running containers
flux(action="container", subaction="list", host="myhost", state="running")

# 3. Inspect a container
flux(action="container", subaction="inspect", host="myhost", container_id="abc123")

# 4. Read recent logs
flux(action="container", subaction="logs", host="myhost", container_id="abc123", lines=100, grep="ERROR")

# 5. Check overall disk usage
flux(action="docker", subaction="df", host="myhost")
```

### Diagnose a failing Compose stack

```text
# 1. List all compose projects
flux(action="compose", subaction="list", host="myhost")

# 2. Get project status
flux(action="compose", subaction="status", host="myhost", project="mystack")

# 3. Read logs
flux(action="compose", subaction="logs", host="myhost", project="mystack", lines=200)

# 4. Inspect resource pressure before mutating anything
flux(action="host", subaction="resources", host="myhost")

# 5. Only if explicitly approved: restart the stack
flux(action="compose", subaction="restart", host="myhost", project="mystack")
```

### Investigate a remote host via SSH

```text
# 1. List all nodes
scout(action="nodes")

# 2. Check processes
scout(action="ps", host="myhost", sort="cpu", limit=20)

# 3. Read a config file
scout(action="peek", host="myhost", path="/etc/nginx/nginx.conf")

# 4. Read system logs
scout(action="logs", subaction="journal", host="myhost", since="-30m", priority="err")
```

### Check ZFS pool health (Unraid / TrueNAS)

```text
scout(action="zfs", subaction="pools", host="myhost")
scout(action="zfs", subaction="datasets", host="myhost", pool="tank")
scout(action="zfs", subaction="snapshots", host="myhost", dataset="tank/data")
```

## Fallback Tiers

### Tier 1: MCP tools

Use MCP first whenever the `flux` and `scout` tools are available.

```text
scout(action="nodes")
flux(action="docker", subaction="info")
```

### Tier 2: CLI binary

Use the `synapse` binary when MCP transport is unavailable but local shell access
exists:

```bash
synapse scout nodes --response-format json
synapse flux docker info --response-format json
synapse doctor --json
synapse setup check
```

### Tier 3: REST API

When the MCP transport is unavailable, use `POST /v1/synapse2` with a bearer token:

```bash
curl -sX POST "http://${SYNAPSE_MCP_HOST:-127.0.0.1}:${SYNAPSE_MCP_PORT:-40080}/v1/synapse2" \
  -H "Authorization: Bearer $SYNAPSE_MCP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"action":"flux.docker.info","params":{}}'
```

## Error Handling

| Error type | Cause | Recovery |
|---|---|---|
| `invalid_params` | Missing required param or unknown action/subaction | Check param names; use `help` action |
| `invalid_request` | Destructive op denied at confirmation gate | User declined; no state changed |
| `unauthorized` | Missing or bad bearer token / OAuth token | Check `SYNAPSE_MCP_TOKEN`, auth mode, and plugin settings |
| `internal_error` | Service error (Docker unavailable, SSH timeout) | Run diagnostics, check host connectivity, retry |

Recovery commands:

```text
flux(action="help")
scout(action="help")
```

```bash
synapse doctor --json
synapse setup check
```

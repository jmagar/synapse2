# synapse2

Rust MCP and CLI server for local Synapse workflows — a full-parity port of
[synapse-mcp](https://github.com/jmagar/synapse-mcp) implemented in Rust with
the [rmcp](https://github.com/modelcontextprotocol/rust-sdk) framework.

`synapse2` exposes two MCP tools (`flux` and `scout`) plus equivalent CLI
commands, covering all 59 production actions from the original TypeScript server.

## Surfaces

| Surface | Status | Purpose |
|---|---:|---|
| MCP | Required | Agent-facing `flux` and `scout` tools |
| CLI | Required | Scriptable parity surface |
| REST | Present | Thin local action endpoint |
| Web | Present | Lightweight template admin shell |

## Tools and Actions

### `flux` — Docker infrastructure management

#### `flux docker` — Docker daemon operations (9 actions)

| Subaction | Scope | Description |
|---|---|---|
| `info` | `synapse:read` | Docker daemon information |
| `df` | `synapse:read` | Docker disk usage |
| `images` | `synapse:read` | List Docker images; `dangling_only` to filter untagged |
| `networks` | `synapse:read` | List Docker networks |
| `volumes` | `synapse:read` | List Docker volumes |
| `pull` | `synapse:read` | Pull a Docker image; requires `host`, `image` |
| `build` | `synapse:write` | Build a Docker image; requires `host`, `context`, `tag`; optional `dockerfile`, `no_cache` |
| `rmi` | `synapse:write` | Remove a Docker image; requires `host`, `image`, `force=true` |
| `prune` | `synapse:write` | Remove unused resources; requires `host`, `prune_target`, `force=true` |

#### `flux container` — Container lifecycle + inspection (14 actions)

| Subaction | Scope | Description |
|---|---|---|
| `list` | `synapse:read` | List containers; optional `state`, `name_filter`, `image_filter`, `label_filter` |
| `inspect` | `synapse:read` | Detailed container info; requires `container_id`; optional `summary` |
| `logs` | `synapse:read` | Container logs; requires `container_id`; optional `lines`, `since`, `until`, `grep`, `stream` |
| `stats` | `synapse:read` | Resource usage stats; optional `container_id` |
| `top` | `synapse:read` | Show running processes; requires `container_id` |
| `search` | `synapse:read` | Full-text search by name/image/labels; requires `query` |
| `start` | `synapse:read` | Start a stopped container; requires `container_id` |
| `stop` | `synapse:write` | Stop a running container (destructive); requires `container_id` |
| `restart` | `synapse:read` | Restart a container; requires `container_id` |
| `pause` | `synapse:read` | Pause a running container; requires `container_id` |
| `resume` | `synapse:read` | Resume a paused container; requires `container_id` |
| `pull` | `synapse:read` | Pull latest image for a container; requires `container_id` |
| `recreate` | `synapse:write` | Recreate container with image pull (destructive); requires `container_id`; optional `pull` (default true) |
| `exec` | `synapse:write` | Execute command inside container (destructive, execvp); requires `container_id`, `command` array |

#### `flux host` — Host inspection (9 actions)

| Subaction | Scope | Description |
|---|---|---|
| `status` | `synapse:read` | Check Docker connectivity on a host |
| `info` | `synapse:read` | OS, kernel, architecture |
| `uptime` | `synapse:read` | System uptime |
| `resources` | `synapse:read` | CPU, memory, disk usage |
| `services` | `synapse:read` | Systemd service status; requires `host`; optional `state`, `service` |
| `network` | `synapse:read` | Network interfaces |
| `mounts` | `synapse:read` | Mounted filesystems; requires `host` |
| `ports` | `synapse:read` | Port mappings; requires `host`; optional `protocol`, `limit`, `offset` |
| `doctor` | `synapse:read` | Diagnostic checks; requires `host`; optional `checks` (comma-separated) |

#### `flux compose` — Docker Compose project management (10 actions)

| Subaction | Scope | Description |
|---|---|---|
| `list` | `synapse:read` | List all Docker Compose projects; requires `host` |
| `status` | `synapse:read` | Get project service status; requires `host`, `project`; optional `service` |
| `up` | `synapse:read` | Start a compose project; requires `host`, `project` |
| `down` | `synapse:write` | Stop a compose project (destructive); requires `host`, `project`; optional `remove_volumes`, `force` |
| `restart` | `synapse:write` | Restart a compose project (destructive); requires `host`, `project` |
| `recreate` | `synapse:write` | Recreate compose containers (destructive); requires `host`, `project` |
| `logs` | `synapse:read` | Get project logs; requires `host`, `project`; optional `service`, `lines`, `since` |
| `build` | `synapse:read` | Build compose project images; requires `host`, `project`; optional `service` |
| `pull` | `synapse:read` | Pull compose project images; requires `host`, `project`; optional `service` |
| `refresh` | `synapse:read` | Refresh compose project cache; requires `host` |

#### `flux help` — Auto-generated flux docs

| Action | Scope | Description |
|---|---|---|
| `help` | public | Return flux action reference; optional `topic` (e.g. `"container:list"`), `format` (`markdown`\|`json`) |

---

### `scout` — SSH/local host inspection

#### Scout simple actions (9 actions)

| Action | Scope | Description |
|---|---|---|
| `nodes` | `synapse:read` | List all configured SSH hosts |
| `peek` | `synapse:read` | Read a file or directory listing; requires `host`, `path`; optional `tree`, `depth` |
| `find` | `synapse:read` | Find files by glob; requires `host`, `path`, `pattern`; optional `depth`, `limit` |
| `ps` | `synapse:read` | List processes; requires `host`; optional `sort` (`cpu`\|`mem`\|`pid`\|`time`), `grep`, `user`, `limit` |
| `df` | `synapse:read` | Disk usage; requires `host`; optional `path` |
| `delta` | `synapse:read` | Compare files or content; requires `source_host`, `source_path`; then either `target_host`+`target_path` or `content` |
| `exec` | `synapse:write` | Execute allowlisted command (destructive, execvp); requires `host`, `command`; optional `path`, `args`, `timeout_secs` |
| `emit` | `synapse:write` | Multi-host execution (destructive); requires `targets` array, `command`; optional `args`, `timeout_secs` |
| `beam` | `synapse:write` | File transfer between hosts (destructive); requires `source_host`, `source_path`, `dest_host`, `dest_path` |

#### `scout zfs` — ZFS introspection (3 subactions)

| Subaction | Scope | Description |
|---|---|---|
| `pools` | `synapse:read` | List ZFS pools; requires `host`; optional `pool` name filter |
| `datasets` | `synapse:read` | List ZFS datasets; requires `host`; optional `pool`, `dataset_type`, `recursive` |
| `snapshots` | `synapse:read` | List ZFS snapshots; requires `host`; optional `pool`, `dataset`, `limit` |

#### `scout logs` — Log retrieval (4 subactions)

| Subaction | Scope | Description |
|---|---|---|
| `syslog` | `synapse:read` | Read `/var/log/syslog` (falls back to `/var/log/messages`); requires `host`; optional `lines`, `grep` |
| `journal` | `synapse:read` | Read systemd journal; requires `host`; optional `lines`, `unit`, `priority`, `since`, `until`, `grep` |
| `dmesg` | `synapse:read` | Read kernel ring buffer; requires `host`; optional `lines`, `grep` |
| `auth` | `synapse:read` | Read `/var/log/auth.log` (falls back to `/var/log/secure`); requires `host`; optional `lines`, `grep` |

#### `scout help` — Auto-generated scout docs

| Action | Scope | Description |
|---|---|---|
| `help` | public | Return scout action reference; optional `topic` (e.g. `"zfs:pools"`), `format` (`markdown`\|`json`) |

## Configuration

```bash
SYNAPSE_MCP_HOST=127.0.0.1
SYNAPSE_MCP_PORT=40080
SYNAPSE_MCP_TOKEN=change-me
```

See `.env.example` for the full list of variables and `docs/CONFIG.md` for auth
configuration details.

## Run

```bash
# Start MCP server (stdio transport)
cargo run -- mcp

# Start HTTP server
cargo run -- serve

# CLI examples
cargo run -- flux docker info
cargo run -- flux container list
cargo run -- flux compose list --host myhost
cargo run -- scout nodes
cargo run -- scout exec --host myhost --command hostname
cargo run -- scout zfs pools --host myhost
cargo run -- scout logs journal --host myhost --unit docker
```

MCP examples:

```json
{"name":"flux","arguments":{"action":"docker","subaction":"info"}}
{"name":"flux","arguments":{"action":"container","subaction":"list","state":"running"}}
{"name":"flux","arguments":{"action":"compose","subaction":"status","host":"myhost","project":"mystack"}}
{"name":"scout","arguments":{"action":"nodes"}}
{"name":"scout","arguments":{"action":"exec","host":"myhost","command":"hostname"}}
{"name":"scout","arguments":{"action":"zfs","subaction":"pools","host":"myhost"}}
{"name":"scout","arguments":{"action":"logs","subaction":"journal","host":"myhost","unit":"docker"}}
```

## Architecture

```text
FluxService   (src/flux_service/)  Docker/container/compose/host ops
ScoutService  (src/scout_service/) SSH/exec/fs/zfs/logs ops
      ↓ via SynapseService facade (src/app.rs)
MCP shims     (src/mcp/tools.rs)  tool args → service → Value
CLI shim      (src/cli.rs)        argv → service → stdout
REST layer    (src/api.rs)        POST /v1/synapse2 → service → JSON
```

## Development

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build --release

just dev     # serve with no auth (loopback, dev mode)
just test    # cargo test
just lint    # clippy
just fmt     # cargo fmt
```

Useful docs:

- `docs/API.md` for full tool contracts
- `docs/CONFIG.md` for environment and auth
- `docs/QUICKSTART.md` for local smoke tests
- `plugins/synapse2/skills/synapse2/SKILL.md` for agent usage guidance
- `tests/parity.rs` for automated parity verification against synapse-mcp INVENTORY

# synapse2 API

`synapse2` exposes two MCP tools (`flux` and `scout`), a REST action endpoint at
`POST /v1/synapse2`, and equivalent CLI commands. All three surfaces call the
same service layer and produce identical results.

## MCP Tools

Both tools follow the same calling convention:

```json
{"name": "<tool>", "arguments": {"action": "<action>", "subaction": "<subaction>", ...params}}
```

### `flux` — Docker infrastructure management

The `flux` tool manages Docker hosts, containers, images, networks, volumes, and
Compose stacks. All read-only ops fan out across all configured hosts when `host`
is omitted; destructive ops always require an explicit `host`.

#### `action="docker"` — Docker daemon (9 subactions)

```json
{"name":"flux","arguments":{"action":"docker","subaction":"info"}}
{"name":"flux","arguments":{"action":"docker","subaction":"df"}}
{"name":"flux","arguments":{"action":"docker","subaction":"images"}}
{"name":"flux","arguments":{"action":"docker","subaction":"images","dangling_only":true}}
{"name":"flux","arguments":{"action":"docker","subaction":"networks"}}
{"name":"flux","arguments":{"action":"docker","subaction":"volumes"}}
{"name":"flux","arguments":{"action":"docker","subaction":"pull","host":"myhost","image":"nginx:latest"}}
{"name":"flux","arguments":{"action":"docker","subaction":"build","host":"myhost","context":"/srv/app","tag":"myapp:dev"}}
{"name":"flux","arguments":{"action":"docker","subaction":"rmi","host":"myhost","image":"nginx:old","force":true}}
{"name":"flux","arguments":{"action":"docker","subaction":"prune","host":"myhost","prune_target":"containers","force":true}}
```

`prune_target` values: `containers`, `images`, `volumes`, `networks`,
`buildcache`, `all`.

#### `action="container"` — Container lifecycle + inspection (14 subactions)

```json
{"name":"flux","arguments":{"action":"container","subaction":"list"}}
{"name":"flux","arguments":{"action":"container","subaction":"list","state":"running","name_filter":"nginx"}}
{"name":"flux","arguments":{"action":"container","subaction":"inspect","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"inspect","container_id":"abc123","summary":true}}
{"name":"flux","arguments":{"action":"container","subaction":"logs","container_id":"abc123","lines":100}}
{"name":"flux","arguments":{"action":"container","subaction":"logs","container_id":"abc123","since":"30m","grep":"ERROR"}}
{"name":"flux","arguments":{"action":"container","subaction":"stats"}}
{"name":"flux","arguments":{"action":"container","subaction":"top","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"search","query":"nginx"}}
{"name":"flux","arguments":{"action":"container","subaction":"start","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"stop","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"restart","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"pause","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"resume","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"pull","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"recreate","container_id":"abc123"}}
{"name":"flux","arguments":{"action":"container","subaction":"exec","container_id":"abc123","command":["ls","-la","/var/log"]}}
```

`exec` uses execvp semantics — `command` is an argv array; index 0 is the
binary. No shell is involved. Optional `exec_user`, `exec_workdir`,
`exec_timeout_ms` (default 30000, range [1000, 300000]).

`state` values for `list`: `running`, `exited`, `paused`, `restarting`, `all`.
`stream` values for `logs`: `stdout`, `stderr`, `both`.

#### `action="host"` — Host inspection (9 subactions)

```json
{"name":"flux","arguments":{"action":"host","subaction":"status"}}
{"name":"flux","arguments":{"action":"host","subaction":"info"}}
{"name":"flux","arguments":{"action":"host","subaction":"uptime"}}
{"name":"flux","arguments":{"action":"host","subaction":"resources"}}
{"name":"flux","arguments":{"action":"host","subaction":"services","host":"myhost"}}
{"name":"flux","arguments":{"action":"host","subaction":"services","host":"myhost","state":"running","service":"nginx"}}
{"name":"flux","arguments":{"action":"host","subaction":"network"}}
{"name":"flux","arguments":{"action":"host","subaction":"mounts","host":"myhost"}}
{"name":"flux","arguments":{"action":"host","subaction":"ports","host":"myhost","protocol":"tcp","limit":50}}
{"name":"flux","arguments":{"action":"host","subaction":"doctor","host":"myhost"}}
{"name":"flux","arguments":{"action":"host","subaction":"doctor","host":"myhost","checks":"docker,ssh,disk"}}
```

#### `action="compose"` — Compose project management (10 subactions)

```json
{"name":"flux","arguments":{"action":"compose","subaction":"list","host":"myhost"}}
{"name":"flux","arguments":{"action":"compose","subaction":"status","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"up","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"down","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"down","host":"myhost","project":"mystack","remove_volumes":true,"force":true}}
{"name":"flux","arguments":{"action":"compose","subaction":"restart","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"recreate","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"logs","host":"myhost","project":"mystack","lines":200}}
{"name":"flux","arguments":{"action":"compose","subaction":"build","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"pull","host":"myhost","project":"mystack"}}
{"name":"flux","arguments":{"action":"compose","subaction":"refresh","host":"myhost"}}
```

#### `action="help"` — Flux documentation

```json
{"name":"flux","arguments":{"action":"help"}}
{"name":"flux","arguments":{"action":"help","topic":"container:list"}}
{"name":"flux","arguments":{"action":"help","topic":"docker:prune","format":"json"}}
```

---

### `scout` — SSH/local host inspection

The `scout` tool inspects hosts via SSH or local execution. Destructive ops
(`exec`, `emit`, `beam`) require `synapse:write` scope and go through the
elicitation confirmation gate.

#### `action="nodes"` — List configured hosts

```json
{"name":"scout","arguments":{"action":"nodes"}}
```

#### `action="peek"` — Read file/directory

```json
{"name":"scout","arguments":{"action":"peek","host":"myhost","path":"/etc/nginx/nginx.conf"}}
{"name":"scout","arguments":{"action":"peek","host":"myhost","path":"/var/log","tree":true,"depth":3}}
```

#### `action="find"` — Find files by glob

```json
{"name":"scout","arguments":{"action":"find","host":"myhost","path":"/etc","pattern":"*.conf"}}
{"name":"scout","arguments":{"action":"find","host":"myhost","path":"/var/log","pattern":"*.log","depth":5,"limit":100}}
```

#### `action="ps"` — List processes

```json
{"name":"scout","arguments":{"action":"ps","host":"myhost"}}
{"name":"scout","arguments":{"action":"ps","host":"myhost","sort":"mem","grep":"nginx","limit":20}}
```

`sort` values: `cpu`, `mem`, `pid`, `time`.

#### `action="df"` — Disk usage

```json
{"name":"scout","arguments":{"action":"df","host":"myhost"}}
{"name":"scout","arguments":{"action":"df","host":"myhost","path":"/var"}}
```

#### `action="delta"` — Compare files/content

```json
{"name":"scout","arguments":{"action":"delta","source_host":"host1","source_path":"/etc/nginx/nginx.conf","target_host":"host2","target_path":"/etc/nginx/nginx.conf"}}
{"name":"scout","arguments":{"action":"delta","source_host":"myhost","source_path":"/etc/hosts","content":"127.0.0.1 localhost\n"}}
```

#### `action="exec"` — Execute allowlisted command (destructive)

```json
{"name":"scout","arguments":{"action":"exec","host":"myhost","command":"hostname"}}
{"name":"scout","arguments":{"action":"exec","host":"myhost","command":"tail","args":["-n","50","/var/log/syslog"]}}
```

Allowlisted commands: `cat`, `head`, `tail`, `grep`, `rg`, `find`, `ls`,
`tree`, `wc`, `sort`, `uniq`, `diff`, `stat`, `file`, `du`, `df`, `pwd`,
`hostname`, `uptime`, `whoami`. `git` is explicitly excluded.

#### `action="emit"` — Multi-host execution (destructive)

```json
{"name":"scout","arguments":{"action":"emit","targets":[{"host":"host1"},{"host":"host2"}],"command":"uptime"}}
```

#### `action="beam"` — File transfer (destructive)

```json
{"name":"scout","arguments":{"action":"beam","source_host":"host1","source_path":"/etc/nginx/nginx.conf","dest_host":"host2","dest_path":"/etc/nginx/nginx.conf"}}
```

#### `action="zfs"` — ZFS introspection (3 subactions)

```json
{"name":"scout","arguments":{"action":"zfs","subaction":"pools","host":"myhost"}}
{"name":"scout","arguments":{"action":"zfs","subaction":"pools","host":"myhost","pool":"tank"}}
{"name":"scout","arguments":{"action":"zfs","subaction":"datasets","host":"myhost","pool":"tank","recursive":true}}
{"name":"scout","arguments":{"action":"zfs","subaction":"datasets","host":"myhost","dataset_type":"filesystem"}}
{"name":"scout","arguments":{"action":"zfs","subaction":"snapshots","host":"myhost","dataset":"tank/data","limit":50}}
```

`dataset_type` values: `filesystem`, `volume`, `snapshot`, `bookmark`, `all`.

#### `action="logs"` — Log retrieval (4 subactions)

```json
{"name":"scout","arguments":{"action":"logs","subaction":"syslog","host":"myhost","lines":100}}
{"name":"scout","arguments":{"action":"logs","subaction":"journal","host":"myhost","unit":"docker","priority":"err"}}
{"name":"scout","arguments":{"action":"logs","subaction":"journal","host":"myhost","since":"-1h","until":"now","lines":200}}
{"name":"scout","arguments":{"action":"logs","subaction":"dmesg","host":"myhost","grep":"error"}}
{"name":"scout","arguments":{"action":"logs","subaction":"auth","host":"myhost","lines":50}}
```

`lines` is clamped to [1, 500]; default 100. All log subactions support
`grep` (applied locally after retrieval, injection-safe).

#### `action="help"` — Scout documentation

```json
{"name":"scout","arguments":{"action":"help"}}
{"name":"scout","arguments":{"action":"help","topic":"zfs:pools"}}
{"name":"scout","arguments":{"action":"help","topic":"exec","format":"json"}}
```

---

## CLI Parity

Every MCP action is also reachable from the CLI. The tool name becomes the first
positional argument, action the second, and subaction the third.

```bash
# flux docker
synapse flux docker info
synapse flux docker images
synapse flux docker pull --host myhost --image nginx:latest

# flux container
synapse flux container list
synapse flux container list --state running
synapse flux container logs --container-id abc123 --lines 100
synapse flux container exec --container-id abc123 -- ls -la /var/log

# flux host
synapse flux host status
synapse flux host resources
synapse flux host services --host myhost

# flux compose
synapse flux compose list --host myhost
synapse flux compose status --host myhost --project mystack

# scout simple
synapse scout nodes
synapse scout peek --host myhost --path /etc/nginx/nginx.conf
synapse scout exec --host myhost --command hostname

# scout zfs
synapse scout zfs pools --host myhost
synapse scout zfs datasets --host myhost --pool tank

# scout logs
synapse scout logs syslog --host myhost
synapse scout logs journal --host myhost --unit docker --priority err
```

---

## REST Endpoint

`POST /v1/synapse2`

```json
{
  "action": "flux.docker.info",
  "params": {}
}
```

REST is a thin compatibility surface. MCP and CLI are the primary supported
surfaces. Some write-scope actions are not available over REST.

---

## Security Rules

| Rule | Detail |
|---|---|
| `help` actions | Public — no auth required |
| Read actions | Require `synapse:read` scope |
| Write / destructive actions | Require `synapse:write` scope |
| `synapse:write` satisfies read | Yes — write ⊇ read |
| Command execution | Allowlist-based; no shell; execvp semantics |
| Path validation | Absolute paths only; no `..`, `~`, `$`; no local symlinks |
| Response size | Token budget enforced before returning to MCP clients |

---

## Parity Verification

`tests/parity.rs` asserts every action in
`../synapse-mcp/docs/INVENTORY.md` is covered by synapse2's `ACTION_SPECS`
and help map. Run with:

```bash
cargo test --test parity -- --nocapture
```

Expected output: `synapse-mcp parity: 61 rows parsed → 61 matched, 0 missing`

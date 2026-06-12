# Synapse2 Action Reference

This reference expands the quick table in `../SKILL.md`. Prefer live MCP help
when available:

```text
flux(action="help")
flux(action="help", topic="container:list")
scout(action="help")
scout(action="help", topic="logs:journal")
```

## `flux docker` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"docker"` |
| `subaction` | string | yes | `info\|df\|images\|networks\|volumes\|pull\|build\|rmi\|prune` |
| `host` | string | for write ops | Target host name; omit to fan out for read ops |
| `dangling_only` | boolean | no | `images`: only list untagged images |
| `image` | string | for pull/rmi | Image reference, e.g. `nginx:latest` |
| `force` | boolean | for rmi/prune | Must be `true` to allow destructive ops |
| `context` | string | for build | Absolute build context path |
| `tag` | string | for build | Image tag, e.g. `myapp:latest` |
| `dockerfile` | string | no | Dockerfile path relative to context |
| `no_cache` | boolean | no | Pass `--no-cache` to build |
| `prune_target` | string | for prune | `containers\|images\|volumes\|networks\|buildcache\|all` |

## `flux container` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"container"` |
| `subaction` | string | yes | `list\|inspect\|logs\|stats\|top\|search\|start\|stop\|restart\|pause\|resume\|pull\|recreate\|exec` |
| `host` | string | no | Target host; fan out when omitted |
| `container_id` | string | most subactions | Container id or name |
| `state` | string | no | `list`: `running\|exited\|paused\|restarting\|all` |
| `name_filter` | string | no | `list`: partial match on container name |
| `image_filter` | string | no | `list`: partial match on image |
| `label_filter` | string | no | `list`: `key=value` label match |
| `lines` | integer | no | `logs`: tail line count, default 50 |
| `since` | string | no | `logs`: ISO8601, unix seconds, or duration, e.g. `"30m"` |
| `until` | string | no | `logs`: same formats as `since` |
| `grep` | string | no | `logs`: keep only lines containing this string |
| `stream` | string | no | `logs`: `stdout\|stderr\|both`, default `both` |
| `summary` | boolean | no | `inspect`: return abbreviated info only |
| `query` | string | for search | `search`: full-text query |
| `command` | array of strings | for exec | `exec`: argv, e.g. `["ls", "-la", "/var/log"]` |
| `exec_user` | string | no | `exec`: run as this user inside the container |
| `exec_workdir` | string | no | `exec`: working directory inside the container |
| `exec_timeout_ms` | integer | no | `exec`: timeout in ms [1000-300000], default 30000 |
| `pull` | boolean | no | `recreate`: pull image before recreating, default true |

## `flux host` Parameters

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
| `checks` | string | no | `doctor`: comma-separated check names, default all |

## `flux compose` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"compose"` |
| `subaction` | string | yes | `list\|status\|up\|down\|restart\|recreate\|logs\|build\|pull\|refresh` |
| `host` | string | yes | Target host name |
| `project` | string | most subactions | Compose project name |
| `service` | string | no | `logs\|status\|build\|pull`: restrict to one service |
| `lines` | integer | no | `logs`: tail line count |
| `since` | string | no | `logs`: start time filter |
| `remove_volumes` | boolean | no | `down`: also remove named volumes |
| `force` | boolean | for `down` with `remove_volumes` | Must be `true` when `remove_volumes=true` |

## `flux help` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"help"` |
| `topic` | string | no | Topic key, e.g. `"container:list"`, `"docker:prune"` |
| `format` | string | no | `markdown\|json`, default `markdown` |

## `scout nodes` Parameters

No parameters besides `action`.

## `scout peek` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"peek"` |
| `host` | string | yes | Target host |
| `path` | string | yes | Absolute path to file or directory |
| `tree` | boolean | no | Emit a depth-limited directory tree |
| `depth` | integer | no | Tree depth [1-20], default 3 |

## `scout find` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"find"` |
| `host` | string | yes | Target host |
| `path` | string | yes | Search root, absolute |
| `pattern` | string | yes | Glob pattern for `-name`, must not start with `-` |
| `depth` | integer | no | Max depth [1-20], default 10 |
| `limit` | integer | no | Max results, default 500 |

## `scout ps` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"ps"` |
| `host` | string | yes | Target host |
| `sort` | string | no | Sort field: `cpu\|mem\|pid\|time`, default `cpu` |
| `grep` | string | no | Substring filter on process lines |
| `user` | string | no | Prefix-match filter on user column |
| `limit` | integer | no | Max results, default 50 |

## `scout df` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"df"` |
| `host` | string | yes | Target host |
| `path` | string | no | Restrict to this path |

## `scout delta` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"delta"` |
| `source_host` | string | yes | Source host |
| `source_path` | string | yes | Source absolute path |
| `target_host` | string | mutually exclusive with `content` | Target host |
| `target_path` | string | with `target_host` | Target absolute path |
| `content` | string | mutually exclusive with `target_host` | Inline content to compare, max 1 MB |

## `scout exec` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"exec"` |
| `host` | string | yes | Target host |
| `command` | string | yes | Command name from allowlist |
| `args` | array of strings | no | Positional arguments, execvp-style |
| `path` | string | no | Working directory, local hosts only |
| `timeout_secs` | integer | no | Per-host timeout in seconds, default 30 |

## `scout emit` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"emit"` |
| `targets` | array | yes | `[{"host": "h1"}, {"host": "h2", "path": "/srv"}]` |
| `command` | string | yes | Command name from allowlist |
| `args` | array of strings | no | Positional arguments |
| `timeout_secs` | integer | no | Per-host timeout in seconds, default 30 |

## `scout beam` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"beam"` |
| `source_host` | string | yes | Source host |
| `source_path` | string | yes | Source absolute path |
| `dest_host` | string | yes | Destination host |
| `dest_path` | string | yes | Destination absolute path |

## `scout zfs` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"zfs"` |
| `subaction` | string | yes | `pools\|datasets\|snapshots` |
| `host` | string | yes | Target host |
| `pool` | string | no | `pools`: exact pool filter. `datasets`: restrict to pool. `snapshots`: restrict to pool if `dataset` not given. |
| `dataset_type` | string | no | `datasets`: `filesystem\|volume\|snapshot\|bookmark\|all` |
| `recursive` | boolean | no | `datasets`: list recursively, default false |
| `dataset` | string | no | `snapshots`: restrict to this dataset |
| `limit` | integer | no | `snapshots`: max results |

## `scout logs` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"logs"` |
| `subaction` | string | yes | `syslog\|journal\|dmesg\|auth` |
| `host` | string | yes | Target host |
| `lines` | integer | no | Lines to retrieve [1-500], default 100 |
| `grep` | string | no | Local filter applied after retrieval, injection-safe |
| `unit` | string | no | `journal`: systemd unit filter |
| `priority` | string | no | `journal`: priority filter: `err\|warning\|info\|debug` |
| `since` | string | no | `journal`: start time, e.g. `"2026-05-29 00:00:00"` or `"-1h"` |
| `until` | string | no | `journal`: end time |

## `scout help` Parameters

| Param | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | `"help"` |
| `topic` | string | no | Topic key, e.g. `"exec"`, `"zfs:pools"` |
| `format` | string | no | `markdown\|json`, default `markdown` |

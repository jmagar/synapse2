# synapse2 MCP Schema Contract

`synapse2` exposes two MCP tools: `flux` and `scout`.

Run:

```bash
python3 scripts/check-schema-docs.py --write
python3 scripts/check-schema-docs.py --check
```

## Tool

| Tool | Dispatch parameter | Purpose |
|---|---|---|
| `flux` | `action` | Docker, container, host, and compose operations |
| `scout` | `action` | SSH/local filesystem, process, ZFS, log, and command operations |

## Actions

| Tool | Action | Scope | Description |
|---|---|---|---|
| `flux` | `help` | public | Return the in-tool action reference. Public; no scope required. |
| `scout` | `help` | public | Return the in-tool action reference. Public; no scope required. |
| `flux` | `docker` | `synapse:read` | Docker daemon and image operations. |
| `flux` | `container` | `synapse:read` | Container read and lifecycle operations. |
| `flux` | `host` | `synapse:read` | Host status, resource, service, network, mount, port, and doctor operations. |
| `flux` | `compose` | `synapse:read` | Docker Compose project operations. |
| `scout` | `nodes` | `synapse:read` | List configured hosts. |
| `scout` | `peek` | `synapse:read` | Read a file or directory listing. |
| `scout` | `find` | `synapse:read` | Find files by glob. |
| `scout` | `ps` | `synapse:read` | List processes. |
| `scout` | `df` | `synapse:read` | Report disk usage. |
| `scout` | `delta` | `synapse:read` | Compare files or inline content. |
| `scout` | `exec` | `synapse:write` | Run an allowlisted command. |
| `scout` | `emit` | `synapse:write` | Run an allowlisted command across multiple targets. |
| `scout` | `beam` | `synapse:write` | Transfer a file between hosts. |
| `scout` | `zfs` | `synapse:read` | Read ZFS pools, datasets, and snapshots. |
| `scout` | `logs` | `synapse:read` | Read syslog, journal, dmesg, and auth logs. |

## Drift Rules

- `ACTION_SPECS` in `src/actions.rs` is the canonical action and scope list.
- `src/mcp/schemas.rs` must expose exactly the `flux` and `scout` tool schemas.
- Both MCP tool schemas must reject unknown top-level parameters.
- `help` is intentionally public and must have no required scope.
- `README.md`, `docs/API.md`, and `plugins/synapse2/skills/synapse2/SKILL.md` must mention every shipped action.
- `src/mcp/resources.rs` owns stable resources and must keep `synapse://schema/flux` and `synapse://schema/scout` wired to `tool_definitions()`.
- `src/mcp/prompts.rs` owns stable prompts and must keep `quick_start` covered by prompt tests.

## Resources

| URI | Source | Contract |
|---|---|---|
| `synapse://schema/flux` | `src/mcp/resources.rs` | Returns the `flux` schema from `tool_definitions()` as `application/json`. |
| `synapse://schema/scout` | `src/mcp/resources.rs` | Returns the `scout` schema from `tool_definitions()` as `application/json`. |

## Prompts

| Prompt | Source | Contract |
|---|---|---|
| `quick_start` | `src/mcp/prompts.rs` | Guides a client to call `scout` `nodes` and `flux` `host`. |

## Input Validation

- `action` is always required.
- Unknown top-level parameters are rejected by the schema.
- Destructive operations require `synapse:write` and a service-layer confirmation gate.

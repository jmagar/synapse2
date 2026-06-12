# Plugin Surfaces

Synapse2 ships one service plugin package with three host-specific entrypoints:

- Claude Code: `plugins/synapse2/.claude-plugin/plugin.json`
- Codex: `plugins/synapse2/.codex-plugin/plugin.json`
- Gemini: `plugins/synapse2/gemini-extension.json`

All three surfaces should describe the same MCP server, expose the same skills, and connect to the same HTTP MCP endpoint. The host manifests differ, but the service behavior should not.

## Layout

```text
plugins/synapse2/
  .claude-plugin/
    plugin.json          # Claude Code manifest
  .codex-plugin/
    plugin.json          # Codex manifest
    README.md            # Codex manifest field reference
  mcp.json               # Shared Claude/Codex MCP connection config
  gemini-extension.json  # Gemini CLI extension manifest
  hooks/
    hooks.json           # Claude lifecycle hook declarations
    plugin-setup.sh      # Thin adapter to the binary setup command
  bin/
    synapse             # Optional Git LFS-tracked plugin binary artifact
  skills/
    synapse2/
      SKILL.md           # Shared action documentation
```

When changing the action surface, keep the plugin package, skill text, and manifests aligned with the Synapse2 binary and `flux`/`scout` tools.

## Shared Contract

Each plugin surface should agree on:

- service name and repository URL
- MCP server name
- HTTP MCP URL shape: `<server_url>/mcp`
- bearer token setting name
- upstream service credential names
- action list and skill documentation
- read/write capability claims

Keep the plugin manifests thin. Runtime setup belongs in the service binary, not in manifest-specific shell code.

## Claude Code

Claude Code uses `plugins/synapse2/.claude-plugin/plugin.json`.

Responsibilities:

- identifies the plugin and repository
- declares `mcpServers`, `hooks`, and `skills` paths
- defines `userConfig` settings exposed in Claude Code
- marks sensitive values with `sensitive: true`

Claude-specific lifecycle hooks live in `plugins/synapse2/hooks/hooks.json`. The default hooks are:

| Hook | Trigger | Command |
| --- | --- | --- |
| `SessionStart` | every Claude Code session start | `${CLAUDE_PLUGIN_ROOT}/hooks/plugin-setup.sh` |
| `ConfigChange` | plugin user settings change | `${CLAUDE_PLUGIN_ROOT}/hooks/plugin-setup.sh` |

`plugin-setup.sh` must stay a thin adapter. The standard command is:

```bash
<binary> setup plugin-hook
```

For rollout audits, the binary must also support:

```bash
<binary> setup plugin-hook --no-repair
```

The hook script may map `CLAUDE_PLUGIN_OPTION_*` values into runtime env vars, create the appdata directory, ensure the binary is available, and call the binary. It should not own Docker/systemd orchestration, config rewriting, smoke-test policy, or failure classification.

## Codex

Codex uses `plugins/synapse2/.codex-plugin/plugin.json`.

Responsibilities:

- identifies the plugin for Codex listings
- points at shared `skills` and `mcp.json`
- describes the interface shown in Codex UI
- declares read/write capabilities
- provides example prompts
- provides branding fields such as `brandColor`, `composerIcon`, and `logo`

Codex does not use Claude lifecycle hooks. Its manifest should still point to the same MCP server and shared skills so behavior stays aligned with Claude Code.

Codex-specific fields to adapt:

| Field | Purpose |
| --- | --- |
| `interface.displayName` | human-readable plugin name |
| `interface.shortDescription` | short listing text |
| `interface.longDescription` | full listing text |
| `interface.capabilities` | `["Read"]` or `["Read", "Write"]` |
| `interface.defaultPrompt` | three realistic prompts |
| `interface.brandColor` | service-appropriate hex color |

See `plugins/synapse2/.codex-plugin/README.md` for the full manifest field reference.

## Gemini

Gemini uses `plugins/synapse2/gemini-extension.json`.

Responsibilities:

- identifies the extension
- declares Gemini settings
- connects to the MCP HTTP endpoint
- points at shared skills
- optionally points Gemini at a context file with `contextFileName`

The Gemini manifest uses `settings.*` interpolation instead of Claude/Codex `user_config.*` interpolation:

```json
"url": "${settings.server_url}/mcp"
```

Sensitive Gemini settings use:

```json
"secret": true
```

Keep Gemini setting names aligned with Claude/Codex where possible. For example, prefer `server_url`, `api_token`, `<service>_api_url`, and `<service>_api_key` across all three surfaces.

## Plugin Validation

Run the plugin layout validator after changing manifests, MCP config, hooks, or
skills:

```bash
just validate-plugin
# or
scripts/validate-plugin-layout.sh
```

The validator checks:

- Claude, Codex, and Gemini manifests are valid JSON
- plugin manifests do not contain a `version` field
- manifests point to the shared `mcp.json`, hooks, and skills paths
- shared MCP config exposes the `synapse2` HTTP server at `${user_config.server_url}/mcp`
- Gemini config exposes the same `synapse2` HTTP server at `${settings.server_url}/mcp`
- hook config runs `${CLAUDE_PLUGIN_ROOT}/hooks/plugin-setup.sh`
- every skill has `name:` and `description:` frontmatter

Use `PLUGIN_ROOT=plugins/<service>` when validating an adapted service package.

For release checks, `just pre-release` includes this validator and the other
template gates.

## Shared MCP Config

Claude Code and Codex share `plugins/synapse2/mcp.json`:

```json
{
  "mcpServers": {
    "synapse2": {
      "type": "http",
      "url": "${user_config.server_url}/mcp",
      "headers": {
        "Authorization": "Bearer ${user_config.api_token}"
      }
    }
  }
}
```

Gemini carries equivalent MCP config directly in `gemini-extension.json` because its interpolation model is different.

## Skills

`plugins/synapse2/skills/synapse2/SKILL.md` is shared across Claude, Codex, and Gemini. Every skill follows the three-tier fallback pattern — agents try each tier in order and stop when one works:

```markdown
# synapse2 — Claude Code Skill

Use this skill whenever you need to query or manage Synapse2.

## Tier 1: MCP tool (preferred)
Use when the Synapse2 MCP server is configured in your agent.

scout(action="nodes")
flux(action="docker", subaction="info")
scout(action="help")          # always available, no auth required

## Tier 2: CLI binary
Use when MCP is unavailable but the binary is installed in $PATH.

synapse scout nodes --json
synapse flux docker info --json
synapse doctor

Env required for HTTP mode: SYNAPSE_MCP_TOKEN, SYNAPSE_MCP_HOST, SYNAPSE_MCP_PORT

## Tier 3: Direct API (last resort)
Use when neither MCP nor CLI is available.

curl -H "Authorization: Bearer $SYNAPSE_MCP_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"action":"scout.nodes","params":{}}' \
     "http://${SYNAPSE_MCP_HOST:-127.0.0.1}:${SYNAPSE_MCP_PORT:-40080}/v1/synapse2"

## Gotchas
- [service-specific pitfalls go here]
- [e.g. pagination, required headers, rate limits]
```

The skill should also include:

- quick action table (action → description → required params)
- full parameter reference with types
- common workflows (status check → list → inspect)
- response shapes for key actions
- sensitive-value handling notes (never log tokens, etc.)

Do not maintain separate skill docs per host. Update the shared skill when the action surface changes; Claude, Codex, and Gemini all read the same file.

## Binary-Owned Hook Standard

Every Rust server with a Claude plugin should expose:

```bash
<binary> setup plugin-hook
<binary> setup plugin-hook --no-repair
<binary> setup check
<binary> setup repair
```

`setup plugin-hook` should:

- run `setup check` first
- run `setup repair` only when needed and only when `--no-repair` is absent
- emit structured JSON when the global JSON flag is used
- include `exit_policy`, `blocking_failures`, `advisory_failures`, `ran_repair`, and `no_repair`
- exit `0` for success or advisory failures
- exit nonzero for blocking failures
- enforce a bounded total hook runtime

Advisory failures are non-blocking local conditions such as missing `.env` files when process env already supplies values, occupied MCP ports, optional startup proofs, or model prewarm. Blocking failures are prerequisites required for the plugin to function, such as missing appdata directories, missing required upstream credentials, or invalid OAuth/auth configuration.

## Version And Release Sync

Keep version and metadata synchronized across:

| File | Fields |
| --- | --- |
| `Cargo.toml` | package `version`, homepage/repository when present |
| `plugins/synapse2/.claude-plugin/plugin.json` | identity, repository, user config; no `version` field |
| `plugins/synapse2/.codex-plugin/plugin.json` | identity, repository, interface metadata; no `version` field |
| `plugins/synapse2/gemini-extension.json` | identity, repository, settings |
| `server.json` | package version and registry metadata, when present |

`Cargo.toml` is the canonical version source. Use
`scripts/bump-version.sh` to update Cargo and `server.json` together, then use
`scripts/check-version-sync.sh` or `just pre-release` to verify that
version-bearing files still agree. Plugin manifests should remain versionless.

Synapse2 has write-capable `flux` and `scout` actions guarded by confirmation. Keep Codex/Claude/Gemini capability claims synchronized with those guarded write paths.

## Adaptation Checklist

When updating the Synapse2 plugin:

1. Update all three manifests with the current repository, description, author, keywords, and capability claims.
3. Keep credential names aligned across Claude `userConfig`, Codex shared `mcp.json`, and Gemini `settings`.
4. Update `plugins/synapse2/hooks/plugin-setup.sh` to map service-specific plugin options into env vars.
5. Keep `synapse setup plugin-hook`, `--no-repair`, `check`, and `repair` working.
7. Update shared skill docs for the actual action surface.
8. Replace Codex `defaultPrompt` entries with realistic prompts.
9. Update Gemini `description`, `settings`, and `contextFileName` if needed.
10. Run `just validate-plugin` and plugin contract tests before release.

## Required Tests

Each server should include tests that prove:

- Claude hook config points to `hooks/plugin-setup.sh`
- hook script delegates to `<binary> setup plugin-hook`
- `setup plugin-hook --no-repair` parses and does not mutate appdata
- JSON plugin-hook output contains `exit_policy`, `blocking_failures`, `advisory_failures`, `ran_repair`, and `no_repair`
- advisory failures exit `0`
- blocking failures exit nonzero
- Claude, Codex, and Gemini manifests use the same service name, endpoint, token setting, and credential fields

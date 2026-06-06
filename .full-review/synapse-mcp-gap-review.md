# Synapse MCP Original Gap Review

## Scope

Compared the current Rust `synapse2` checkout against the TypeScript original at `jmagar/synapse-mcp` main (`f6b4ea2a45ba5ee2f97546c7ff05a0a4f1f96b46`), cloned to `/tmp/synapse-mcp-review`.

## Findings

### High Priority

- Claude channel notifications are not ported. The original advertises `experimental: { "claude/channel": {} }`, registers Docker event watchers and log tailers, and forwards events as `notifications/claude/channel`. Rust has no equivalent `channel/`, `events/`, watcher, tailer, or notification bridge.
- MCP resource parity is incomplete. Original resources include templated host, stack, and container resources: `synapse://hosts/{host}`, `synapse://hosts/{host}/stacks`, `synapse://stacks`, `synapse://stacks/{host}/{stack}`, `synapse://stacks/{host}/{stack}/env`, `synapse://containers/{host}`, and `synapse://containers/{host}/{id}`. Rust currently exposes schema resources, `synapse://hosts`, `synapse://compose/projects`, and help resources.
- Original root SSH login protection is absent. Original gates `sshUser=root` through elicitation unless `SYNAPSE_ALLOW_ROOT_LOGIN=true`; Rust has destructive operation elicitation but no root-login gate.

### Medium Priority

- Original TOFU fingerprint store is not ported. TypeScript persists fingerprints to `~/.config/synapse/known_hosts.json` and rejects changed fingerprints. Rust uses strict OpenSSH `known_hosts` instead and warns on wildcard entries, which is different operator behavior.
- `SYNAPSE_EXCLUDE_HOSTS` is missing from Rust host discovery.
- `SYNAPSE_MCP_ALLOW_YOLO` is missing. Rust has `SYNAPSE_MCP_ALLOW_DESTRUCTIVE`, but not the original's "skip all confirmation gates" mode.
- `SYNAPSE_DEBUG_ERRORS` is missing. Rust returns sanitized internal tool errors without the original opt-in debug detail mode.
- `scout exec` no longer allows `git`. The original includes `git` in `ALLOWED_READ_COMMANDS` and blocks dangerous `git -c` / `--config` flags. Rust deliberately excludes `git`, so this is a parity gap even if the security tradeoff is intentional.

### Lower Priority / Mostly Covered

- Core flux/scout action families appear present in Rust: docker, container, host, compose, scout simple actions, zfs, logs, and help.
- Host config shape is mostly covered, including `execAllowlist`, SSH config discovery, compose search paths, and local socket fallback.
- Response size truncation is present in Rust.
- Explicit `response_format` handling is present in the current Rust working tree, but the formatter coverage review still flagged lossy markdown fallback for unmapped action results.

# synapse2 — Claude Code instructions

## What this project is

`synapse2` is the Rust MCP and CLI server for local Synapse workflows. It is a
full-parity Rust port of `synapse-mcp`, exposing two MCP tools:

- `flux` for Docker daemon, container, host, and Compose operations.
- `scout` for SSH/local filesystem, process, ZFS, log, transfer, and allowlisted
  command operations.

The binary is named `synapse`. HTTP MCP defaults to `127.0.0.1:40080`, and the
REST compatibility endpoint is `POST /v1/synapse2`.

## Module map

| File | Role |
|------|------|
| `src/app.rs` | `SynapseService` facade over `FluxService` and `ScoutService`; keep it thin. |
| `src/flux_service.rs` | Flux domain root: shared host resolution, Docker cache, Compose discovery, help. |
| `src/flux_service/docker_driver.rs` | Flux Docker driver methods. |
| `src/flux_service/container_driver.rs` | Flux container driver methods. |
| `src/flux_service/host_driver.rs` | Flux host inspection driver methods. |
| `src/flux_service/compose_driver.rs` | Flux Compose driver methods. |
| `src/flux_service/docker.rs` | Pure Docker helper functions and validation. |
| `src/flux_service/container_read.rs` | Pure container read helpers. |
| `src/flux_service/container_lifecycle.rs` | Pure container lifecycle helpers. |
| `src/flux_service/host.rs` | Pure host exec helpers. |
| `src/flux_service/compose_ops.rs` | Pure Compose command-building/result helpers. |
| `src/scout_service.rs` | Scout domain root and shared helpers. |
| `src/scout_service/exec.rs` | Scout exec/emit/beam implementations. |
| `src/scout_service/fs.rs` | Scout filesystem (peek/find/delta) implementations. |
| `src/scout_service/logs.rs` | Scout log retrieval (syslog/journal/dmesg/auth) implementations. |
| `src/scout_service/proc.rs` | Scout process/disk (ps/df) implementations. |
| `src/scout_service/zfs.rs` | Scout ZFS introspection (pools/datasets/snapshots) implementations. |
| `src/actions.rs` | Top-level action metadata, scope constants, `SynapseAction` enum, shared param helpers. |
| `src/actions/dispatch.rs` | `execute_service_action` dispatch, error-type helpers. |
| `src/actions/flux.rs` | Typed flux argument structs and `from_flux_args` parser. |
| `src/actions/scout.rs` | Typed scout argument structs and `from_scout_args` parser. |
| `src/mcp/tools.rs` | MCP shim: parse JSON args, call service, return `Value`. |
| `src/mcp/schemas.rs` | MCP tool JSON schema derived from action metadata. |
| `src/mcp/help.rs` | Topic index and dispatch for `src/mcp/help_topics.rs`. |
| `src/mcp/help_topics.rs` | Full topic help text for every shipped action/subaction. |
| `src/mcp/rmcp_server.rs` | `ServerHandler` impl: tools/resources/prompts and scope checks. |
| `src/mcp/resources.rs` | MCP resource definitions (`synapse://hosts`, `synapse://compose/projects`, etc.). |
| `src/mcp/prompts.rs` | MCP prompt definitions. |
| `src/mcp/response.rs` | MCP response shaping helpers. |
| `src/mcp/transport.rs` | Streamable HTTP transport wiring and session lifecycle. |
| `src/server.rs` and `src/server/routes.rs` | HTTP server state, auth policy, Axum routes. |
| `src/api.rs` | REST compatibility handlers for `/v1/synapse2`, `/health`, `/status`. |
| `src/config.rs` | `Config`, `McpConfig`, `AuthConfig`, dotenv/env/config loading. |
| `src/cli.rs` | CLI entry: mode dispatch, global flags, top-level help. |
| `src/cli/flux.rs` | CLI flux subcommand parsing and dispatch. |
| `src/cli/scout.rs` | CLI scout subcommand parsing and dispatch. |
| `src/cli/doctor.rs` | Pre-flight checks: env, connectivity, config validation. |
| `src/cli/setup.rs` | Interactive first-run / plugin setup wizard. |
| `src/cli/watch.rs` | Polls `/health` and emits state-change lines for plugin monitor. |
| `src/cli/help.rs` | CLI help text rendering. |
| `src/docker_client.rs` | Docker client module entry: re-exports, mode dispatch. |
| `src/docker_client/traits.rs` | `DockerClient` and `DockerImageClient` async trait definitions. |
| `src/docker_client/bollard_client.rs` | Bollard-backed implementation of Docker traits. |
| `src/docker_client/cache.rs` | Per-host Docker client cache with transport-death eviction. |
| `src/docker_client/mock.rs` | `MockDockerClient` for unit tests. |
| `src/ssh.rs` | SSH execution/session pool and forwarded Docker socket support. |
| `src/ssh/` | SSH pool, config, executor, transport implementations. |
| `src/fanout.rs` | Cross-host request fan-out with concurrency cap, timeout, and partial-failure handling. |
| `src/elicitation_gate.rs` | `Confirmer` trait + `MCP`/`Cli`/`NoConfirm`/`DenyConfirm` implementations. |
| `src/cache.rs` | Generic TTL-keyed async cache shared by Docker client and Compose discovery. |
| `src/formatters.rs` | Response formatting helpers (table, JSON, markdown). |
| `src/formatters/` | Per-format and per-action formatter implementations. |
| `src/logging.rs` | Tracing subscriber setup, log format selection, color policy. |
| `src/logging/aurora.rs` | Aurora-themed tracing formatter. |
| `src/logging/formatter.rs` | Generic tracing event formatter. |
| `src/color_policy.rs` | `NO_COLOR`/`FORCE_COLOR` detection and terminal color capability. |
| `src/scaffold.rs` | First-run directory/config scaffolding for bare-metal installs. |
| `src/synapse.rs` | Cross-cutting types and traits shared by flux and scout. |
| `src/compose.rs` | Compose project discovery and caching logic. |
| `src/scout.rs` | Scout domain types and shared SSH execution helpers. |
| `src/docker.rs` | Docker domain types and shared container/image helpers. |
| `src/token_limit.rs` and `src/runtime_budget.rs` | Response byte caps and operation deadlines. |
| `src/host_config.rs` | Shared host topology loading from `SYNAPSE_HOSTS_CONFIG`, `SYNAPSE_CONFIG_FILE`, and `~/.ssh/config`. |
| `src/web.rs` | Optional static web UI: asset serving and SPA fallback. |
| `src/main.rs` | Mode dispatch: HTTP server, stdio MCP, CLI. |
| `src/lib.rs` | Public API plus `testing` helpers for integration tests. |
| `tests/cli_parse.rs` | CLI argument parsing tests. |
| `tests/tool_dispatch.rs` | MCP tool dispatch tests using loopback state. |
| `tests/api_routes.rs` | REST route/auth tests. |

## Thin-shim rule

`src/mcp/tools.rs`, `src/api.rs`, and `src/cli.rs` contain no business logic.
They only parse their input format, call the relevant service method, and render
or return the result. Put validation, filtering, mutation sequencing, fanout,
Docker/SSH behavior, and response shaping in the domain modules.

## No monoliths

Production Rust modules are checked by `scripts/check-rust-module-size.sh` via
lefthook, `just module-size-check`, and CI:

- Soft advisory: 400 real-code lines.
- Hard failure: 1000 real-code lines.
- Test sidecars (`*_tests.rs`) and `tests/` are exempt.

Use sibling modules (`foo.rs` plus `foo/` children) and never create `mod.rs`.
`xtask` and the pattern checker enforce this.

`SynapseService` must remain a thin facade. Add `flux` behavior to
`FluxService` or its focused submodules. Add `scout` behavior to `ScoutService`
or its focused submodules.

## Adding or changing an action

1. Update the relevant domain service:
   - Flux: `src/flux_service/*`
   - Scout: `src/scout_service/*`
2. Update typed action parsing and scope semantics in `src/actions/`.
3. Update MCP dispatch in `src/mcp/tools.rs`.
4. Update CLI parsing and run dispatch in `src/cli.rs` or `src/cli/flux.rs` /
   `src/cli/scout.rs`.
5. Update MCP schema parameters in `src/mcp/schemas.rs`.
6. Update topic help in `src/mcp/help.rs`.
7. Update docs that list actions: `README.md`, `docs/API.md`,
   `docs/MCP_SCHEMA.md`, and `plugins/synapse2/skills/synapse2/`.
8. Add tests:
   - parser coverage in `tests/cli_parse.rs`
   - MCP dispatch coverage in `tests/tool_dispatch.rs`
   - service/helper coverage in the nearest sibling `*_tests.rs`
9. Add a `CHANGELOG.md` entry when the change is user-visible.

The help map is manual. If a new action lacks a `src/mcp/help.rs` topic, live
`help` calls will drift even when schemas compile.

## Auth and scope model

| Policy | When | Effect |
|---|---|---|
| `AuthPolicy::LoopbackDev` | loopback bind or loopback no-auth | No auth middleware; scopes bypassed. |
| `AuthPolicy::TrustedGatewayUnscoped` | `SYNAPSE_NOAUTH=true` on a non-loopback trusted gateway deployment | No local auth middleware; scopes bypassed because the gateway owns authz. |
| `AuthPolicy::Mounted { auth_state: None }` | default non-loopback bearer mode | Static bearer token required. |
| `AuthPolicy::Mounted { auth_state: Some(_) }` | `SYNAPSE_MCP_AUTH_MODE=oauth` | Google OAuth plus RS256 JWT issuance. |

Scopes are `synapse:read` and `synapse:write`; write satisfies read. Public
`help` actions require no scope. Unknown actions fail closed.

Destructive operations use the `Confirmer` gate. MCP uses elicitation, CLI prints
a warning and proceeds, and REST denies confirmation-gated actions unless
`SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true` substitutes `NoConfirm`. Startup refuses
that override on non-loopback binds.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `SYNAPSE_MCP_HOST` | `127.0.0.1` | HTTP bind host. |
| `SYNAPSE_MCP_PORT` | `40080` | HTTP bind port. |
| `SYNAPSE_MCP_SERVER_NAME` | `synapse2` | MCP server name. |
| `SYNAPSE_MCP_NO_AUTH` | `false` | Disable auth for loopback dev only. |
| `SYNAPSE_NOAUTH` | `false` | Trusted gateway no-auth mode. |
| `SYNAPSE_MCP_ALLOW_DESTRUCTIVE` | `false` | Skip destructive confirmation prompts; loopback only. |
| `SYNAPSE_MCP_MAX_CONCURRENCY` | `50` | Global concurrency cap on `/mcp` and `/v1/synapse2`; excess requests queued. `0` = disable. `/health`/`/status` exempt. |
| `SYNAPSE_MCP_TOKEN` | unset | Static bearer token. |
| `SYNAPSE_MCP_ALLOWED_HOSTS` | unset | Extra accepted Host header values. |
| `SYNAPSE_MCP_ALLOWED_ORIGINS` | unset | Extra CORS origins. |
| `SYNAPSE_MCP_PUBLIC_URL` | unset | Public URL for OAuth metadata. |
| `SYNAPSE_MCP_AUTH_MODE` | `bearer` | `bearer` or `oauth`. |
| `SYNAPSE_MCP_GOOGLE_CLIENT_ID` | unset | Google OAuth client id. |
| `SYNAPSE_MCP_GOOGLE_CLIENT_SECRET` | unset | Google OAuth secret. |
| `SYNAPSE_MCP_AUTH_ADMIN_EMAIL` | unset | Bootstrap OAuth admin email. |
| `SYNAPSE_MCP_AUTH_SQLITE_PATH` | `/data/auth.db` | OAuth session/client database path. |
| `SYNAPSE_MCP_AUTH_KEY_PATH` | `/data/auth-jwt.pem` | OAuth JWT signing key path. |
| `SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS` | `3600` | OAuth access-token TTL in seconds. |
| `SYNAPSE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS` | `2592000` | OAuth refresh-token TTL in seconds (30 days). |
| `SYNAPSE_MCP_AUTH_CODE_TTL_SECS` | `300` | OAuth authorization-code TTL in seconds. |
| `SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE` | `10` | OAuth dynamic-registration rate limit. |
| `SYNAPSE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE` | `60` | OAuth authorization rate limit. |
| `SYNAPSE_MCP_AUTH_DISABLE_STATIC_TOKEN_WITH_OAUTH` | `true` | Disable static bearer tokens when OAuth is active. |
| `SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS` | unset | Extra OAuth redirect URI patterns (comma-separated). |
| `SYNAPSE_HOSTS_CONFIG` | unset | Inline host topology JSON. |
| `SYNAPSE_CONFIG_FILE` | unset | Host config file path. |
| `SYNAPSE_HOME` | platform appdata | Appdata root; defaults to `~/.synapse2` outside containers and `/data` in containers. |
| `DOCKER_GID` | unset | Host docker group id; required when the Docker socket is mounted in Docker. |
| `DOCKER_NETWORK` | `mcp` | Docker network name for the production compose stack. |
| `SYNAPSE2_VERSION` | `latest` | Image tag used by `docker-compose.prod.yml`. |
| `SYNAPSE_MCP_HOST_PORT` | `40080` | Host port published to the container's MCP port. |
| `RUST_LOG` | `info` | Tracing filter. |
| `NO_COLOR` | unset | Disable ANSI color in console output when set. |
| `FORCE_COLOR` | unset | Force ANSI color even when stderr is not a TTY. |

See `.env.example`, `config.example.toml`, `docs/CONFIG.md`, and `docs/ENV.md`
for the full runtime contract.

## Build commands

```bash
cargo build --release
cargo test --locked
cargo clippy --locked -- -D warnings
cargo fmt --check

just dev
just test
just lint
just fmt
just gen-token
just health
```

`cargo-llvm-cov` coverage currently needs the direct-rustc workaround on this
host when the local `sccache-wrapper` mishandles `--check-cfg`:

```bash
env -u RUSTC_WRAPPER \
  RUSTC=/home/jmagar/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu/bin/rustc \
  cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info
```

## Test helpers

`src/lib.rs` exports `testing::loopback_state()` and `testing::bearer_state(token)`
behind `features = ["test-support"]` or `cfg(test)`. Use these in integration
tests to build `AppState` without real credentials.

Prefer sibling sidecar tests for production modules (`src/foo_tests.rs` or
`src/foo/bar_tests.rs`). Integration tests belong in `tests/`.

## CLI and MCP parity

Every production MCP action must also be reachable from the CLI, except protocol
concepts such as MCP resources/prompts and elicitation-only flows. Current
production action families:

- `flux docker`: `info`, `df`, `images`, `networks`, `volumes`, `pull`, `build`,
  `rmi`, `prune`
- `flux container`: `list`, `inspect`, `logs`, `stats`, `top`, `search`,
  `start`, `stop`, `restart`, `pause`, `resume`, `pull`, `recreate`, `exec`
- `flux host`: `status`, `info`, `uptime`, `resources`, `services`, `network`,
  `mounts`, `ports`, `doctor`
- `flux compose`: `list`, `status`, `up`, `down`, `restart`, `recreate`,
  `logs`, `build`, `pull`, `refresh`
- `scout`: `nodes`, `peek`, `find`, `ps`, `df`, `delta`, `exec`, `emit`, `beam`,
  `zfs`, `logs`
- `flux help` and `scout help`

Run `cargo test --locked --test parity` after action-surface changes.

## Plugin versioning

Plugin manifests (`.claude-plugin/plugin.json`, `.codex-plugin/plugin.json`,
`gemini-extension.json`) do not contain a `version` field. The marketplace
derives versions from git commits. Do not add `version` or run version-bump
scripts against plugin manifests.

## Common gotchas

- Stdio mode lowers log verbosity so JSON-RPC on stdout is not corrupted.
- `help` is public; all non-help actions require at least `synapse:read`.
- `watch`, `serve`, `mcp`, `doctor`, and `setup` are CLI infrastructure, not MCP
  production actions.
- `scout exec` uses an allowlist and execvp semantics; no shell metacharacters.
- `container exec` takes an argv array and runs inside the target container.
- Read operations may fan out when `host` is omitted; destructive operations
  require explicit targets and confirmation.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

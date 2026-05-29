# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- TEMPLATE: When releasing, move items from [Unreleased] to a new version section.
               Format: ## [X.Y.Z] — YYYY-MM-DD
               Use Added / Changed / Deprecated / Removed / Fixed / Security headers. -->

## [Unreleased]

<!-- TEMPLATE: Add changes here as you work. They move to a version section on release. -->

### Added

- **flux compose operations (B13)** — 10 compose subactions reachable from both MCP (`flux` tool `action=compose`) and CLI (`synapse2 flux compose …`):
  - `list` — run `docker compose ls --format json` on a host; returns discovered projects. Also invalidates the B12 cache via `refresh`.
  - `refresh` — invalidate the B12 compose discovery cache for a host, forcing a re-scan on the next `list`.
  - `status` — `docker compose ps --format json` for a project; optional `service` filter.
  - `up` — `docker compose up -d`. Not destructive (creates, not destroys).
  - `down` — `docker compose down [--volumes]`. **DESTRUCTIVE** — gated via B5 elicitation (`confirmer.require`). `remove_volumes=true` requires `force=true` (validated at service layer before the gate runs, not in the shim).
  - `restart` — `docker compose restart`. **DESTRUCTIVE** — gated via B5 elicitation.
  - `recreate` — `docker compose up -d --force-recreate`. **DESTRUCTIVE** — gated via B5 elicitation.
  - `logs` — `docker compose logs [--tail N] [--since T] [<service>]`. Duration/timestamp forms passed through to docker compose unchanged. Not gated.
  - `build` — `docker compose build [<service>]`. Not gated (parity with synapse-mcp; does not destroy state).
  - `pull` — `docker compose pull [<service>]`. Not gated.
  - All ops resolve the project's compose file via B12's `ComposeDiscovery.list()`, then invoke `docker compose -f <config_file> <subcommand>` over the B11 `HostExec` seam (local or SSH).
- `src/flux_service/compose_ops.rs` — pure per-host compose op functions (`up_on_host`, `down_on_host`, `restart_on_host`, `recreate_on_host`, `status_on_host`, `logs_on_host`, `build_on_host`, `pull_on_host`, `list_on_host`) + `DownArgs` + `validate_down_args` + `ComposeLogOptions`.
- `src/flux_service/compose_ops_tests.rs` — unit tests: argv construction for all 10 subactions, `validate_down_args` cross-field validation (remove_volumes/force), confirmer accept/deny behaviour.
- **flux host full parity (B11)** — 9 host subactions reachable from both MCP (`flux` tool `action=host`) and CLI (`synapse2 flux host …`):
  - `status` — Docker connectivity probe + container count + failed systemd service count (best-effort), fans out across all hosts when `host` unspecified.
  - `info` — `uname -a` output, fans out when `host` unspecified.
  - `uptime` — `uptime` output, fans out when `host` unspecified.
  - `resources` — CPU (load avg from `/proc/loadavg`), memory (`/proc/meminfo`), disk (`df -h`), fans out when `host` unspecified.
  - `services` — `systemctl list-units --type=service --no-pager` with optional `state` and `service` name filters; single-host.
  - `network` — `ip addr show` (falls back to `cat /proc/net/dev`); fans out when `host` unspecified.
  - `mounts` — `df -h` output; single-host.
  - `ports` — container port mappings via bollard with optional `protocol` filter and `limit`/`offset` pagination; single-host.
  - `doctor` — aggregated health checks: `docker`, `containers` (bollard), `resources`, `network`, `services`, `logs` (journald), `processes`; accepts `checks` list to run a subset; single-host.
  - Local hosts (`HostProtocol::Local` / `localhost`) use `std::process::Command`; remote hosts use the SSH pool (execvp-style, no shell).
  - Shell commands are developer-hardcoded — `validate_command` / `EXEC_ALLOWLIST` guard only applies to user-supplied `scout exec` input.
- `src/flux_service/host.rs` — pure per-host functions + `HostExec` seam (`LocalExec` / `RemoteExec`), `CheckResult`/`CheckStatus` types, `strip_systemctl_footer`, `parse_meminfo`, `parse_loadavg`.
- `src/flux_service/host_tests.rs` — 22 unit tests with a `MockExec` returning canned `CommandOutput`; no live SSH server required.
- `HostArgs` params struct in `actions.rs` (mirrors `ContainerArgs`/`DockerArgs` pattern); `dispatch_flux_host` dispatcher.
- `ssh_pool` field on `FluxService` — shared `Arc<SshPool>` for host shell commands.

- **flux docker full parity (B10)** — `info`, `df`, `images` (with `dangling_only`), `networks`, `volumes`, `pull`, `build`, `rmi`, `prune` (target: containers/images/volumes/networks/buildcache/all), via bollard, reachable from MCP (`flux` tool) and CLI. Read-only ops fan out across hosts; `pull`/`build`/`rmi`/`prune` are single-host. `build`/`rmi`/`prune` are gated through the B5 destructive-op elicitation gate (decline → hard error unless `SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true`). `build` shells out to `docker build` (bollard's build needs a streamed tar); all other ops use bollard. New `src/flux_service/docker.rs` with build-context/Dockerfile validation and `PruneTarget` parsing.

- **flux container read-only ops (B8)** — replaced the local-`docker`-CLI stubs for `list`/`inspect`/`logs` with bollard-backed implementations and added `stats`, `top`, and `search`, all reachable from both MCP (`flux` tool) and CLI (`synapse2 flux container …`):
  - `list` — filters: `state` (running/exited/paused/restarting/all), `name_filter`, `image_filter` (case-insensitive substring), `label_filter` (`key=value`, bollard server-side).
  - `logs` — one-shot tail (`follow=false`); `lines` (1–500, default 50), `since`/`until` (ISO 8601, unix seconds, or relative `"1h"`/`"30m"`), `grep` (substring filter on lines), `stream` (stdout/stderr/both).
  - `inspect` — `summary` flag for abbreviated output.
  - `stats` — one-shot resource stats for one container, or all containers on the host(s) when `container_id` is omitted.
  - `top` — running processes (bollard-wrapped `docker top`).
  - `search` — full-text substring match over container name + image + labels (client-side grep, not a bollard server-side filter).
  - Multi-host behavior: `list`/`search`/`stats(no id)` fan out across all configured hosts and return a flat, host-tagged list with a `partial` flag and per-host `errors`; `inspect`/`logs`/`top` target a named host or fan out to find the owning host (first match wins).
  - `response_format` (`markdown`/`json`) is validated at the shim per the B4 contract; output-rendering wiring remains a separate codebase-wide concern (actions return structured JSON today).
- `src/flux_service/container_read.rs` (+ `_tests.rs`) — pure per-host container ops over `&dyn ContainerOps`, fully unit-testable with `MockDockerClient` (no live daemon). Includes `parse_time_spec` for log time ranges.
- `MockDockerClient` gains scriptable `log_frames` / `stats_frames` fields for B8 streaming tests.
- `ContainerArgs` — shared boxed parameter struct for `flux container` subactions, used by both `SynapseAction::FluxContainer` and the CLI `Command`.

## [0.5.0] — 2026-05-28

### Added

- `src/cache.rs` / `src/cache_tests.rs` — generic synchronous `Cache<K, V>` trait and `MemoryCache` implementation: per-entry TTL (default 60s), bounded capacity with LRU eviction (default 10k entries), lazy expiration, and `DashMap`-backed thread safety. Adds the `dashmap` dependency.
- `allow_destructive` config option (`SYNAPSE_MCP_ALLOW_DESTRUCTIVE` env var, default `false`) gating destructive shell operations. Documented in `config.example.toml`.

### Security

- `validate_safe_path` now requires absolute paths and rejects symlinks via `symlink_metadata` before any read — prevents symlink-based arbitrary file reads in world-writable directories.
- Removed `git` from the exec allowlist (`EXEC_ALLOWLIST`).
- The MCP server returns a generic `invalid request` error to unauthenticated callers for unknown actions and scope mismatches, preventing unauthenticated probes from enumerating valid action names.
- The server refuses to start when `SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true` is set on a non-loopback bind address, and warns when enabled on loopback.
- Documented the CORS allowlist policy in `src/server/routes.rs` and `config.example.toml`: auth (bearer/OAuth) is the primary control; CORS is defense-in-depth for browser clients.

### Changed

- Dependency bumps via Dependabot: `serde_json` 1.0.149 → 1.0.150, `EmbarkStudios/cargo-deny-action`, and (web app) `postcss` 8.5.14 → 8.5.15, `@types/react`.

## [0.4.0] — 2026-05-14

### Added

- `.github/workflows/codeql.yml` — CodeQL SAST analysis on push to main and weekly scheduled scan; results surface in the GitHub Security tab.
- `.github/workflows/cargo-deny.yml` — license compliance, duplicate dependency, advisory, and source checks via `cargo-deny`.
- `.github/workflows/msrv.yml` — compiles against the declared `rust-version` to catch MSRV regressions early.

## [0.3.0] — 2026-05-14

### Added

- `src/cli/watch.rs` — `example watch` subcommand for live file-system monitoring.
- `plugins/example/monitors/` — plugin monitor definitions for event-driven automation.
- `plugins/example/gemini-extension.json` — Gemini extension manifest for multi-platform plugin distribution.
- `.github/dependabot.yml` + `.github/workflows/dependabot-auto-merge.yml` — automated dependency updates with auto-merge for minor/patch bumps.
- `scripts/asciicheck.py`, `scripts/check-blob-size.py`, `scripts/check-dependency-updates.sh`, `scripts/check-file-size.sh`, `scripts/check-runtime-current.sh`, `scripts/validate-plugin-layout.sh`, `scripts/blob-size-allowlist.txt` — repository validation and quality scripts.
- `tests/plugin_contract.rs` — plugin contract integration tests.
- `docs/PLUGINS.md` — documentation for the plugin system and distribution model.
- `plugins/README.md`, `plugins/example/README.md`, `plugins/example/CLAUDE.md` — plugin-level documentation and agent guidance.
- `apps/web/README.md`, `xtask/README.md`, `tests/README.md`, `scripts/README.md` — README coverage for every major directory.
- `.claude/` — Claude Code project settings for agent-assisted development.

### Changed

- `plugins/example/hooks/plugin-setup.sh` — significant simplification; reduced from ~500 to ~50 lines by extracting reusable logic and removing duplication.
- `Justfile` — expanded with additional recipes covering plugin validation, script checks, and workflow shortcuts.
- `lefthook.yml` — pre-commit hook additions aligned with new script suite.
- `AGENTS.md`, `CLAUDE.md` — updated agent and AI tooling guidance to reflect current project structure.
- `README.md`, `docs/PATTERNS.md` — documentation refreshed for new scripts and plugin layout.

## [0.2.0] — 2026-05-14

### Changed

- Split `src/mcp.rs` into three focused modules: `src/server.rs` (`AppState`, `AuthPolicy`, `build_auth_layer`), `src/server/routes.rs` (Axum router wiring), and `src/api.rs` (REST API handlers). `src/mcp/` now contains only MCP protocol concerns (tools, schemas, prompts, server handler).
- `mcp/rmcp_server.rs` and `mcp/tools.rs` now import `AppState`/`AuthPolicy` from `crate::server` instead of `super`.
- `allowed_origins` visibility widened from `pub(super)` to `pub` to support cross-module access from `server/routes.rs`.
- Updated `src/lib.rs` and `src/main.rs` to reflect new module layout (`pub mod api`, `pub mod server`).

### Added

- `deny.toml` — `cargo-deny` configuration enforcing license allowlist, banning `openssl`/`openssl-sys`, denying yanked crates, and restricting dependency sources to crates.io and `github.com/jmagar/lab.git`. RUSTSEC-2023-0071 acknowledged with rationale.
- `apps/web/CLAUDE.md` — guidance for using the Aurora design system shadcn registry in the Next.js web app: install commands, token conventions, full component catalog, and usage rules.
- `.git/hooks/pre-commit` — enforces the no-`mod.rs` rule at commit time; blocks any staged `mod.rs` file with a clear error message.
- `docs/PATTERNS.md` updated: §1/§1a module layouts reflect new `server`/`api` structure with all `mod.rs` references removed; §5 auth section headers updated; §45 No mod.rs section now includes the git hook script; §A1/§A2 advanced patterns updated to match actual file locations.

### Removed

- `src/mcp/routes.rs` — moved to `src/server/routes.rs`.
- Several obsolete scripts: `backup.sh`, `check-runtime-current.sh`, `plugin-setup.sh`, `reset-db.sh`, `smoke-test.sh`, `test-check-runtime-current.sh`, `validate-marketplace.sh`.
- `docs/server-json-guide.md` — content superseded by `docs/MCP-REGISTRY-PUBLISH-GUIDE.md`.

## [0.1.0] — 2026-05-13

### Added

- Layered architecture: `ExampleClient` (transport) → `ExampleService` (business logic) → MCP/CLI shims
- Action-based dispatch: single `example` MCP tool with `action` parameter routing
- Both transports: Streamable HTTP (`example serve`) and stdio (`example mcp`)
- Bearer token authentication via `EXAMPLE_MCP_TOKEN`
- Google OAuth authentication via `EXAMPLE_MCP_AUTH_MODE=oauth` (issues RS256 JWTs)
- Loopback/no-auth mode for local development
- MCP elicitation support (`elicit_name` action, spec 2025-06-18) with graceful fallback
- MCP resources: exposes tool schema at `example://schema/mcp-tool`
- MCP prompts: `quick_start` prompt
- CLI with `greet`, `echo`, and `status` subcommands
- Test helpers: `loopback_state()` and `bearer_state()` for credential-free integration tests
- `AuthPolicy` enum making auth choice explicit at construction time
- CORS, Host header validation, request body size limiting built-in
- `resolve_auth_policy_kind()` — refuses to bind `0.0.0.0` without auth (Pattern §27)
- `default_data_dir()` — detects container vs bare-metal, returns `/data` or `~/.example`
- `entrypoint.sh` — Docker entrypoint with permission setup and privilege drop to UID 1000
- `xtask` crate with `dist`, `ci`, `symlink-docs`, `check-env` commands
- `.config/nextest.toml` — nextest configuration with `default` and `ci` profiles
- `taplo.toml` — TOML formatter configuration
- `lefthook.yml` — minimal pre-commit hooks (diff_check, toml_fmt, env_guard)
- `.github/workflows/ci.yml` — CI: fmt, clippy, nextest, taplo, audit, gitleaks
- `.github/workflows/docker-publish.yml` — multi-platform Docker build + Trivy scan
- `.github/workflows/release.yml` — release binaries for linux/amd64 and linux/arm64
- `config.example.toml` — fully annotated config template
- `.env.example` — documented secrets template
- `CHANGELOG.md` following Keep a Changelog format
- Workspace structure: root crate + `xtask/` member
- `symlink-docs` and `symlink-docs-inline` Justfile recipes

# Comprehensive Code Review Report

## Review Target

Full repository review of `/home/jmagar/workspace/synapse2` at `main` commit `f2dcd16` (`docs: save session log`), covering Rust server code, MCP/REST/CLI surfaces, web UI, docs, tests, workflows, scripts, and plugin packaging.

## Executive Summary

Synapse2's Rust core is in comparatively good shape: service boundaries are clean, destructive operations are gated, parsed scope checks are present, and `cargo test --locked` passes. The major risk is adaptation drift: the web UI, release/Docker workflows, installer, and several active docs still describe or operate on the old `example`/`rmcp-template` contract, and the web test suite is currently red because of that drift.

## Findings by Priority

### Critical Issues

- None found in the reviewed phases.

### High Priority

- High — Phase 1/3/4 — `apps/web/lib/template.ts:1`, `apps/web/lib/api.ts:68`, `apps/web/app/page.tsx:49`
  The embedded web UI still targets template actions and `/v1/example`. The current web tests fail because the web action metadata disagrees with generated OpenAPI.
  Fix by replacing the web action model and dashboard/tool-runner calls with Synapse2 `flux`/`scout` REST metadata.

- High — Phase 1/2/4 — `.github/workflows/release.yml:29`
  Release automation packages `example` instead of the real `synapse` binary.
  Fix by setting `BINARY_NAME=synapse` and adding a workflow invariant check against `Cargo.toml`.

- High — Phase 1/2/4 — `.github/workflows/docker-publish.yml:28`
  Docker publishing and Trivy scanning target `ghcr.io/jmagar/example-mcp`.
  Fix by using the Synapse2 image identity and rejecting stale template refs in workflow checks.

- High — Phase 3 — `docs/AUTH.md:17`
  Auth docs still use `example:*`, `EXAMPLE_*`, and `/v1/example`.
  Fix by rewriting the auth guide around `synapse:*`, `SYNAPSE_*`, `/v1/synapse2`, and the current bearer/OAuth behavior.

- High — Phase 3 — `install.sh:27`
  Installer metadata still points to `your-org/example-mcp`, `example`, and `EXAMPLE_MCP_*`.
  Fix by adapting it to Synapse2 or removing it until supported.

### Medium Priority

- Medium — Phase 1 — `src/scout.rs:1`
  Stale MVP `peek` and `exec` helpers remain beside the active `ScoutService` implementation.
  Fix by reducing the module to host helpers or moving the helpers and deleting stale functions.

- Medium — Phase 1/4 — `src/actions/flux.rs:1`, `src/cli/flux.rs:1`, `src/cli/help.rs:1`, `src/config.rs:1`, `src/mcp/help.rs:1`
  Several modules exceed the advisory size budget.
  Fix opportunistically with cohesive submodules.

- Medium — Phase 1/3 — `tests/tool_dispatch.rs:11`
  Direct MCP dispatch coverage does not cover many action families.
  Fix with table-driven action-family dispatch tests.

- Medium — Phase 2 — `src/server.rs:165`
  Static bearer tokens are read-scoped only with no visible write-scoped bearer path.
  Fix by documenting this explicitly or adding separate read/write bearer token configuration.

- Medium — Phase 3 — `docs/ARCHITECTURE.md:18`, `docs/DOCKER.md:41`, `docs/SYSTEMD.md:16`, `docs/ENV.md:18`
  Active docs retain template identifiers and examples.
  Fix by refreshing docs and adding a forbidden-template-identifier docs check.

- Medium — Phase 4 — `src/mcp/rmcp_server.rs:121`
  Response-format validation sits in the protocol server file.
  Fix by keeping it minimal or centralizing validation with shared action parsing.

- Medium — Phase 4 — `apps/web/package.json:21`
  pnpm ignores the current `"pnpm"` override field.
  Fix by moving overrides to supported pnpm configuration.

### Low Priority

- Low — Phase 2/4 — `src/scout_service/fs.rs:52`
  `peek` reads whole allowed files before response truncation.
  Fix with bounded/streaming file reads and remote byte caps.

- Low — Phase 2 — `src/scout_service/exec.rs:139`
  `emit` accepts target paths but ignores them during fanout execution.
  Fix by rejecting path for unsupported modes or applying/documenting cwd behavior consistently.

## Findings by Category

### Architecture and Code Quality

The service-layer split into `FluxService` and `ScoutService` is strong, and MCP/CLI shims mostly remain thin. Current architecture debt is concentrated in stale template/web surfaces, leftover MVP Scout helpers, and several coordination-heavy modules that are over the soft budget.

### Security

Core auth and destructive-operation controls are meaningfully stronger than the surrounding docs suggest. The main security risk is operational: stale Docker/release identities and incorrect auth docs can lead users to trust or configure the wrong artifacts and scopes.

### Performance

No broad runtime performance defect was found. The main concrete performance issue is `peek` reading full allowed files before truncation.

### Testing

Rust coverage is broad and currently green. Web tests are red and accurately catch stale metadata. Direct MCP dispatch coverage should be broadened so action-family drift is caught earlier.

### Documentation

`docs/API.md` and `docs/MCP_SCHEMA.md` are current enough to be useful. Several other active docs still read like template docs and should be refreshed or clearly marked as inherited template reference.

### Standards and Operations

Hard repo gates pass, but release and Docker publish workflows do not meet the operational standard expected for a deployable repo because they target stale artifact identities.

## Recommended Fix Order

1. Fix the web contract drift and restore `cd apps/web && pnpm test`.
2. Fix release and Docker workflow artifact/image identities, then add static invariant tests for both.
3. Refresh auth/deployment docs and either adapt or remove `install.sh`.
4. Add direct MCP dispatch tests for missing action families.
5. Clarify static bearer write-scope policy.
6. Remove stale `src/scout.rs` MVP helpers.
7. Address bounded `peek` reads and `emit` path semantics.
8. Opportunistically split modules over the advisory size budget.

## Residual Risks

- I did not run live destructive Docker/Compose/SSH actions; review relied on code, unit/integration tests, and safe checks.
- I did not run a full release workflow or Docker publish dry-run; workflow findings are static but direct.
- Stale template text is widespread in historical session logs and generic template references; remediation should avoid rewriting archived history and focus on active operator docs.

## Commands Run

- `cargo test --locked` — passed.
- `cd apps/web && pnpm test` — failed in `apps/web/lib/template.test.ts`.
- `python3 scripts/check-openapi.py --check` — passed.
- `python3 scripts/check-schema-docs.py --check` — passed.
- `cargo xtask patterns` — passed hard checks with warnings.
- `scripts/check-rust-module-size.sh` — passed hard gate with advisory warnings.

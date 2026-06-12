# Phase 3: Testing and Documentation

Read first:

- `.full-review/01-quality-architecture.md`
- `.full-review/02-security-performance.md`

## Findings

- High — `apps/web/lib/template.test.ts:27`
  The web metadata drift test fails: `REST_ACTIONS` is still `["greet", "echo", "status", "help"]`, while generated OpenAPI exposes `help`, `flux.docker.*`, `flux.container.list`, `scout.nodes`, `scout.peek`, and `scout.exec`.
  Impact: the web app is known-broken under the current test suite. This is not a missing-test issue; it is a failing accepted contract test.
  Fix: update `apps/web/lib/template.ts`, `apps/web/lib/api.ts`, and the dashboard/tool-runner pages to the Synapse2 REST contract, then rerun `cd apps/web && pnpm test`.

- High — `docs/AUTH.md:17`
  Core auth documentation still uses `example:read`, `example:write`, `EXAMPLE_MCP_TOKEN`, `EXAMPLE_NOAUTH`, and `/v1/example`.
  Impact: operators following the auth guide will configure wrong environment variables and reason about the wrong scope names.
  Fix: rewrite `docs/AUTH.md` for `SYNAPSE_*`, `synapse:*`, `/v1/synapse2`, current bearer/OAuth behavior, and the static-token read-only limitation.

- High — `install.sh:27`
  The installer is still a template stub: `REPO="your-org/example-mcp"`, `BINARY_NAME="example"`, `SERVICE_NAME="example-mcp"`, and `EXAMPLE_MCP_*` override variables.
  Impact: published install instructions or copied installer use would install the wrong binary/service name and fail to fetch Synapse2 release assets.
  Fix: either remove the installer until it is supported or adapt it fully to `jmagar/synapse2`, `synapse`, `synapse2`, and `SYNAPSE_*`.

- Medium — `docs/ARCHITECTURE.md:18`
  Architecture docs still describe `rmcp-template`, `ExampleClient`, `ExampleService`, `/v1/example`, and `example:*` scopes, despite current code having `SynapseService`, `FluxService`, `ScoutService`, and `/v1/synapse2`.
  Impact: contributor onboarding points at obsolete module names and the old one-tool template architecture rather than the two-tool Synapse2 shape.
  Fix: rewrite the architecture guide around `FluxService`/`ScoutService`, typed `SynapseAction`, two MCP tools, REST dotted actions, and current auth policy states.

- Medium — `docs/DOCKER.md:41`, `docs/SYSTEMD.md:16`, `docs/ENV.md:18`, `docs/JUSTFILE.md:22`
  Multiple operational docs are still template-oriented (`example`, `EXAMPLE_*`, `~/.example`, `example-mcp.service`, `target/release/example`).
  Impact: deployment and runtime troubleshooting docs disagree with the shipped binary and configuration.
  Fix: refresh the operational docs in one pass and add a docs grep/invariant test for forbidden template identifiers outside intentional historical/session docs and scaffold examples.

- Medium — `tests/tool_dispatch.rs:11`
  Direct MCP dispatch coverage is thin. `cargo xtask patterns` warns that `container`, `compose`, `peek`, `find`, `df`, `delta`, `emit`, `beam`, `zfs`, and `logs` may be missing action coverage.
  Impact: schema/parser/service parity regressions can survive until broader tests or live smoke tests run.
  Fix: add table-driven MCP dispatch tests for every action family and document explicit exceptions for actions that require live resources or destructive confirmation.

- Medium — `.github/workflows/release.yml:29`, `.github/workflows/docker-publish.yml:28`
  There is no invariant test that compares workflow release names and Docker image refs to the crate binary and repository identity.
  Impact: stale workflow identities survived normal checks and would break release/publish later.
  Fix: extend `tests/template_invariants.rs` or `cargo xtask patterns` to reject stale `BINARY_NAME: example`, `example-mcp`, and mismatched package references in active workflow files.

## Verification Notes

- `cargo test --locked` passed.
- `python3 scripts/check-openapi.py --check` passed.
- `python3 scripts/check-schema-docs.py --check` passed.
- `cargo xtask patterns` passed hard checks but emitted test coverage and module cohesion warnings.
- `cd apps/web && pnpm test` failed in `apps/web/lib/template.test.ts`.

## Documentation Strengths

- `docs/API.md` and `docs/MCP_SCHEMA.md` are substantially aligned with current `flux` and `scout` behavior.
- Generated OpenAPI is current according to `scripts/check-openapi.py --check`.
- Session logs document recent remediation work and can help reconstruct why the current shape exists, though they should not substitute for active docs.

# Phase 1: Code Quality and Architecture

## Findings

- High — `apps/web/lib/template.ts:1`
  The embedded web UI is still architected around the template service rather than Synapse2. `WEB_APP_CONFIG` identifies `serviceName: "example"`, `displayName: "rmcp-template"`, `NEXT_PUBLIC_EXAMPLE_API_BASE_URL`, and `restEndpoint: "/v1/example"` even though the Rust router exposes `/v1/synapse2` and the actual tool model is `flux`/`scout`.
  Impact: the dashboard, API explorer, and tool runner are built on the wrong product contract; users will see invalid actions and calls will target a nonexistent REST path when served by the current binary.
  Fix: replace the web action model with Synapse2 REST metadata (`help`, `flux.docker.*`, `flux.container.list`, `scout.*`), update the public env var name, and keep it generated or checked directly against `docs/generated/openapi.json`.

- High — `apps/web/lib/api.ts:68`
  The typed web client still exports `greet`, `echo`, and `status` helpers for the template `/v1/example` API. `apps/web/app/page.tsx:49` wires dashboard quick actions to those helpers.
  Impact: the first-screen user workflow is not representative of Synapse2 and fails against the backend. This is already detected by `cd apps/web && pnpm test`, which fails because `REST_ACTIONS` is `greet/echo/status/help` while OpenAPI lists the real Synapse2 actions.
  Fix: remove template helper methods and add explicit Synapse2 helpers or a generic action runner that uses real dotted REST actions.

- High — `.github/workflows/release.yml:29`
  The release workflow still sets `BINARY_NAME: example` even though `Cargo.toml` defines the binary as `synapse`. The artifact copy step uses that variable at `.github/workflows/release.yml:109`.
  Impact: tag releases will build the crate but fail when packaging `target/.../release/example`, so release assets and plugin binary updates cannot be trusted.
  Fix: set `BINARY_NAME: synapse`, refresh comments, and add a workflow/static check that validates workflow binary names against `Cargo.toml`.

- High — `.github/workflows/docker-publish.yml:28`
  Docker publishing still targets `ghcr.io/jmagar/example-mcp`, and the Trivy scan checks `ghcr.io/jmagar/example-mcp:latest` at `.github/workflows/docker-publish.yml:104`.
  Impact: successful pushes would publish and scan the wrong image identity, leaving the Synapse2 image unpublished or unscanned under the expected package name.
  Fix: change the image to the Synapse2 package name and add a release/ops invariant test for workflow image references.

- Medium — `src/scout.rs:1`
  `src/scout.rs` still contains an older MVP implementation for `nodes`, `peek`, and `exec` with direct local-only behavior, while the active `ScoutService` implementation lives under `src/scout_service/`. The active code still imports `scout::nodes` and `scout::resolve_host`, but the stale `peek`/`exec` functions remain public within the crate.
  Impact: future changes can accidentally patch or call the wrong Scout implementation, especially because the stale functions perform their own subprocess logic and error text that no longer matches the service-layer policy.
  Fix: reduce `src/scout.rs` to host-resolution/node helpers only, or move those helpers to a clearer module and delete the stale MVP functions.

- Medium — `src/actions/flux.rs:1`, `src/cli/flux.rs:1`, `src/cli/help.rs:1`, `src/config.rs:1`, `src/mcp/help.rs:1`
  The module-size gate reports these production modules over the 400-line soft budget. None exceed the 1000-line hard gate, but several are coordination-heavy files where unrelated parser/help/config concerns are growing together.
  Impact: review and parity changes are harder to reason about, especially in action-dispatch code where schema, CLI, REST, and MCP drift are common failure modes.
  Fix: split by subdomain where cohesive: `actions/flux/{args,parse,dispatch}.rs`, `cli/help/{top_level,flux,scout}.rs`, and `mcp/help/{index,topics}.rs`.

- Medium — `tests/tool_dispatch.rs:11`
  The direct MCP dispatch test suite exercises only `flux help`, `flux docker info`, `scout nodes`, and one denied `scout exec` path. `cargo xtask patterns` warns that `container`, `compose`, `peek`, `find`, `df`, `delta`, `emit`, `beam`, `zfs`, and `logs` may be missing direct tool-dispatch coverage.
  Impact: parser/schema/service drift can pass the focused dispatch suite and only surface through broader tests or live MCP calls.
  Fix: add table-driven dispatch tests for each action family, using mocked service seams or loopback-safe inputs where possible.

## Positive Architecture Notes

- `src/app.rs` is now a thin facade over `FluxService` and `ScoutService`, which matches the repo instruction to avoid a growing `SynapseService` god object.
- `src/mcp/tools.rs` is a thin protocol shim that delegates to typed `SynapseAction` parsing and `execute_service_action`.
- Parsed-action scope derivation in `src/actions.rs` correctly handles mutating `flux` subactions instead of relying only on top-level action names.
- `cargo test --locked`, OpenAPI/schema docs checks, and hard pattern gates pass, so the current issues are not broad Rust compile failures.

## Critical Issues for Phase 2 Context

- The web UI exposes stale template actions and endpoints; phase 2 should treat this as a user-facing contract/integrity problem, not only polish.
- Release and Docker workflows reference stale artifact/image names; phase 2 should assess supply-chain and deployability impact.
- Static bearer tokens appear read-only by construction; phase 2 should check whether write-scope operations have an intended non-OAuth bearer path.

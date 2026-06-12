# Phase 4: Best Practices and Standards

Read first:

- `.full-review/00-scope.md`
- `.full-review/01-quality-architecture.md`
- `.full-review/02-security-performance.md`
- `.full-review/03-testing-documentation.md`

## Findings

- High — `.github/workflows/release.yml:29`
  Release automation violates the repo's own packaging convention by using a stale template binary name. This is a standards/operations issue in addition to a quality issue.
  Impact: tag-driven release readiness cannot be trusted.
  Fix: wire workflow values to current crate metadata or add an invariant checker that fails before merge.

- High — `.github/workflows/docker-publish.yml:28`
  Docker image naming is not synchronized with repository identity. The workflow would publish under `example-mcp` and then scan that stale image.
  Impact: downstream consumers and security tooling may observe a green workflow for the wrong artifact.
  Fix: use a Synapse2 image ref and enforce this with a static workflow contract check.

- High — `apps/web/package.json:2`
  The web package is still named `rmcp-template-web`, and active web code/docs use template env/action names.
  Impact: package metadata, web tests, and app behavior all indicate the web app was not fully adapted as a Synapse2 surface.
  Fix: rename the package and public config to Synapse2, and consider deriving action metadata from OpenAPI instead of duplicating it.

- Medium — `docs/` active guides
  Active documentation frontmatter frequently retains `owner: "rmcp-template"` and `scope: "template"` even in a derived Synapse2 repo.
  Impact: contributors cannot tell which docs are normative for Synapse2 versus inherited template reference material.
  Fix: classify active docs as `service` or move generic template docs into a clearly labeled reference section.

- Medium — `src/mcp/rmcp_server.rs:121`
  `validate_response_format_arg` lives in the protocol server file. `cargo xtask patterns` flags this as suspicious surface logic.
  Impact: the helper is currently protocol glue, not business logic, but this file already handles auth, parsing errors, rendering, and tool result construction; more validation here would erode the thin-boundary rule.
  Fix: keep this helper minimal or move response-format validation into the shared action parsing layer so MCP/REST/CLI semantics stay unified.

- Medium — `apps/web/package.json:21`
  pnpm warns that the `"pnpm"` field is no longer read, so the `next>postcss` override is ignored.
  Impact: dependency hygiene controls are not applied as intended.
  Fix: move overrides to a supported `pnpm-workspace.yaml` or current pnpm configuration location.

- Low — `src/scout_service/fs.rs:52`
  `peek` lacks a streaming/bounded-read API despite the project having runtime response caps.
  Impact: bounded output is good, but bounded IO is a better performance and reliability standard for infrastructure file viewers.
  Fix: use streaming reads and remote byte caps.

## Standards Strengths

- `mod.rs` is absent, matching the repo convention.
- Plugin manifests omit explicit `version` fields.
- Rust test coverage is broad and currently passing.
- The server has explicit auth policy states rather than boolean soup.
- Destructive operations have a service-layer confirmation abstraction shared across surfaces.

# Phase 2: Security and Performance

Read first: `.full-review/01-quality-architecture.md`.

## Findings

- High — `.github/workflows/docker-publish.yml:28`
  The Docker publish pipeline still publishes `ghcr.io/jmagar/example-mcp` and scans that same stale image reference at `.github/workflows/docker-publish.yml:104`.
  Impact: this is a supply-chain integrity risk. A green Docker workflow would not prove that the Synapse2 image was built, published, or scanned under the package consumers expect.
  Fix: update the image reference to the Synapse2 package, then add a static check that fails if workflow image refs contain `example-mcp` or do not match the repo/binary metadata.

- High — `.github/workflows/release.yml:29`
  The release workflow packages `BINARY_NAME=example`, so release artifact generation targets a binary that this crate does not build.
  Impact: plugin consumers can be left with stale bundled binaries while CI/release automation looks present. That undermines operational trust in releases and makes rollback/version verification ambiguous.
  Fix: set `BINARY_NAME=synapse`, run a dry-run or tag workflow validation, and add an invariant check against `Cargo.toml [[bin]]`.

- Medium — `src/server.rs:165`
  Static bearer auth always grants `synapse:read` via `.with_static_token_scopes(vec![crate::actions::READ_SCOPE.into()])`, while write operations require `synapse:write`. OAuth supports both scopes in `src/main.rs:206`, but there is no visible config path for a static bearer token with write scope.
  Impact: this is safe by default, but operationally surprising: the documented plugin bearer-token path cannot call write-scope operations even when the operator has intentionally enabled destructive confirmation. Teams may work around this by switching to `SYNAPSE_NOAUTH=true`, which removes local auth and scope checks entirely.
  Fix: either document bearer tokens as read-only and direct all write automation through OAuth/trusted gateway, or add an explicit scoped-token configuration for separate read/write bearer tokens.

- Medium — `docs/AUTH.md:17`
  Auth documentation still uses `example:read`, `example:write`, `EXAMPLE_MCP_TOKEN`, and `/v1/example` in its core configuration section, while the code uses `synapse:read`, `synapse:write`, `SYNAPSE_MCP_TOKEN`, and `/v1/synapse2`.
  Impact: incorrect security docs can lead operators to configure nonexistent variables or reason about the wrong scopes. This is especially risky around no-auth/trusted-gateway deployments and write operations.
  Fix: refresh `docs/AUTH.md` from the actual `SYNAPSE_*` config and include a clear matrix for bearer, OAuth, loopback, and trusted gateway modes.

- Medium — `apps/web/lib/template.ts:1`
  The web UI publishes stale template action metadata and examples. The problem is not directly exploitable, but it can cause operators to submit invalid calls, misunderstand available write operations, and miss the real scope/confirmation model.
  Impact: misleading administrative UI around infrastructure operations increases the chance of misconfiguration and unsafe manual workarounds.
  Fix: generate the web action list from OpenAPI or the action metadata, and make write/destructive operations visibly distinct from read-only operations.

- Low — `src/scout_service/fs.rs:52`
  File `peek` reads local files with `std::fs::read_to_string` and remote files with `cat`, then relies on later runtime response caps to truncate large payloads. The path/root/sensitive-file checks are good, but IO still reads the whole allowed file before truncation.
  Impact: an allowed but large log/config file can consume memory and request time before the cap is applied.
  Fix: enforce byte/line limits during read (`take`, bounded async reader, `head -c` remotely), and expose `limit`/`offset` or `lines` parameters for `peek`.

- Low — `src/scout_service/exec.rs:139`
  `emit` validates each optional target `path` when parsing `exec`, but the multi-host `emit` implementation currently ignores target paths during fanout (`exec_local_fanout(..., None)` and remote no-cwd behavior). This is safer than using paths unsafely, but it is a security-adjacent contract mismatch.
  Impact: operators may believe commands ran in a constrained directory when they did not, which can change the data exposed by read commands such as `ls`, `du`, or `grep`.
  Fix: either reject `path` for remote/multi-host emit until cwd support exists, or apply local cwd consistently and document remote behavior in the schema/help.

## Security Strengths

- Non-loopback no-auth is refused unless the explicit trusted-gateway policy is selected.
- `SYNAPSE_MCP_ALLOW_DESTRUCTIVE` is refused on non-loopback binds.
- Parsed-action scope checks require `synapse:write` for mutating `flux` subactions.
- Destructive MCP operations use an elicitation gate with timeout; REST hard-denies destructive confirmation unless the explicit override is enabled.
- Scout file reads enforce absolute paths, traversal rejection, sensitive-path rejection, and per-host read roots.
- Command execution uses allowlists and execvp-style argument passing rather than shell wrapping.
- OpenAPI and MCP schema docs are current according to the repo scripts.

## Performance Notes

- `cargo test --locked` passed, including SSH pool, Docker client, action, formatter, and route coverage.
- `scripts/check-rust-module-size.sh` passed its hard gate. The soft-budget warnings are maintainability and review-load risks rather than immediate runtime performance defects.
- `cargo xtask patterns` passed hard checks but warned about missing direct tool dispatch coverage for many action families.

## Critical Issues for Phase 3 Context

- Phase 3 should prioritize the failing web tests and missing dispatch coverage, because both directly allowed stale user-facing contracts to persist.
- Phase 3 should verify docs for auth, architecture, web, release, and Docker publishing against current code, because multiple template identifiers remain in operator-facing material.

# Best Practices and Standards

## Findings

- Medium - `scripts/check-openapi.py:39`
  The OpenAPI generator scrapes `src/actions.rs` with regular expressions instead of consuming structured Rust metadata or a generated JSON contract.
  Impact: action metadata formatting changes can silently drop actions from OpenAPI output if the regex fails to match an `ActionSpec` entry.
  Fix: keep the current script for now, but add stronger validation that every `ActionSpec` with `ActionTransport::Any` appears in the rendered schema and consider a Rust-side contract dump for future work.

- Medium - `docs/generated/openapi.json:167`
  `/v1/example` is documented with `security: [{"BearerAuth": []}]`, while loopback and trusted-gateway deployments intentionally have no local auth. The description mentions this nuance, but the operation-level contract still looks unconditionally bearer-protected.
  Impact: generated clients may always require a bearer token even for local development mode.
  Fix: document alternate security requirements explicitly, for example `security: [{"BearerAuth": []}, {}]`, with description text explaining when unauthenticated local access is valid.

- Low - `src/server/routes.rs:106`
  Invalid configured CORS origins are skipped with a warning instead of failing startup.
  Impact: operators can think an origin is allowed when it was silently omitted.
  Fix: consider validating CORS origins at config-load/startup time in a later hardening pass.

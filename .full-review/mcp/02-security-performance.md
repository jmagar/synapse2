# Phase 2: Security and Performance

## Findings

- High - `src/app.rs:69`, `docs/specs/scaffold-intent-handoff.md:111`, `docs/contracts/scaffold-intent.schema.json:7`
  The side-effect-free `scaffold_intent` action is intended to hand machine-readable JSON to a plugin skill, but malformed user fields can still produce contract-invalid values. The checked-in contract sets `additionalProperties=false`, `minLength`, and identifier patterns; the service does not enforce those constraints for live elicitation input.
  Impact: downstream planning code may receive invalid crate names, binary names, service identifiers, or env prefixes. In a scaffold workflow, malformed identifiers can lead to unsafe filenames, broken generated code, or confusing approval plans.
  Fix: enforce the same identifier and required-field constraints at the service boundary before returning JSON. Invalid input should produce a validation error or safe defaults that satisfy the contract.

- Medium - `src/mcp/rmcp_server.rs:126`
  Internal tool execution errors are mapped to a generic MCP internal error message containing only the action name. This is safe for secrets but currently loses enough detail that operators cannot distinguish upstream failure, service validation mistakes, and unexpected elicitation errors from the MCP response alone.
  Impact: production MCP clients and integration tests have weak failure diagnostics, increasing mean time to repair.
  Fix: keep sensitive details out of client messages, but return a stable error kind/category and correlation-friendly message while logging full details server-side.

- Medium - `src/mcp/transport.rs:61`
  `allowed_origins` accepts `config.allowed_origins` verbatim without the same URL parsing and wildcard filtering applied to `public_url`. This may be intentional for explicit operator configuration, but it lacks a guard against malformed or wildcard origins.
  Impact: a misconfigured template adaptation can silently create a broader CORS policy than intended.
  Fix: validate configured origins with URL parsing and reject or warn on wildcard/invalid values before handing them to the rmcp transport config.

## No Findings

- `require_auth_context` correctly requires `AuthContext` for mounted HTTP policy and bypasses it only for documented loopback/gateway policies.
- Scope satisfaction correctly allows write scope to satisfy read scope and does not allow read scope to satisfy write scope.
- Tool results are compacted and capped through `token_limit::truncate_if_needed`, limiting accidental large MCP responses.
- `scaffold_intent` does not write files, commit, push, install dependencies, or call external project-generation services.

## Critical Issues for Phase 3 Context

- No Critical issues found in this phase.
- Testing should prove live `scaffold_intent` outputs satisfy `docs/contracts/scaffold-intent.schema.json`, including malformed input cases.

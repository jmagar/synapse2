# Phase 1: Code Quality and Architecture

## Findings

- High - `src/mcp/tools.rs:202`
  `elicit_name` contains response construction and name normalization directly in the MCP shim. The project invariant says MCP shims parse protocol input, call `ExampleService`, and return values; business behavior belongs in `src/app.rs`. This also makes the action the only greeting path that cannot share service-level validation or future client behavior.
  Impact: MCP behavior can drift from CLI/service behavior, and new template users may copy business logic into the protocol layer.
  Fix: move the elicited-name response construction into an `ExampleService` method and have `elicit_name` delegate after collecting the MCP-only input.

- High - `src/app.rs:69`, `src/mcp/tools.rs:72`, `docs/contracts/scaffold-intent.schema.json:48`
  `scaffold_intent` normalizes user fields but does not validate that required contract fields are non-empty and pattern-safe. For example, blank `crate_name`, blank `binary_name`, or `env_prefix="bad prefix"` can produce a payload that violates the checked-in JSON contract. The elicitation input schema currently exposes plain `String` fields without constraints, so clients may submit invalid values.
  Impact: the MCP tool can return machine-readable handoff JSON that the repository's own contract rejects, breaking the scaffold-project handoff path.
  Fix: add service-layer validation/sanitization before producing the handoff payload, and cover invalid elicitation/service inputs with tests.

- Medium - `src/mcp/rmcp_server.rs:70`
  `call_tool` extracts `action` and enforces `required_scope_for_action` before the shared parser runs. Unknown actions in mounted auth mode hit the deny scope path and return `forbidden: requires scope: example:__deny__` instead of the parser's `unknown example action` validation error.
  Impact: authenticated clients get a misleading authorization error for typoed action names, and diagnostics differ between loopback and mounted auth policies.
  Fix: parse the action through the shared action parser before scope enforcement, or explicitly distinguish unknown action validation from the deny-scope fallback.

- Medium - `tests/tool_dispatch.rs:11`
  The integration file named for MCP tool dispatch bypasses the MCP dispatcher and calls `AppState.service` directly because `execute_tool` requires a `Peer<RoleServer>`. That leaves the actual MCP adapter path, JSON argument parsing, error mapping, and tool result serialization weakly covered.
  Impact: changes in `call_tool`, `execute_tool`, or tool result conversion can regress while the "tool_dispatch" tests still pass.
  Fix: add MCP-adapter tests for non-elicitation actions using `call_tool`/dispatcher helpers, or expose a testable non-elicitation dispatch path that does not require a live peer.

## Positive Notes

- `src/actions.rs` is the central action/scope source of truth and is reused by MCP schema generation.
- `src/mcp/rmcp_server.rs` cleanly separates tool, resource, prompt, server-info, and auth helper concerns.
- `src/mcp/transport.rs` isolates host/origin calculation and has focused unit tests.
- `scaffold_intent` file mutation boundaries are documented and the core transformation currently lives in `src/app.rs`, which matches the intended architecture.

## Critical Issues for Phase 2 Context

- No Critical issues found in this phase.
- The P1 scaffold intent validation gap should influence the security review because malformed handoff JSON can cross a trust boundary into plugin skill planning.

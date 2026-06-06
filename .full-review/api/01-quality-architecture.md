# Code Quality and Architecture

## Findings

- High - `src/api.rs:127`, `src/actions.rs:77`, `src/example.rs:88`
  The unauthenticated `GET /status` route calls `state.service.status().await`, while the same `status` business action is declared as requiring `example:read` for REST/MCP action dispatch. The route bypasses the action authorization model and delegates to the upstream-facing service method that template consumers are told to replace with real health/status logic.
  Impact: a derived server can accidentally expose read-scoped upstream status data, topology, or expensive upstream checks through a public route.
  Fix: make `/status` return only local, redacted runtime metadata or move a dedicated public-status method into the service layer that is explicitly separate from the read-scoped business action.

- Medium - `src/actions.rs:151`, `src/api_tests.rs:6`, `tests/api_routes.rs:60`
  `ExampleAction::from_rest()` checks `is_rest_action(action)` before parsing the typed action. An omitted action defaults to `""` and is reported as `NotAvailableOverRest` instead of `MissingAction`.
  Impact: REST clients get an inaccurate error for the most basic malformed request, and the current route test only checks that some `error` field exists.
  Fix: parse through `from_params()` first for empty/unknown action handling, then reject only known MCP-only actions as not available over REST.

- Medium - `docs/API.md:44`
  The REST handler example shows business logic in the API shim (`match body.action.as_str()`, direct parameter extraction, and direct `state.service.*` calls). The current code uses `ExampleAction::from_rest()` plus `execute_service_action()`, and the project invariant says API shims must not contain business logic.
  Impact: this template documentation teaches future adaptations to reintroduce the exact architecture violation the code now avoids.
  Fix: replace the stale handler sample with a thin-shim example that delegates to `src/actions.rs` and the service layer.

## Critical Issues for Phase 2 Context

- The public `/status` route bypasses the action scope model and calls service status logic, so the security review must treat it as an authorization and data exposure risk.

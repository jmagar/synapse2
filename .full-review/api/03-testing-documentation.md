# Testing and Documentation

## Findings

- Medium - `tests/api_routes.rs:60`, `src/api_tests.rs:6`
  REST validation tests do not assert the specific missing-action error. They accept any JSON `error`, so the current `NotAvailableOverRest` regression for `{ "params": {} }` passes.
  Impact: clients lose actionable validation errors without test failure.
  Fix: add a route-level assertion that omitted/empty `action` returns HTTP 400 with `action is required`.

- Medium - `tests/api_routes.rs:129`
  The route tests assert that `/status` returns `"status": "ok"` and local metadata, but they do not prove sensitive fields such as `api_url` are absent.
  Impact: a future implementation can reintroduce topology leakage while preserving the current positive assertions.
  Fix: add negative assertions for `api_url` and any credential-bearing fields on `/status`.

- Medium - `tests/api_routes.rs:117`, `scripts/check-openapi.py:320`
  OpenAPI tests check the action enum but not sensitive status fields or drift between public route behavior and documented schemas.
  Impact: stale or unsafe public status contract changes can pass both `cargo test --test api_routes` and `scripts/check-openapi.py --check`.
  Fix: extend the OpenAPI check or route test to assert that `StatusResponse` excludes `api_url`.

- Medium - `docs/API.md:44`
  `docs/API.md` contains a stale handler sample with business logic in the API layer and does not mention the generated `/openapi.json` endpoint in the endpoint table.
  Impact: API consumers and template adapters receive outdated guidance.
  Fix: update the docs to match the current thin dispatch and include `/openapi.json`.

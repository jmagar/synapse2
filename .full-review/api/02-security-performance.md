# Security and Performance

## Findings

- High - `src/api.rs:127`, `src/server/routes.rs:84`, `docs/AUTH.md:135`
  `/status` is public by route design, but it calls `ExampleService::status()`. The action metadata marks `status` as read-scoped, and `ExampleClient::status()` comments acknowledge that real servers will replace the stub with a remote service status call. This creates a split where `POST /v1/example {"action":"status"}` requires auth under `Mounted`, while `GET /status` does not.
  Impact: adapted servers can leak upstream runtime details or perform unauthenticated upstream I/O from a route that operators expect to be redacted and local.
  Fix: ensure public `/status` does not call the read-scoped business action. Return local metadata only, or add a clearly named, redacted public status service method with tests proving secrets/topology are absent.

- Medium - `docs/generated/openapi.json:267`, `scripts/check-openapi.py:251`
  The OpenAPI `StatusResponse` schema still documents `api_url` as a possible status property, even though `src/example.rs:90` explicitly says unauthenticated status must not include sensitive topology such as `api_url`.
  Impact: generated clients and derived-server authors can treat topology exposure as part of the public contract.
  Fix: remove `api_url` from the public status schema and add a check/test that status docs do not advertise redacted topology fields.

- Medium - `src/server/routes.rs:102`, `docs/API.md:110`
  The 64 KiB request body limit is mounted globally, but there is no explicit response-size/token-budget enforcement on REST action responses. MCP responses are capped in `src/mcp/rmcp_server.rs:254`, while REST `api_dispatch()` returns raw `Value`.
  Impact: derived REST actions can return oversized JSON responses despite the documented agent-first output rule, creating context-budget and latency problems.
  Fix: either document REST as uncapped or add a REST response cap compatible with JSON responses.

## Critical Issues for Phase 3 Context

- Tests need to prove that public `/status` does not call the read-scoped business status path and that OpenAPI no longer advertises redacted topology fields.

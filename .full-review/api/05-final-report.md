# Comprehensive Code Review Report

## Review Target

API server surface for `rmcp-template`: REST handlers, Axum routing, auth boundary, OpenAPI generation/docs, and API tests.

## Executive Summary

The API surface mostly follows the template's thin-shim pattern for `POST /v1/example`, but the public `/status` route crosses the action authorization boundary by calling the read-scoped service status path. The generated OpenAPI/docs and tests need tightening so future template adaptations do not reintroduce business logic in API shims or advertise sensitive public status fields.

## Findings by Priority

### Critical Issues

- None found.

### High Priority

- High - Phase 1/2 - `src/api.rs:127`, `src/actions.rs:77`, `src/example.rs:88`
  Public `GET /status` calls `ExampleService::status()` even though the `status` action requires `example:read` through the action metadata. Fix by making public `/status` local/redacted or by adding a dedicated redacted public-status service method.

### Medium Priority

- Medium - Phase 1/3 - `src/actions.rs:151`, `src/api_tests.rs:6`, `tests/api_routes.rs:60`
  Missing REST `action` currently reports "not available over REST" because the transport check runs before typed parsing. Fix parse order and assert the specific missing-action error.

- Medium - Phase 1/3 - `docs/API.md:44`
  API docs show business logic in the REST handler sample and omit `/openapi.json` from the endpoint table. Replace the sample with current thin-shim delegation.

- Medium - Phase 2/3 - `docs/generated/openapi.json:267`, `scripts/check-openapi.py:251`, `tests/api_routes.rs:117`
  OpenAPI still advertises `api_url` on public `StatusResponse`, contradicting the redaction requirement in `src/example.rs`. Remove it and add a check/test.

- Medium - Phase 2 - `src/server/routes.rs:102`, `docs/API.md:110`
  REST responses are not capped while MCP responses are token-limited and docs require agent-first output limits. Either add a REST cap or document REST as uncapped.

- Medium - Phase 4 - `scripts/check-openapi.py:39`
  The OpenAPI generator scrapes Rust metadata with regex. Add stronger validation or later replace with a structured contract dump.

- Medium - Phase 4 - `docs/generated/openapi.json:167`
  `/v1/example` OpenAPI security is documented as unconditionally bearer-protected even though loopback/trusted-gateway modes may be unauthenticated.

### Low Priority

- Low - Phase 4 - `src/server/routes.rs:106`
  Invalid CORS origins are skipped with warnings rather than being rejected at startup.

## Findings by Category

### Architecture and Code Quality

- Public `/status` delegates to a read-scoped business action path.
- Missing-action parsing order produces the wrong validation error.
- API docs show stale business logic in the handler.

### Security

- Public `/status` can expose derived-server upstream status data.
- OpenAPI advertises `api_url` as a public status property.

### Performance

- REST responses do not share the MCP response-size cap.

### Testing

- Missing-action errors are not asserted precisely.
- `/status` redaction and OpenAPI sensitive-field absence are not tested.

### Documentation

- `docs/API.md` is stale.
- OpenAPI security requirements do not clearly model loopback/trusted-gateway unauthenticated deployments.

### Standards and Operations

- Regex-based OpenAPI generation is fragile.
- Invalid CORS origin handling is warning-only.

## Recommended Fix Order

1. Fix public `/status` so it cannot call the read-scoped service status action; add route tests for redaction.
2. Fix REST missing-action parsing and route assertions.
3. Remove `api_url` from OpenAPI `StatusResponse` and update docs/OpenAPI checks.
4. File follow-up work for REST response-size policy and OpenAPI generator hardening.

## Residual Risks

- OAuth-specific REST scope behavior was reviewed through code paths but not exercised with a live OAuth `AuthState`.
- The API review did not remediate CLI/MCP/Web surfaces except where their action metadata crossed the API contract.

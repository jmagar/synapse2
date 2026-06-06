# Review Scope

## Target

API server surface for `rmcp-template` on branch `fix/docker-network-default`.

## Files

- `src/api.rs`
- `src/server.rs`
- `src/server/routes.rs`
- `src/server/routes_tests.rs`
- `src/api_tests.rs`
- `tests/api_routes.rs`
- `docs/API.md`
- `docs/generated/openapi.json`
- `scripts/check-openapi.py`
- Cross-surface API contract references in `src/actions.rs`, `src/app.rs`, `src/example.rs`, `src/mcp/transport.rs`, and API-related docs/tests.

## Review Flags

- Security focus: yes
- Performance critical: no
- Strict mode: yes
- Framework: Rust, Axum, rmcp, lab-auth

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

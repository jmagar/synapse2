# Comprehensive Code Review Report

## Review Target

MCP surface for `rmcp-template`: MCP modules, MCP tests, mcporter harness, MCP/auth/schema/scaffold docs, and closely related action/service code where MCP contracts cross surfaces.

## Executive Summary

No Critical issues were found. The main MCP risks are two High-priority contract/architecture problems: `elicit_name` keeps business response logic inside the MCP shim, and live `scaffold_intent` output can violate the checked-in scaffold handoff schema for malformed elicitation input. Medium findings are mostly test-contract gaps and configuration/error-message hardening work.

## Findings by Priority

### Critical Issues

- None.

### High Priority

- High - `src/mcp/tools.rs:202` (Phase 1, Phase 4)
  `elicit_name` builds greeting responses and normalizes names in the MCP shim instead of delegating that behavior to `ExampleService`.
  Fix by moving response construction into `src/app.rs` and leaving MCP code to collect elicited input and delegate.

- High - `src/app.rs:69`, `src/mcp/tools.rs:72`, `docs/contracts/scaffold-intent.schema.json:48` (Phase 1, Phase 2, Phase 3)
  `scaffold_intent` can return contract-invalid JSON for blank or malformed elicitation fields because live service output is not validated against the handoff contract.
  Fix by validating/sanitizing service input before output and adding runtime-output contract tests.

### Medium Priority

- Medium - `src/mcp/rmcp_server.rs:70` (Phase 1)
  Mounted-auth unknown actions return a deny-scope authorization error instead of the shared parser's unknown-action validation error.
  Fix by parsing or explicitly classifying unknown actions before scope enforcement.

- Medium - `tests/tool_dispatch.rs:11` (Phase 1, Phase 3)
  The MCP tool dispatch integration tests call service methods directly, leaving the actual MCP adapter path undercovered.
  Fix by adding a non-elicitation MCP adapter test harness.

- Medium - `src/mcp/rmcp_server.rs:126` (Phase 2)
  Internal tool errors are too generic for operators and clients to diagnose failure classes.
  Fix by returning stable non-sensitive error kinds/categories while logging full details.

- Medium - `src/mcp/transport.rs:61` (Phase 2, Phase 4)
  Explicit `allowed_origins` are accepted verbatim without URL/wildcard validation.
  Fix by validating configured origins consistently with `public_url` handling.

- Medium - `tests/mcporter/test-mcp.sh:477` (Phase 3)
  Live mcporter coverage skips elicitation fallback behavior for the MCP-only actions.
  Fix by adding fallback coverage or documenting the live-client limitation and covering it below the live harness.

- Medium - `docs/MCP_SCHEMA.md:31` (Phase 3)
  MCP schema docs do not describe prompt/resource drift rules beyond the schema resource URI.
  Fix by expanding docs or adding explicit source/test references for resources and prompts.

- Medium - `src/mcp/schemas.rs:32` (Phase 4)
  The flat single-tool schema cannot express action-specific required fields or elicitation field constraints.
  Fix by documenting action-specific validation or considering generated `oneOf` action schemas.

## Findings by Category

### Architecture and Code Quality

- High: `elicit_name` business behavior belongs in `ExampleService`, not `src/mcp/tools.rs`.
- Medium: mounted-auth unknown actions produce authz-shaped errors before parser validation.

### Security

- High: malformed scaffold intent handoff payloads can cross into plugin planning despite the schema contract.
- Medium: raw configured origins lack validation.

### Performance

- No material performance issues found. Response serialization is compact and capped.

### Testing

- High: live service-generated scaffold intent is not validated against the contract.
- Medium: MCP adapter and elicitation fallback paths need stronger coverage.

### Documentation

- Medium: MCP docs understate resources/prompts drift rules.

### Standards and Operations

- Medium: flat action schema is a pragmatic pattern but should be documented or strengthened for client-side validation.

## Recommended Fix Order

1. Move `elicit_name` response construction to `ExampleService` and add service tests.
2. Validate/sanitize `scaffold_intent` input/output and add runtime contract tests.
3. Add or improve focused MCP adapter tests for non-elicitation calls and invalid actions.
4. Harden configured origin validation and error categories.
5. Expand MCP docs for resources/prompts and action-specific schema limitations.

## Residual Risks

- Elicitation behavior still needs either a real client-level test or a targeted lower-level fallback test after P1 remediation.
- Live mcporter verification depends on a running server and installed `mcporter`.

## Review Commands

- `python3 scripts/check-schema-docs.py --check` - passed.
- `python3 scripts/check-scaffold-intent-contract.py` - passed.

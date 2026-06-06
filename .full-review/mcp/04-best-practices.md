# Phase 4: Best Practices and Standards

## Findings

- High - `src/mcp/tools.rs:202`, `src/app.rs:53`
  The template's own best-practice rule is that MCP and CLI shims delegate business behavior to `ExampleService`. `greet`, `echo`, and `status` follow the rule; `elicit_name` does not.
  Impact: template users get a contradictory example for adding MCP-only workflows.
  Fix: expose a service method for constructing the elicited-name response and keep the MCP function focused on peer interaction and result mapping.

- Medium - `src/mcp/schemas.rs:32`
  The MCP tool schema uses a single flat object with `action`, `name`, and `message`. It cannot express conditional required fields per action or the elicitation field contract. This is a known tradeoff of single-tool action dispatch, but the current schema gives clients little validation help beyond `message.minLength`.
  Impact: clients can send invalid combinations that are only rejected after tool execution starts.
  Fix: consider documenting action-specific validation in the schema description or generating a stricter `oneOf` schema per action while preserving the single-tool dispatch pattern.

- Medium - `src/mcp/transport.rs:77`
  Host and origin helpers are deterministic and tested, but origin validation policy is asymmetric: `public_url` gets parsed and wildcard-filtered, while explicit `allowed_origins` are accepted raw.
  Impact: configuration mistakes can survive local tests and become runtime CORS issues.
  Fix: add validation helpers and tests for malformed/wildcard configured origins.

## Standards Checks

- Plugin manifest versioning was not changed during this review.
- RTK meta commands were not needed.
- Review artifacts were written only under `.full-review/mcp/`.

## Operational Notes

- The live `just test-mcporter` path requires a running server and installed `mcporter`; it is appropriate as a remediation verification gate if MCP behavior changes.
- `just schema-docs-check` and `just scaffold-contract-check` are the relevant fast contract gates for this surface.

# Review Scope

## Target

MCP surface review and P0/P1 remediation planning for `rmcp-template` on branch `fix/docker-network-default`.

## Files

- `src/mcp.rs`
- `src/mcp/tools.rs`
- `src/mcp/schemas.rs`
- `src/mcp/rmcp_server.rs`
- `src/mcp/prompts.rs`
- `src/mcp/transport.rs`
- `src/mcp_tests.rs`
- `src/mcp/*_tests.rs`
- `tests/tool_dispatch.rs`
- `tests/mcporter/test-mcp.sh`
- `docs/MCP_SCHEMA.md`
- `docs/MCPORTER.md`
- `docs/AUTH.md`
- `docs/specs/scaffold-intent-handoff.md`
- `docs/contracts/*`
- Closely related action/service code where MCP contracts cross surfaces: `src/actions.rs`, `src/app.rs`, `src/app_tests.rs`, `src/cli.rs`, `Justfile`, `scripts/check-schema-docs.py`, `scripts/check-scaffold-intent-contract.py`

## Review Flags

- Security focus: yes
- Performance critical: no
- Strict mode: yes
- Framework: Rust, rmcp, Axum HTTP transport

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

## Commands Run During Scope Discovery

- `bd prime` - loaded Beads workflow context.
- `bd search "MCP surface review" --json` - no active matching epic existed.
- `bd create --title "MCP surface review/remediation" ... --type epic --priority 1 --json` - created `rmcp-template-4q7`.
- `python3 scripts/check-schema-docs.py --check` - passed with `schema docs are current`.
- `python3 scripts/check-scaffold-intent-contract.py` - passed with `scaffold intent contract and examples are valid`.

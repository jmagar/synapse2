# Phase 3: Testing and Documentation

## Findings

- High - `src/app_tests.rs:95`, `src/mcp/tools_tests.rs:34`, `docs/contracts/scaffold-intent.schema.json:48`
  Tests only assert that happy-path scaffold intent output includes required root fields. They do not validate service-generated payloads against the full contract, nor do they cover invalid identifiers, blank required fields, duplicate crawl inputs, or pattern violations.
  Impact: the live MCP handoff can drift from the checked-in schema while `just scaffold-contract-check` still passes because it validates static examples, not runtime output.
  Fix: add runtime-output validation tests for scaffold intent and invalid input tests that prove emitted JSON satisfies the contract or returns an error.

- Medium - `tests/tool_dispatch.rs:11`
  MCP integration tests exercise service methods directly rather than the MCP dispatcher. Existing sidecar tests cover schema and scope helpers, but not a full non-elicitation `tools/call` path with request params, auth context behavior, `CallToolResult` serialization, and invalid action mapping.
  Impact: adapter regressions can slip through unit tests and only show up in live mcporter testing.
  Fix: add a focused MCP adapter test harness for `greet`, `echo`, `status`, `help`, missing action, wrong type, and unknown action.

- Medium - `tests/mcporter/test-mcp.sh:477`
  The live mcporter harness covers auth, core actions, and schema resources, but skips elicitation fallback behavior entirely. `elicit_name` and `scaffold_intent` are the distinctive MCP actions in this template, and both have graceful fallback branches for clients without elicitation support.
  Impact: the MCP-only actions can regress even when the live HTTP smoke test passes.
  Fix: add a live or protocol-level test for elicitation-not-supported fallback, or document why the installed test client cannot exercise that branch and cover it with a lower-level test.

- Medium - `docs/MCP_SCHEMA.md:31`
  The generated MCP schema doc is intentionally sparse and says `src/mcp/tools.rs`, README, and plugin skill docs must mention every action, but it does not document prompt/resource contracts beyond the schema resource URI. This is weaker than the implemented MCP surface, which includes resources and prompts.
  Impact: users adapting the template can update tool actions while missing resource/prompt drift.
  Fix: expand generated or maintained MCP docs to include stable resources, prompts, and their drift rules, or point explicitly to the source files and tests that guard them.

## Checks Run

- `python3 scripts/check-schema-docs.py --check` - passed.
- `python3 scripts/check-scaffold-intent-contract.py` - passed.

## Documentation Positives

- `docs/specs/scaffold-intent-handoff.md` clearly describes the approval boundary and no-mutation guarantee.
- `docs/AUTH.md` documents loopback, bearer, OAuth, trusted-gateway, and stdio policy differences.
- `docs/MCPORTER.md` documents the live test harness and its semantic-test philosophy.

# Comprehensive Code Review Report

## Review Target

CLI surface for `rmcp-template`: `src/cli*`, CLI operational modules, CLI docs, `bin/example`, `scripts/generate-cli.sh`, and closely related CLI contracts.

## Executive Summary

No Critical issues were found. The main P1 risks are a checked-in opaque CLI binary, missing CLI parity for the registered `help` action, and an unbounded recursive doctor filesystem walk. The P2 backlog is mostly parser strictness, setup/doctor test depth, stale docs, and generated-CLI hardening.

## Findings by Priority

### Critical Issues

- None.

### High Priority

- High - Phase 1 - `src/cli.rs:27`, `src/actions.rs:65`, `src/actions.rs:91`
  CLI omits the registered `help` action and manually duplicates action coverage, violating the MCP + CLI parity policy for `ActionTransport::Any` actions.

- High - Phase 2 - `src/cli/doctor/checks.rs:111`
  Doctor recursively sizes appdata without symlink or traversal bounds, making a diagnostic command vulnerable to hangs or unexpected large traversals.

- High - Phase 4 - `bin/example:1`
  A 25 MB opaque Linux ELF binary is committed under the CLI surface without source-reviewable provenance in the reviewed files.

### Medium Priority

- Medium - Phase 1 - `src/cli.rs:81`, `src/cli.rs:96`, `src/cli.rs:110`
  Parser accepts unknown trailing args and typoed flags.

- Medium - Phase 1 - `src/main.rs:185`, `src/cli.rs:69`
  CLI usage text is manually maintained outside the parser module.

- Medium - Phase 1 - `src/cli/doctor/checks.rs:111`
  Doctor readiness checks are mixed with recursive inventory work.

- Medium - Phase 1 - `src/cli/setup.rs:90`, `src/cli/doctor.rs:50`
  Setup and doctor duplicate readiness logic with divergent semantics.

- Medium - Phase 2 - `src/cli/setup.rs:238`
  `.env` writer does not escape or reject dotenv-special characters.

- Medium - Phase 2 - `scripts/generate-cli.sh:18`
  Generated-CLI schema cache can skip regeneration after failed schema discovery.

- Medium - Phase 2 - `scripts/generate-cli.sh:29`
  Generated CLI artifacts that embed bearer tokens are not permission-hardened.

- Medium - Phase 3 - `src/cli/setup_tests.rs:1`
  Setup tests miss the public setup behavior and plugin-hook contract.

- Medium - Phase 3 - `src/cli_tests.rs:71`, `tests/cli_parse.rs:41`
  Parser tests do not cover unknown trailing flag rejection.

- Medium - Phase 3 - `src/cli/doctor_tests.rs:1`, `src/cli/doctor/checks_tests.rs:134`
  Doctor tests miss auth reporting, upstream probe behavior, and symlink traversal.

- Medium - Phase 3 - `docs/CONFIG.md:153`, `docs/ENV.md:31`
  Docs claim the default bind host is `0.0.0.0`; code defaults to `127.0.0.1`.

- Medium - Phase 3 - `docs/CONFIG.md:116`, `docs/CONFIG.md:121`
  Config loading snippet references stale fields and helper types.

- Medium - Phase 4 - `src/cli/doctor.rs:118`, `src/cli/setup.rs:34`
  Doctor/setup call `std::process::exit` from library entry points.

- Medium - Phase 4 - `src/cli/doctor/checks.rs:305`, `src/cli/setup.rs:216`
  Port diagnostics check loopback instead of the configured bind host.

- Medium - Phase 4 - `src/main.rs:157`, `src/cli/doctor/checks.rs:334`, `src/cli/setup.rs:170`
  Auth policy checks re-read env instead of using typed `config.mcp.trusted_gateway`.

### Low Priority

- Low - Phase 2 - `src/cli/doctor/checks.rs:236`
  Invalid TLS cert bypass for doctor should remain diagnostic-only and documented.

- Low - Phase 3 - `docs/JUSTFILE.md:83`
  Sample doctor output is stale.

- Low - Phase 4 - `src/cli/watch.rs:42`
  Watch lacks a one-shot probe test seam.

## Findings by Category

### Architecture and Code Quality

Primary issues are manual CLI/action duplication, permissive parsing, and duplicated setup/doctor readiness logic.

### Security

Main security-adjacent issues are unbounded filesystem traversal, unescaped `.env` serialization, and sensitive generated-CLI artifact handling.

### Performance

The doctor recursive appdata walk is the only P1 performance risk; it can make a preflight command scale with arbitrary appdata contents.

### Testing

Setup and doctor have helper-level tests but lack behavioral coverage for their public contracts and operational edge cases.

### Documentation

CLI-adjacent docs have stale host defaults and a stale config loading snippet.

### Standards and Operations

Direct exits from library functions, host-insensitive port checks, and env re-reads instead of typed config make operational behavior harder to test and reason about.

## Recommended Fix Order

1. Remove or replace the checked-in `bin/example` binary.
2. Make doctor directory traversal bounded or remove recursive sizing.
3. Restore CLI parity for the registered `help` action.
4. Reject unknown CLI args/flags and add parser regression tests.
5. Fix typed trusted-gateway usage in CLI/server call sites.
6. Add setup/doctor behavior tests and update stale CLI docs.

## Residual Risks

- This review did not remediate API/MCP/Web issues except where CLI contracts cross them.
- Other agents are active in the repository; shared files should be rechecked in the remediation worktree before edits.
- Live generated-CLI behavior was reviewed from script logic, not from a running MCP server.

## Verification

- Read and reviewed CLI source, sidecar tests, CLI integration tests, docs, `bin/example`, `scripts/generate-cli.sh`, and related contracts.
- Ran `bd prime`.
- Ran `file bin/example`, `ls -lh bin/example`, and `git ls-files -s bin/example`; result: tracked executable, 25 MB, ELF 64-bit x86-64.

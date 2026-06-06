# Phase 4: Best Practices and Standards

Prior phase files read before this pass.

## Findings

- High - `bin/example:1`
  `bin/example` is a checked-in 25 MB ELF executable (`file`: ELF 64-bit x86-64, not stripped). A template repository should avoid opaque platform binaries unless they are deliberately managed release artifacts with provenance, checksums, and LFS policy. Keeping the binary in the CLI surface makes reviews noisy, can go stale relative to source, and expands clone size. Fix by removing it from source control or replacing it with a small documented wrapper; if plugin binaries must be committed, use the existing blob-size allowlist plus documented release provenance and refresh workflow.

- Medium - `src/cli/doctor.rs:118`, `src/cli/setup.rs:34`
  Library-style CLI entry points call `std::process::exit(1)` directly. That makes tests and embedding harder because callers cannot inspect structured exit status. It also makes future command composition brittle. Fix by returning a result enum or exit-code value from doctor/setup and letting `main.rs` exit.

- Medium - `src/cli/doctor/checks.rs:305`, `src/cli/setup.rs:216`
  Port checks bind `127.0.0.1` regardless of the configured MCP host. The real server binds `config.mcp.host:config.mcp.port`, so diagnostics can pass while `example serve` fails for host-specific conflicts, or fail in a way that does not match the actual bind target. Fix by checking the configured bind host or by documenting that the check is loopback-only.

- Medium - `src/main.rs:157`, `src/cli/doctor/checks.rs:334`, `src/cli/setup.rs:170`
  Auth policy callers pass `trusted_gateway_from_env()` or re-read `EXAMPLE_NOAUTH` instead of using `config.mcp.trusted_gateway`, even though `src/server.rs:66` says to prefer typed config when available. This risks divergence between config-file values and env-only diagnostics. Fix call sites to pass `config.mcp.trusted_gateway` after `Config::load()`.

- Low - `src/cli/watch.rs:42`
  `watch` is intentionally long-running and has unit tests for formatting, but no seam for a one-shot probe test. A small injectable probe or max-iterations test hook would improve coverage without changing runtime behavior.

## Severity Counts

- Critical: 0
- High: 1
- Medium: 3
- Low: 1

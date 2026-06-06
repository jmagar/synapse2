# Phase 3: Testing and Documentation

Prior context read from `01-quality-architecture.md` and `02-security-performance.md`.

## Findings

- Medium - `src/cli/setup_tests.rs:1`
  Setup tests exercise only `SetupReport` state transitions and enum distinctness. There is no coverage for `setup_check`, `setup_repair`, `setup_plugin_hook`, `.env` creation permissions, no-repair audit behavior, missing credential failure codes, or auth-mode checks. This leaves the plugin setup contract effectively untested. Fix by adding tempfile-backed setup tests around the public setup behavior or exposing testable report builders that do not call `std::process::exit`.

- Medium - `src/cli_tests.rs:71`, `tests/cli_parse.rs:41`
  Parser tests cover happy paths and bad interval parsing but do not assert unknown trailing flags are rejected for `doctor`, `status`, `greet`, `setup plugin-hook`, or `watch`. This gap let permissive parsing become the accepted contract. Fix by adding rejection tests for extra args and unknown flags before changing parser behavior.

- Medium - `src/cli/doctor_tests.rs:1`, `src/cli/doctor/checks_tests.rs:134`
  Doctor tests cover a few helper functions but not `check_auth_config`, upstream probe behavior, symlinked appdata traversal, or JSON output shape. The comment in `checks_tests.rs:12` says auth is covered by integration tests, but the referenced `tests/tool_dispatch.rs` exercises MCP dispatch, not doctor auth reporting. Fix by adding direct `Config`-based tests for auth modes and bounded filesystem traversal.

- Medium - `docs/CONFIG.md:153`, `docs/ENV.md:31`
  Documentation says the HTTP bind host defaults to `0.0.0.0`, but `src/config.rs:120` defaults to `127.0.0.1`. This is a safety-relevant operational mismatch: users may expect an externally reachable server or misunderstand no-auth behavior. Fix both docs to match the current loopback default.

- Medium - `docs/CONFIG.md:116`, `docs/CONFIG.md:121`
  The config loading snippet is stale relative to `src/config.rs`: it references `config.mcp.public_url`, `config.mcp.noauth`, `config.example.url`, and `env_opt_str` for list fields that do not exist in the current structs. This teaches derived-service authors to copy code that will not compile. Fix the snippet or replace it with a link to the current source.

- Low - `docs/JUSTFILE.md:83`
  The sample doctor output is stale compared with the current renderer: current check names include full path labels such as `Data directory: <path>` and the summary prints `Fix before running:`. This is not blocking, but it reduces operator trust in diagnostics.

## Severity Counts

- Critical: 0
- High: 0
- Medium: 5
- Low: 1

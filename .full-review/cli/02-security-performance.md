# Phase 2: Security and Performance

Prior context read from `01-quality-architecture.md`.

## Findings

- High - `src/cli/doctor/checks.rs:111`
  `dir_size_label()` recursively walks the appdata directory using `entry.metadata()` and follows directories without any depth, entry, byte, or symlink-cycle limit. A user-controlled appdata tree can make `example doctor` hang, traverse unexpectedly large mounted paths, or recurse through symlink loops. This is a denial-of-service risk for a diagnostic command that should be cheap and reliable. Fix by using `symlink_metadata()`, skipping symlinks, and bounding traversal, or remove size calculation from doctor entirely.

- Medium - `src/cli/setup.rs:238`
  `write_env()` writes `.env` lines by simple string interpolation with no dotenv escaping. Values containing newlines, carriage returns, `#`, quotes, or shell metacharacters can produce a malformed `.env` file or inject additional variables into the generated file. The inputs are local operator config, so this is not a remote exploit, but it can corrupt plugin setup and leak incorrect runtime state. Fix by serializing dotenv values with proper quoting/escaping or rejecting newline-bearing values before writing.

- Medium - `scripts/generate-cli.sh:18`
  The schema cache can store or compare the sentinel value `nohash` when the HTTP probe fails. If `dist/.cache/example-cli.schema_hash` already contains `nohash` and `dist/example-cli` exists, a later run skips generation even when the server is unavailable and no real schema was checked. Fix by making failed schema discovery fatal before cache comparison.

- Medium - `scripts/generate-cli.sh:29`
  The generated CLI embeds the current bearer token, but the script only prints a warning and does not set restrictive permissions or write metadata that marks the artifact sensitive. A generated executable under `dist/` can be copied or inspected later without context. Fix by writing with mode `0700`, ensuring `dist/` is ignored, and emitting a clear post-generation path/permission check.

- Low - `src/cli/doctor/checks.rs:236`
  `EXAMPLE_DOCTOR_ACCEPT_INVALID_CERTS=true` disables TLS validation for the upstream reachability check. This is developer-controlled and scoped to doctor, but it should be documented as diagnostic-only and never reused by service clients.

## Severity Counts

- Critical: 0
- High: 1
- Medium: 3
- Low: 1

## Critical Issues for Phase 3 Context

- Tests need to cover symlink-safe or bounded doctor directory traversal.
- Tests need to cover parser rejection of unknown flags and setup `.env` escaping or validation.

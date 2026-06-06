# Review Scope

## Target

CLI surface review for the Rust rmcp template on branch `fix/docker-network-default`.

## Files

- `src/cli.rs`
- `src/cli_tests.rs`
- `src/cli/setup.rs`
- `src/cli/setup_tests.rs`
- `src/cli/watch.rs`
- `src/cli/watch_tests.rs`
- `src/cli/doctor.rs`
- `src/cli/doctor_tests.rs`
- `src/cli/doctor/checks.rs`
- `src/cli/doctor/checks_tests.rs`
- `src/main.rs`
- `src/actions.rs`
- `src/app.rs`
- `src/config.rs`
- `tests/cli_parse.rs`
- `tests/tool_dispatch.rs`
- `bin/example`
- `docs/QUICKSTART.md`
- `docs/CONFIG.md`
- `docs/ENV.md`
- `docs/JUSTFILE.md`
- `scripts/generate-cli.sh`
- closely related CLI recipes and generated-CLI references in `Justfile`, `.gitignore`, and `scripts/README.md`

## Review Flags

- Security focus: yes
- Performance critical: no
- Strict mode: yes
- Framework: Rust, Tokio, rmcp, axum, reqwest

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

# Review Scope

## Target

Full repository review of `/home/jmagar/workspace/synapse2` at `main` commit `f2dcd16` (`docs: save session log`), using the current clean checkout before review artifacts were regenerated.

## Files

- `.github/`
- `apps/web/`
- `bin/`
- `config/`
- `docs/`
- `plugins/synapse2/`
- `scripts/`
- `src/`
- `tests/`
- `xtask/`
- Root manifests and operational files (`Cargo.toml`, `Justfile`, compose files, config examples, changelog, README, lint/test configs)

## Review Flags

- Security focus: yes
- Performance critical: yes
- Strict mode: yes
- Framework: Rust MCP server with rmcp, Axum, lab-auth, Tokio, Docker/SSH operations, Next.js 16 static web UI

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

## Commands Run

- `git status --short --branch` — passed; clean `main...origin/main` before creating this fresh review task/artifacts.
- `bd prime` — passed; loaded project Beads workflow context.
- `bd create --title="Run comprehensive full-repo review" ...` — passed; created `rmcp-template-31a`.
- `bd update rmcp-template-31a --claim --json` — passed; review task is in progress.
- `scripts/check-rust-module-size.sh` — passed hard gate; advisory warning for modules over the 400-line soft budget.
- `cargo test --locked` — passed; 538 lib tests, 4 bin tests, integration tests, and doc tests passed.
- `cd apps/web && pnpm test` — failed; `apps/web/lib/template.test.ts` reports REST/MCP action metadata drift.
- `python3 scripts/check-openapi.py --check` — passed.
- `python3 scripts/check-schema-docs.py --check` — passed.
- `cargo xtask patterns` — passed hard gates; warnings for file-size cohesion, `validate_` helper in MCP server, and missing action coverage in `tests/tool_dispatch.rs`.

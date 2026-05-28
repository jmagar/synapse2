---
date: 2026-05-28 17:02:43 EST
repo: git@github.com:jmagar/synapse2.git
branch: bd-work/synapse2-parity-port
head: 6d311f7
plan: none
agent: Claude
session id: 4668e0a9-ffa5-458a-86e4-d5f2620c6922
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-synapse2/4668e0a9-ffa5-458a-86e4-d5f2620c6922.jsonl
working directory: /home/jmagar/workspace/synapse2
worktree: /home/jmagar/workspace/synapse2 (bd-work/synapse2-parity-port)
pr: none
---

# Session: Security fixes + cache module, version bump 0.4.0 → 0.5.0

## User Request

`/quick-push` — bump version, update changelog, save session, then stage, commit, and push the working-tree changes (security fixes, a new cache module, and dependency bumps) on branch `bd-work/synapse2-parity-port`.

## Session Overview

Packaged an in-progress set of security hardening changes plus a new in-memory cache module for push. Determined a **minor** version bump (`0.4.0 → 0.5.0`) was warranted by the new `pub mod cache` capability and behavior-affecting security changes. Synced all project version-bearing files, regenerated the OpenAPI doc, ran the full test + clippy gates (green), and updated the CHANGELOG.

## Sequence of Events

1. Oriented: confirmed branch is a feature branch (not main), working tree dirty with 12 modified + 2 untracked files.
2. Reviewed diffs (note: `rtk git diff` compressed code bodies, so `cache.rs` was not fully visible at review time — relied on tests/clippy to verify).
3. Read current version `0.4.0` from `Cargo.toml`; confirmed Keep-a-Changelog format with empty `[Unreleased]`.
4. Located version-bearing files; confirmed `openapi.json` derives version from `Cargo.toml` via `scripts/check-openapi.py`.
5. Consulted advisor — confirmed minor bump + version-file set; flagged that `cache.rs` body was unseen and a new `dashmap` dep was added, requiring full `cargo test` + `cargo clippy` (not just `cargo check`).
6. Bumped `Cargo.toml` and `server.json` (version + image tag).
7. Ran `rtk cargo test` (226 passed) and `rtk cargo clippy --all-targets -- -D warnings` (clean); this also updated `Cargo.lock`.
8. Regenerated `docs/generated/openapi.json` via `scripts/check-openapi.py --write`.
9. Verified version sync grep — no stray `0.4.0` in version files.
10. Added `## [0.5.0] — 2026-05-28` section to `CHANGELOG.md`.
11. Saved this session document (pre-staging).

## Key Findings

- `apps/web/package.json` is independently versioned at `0.1.0` — never synced to the crate version (`0.4.0`). Correctly left untouched; it is not part of the project version contract.
- `docs/generated/openapi.json` version is read from `Cargo.toml` (`scripts/check-openapi.py:14,33`) — regenerated rather than hand-edited.
- Plugin manifests carry no `version` field by design (per `CLAUDE.md`); not bumped.
- Security changes in `src/synapse.rs` introduce behavior breaks: `validate_safe_path` now requires absolute paths and rejects symlinks; `git` removed from `EXEC_ALLOWLIST`.

## Technical Decisions

- **Minor bump (0.4.0 → 0.5.0):** new `pub mod cache` (`src/lib.rs`) is a new capability; behavior-affecting security changes also fit a minor bump under 0.x semver. Covers both.
- **Verify with full gates, not `cargo check`:** `cache.rs` body was invisible through `rtk` and pulls in a new `dashmap` dependency; test files were also modified. Ran `cargo test` + `cargo clippy --all-targets`.

## Files Modified

| File | Purpose |
|---|---|
| `Cargo.toml` | version `0.4.0 → 0.5.0` |
| `Cargo.lock` | regenerated (dashmap dep + version) |
| `server.json` | version + OCI image tag `0.4.0 → 0.5.0` |
| `docs/generated/openapi.json` | regenerated; version → `0.5.0` |
| `CHANGELOG.md` | added `[0.5.0]` section (Added / Security / Changed) |
| `src/cache.rs` (new) | `Cache<K,V>` trait + `MemoryCache` (TTL, LRU, DashMap) |
| `src/cache_tests.rs` (new) | cache unit tests |
| `src/config.rs` | `allow_destructive` field + env loading |
| `src/main.rs` | refuse to start if `allow_destructive` on non-loopback bind |
| `src/synapse.rs` | absolute-path + symlink validation; removed `git` from allowlist |
| `src/mcp/rmcp_server.rs` | generic errors for unauth callers (anti-enumeration) |
| `src/server/routes.rs` | CORS policy documentation |
| `src/lib.rs` | `pub mod cache` |
| `config.example.toml` | document `allow_destructive` + CORS notes |
| `src/config_tests.rs`, `src/synapse_tests.rs`, `src/mcp/rmcp_server_tests.rs` | tests for new behavior |

## Commands Executed

- `rtk cargo test` → `226 passed, 8 ignored`
- `rtk cargo clippy --all-targets -- -D warnings` → `No issues found`
- `python3 scripts/check-openapi.py --write` → `wrote docs/generated/openapi.json`
- `git grep -F 0.4.0 -- '*.toml' '*.json' '*.md' ...` → no stray project-version hits

## Behavior Changes (Before/After)

- **Path validation:** before — relative paths and symlinks could be read; after — `validate_safe_path` requires absolute paths and rejects symlinks.
- **Exec allowlist:** before — `git` was allowed; after — `git` removed from `EXEC_ALLOWLIST`.
- **Unauthenticated MCP probes:** before — could distinguish unknown action vs scope error; after — generic `invalid request` (no action enumeration).
- **Destructive ops on non-loopback:** before — no guard; after — server refuses to start when `SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true` on a non-loopback bind.

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `rtk cargo test` | all pass | 226 passed, 8 ignored | ✅ |
| `rtk cargo clippy --all-targets -- -D warnings` | clean | No issues found | ✅ |
| `python3 scripts/check-openapi.py --write` | regenerated | wrote openapi.json (0.5.0) | ✅ |
| version-sync grep | no stray 0.4.0 | none | ✅ |

## Risks and Rollback

- **Risk:** absolute-path requirement may break callers passing relative paths to file actions. Mitigated by tests; acceptable as a security fix.
- **Rollback:** revert the push commit; version files and CHANGELOG revert together since they are in the same commit.

## Decisions Not Taken

- **Major bump:** rejected — project is pre-1.0 (0.x), so behavior breaks land on minor.
- **Bumping `apps/web/package.json`:** rejected — independently versioned, not part of the crate version contract.

## Next Steps

- None started-but-unfinished in this session.
- Follow-on (not started): consider wiring `MemoryCache` into a concrete caller (the new module is added but its integration into request paths was not part of this push).

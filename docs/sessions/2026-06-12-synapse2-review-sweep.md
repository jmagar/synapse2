---
date: 2026-06-12 16:27:22 EDT
repo: git@github.com:jmagar/synapse2.git
branch: main
head: f560870
session id: 80d4059c-aaeb-4665-9dae-48266a42a086
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-synapse2/80d4059c-aaeb-4665-9dae-48266a42a086.jsonl
working directory: /home/jmagar/workspace/synapse2
worktree: /home/jmagar/workspace/synapse2
beads: rmcp-template-8ix, rmcp-template-4ro, rmcp-template-2cp, rmcp-template-ywr, rmcp-template-swc, rmcp-template-vs5, rmcp-template-vj5, rmcp-template-5yx, rmcp-template-0hv
---

# Synapse2 review sweep and merge

## User Request

Address all full-repo review findings for Synapse2, clean up the accidental cross-repo hook-contract audit, quick-push the result, merge it into `main`, and save the session context.

## Session Overview

The review sweep landed in PR #14 and was merged into `main` at `f560870`. The branch fixed eight review findings across auth scope enforcement, REST dotted action checks, operation deadlines, Scout file read constraints, plugin setup binary trust, schema/help/docs drift, OpenAPI/config/prod compose docs, and markdown formatter drift.

After review, `scripts/check-plugin-hook-contract.py` was removed because it audited sibling repos such as Cortex/Gotify and did not belong in Synapse2's repo validation surface. Active docs were updated, `AGENTS.md` was converted to the required `CLAUDE.md` symlink, and a coupled-file false positive was fixed.

## Sequence of Events

1. Created and claimed beads for the eight Lavra review findings.
2. Dispatched two implementation workers over distinct security/auth and docs/runtime/formatter areas.
3. Integrated the workers' changes on `codex/review-sweep-fixes`.
4. Ran local validation: formatting, tests, clippy, schema/OpenAPI checks, plugin layout, whitespace, and release build.
5. Closed the eight review-sweep beads and pushed PR #14.
6. Investigated the failing cross-repo hook-contract check, then removed the script when it proved unrelated to Synapse2.
7. Cleaned active documentation references and converted `AGENTS.md` to a symlink to `CLAUDE.md`.
8. Fixed `scripts/check-coupled-files.sh` so schema source changes can pass when generated schema docs are already current.
9. Re-ran CI-equivalent checks, pushed the amended PR, waited for PR CI to go green, and merged PR #14 into `main`.
10. Audited final repo status: only `main`, one worktree, no open PRs, clean checkout.

## Key Findings

- MCP scope enforcement previously classified top-level Flux actions as read-scoped before parsing mutating subactions.
- REST mounted-auth checks rejected valid dotted action names by checking the raw string instead of the parsed action.
- Several Docker, SSH, compose, and local subprocess paths lacked shared deadlines and service-level byte caps.
- Scout read actions needed safe-root and sensitive-path constraints beyond syntax-only path validation.
- `plugins/synapse2/hooks/plugin-setup.sh` needed to execute the bundled binary path before exposing secrets.
- `scripts/check-plugin-hook-contract.py` was a fleet audit that referenced sibling repos and was not appropriate for Synapse2 PR validation.
- `AGENTS.md` was a regular tracked file that had drifted from `CLAUDE.md`; repo instructions require it to be a symlink.

## Technical Decisions

- Scope checks now derive from parsed `SynapseAction` values so authorization sees destructive subactions.
- Runtime budget handling was centralized in `src/runtime_budget.rs` with tests in `src/runtime_budget_tests.rs`.
- Scout file read policy now uses configured allowed roots and sensitive-path denial instead of allowing arbitrary absolute reads.
- Plugin setup resolves and executes `${CLAUDE_PLUGIN_ROOT}/bin/synapse` directly, avoiding PATH-shadowed binaries.
- The cross-repo hook checker was deleted rather than moved or renamed because this PR was scoped to Synapse2.
- The coupled-file check now accepts unchanged `docs/MCP_SCHEMA.md` when `scripts/check-schema-docs.py --check` proves it is current.

## Files Changed

| Status | Path | Previous path | Purpose | Evidence |
|---|---|---|---|---|
| modified | `README.md` | - | Refresh Synapse2 config and docs contract | Commit `60b424b` |
| modified | `docker-compose.prod.yml` | - | Align production compose auth contract | Commit `60b424b` |
| modified | `docs/CONFIG.md` | - | Document actual `SYNAPSE_MCP_*` settings | Commit `60b424b` |
| modified | `docs/PLUGINS.md` | - | Remove stale hook-checker references | Commit `60b424b` |
| modified | `docs/SCRIPTS.md` | - | Remove deleted script from script inventory | Commit `60b424b` |
| modified | `docs/generated/openapi.json` | - | Refresh OpenAPI identity, route, and scopes | Commit `60b424b` |
| modified | `plugins/synapse2/hooks/plugin-setup.sh` | - | Use bundled binary before exporting secrets | Commit `60b424b` |
| modified | `scripts/README.md` | - | Remove deleted script docs and document coupled-file exception | Commit `60b424b` |
| modified | `scripts/check-coupled-files.sh` | - | Avoid false positive when generated schema docs are current | Commit `60b424b` |
| modified | `scripts/check-openapi.py` | - | Generate current Synapse2 OpenAPI docs | Commit `60b424b` |
| deleted | `scripts/check-plugin-hook-contract.py` | - | Remove cross-repo fleet audit from Synapse2 | Commit `60b424b` |
| type changed | `AGENTS.md` | regular file | Enforce source-of-truth symlink to `CLAUDE.md` | Commit `60b424b` |
| created | `src/runtime_budget.rs` | - | Shared deadlines and output caps | Commit `60b424b` |
| created | `src/runtime_budget_tests.rs` | - | Runtime budget regression tests | Commit `60b424b` |
| modified | `src/actions.rs`, `src/actions/dispatch.rs`, `src/api.rs`, `src/mcp/rmcp_server.rs` | - | Parsed-action scope and REST/MCP dispatch fixes | Commit `60b424b` |
| modified | `src/synapse.rs`, `src/scout_service/fs.rs`, `src/scout_service/exec.rs`, `src/ssh/pool.rs` | - | Scout path policy and runtime deadline/cap plumbing | Commit `60b424b` |
| modified | `src/flux_service/container_lifecycle.rs`, `src/flux_service/docker.rs`, `src/flux_service/host.rs` | - | Runtime budget and output cap integration | Commit `60b424b` |
| modified | `src/formatters/container.rs`, `src/formatters/scout.rs` | - | Render current container/scout log payloads | Commit `60b424b` |
| modified | tests and test siblings | - | Coverage for scope, route, schema, formatter, runtime, and path-policy fixes | Commit `60b424b` |

## Beads Activity

| ID | Title | Actions | Final status | Why it mattered |
|---|---|---|---|---|
| `rmcp-template-8ix` | Require write scope for destructive flux subactions | Claimed and closed | closed | Closed after parsed subaction scope enforcement and validation. |
| `rmcp-template-4ro` | Fix authenticated REST scope checks for dotted action names | Claimed and closed | closed | Closed after REST scope checks used parsed action semantics. |
| `rmcp-template-2cp` | Add operation deadlines and service-level output byte caps | Claimed and closed | closed | Closed after runtime budget module and related plumbing/tests. |
| `rmcp-template-ywr` | Constrain scout file reads to allowed roots or stronger scopes | Claimed and closed | closed | Closed after Scout read root and sensitive-path policy. |
| `rmcp-template-swc` | Invoke bundled synapse binary in plugin setup before exporting secrets to PATH | Claimed and closed | closed | Closed after plugin setup invoked the bundled binary path. |
| `rmcp-template-vs5` | Fix MCP schema/help drift for flux host and compose parameters | Claimed and closed | closed | Closed after schema/help/parser contract updates and tests. |
| `rmcp-template-vj5` | Refresh Synapse2 OpenAPI, config docs, and production compose auth contract | Claimed and closed | closed | Closed after OpenAPI/config/prod compose refresh. |
| `rmcp-template-5yx` | Fix markdown formatter drift for container and scout logs | Claimed and closed | closed | Closed after formatter payload-shape fixes and tests. |
| `rmcp-template-0hv` | Remove stale fleet hook contract audit from Synapse2 | Created, claimed, and closed | closed | Tracked the follow-up cleanup requested after the cross-repo checker confusion. |

## Repository Maintenance

### Plans

`find docs/plans -maxdepth 2 -type f` returned no plan files. No plan moves were needed.

### Beads

The eight review-sweep beads and one cleanup bead were closed. `bd dolt push` completed after merge.

### Worktrees and branches

`git worktree list --porcelain` showed a single worktree at `/home/jmagar/workspace/synapse2` on `main`. `git branch -vv` showed only local `main`, tracking `origin/main`. After merge, `git fetch --prune origin` removed the stale `origin/codex/review-sweep-fixes` remote-tracking ref.

### Stale docs

Active docs references to `scripts/check-plugin-hook-contract.py` were removed from `docs/PLUGINS.md`, `docs/SCRIPTS.md`, and `scripts/README.md`. Historical session notes mentioning the script were intentionally left unchanged because they describe earlier work.

### Transparency

No destructive branch or worktree cleanup was performed beyond the PR merge's remote branch deletion and the subsequent local remote-tracking prune. The final repo status was clean on `main`.

## Tools and Skills Used

- **Skills.** Used `repo-status` for the final live checkout audit and `vibin:save-to-md` for this session artifact.
- **Subagents.** Two implementation workers handled separate slices of the review sweep.
- **Shell/Git/GitHub CLI.** Used `git`, `gh`, `cargo`, `python3`, `bash`, and `bd` for implementation, validation, CI/PR checks, merge, and issue tracking.
- **File tools.** Used patch-based edits for code, docs, script changes, and this session artifact.
- **External services.** GitHub PR #14 and Actions provided CI and merge evidence; Beads/Dolt tracked and pushed issue state.

## Commands Executed

| Command | Result |
|---|---|
| `cargo fmt --check` | Passed before PR merge. |
| `cargo test` | Passed locally with 538 lib tests plus integration/doc tests. |
| `cargo clippy -- -D warnings` | Passed before PR merge. |
| `python3 scripts/check-openapi.py --check` | Passed. |
| `python3 scripts/check-schema-docs.py --check` | Passed. |
| `bash scripts/validate-plugin-layout.sh` | Passed 41 checks. |
| `git diff --check` | Passed. |
| `cargo build --release` | Passed before PR creation. |
| `cargo xtask patterns && cargo xtask check-test-siblings && ...` | Reproduced Template Contracts; initial coupled-file false positive found and fixed. |
| `gh pr merge 14 --merge --delete-branch` | Merged PR #14 and deleted the remote branch. |
| `bd dolt push` | Pushed Beads state. |
| `/home/jmagar/.codex/skills/repo-status/scripts/repo_context.sh --include-gh` | Confirmed clean `main`, one worktree, no open PRs. |

## Errors Encountered

- `scripts/check-plugin-hook-contract.py` failed on a missing sibling `syslog-mcp` hook. Investigation showed the script was a cross-repo fleet audit and not appropriate for Synapse2 validation. The script was removed.
- The first post-cleanup PR CI run failed Template Contracts because `scripts/check-coupled-files.sh` required `docs/MCP_SCHEMA.md` to be changed whenever `src/mcp/schemas.rs` changed, even when generated docs were already current. The guard was fixed to run `scripts/check-schema-docs.py --check` before reporting that issue.
- A direct local `python3 scripts/asciicheck.py` invocation failed because it requires file arguments. CI uses `bash scripts/run-ascii-check.sh`; that wrapper passed.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| MCP auth | Read-scoped callers could pass top-level Flux read checks before mutating subactions were parsed. | Scope is derived from parsed actions, including mutating subactions. |
| REST auth | Valid dotted REST actions could fail mounted-auth checks. | Dotted REST actions are parsed before scope derivation. |
| Runtime budget | Some Docker/SSH/local subprocess paths could hang or buffer too much before final response caps. | Shared deadlines and service-level output caps are applied earlier. |
| Scout reads | Absolute read paths were syntax-validated but broadly available. | Read paths must stay under allowed roots and avoid sensitive paths. |
| Plugin setup | PATH-shadowed `synapse` could receive exported secrets. | The bundled plugin binary is resolved and executed directly. |
| Docs/scripts | Synapse2 carried a cross-repo hook audit and active docs advertised it. | The script and active references are removed. |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test` | Full test suite passes | Passed locally before merge | pass |
| `cargo clippy -- -D warnings` | No clippy warnings | Passed before merge | pass |
| `python3 scripts/check-openapi.py --check` | OpenAPI docs current | Passed | pass |
| `python3 scripts/check-schema-docs.py --check` | MCP schema docs current | Passed | pass |
| `bash scripts/validate-plugin-layout.sh` | Plugin layout valid | Passed 41 checks | pass |
| `bash scripts/check-coupled-files.sh origin/main HEAD` | Coupled-file guard passes | Passed after false-positive fix | pass |
| PR #14 CI | Required PR checks green | CI, MSRV, Template Contracts, Clippy, Web, Cargo Deny, Secret Scan, GitGuardian, and CodeRabbit were green before merge | pass |
| `repo_context.sh --include-gh` | Final repo status clean | `main...origin/main`, no open PRs, one worktree | pass |

## Risks and Rollback

The merged change touches authorization, execution budgets, file-read policy, plugin setup, and deployment docs. Rollback is available by reverting merge commit `f560870` on `main`, though that would also reopen the eight review findings fixed by PR #14.

Post-merge `main` CI was still running for CodeQL and Docker Publish when the session note was prepared. CI and MSRV on `main` had completed successfully for `f560870`.

## Decisions Not Taken

- Did not keep or rename `scripts/check-plugin-hook-contract.py`; it was removed because it audited sibling repos and confused Synapse2 validation.
- Did not edit old `docs/sessions/*` entries mentioning the hook checker; they are historical records.
- Did not create another feature branch for the session note because `save-to-md` commits the generated artifact directly on the current branch by design.

## References

- PR #14: https://github.com/jmagar/synapse2/pull/14
- Merge commit: `f560870cb576356b5e758d86ff8d9284dda161d1`
- Review-sweep branch commit: `60b424b0cbd5a318bc9afb5ca54525c459aa9849`
- Main CI run: https://github.com/jmagar/synapse2/actions/runs/27440958381
- Main MSRV run: https://github.com/jmagar/synapse2/actions/runs/27440958360
- Main CodeQL run: https://github.com/jmagar/synapse2/actions/runs/27440958377
- Main Docker Publish run: https://github.com/jmagar/synapse2/actions/runs/27440958388

## Open Questions

- CodeQL and Docker Publish for merge commit `f560870` were still in progress at the time this note was created.

## Next Steps

1. Check the remaining post-merge `main` workflows:
   - `gh run list --branch main --limit 8`
2. If Docker Publish succeeds and release deployment is desired, rebuild/sync the release binary and verify the container runtime.
3. If another whole-repo review is requested, start from current `main` at `f560870` rather than the pre-merge PR branch.

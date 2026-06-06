# Phase 1: Code Quality and Architecture

## Context

Reviewed the current dirty diff rather than the stale prior `.full-review` scope. The existing artifacts referenced an older output-format change; this pass covers the current Docker dedupe, remote build, CLI parser, plugin-skill, and destructive-smoke documentation changes.

## Findings

- Medium — `src/flux_service.rs:118`
  `target_docker_hosts()` folds daemon-ID discovery, duplicate suppression, Docker client acquisition, and error swallowing into host target resolution. That makes every all-host Docker read path depend on a preflight `docker info` probe before the actual operation, while the real fanout still handles client acquisition and per-host errors separately. The boundary is now doing both target selection and live Docker I/O, which makes behavior harder to reason about and test than the previous pure `target_hosts()` call.
  Impact: future Docker read actions can inherit latency and partial-failure behavior by calling the target helper, even when the action itself already has a fanout/error model.
  Fix: either dedupe after the real fanout using daemon IDs already returned by `docker info`, or split daemon discovery into a small, tested helper that returns typed `{ host, daemon_id }` records and preserves the original fanout error semantics explicitly.

- Low — `src/flux_service.rs:129`
  The daemon ID extraction relies on a JSON pointer into `docker::info_on_host()` output (`/info/ID`). This currently matches Bollard's `SystemInfo` serde rename, but it couples target-selection logic to a presentation-shaped `serde_json::Value`.
  Impact: a future output-shape cleanup in `info_on_host()` could silently disable dedupe or change which aliases survive.
  Fix: add a typed helper such as `docker::daemon_id(client).await -> Result<Option<String>>`, or at minimum add a regression test that proves duplicate aliases with the same `ID` collapse to one host.

- Low — `src/cli/flux.rs:70`
  The `split_command_argv()` parser correctly fixes `--command sh -c ...`, but its contract is implicit: every option after `--command` is treated as container argv, including `--timeout` or `--response-format`. That is a reasonable CLI convention, but it is easy for future parser edits to accidentally reintroduce named-value parsing after `--command`.
  Impact: parser behavior is safe and tested for `-c`, but only one placement is covered.
  Fix: expand CLI parser tests to cover `--command sh --flag`, options before `--command`, and the intentional rule that Synapse options must appear before `--command`.

## Positive Notes

- `src/flux_service/docker.rs:412` moves `docker build` through the existing `HostExec` seam, which is consistent with compose build and avoids incorrectly forcing remote-host builds through the local Docker CLI.
- The `docker build` argv is constructed without a shell and still uses the existing build-context and Dockerfile validators.
- `tests/cli_parse.rs:117` adds a focused regression test for the reported `--command sh -c ...` parsing bug.
- `docs/CLI_DESTRUCTIVE_SMOKE.md` documents broad prune hazards and local-only destructive smoke-test expectations instead of pretending those flows are CI-safe.

## Verification

- `git diff --check` — passed.
- `cargo test container_exec_command_accepts_flags_after_command --test cli_parse` — passed.

## Critical Issues for Phase 2 Context

- The new all-host Docker dedupe helper performs live Docker `info` calls before the actual operation. Security/performance review should check whether that creates hidden privilege, latency, or failure-mode risks.

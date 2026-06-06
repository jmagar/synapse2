# Phase 3: Testing and Documentation

## Prior Phase Context

Phase 1 found architecture concerns around `target_docker_hosts()` mixing target selection with live Docker daemon probes, JSON-shaped daemon ID extraction, and limited parser contract coverage. Phase 2 found a performance concern: all-host Docker reads were doing a serial daemon-ID preflight before normal fanout.

## Testing Findings

### Remediated During Integration

- Medium - `src/flux_service.rs:118`
  Original finding: there was no direct regression test for Docker host deduplication.
  Remediation: `src/flux_service_tests.rs` now covers duplicate daemon IDs and the fallback contract that failed or missing daemon-ID discovery keeps the host. The implementation also uses `fanout` for daemon discovery instead of a serial loop.

- Medium - `src/flux_service/docker.rs:412`
  Original finding: the new `docker build` execution path was not directly covered by a mock `HostExec` test.
  Remediation: `src/flux_service/docker_tests.rs` now records `HostExec` calls and asserts `docker build` argv construction, `--no-cache`, Dockerfile path construction, host tagging, success mapping, and stdout propagation.

- Low - `src/cli/flux.rs:70` and `tests/cli_parse.rs:117`
  Original finding: parser coverage only covered the reported `--command sh -c ...` case.
  Remediation: `tests/cli_parse.rs` now covers double-dash command argv, Synapse options before `--command`, and the intentional rule that option-looking tokens after `--command` remain container argv.

## Documentation Findings

### Remediated During Integration

- Medium - `plugins/synapse2/skills/synapse2/SKILL.md:95`
  Original finding: the skill broadly warned against `sh -c`, contradicting valid `container exec` argv usage and the destructive smoke route.
  Remediation: the gotcha now distinguishes `container exec` literal argv from `scout exec` host commands, keeping the no-shell warning scoped to `scout exec`.

- Low - `docs/CLI_DESTRUCTIVE_SMOKE.md:151`
  Original finding: the historical findings section described the parser bug without saying it had been fixed.
  Remediation: the smoke doc now includes a current-status note after the historical parser finding.

## Positive Notes

- `tests/cli_parse.rs` now covers the original parser regression plus option ordering and command-argv edge cases.
- `src/flux_service/docker_tests.rs` uses a focused `HostExec` recording test instead of requiring a live Docker daemon.
- `docs/CLI_DESTRUCTIVE_SMOKE.md` correctly frames destructive smoke testing as local operator validation, not CI, and warns that Docker prune APIs are not label-scoped.

## Verification

- `git diff --check` - passed during the review lane.
- `cargo test --test cli_parse` - passed during the review lane.
- `cargo test flux_service::docker::tests` - passed during the review lane.
- `cargo test flux_service::tests` - passed during the review lane.
- `cargo fmt --check` - passed during the review lane.
- `cargo clippy -- -D warnings` - passed during the review lane.

## Critical Issues for Phase 4 Context

- The originally missing daemon-dedupe, build-exec, parser-contract, and documentation fixes were addressed during integration. Remaining risk is live Docker/SSH behavior, which was not exercised by this review.

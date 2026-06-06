# Comprehensive Code Review Report

## Review Target

Current uncommitted diff in `/home/jmagar/workspace/synapse2` on `main`, based on `59514f8 chore: remove scaffold-project skill; add ZFS triggers to description`.

Reviewed files:

- `plugins/synapse2/skills/synapse2/SKILL.md`
- `src/cli/flux.rs`
- `src/flux_service.rs`
- `src/flux_service/container_driver.rs`
- `src/flux_service/docker.rs`
- `src/flux_service/docker_driver.rs`
- `src/flux_service/docker_tests.rs`
- `src/flux_service_tests.rs`
- `tests/cli_parse.rs`
- `docs/CLI_DESTRUCTIVE_SMOKE.md`

## Executive Summary

No critical security or correctness issues were found in the scoped diff. Initial review found risk in the new all-host Docker dedupe path because it performed serial daemon discovery and lacked direct regression coverage; the integration pass changed discovery to use concurrent `fanout`, added a typed daemon-ID helper, and added focused tests for dedupe, fallback, parser, and build-exec behavior.

The parser fix for `container exec --command sh -c ...` is sound and now tested beyond the reported case. The remote `docker build` direction matches existing execution seams and has mock-backed coverage for argv construction and result mapping. Read-only live validation with the current `target/debug/synapse` binary confirmed local and all-host Docker reads behave as expected, including deduping `local` when it points at the same daemon as `dookie`.

## Findings by Priority

### Critical Issues

- None.

### High Priority

- None.

### Medium Priority

- Remediated - Phase 2 / Performance - `src/flux_service.rs:126`
  Original issue: all-host Docker reads serially probed each configured host with `docker info` before fanout.
  Resolution: daemon discovery now uses the existing `fanout` helper, so probes run concurrently and preserve the normal partial-failure shape.

- Remediated - Phase 1 / Architecture - `src/flux_service.rs:118`
  Original issue: daemon discovery was embedded inline and coupled to JSON output.
  Resolution: daemon ID extraction now lives in typed `docker::daemon_id()` over `SystemOps`, and dedupe policy is isolated in tested helpers.

- Remediated - Phase 3 / Testing - `src/flux_service.rs:118`
  Original issue: no direct daemon-dedupe tests.
  Resolution: `src/flux_service_tests.rs` covers duplicate daemon IDs and keeps hosts when daemon discovery fails or returns `None`.

- Remediated - Phase 3 / Testing - `src/flux_service/docker.rs:412`
  Original issue: no direct mock coverage for `docker build` through `HostExec`.
  Resolution: `src/flux_service/docker_tests.rs` now records the `HostExec` call and asserts program, argv, host tagging, success, and stdout mapping.

- Remediated - Phase 3 / Documentation - `plugins/synapse2/skills/synapse2/SKILL.md:95`
  Original issue: the skill broadly discouraged `sh -c`, contradicting valid `container exec` argv use.
  Resolution: the skill now scopes the no-shell warning to `scout exec` and documents that `container exec` passes literal argv.

### Low Priority

- Remediated - Phase 1 / Architecture - `src/flux_service.rs:129`
  Original issue: daemon ID extraction depended on `serde_json::Value` pointer `/info/ID`.
  Resolution: `docker::daemon_id()` now reads typed `SystemInfo.id`.

- Remediated - Phase 1 / Testing - `src/cli/flux.rs:70`
  Original issue: parser coverage did not include options before `--command` or flag-like argv after it.
  Resolution: `tests/cli_parse.rs` now covers those cases.

- Remediated - Phase 3 / Documentation - `docs/CLI_DESTRUCTIVE_SMOKE.md:151`
  Original issue: the historical findings section described the parser bug without a current-status note.
  Resolution: the smoke doc now states the parser behavior is fixed.

- Remediated - Phase 4 / Standards - `src/flux_service.rs:127`
  Original issue: daemon discovery fallback behavior was not explicit enough.
  Resolution: unit tests now lock the "unknown daemon ID keeps the host" contract.

- Remediated - Phase 4 / Standards - `plugins/synapse2/skills/synapse2/SKILL.md:3`
  Original issue: the skill frontmatter description was long and mixed trigger phrases with operational instruction.
  Resolution: the frontmatter is now concise trigger-oriented metadata, with direct SSH/Docker fallback guidance moved into the Tier 1 body.

## Findings by Category

### Architecture and Code Quality

The CLI parser fix is localized and keeps business behavior out of the shim. The initial architecture concern around `target_docker_hosts()` was reduced by typed daemon discovery, concurrent fanout, and unit tests for dedupe policy. It still performs Docker I/O as part of target preparation, but that behavior is explicit and covered for the intended fallback semantics.

### Security

No direct vulnerabilities were found. The parser treats post-`--command` tokens as literal argv and does not introduce shell interpolation. `docker build` constructs argv without shell concatenation, and build context validation remains in place.

### Performance

The serial daemon-ID preflight concern was remediated by running daemon discovery through `fanout`. Read-only live validation found all-host `docker info` completing in about 3.10s, `docker images` in about 7.50s, and `container list` in about 7.61s with partial errors limited to Docker-unavailable hosts. A short-TTL daemon-ID cache was considered and intentionally skipped because the current fanout behavior is acceptable and a cache would mainly help repeated calls in a long-lived server process.

### Testing

Targeted parser coverage exists and passes. The integration pass added coverage for daemon dedupe semantics and the `HostExec` docker build handoff.

### Documentation

The destructive smoke route is useful and appropriately warns that prune is broad and not label-scoped. The skill gotcha around `sh -c` was scoped so it does not contradict valid container exec usage and the smoke route.

### Standards and Operations

`cargo fmt --check`, `cargo clippy -- -D warnings`, `git diff --check`, and targeted tests passed during review-lane verification. The destructive smoke route is correctly documented as local operator validation rather than CI.

## Recommended Fix Order

1. Optionally run the destructive smoke route from `docs/CLI_DESTRUCTIVE_SMOKE.md` when operationally safe.
2. Reconsider a short-TTL daemon-ID cache only if repeated all-host reads in the long-lived server process show measurable daemon-discovery overhead.

## Verification

- `git diff --check` - passed during the review lane.
- `cargo test --test cli_parse` - passed during the review lane before integration updates.
- `cargo test flux_service::docker::tests` - passed during the review lane before integration updates.
- `cargo test flux_service::tests` - passed during the review lane before integration updates.
- `cargo fmt --check` - passed during the review lane.
- `cargo clippy -- -D warnings` - passed during the review lane.
- `cargo build --quiet` - passed during live-safe validation.
- `target/debug/synapse flux docker info --host local --response-format json` - passed in about 0.01s; one local daemon result.
- `target/debug/synapse flux docker df --host local --response-format json` - passed in about 7.64s; one local daemon result.
- `target/debug/synapse flux docker images --host local --response-format json` - passed in about 0.09s; 21 local images.
- `target/debug/synapse flux container list --host local --response-format json` - passed in about 0.12s; 21 local containers.
- `target/debug/synapse flux docker info --response-format json` - passed in about 3.10s; 6 successful daemon hosts, `local` deduped against `dookie`, and Docker-unavailable hosts reported as partial errors.
- `target/debug/synapse flux docker images --response-format json` - passed in about 7.50s; 161 images across successful hosts.
- `target/debug/synapse flux container list --response-format json` - passed in about 7.61s; 118 containers across successful hosts.
- `target/debug/synapse flux docker df --response-format json` - passed in about 39.43s; heavy Docker disk-usage path with the same Docker-unavailable partial errors.

No destructive smoke commands were run.

## Residual Risks

- The review did not execute destructive smoke flows.
- Docker disk-usage reads can still be slow because `docker df` itself is heavy on the current inventory; this is not evidence of serial daemon discovery.

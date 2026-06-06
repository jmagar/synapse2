# Phase 4: Best Practices and Standards

## Prior Phase Context

Phases 1-3 found no critical issues. The main recurring concern was boundary behavior: all-host Docker reads perform daemon-ID discovery, and `docker build` runs through the host execution seam. The integration pass moved daemon-ID extraction into a typed helper, made discovery concurrent through `fanout`, and added mock-backed tests for the boundary behavior.

## Findings

### Remediated During Integration

- Medium - `src/flux_service.rs:118`
  Original finding: `target_docker_hosts()` reached directly into `DockerClientCache`, called live `docker info`, and consumed presentation-shaped JSON inline.
  Remediation: daemon discovery now uses typed `docker::daemon_id()` over the existing `SystemOps` trait, discovery runs through `fanout`, and dedupe policy is covered by unit tests.

- Low - `src/flux_service.rs:127`
  Original finding: discovery errors were intentionally swallowed, but the fallback contract was not covered by tests.
  Remediation: `src/flux_service_tests.rs` now verifies that failed or missing daemon-ID discovery keeps the host rather than deduping it away.

- Low - `plugins/synapse2/skills/synapse2/SKILL.md:3`
  Original finding: the plugin skill frontmatter description was long and mixed trigger phrases with operational instruction.
  Remediation: the frontmatter is now concise trigger-oriented metadata, and the direct SSH/Docker fallback guidance lives in the Tier 1 body with the existing critical gotchas.

## Standards Checks

- Rust style: `cargo fmt --check` passed during the review lane.
- Rust linting: `cargo clippy -- -D warnings` passed during the review lane.
- Live-safe validation: read-only `target/debug/synapse flux docker ...` commands validated local and all-host Docker reads without destructive smoke operations.
- Shim boundary: the CLI parser remains a thin parser and delegates business behavior to `FluxService`; no business logic was added to the CLI beyond argument partitioning.
- Execution safety: `docker build` still constructs argv without shell concatenation and now uses the established `HostExec` seam.
- Destructive operation standard: destructive smoke documentation explicitly avoids CI and highlights non-label-scoped prune behavior.
- Plugin manifest standard: no plugin manifest `version` field was added in the reviewed diff.

## Positive Notes

- Moving `docker build` through `HostExec` aligns it with compose and host command execution patterns.
- The parser helper is documented in code and localized to the container CLI parsing path.
- The destructive smoke document sets an appropriate operational boundary by separating local validation from CI automation and now clarifies that the historical `container exec` parser bug is fixed.

## Verification

- `git diff --check` - passed during the review lane.
- `cargo fmt --check` - passed during the review lane.
- `cargo clippy -- -D warnings` - passed during the review lane.
- `cargo test --test cli_parse` - passed during the review lane.
- `cargo test flux_service::docker::tests` - passed during the review lane.
- `cargo test flux_service::tests` - passed during the review lane.
- `cargo build --quiet` - passed during live-safe validation.
- `target/debug/synapse flux docker info --host local --response-format json` - passed in about 0.01s.
- `target/debug/synapse flux docker info --response-format json` - passed in about 3.10s; `local` was deduped against `dookie`, and Docker-unavailable hosts were returned as partial errors.
- `target/debug/synapse flux docker images --response-format json` - passed in about 7.50s.
- `target/debug/synapse flux container list --response-format json` - passed in about 7.61s.

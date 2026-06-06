# Review Scope

## Target

Current uncommitted diff in `/home/jmagar/workspace/synapse2` on `main`, based on `59514f8 chore: remove scaffold-project skill; add ZFS triggers to description`.

The diff primarily changes:

- `flux container exec` CLI parsing so arguments after `--command` are treated as container argv.
- Docker all-host fanout target resolution so duplicate aliases to the same Docker daemon are deduped by daemon ID.
- `docker build` execution so builds run through the selected host execution seam instead of always spawning local `docker`.
- Synapse plugin skill trigger wording.
- Destructive CLI smoke-test documentation.

## Files

- `plugins/synapse2/skills/synapse2/SKILL.md`
- `src/cli/flux.rs`
- `src/flux_service.rs`
- `src/flux_service/container_driver.rs`
- `src/flux_service/docker.rs`
- `src/flux_service/docker_driver.rs`
- `tests/cli_parse.rs`
- `docs/CLI_DESTRUCTIVE_SMOKE.md`

## Review Flags

- Security focus: yes
- Performance critical: no
- Strict mode: no
- Framework: Rust CLI/MCP server

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

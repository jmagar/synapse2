# Phase 2: Security and Performance

## Prior Phase Context

Phase 1 found no architectural breakage, but did identify that `target_docker_hosts()` now performs live Docker daemon discovery before normal all-host fanout.

## Security Findings

No direct security vulnerabilities found in the reviewed diff.

Notes:

- `src/cli/flux.rs:70` treats tokens after `--command` as literal exec argv and no longer tries to parse `-c` or `--flag` as Synapse flags. That is the expected behavior for container exec and does not introduce shell interpolation.
- `src/flux_service/docker.rs:417` builds a Docker CLI argv vector and passes it through `HostExec::run("docker", &args)`, so user-controlled build fields are not shell-concatenated.
- `src/flux_service/docker.rs:389` still validates build context and Dockerfile inputs before the remote/local build execution path.
- `docs/CLI_DESTRUCTIVE_SMOKE.md` correctly warns that Docker prune operations are broad and not label-scoped.

## Performance Findings

- Medium — `src/flux_service.rs:126`
  All-host Docker read operations now serially probe each configured host with `docker info` before running the actual fanout. The affected paths include `docker info`, `docker df`, `docker images`, `docker networks`, `docker volumes`, `container list`, `container search`, and all-container `container stats`. After that serial preflight, most paths immediately perform another Docker operation through the normal fanout. On slow SSH hosts or unreachable Docker sockets, this can add one timeout per host before any results are emitted.
  Impact: all-host Docker reads can become noticeably slower and less responsive, especially in homelab inventories where one configured host is offline or has a slow SSH-forwarded Docker socket.
  Fix: perform daemon-ID discovery concurrently with the same fanout helper, cache daemon IDs with a short TTL in `DockerClientCache`, or dedupe after collecting the real operation results where the action already returns daemon-identifying data.

## Verification

- `git diff --check` — passed.
- `cargo test container_exec_command_accepts_flags_after_command --test cli_parse` — passed.
- Checked Bollard's generated `SystemInfo` model in the local cargo registry: `id` serializes as `ID`, so `/info/ID` currently points at the expected field.

## Critical Issues for Phase 3 Context

- There is targeted parser coverage for the `--command sh -c ...` regression, but no direct test coverage for Docker host dedupe behavior, dedupe failure semantics, or the remote `docker build` execution path.

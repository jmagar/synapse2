# Phase 1: Code Quality and Architecture

## Findings

- High - `src/cli.rs:27`, `src/actions.rs:65`, `src/actions.rs:91`
  CLI command coverage is manually duplicated from the action registry, and the CLI omits the `help` action even though `ACTION_SPECS` declares it as `ActionTransport::Any`. The template policy says every business action must have MCP and CLI parity, while the current CLI only exposes `greet`, `echo`, and `status` as service-backed commands. This makes the CLI contract drift-prone as actions are added. Fix by driving normal CLI action dispatch through `ExampleAction` or adding an explicit `help` CLI command with parity tests.

- Medium - `src/cli.rs:81`, `src/cli.rs:96`, `src/cli.rs:110`
  Several parser arms silently ignore unexpected trailing arguments and misspelled flags. Examples: `example status --json`, `example doctor --bogus`, and `example setup plugin-hook --no-reapir` all parse as valid commands or no-op variants. Operational commands should reject unknown flags because typos can change behavior, especially setup audit/repair commands. Fix by validating every command's full remainder and returning an error for unknown or extra arguments.

- Medium - `src/main.rs:185`, `src/cli.rs:69`
  Usage/help text is owned by `main.rs`, while parse/command definitions live in `src/cli.rs`. The comment in `src/cli.rs:68` tells contributors to update `print_usage()` manually. This is a known drift point for a template whose required policy is surface parity. Fix by moving command usage metadata into the CLI module or by testing `--help` output against parseable commands.

- Medium - `src/cli/doctor/checks.rs:111`
  The doctor check for directory writability also performs recursive directory sizing in the CLI check function. That mixes a side-effect readiness check with potentially expensive filesystem inventory logic and creates a hidden performance dependency on arbitrary appdata contents. Fix by removing recursive sizing from readiness checks or making it bounded and symlink-safe.

- Medium - `src/cli/setup.rs:90`, `src/cli/doctor.rs:50`
  `setup` and `doctor` each implement overlapping readiness checks for config, auth, port availability, appdata, and credentials. The two paths already diverge in severity semantics: setup treats missing `.env` as advisory and missing upstream values as blocking, while doctor checks upstream reachability and directory size. Fix by extracting shared CLI readiness primitives or documenting why setup and doctor intentionally differ.

## Positives

- `src/cli.rs:132` keeps normal command dispatch thin by constructing `ExampleService` and delegating to service methods.
- `src/app.rs:68` keeps scaffold handoff transformation in the service layer, not in MCP or CLI shims.
- Sidecar tests exist for parser, watch, setup, and doctor helpers.

## Severity Counts

- Critical: 0
- High: 1
- Medium: 4
- Low: 0

## Critical Issues for Phase 2 Context

- The recursive appdata sizing in `src/cli/doctor/checks.rs:111` needs security/performance review because appdata can contain symlinks, large trees, or mounted paths.
- Parser permissiveness in `src/cli.rs` affects setup and doctor operations, not just harmless business-action commands.

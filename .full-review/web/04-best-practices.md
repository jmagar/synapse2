# Phase 4: Best Practices and Standards

## Findings

- High - `apps/web/components/dashboard/action-button.tsx:3-21`, `apps/web/components/tools/param-input.tsx:31-64`
  The app-specific dashboard/tool components use raw `<button>` and `<input>` controls plus imperative DOM style mutation. This conflicts with `apps/web/CLAUDE.md`'s own Aurora rule to avoid raw controls outside UI component wrappers, and the `ParamInput` header claims CSS focus handling while the implementation uses `onFocus`/`onBlur` mutation.
  Impact: focus/hover behavior is harder to test, easier to break under React concurrent rendering, and inconsistent with the template's design-system guidance.
  Fix by using the existing `components/ui/button.tsx` and `components/ui/input.tsx` wrappers, or moving these wrappers into `components/ui` with class-based focus/hover states.

- Medium - `scripts/build-web.sh:14-17`
  The web build script runs `pnpm install` without `--frozen-lockfile` when `node_modules` is absent. Docker correctly uses `pnpm install --frozen-lockfile` in `config/Dockerfile:28-32`.
  Impact: local/release-helper builds can update dependency resolution differently from CI/Docker and mask lockfile drift.
  Fix by using `pnpm install --frozen-lockfile` in the script, with an explicit message if the lockfile needs updating.

- Medium - `scripts/web-watch.sh:13-26`
  The watch script message says it builds once and then watches, but the command only delegates to `watchexec`; there is no explicit initial `pnpm build` before the watcher starts.
  Impact: developers can start the watcher from a clean `apps/web/out` and assume embedded assets exist before any file change triggers a build.
  Fix by calling `bash scripts/build-web.sh` once before starting `watchexec`, then keep the current watch command for subsequent rebuilds.

## Standards Notes

- Plugin manifests were not modified and no `version` fields were added.
- No REST/API/CLI/MCP remediation is recommended except where the web surface consumes REST status/error contracts.

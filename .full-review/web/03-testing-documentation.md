# Phase 3: Testing and Documentation

## Findings

- Medium - `src/web_tests.rs:4-55`
  The web Rust tests only verify `web_assets_available()` is callable and MIME mapping helpers return constants. They do not call `serve_web_assets`, inspect status/cache headers, or cover Next static export route shapes.
  Impact: the `/tools/` and `/api/` fallback regression was not caught, and future cache/header behavior can regress silently.
  Fix by adding async Axum request tests for exact, no-slash, trailing-slash, missing-path fallback, and cache-control behavior.

- Medium - `apps/web/lib/api.ts:39-48`
  There are no unit tests for the browser API client around non-OK JSON, non-JSON, empty-body, or network-error responses.
  Impact: the client can regress from actionable HTTP errors back to opaque parse/network messages without a failing test.
  Fix with a focused test harness around `apiFetch` or an exported parser helper that covers those response classes.

- Medium - `docs/WEB.md:86-104`
  The web guide documents a `build.rs` auto-build flow, but this repository has no `build.rs`. The actual build paths are `Justfile:43-53`, `scripts/build-web.sh`, and `config/Dockerfile:21-32`.
  Impact: contributors can believe `cargo build` will generate `apps/web/out`, when in reality the binary embeds whatever is already present at compile time.
  Fix by replacing the stale `build.rs` section with the actual `just build-web`, `just build-full`, and Docker web-stage behavior.

- Medium - `apps/web/README.md:21-25`, `apps/web/package.json:28-43`
  The README claims TypeScript 5 and `lucide-react`, while the package uses TypeScript 6 and does not declare `lucide-react`.
  Impact: scaffold users may add icons or pin tooling based on stale docs and get dependency/type drift.
  Fix by updating the stack table or adding the missing dependency if icons are intentionally required.

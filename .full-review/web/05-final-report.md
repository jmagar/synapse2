# Comprehensive Code Review Report

## Review Target

Web UI surface for `rmcp-template`: `apps/web/**`, `src/web.rs`, `src/web_tests.rs`, `docs/WEB.md`, `scripts/build-web.sh`, `scripts/web-watch.sh`, and closely related web build/config contracts.

## Executive Summary

The web surface builds successfully, but it has two P1 runtime/quality issues: embedded trailing-slash routes can serve the wrong page, and the configured Biome gate currently fails. The remaining issues are P2 gaps around API error parsing, stale docs, tests, caching, metadata parity, and build-script reproducibility.

## Findings by Priority

### Critical Issues

- None.

### High Priority

- High - Phase 1 - `src/web.rs:50-57`
  Trailing-slash static export routes such as `/tools/` miss `tools/index.html` and fall back to root `index.html`.

- High - Phase 1 - `apps/web/app/api/page.tsx:3`, `apps/web/app/page.tsx:3-7`, `apps/web/app/tools/page.tsx:3-8`, `apps/web/app/layout.tsx:1-5`
  `pnpm -C apps/web check` fails on formatting/import organization/type-only imports.

- High - Phase 2 - `apps/web/lib/api.ts:39-48`
  Browser API client parses JSON before checking status/content type and hides actionable HTTP errors behind parse/network messages.

- High - Phase 4 - `apps/web/components/dashboard/action-button.tsx:3-21`, `apps/web/components/tools/param-input.tsx:31-64`
  App components use raw controls and imperative style mutation despite the local Aurora/UI wrapper convention.

### Medium Priority

- Medium - Phase 1 - `apps/web/lib/template.ts:36-149`
  Web action metadata can drift from Rust action metadata/OpenAPI because there is no parity guard.

- Medium - Phase 2 - `src/web.rs:62-67`
  Immutable caching is applied to all non-index files, including potentially non-hashed static export route artifacts.

- Medium - Phase 2 - `apps/web/lib/template.ts:6`, `apps/web/lib/template.ts:152-154`
  API base URL concatenation does not normalize trailing slashes.

- Medium - Phase 3 - `src/web_tests.rs:4-55`
  Rust web tests do not exercise `serve_web_assets`, route fallback, or cache headers.

- Medium - Phase 3 - `apps/web/lib/api.ts:39-48`
  Browser API client error parsing has no focused tests.

- Medium - Phase 3 - `docs/WEB.md:86-104`
  Docs describe a nonexistent `build.rs` auto-build path.

- Medium - Phase 3 - `apps/web/README.md:21-25`, `apps/web/package.json:28-43`
  README stack table is stale for TypeScript and icons.

- Medium - Phase 4 - `scripts/build-web.sh:14-17`
  Local web build installs dependencies without `--frozen-lockfile`.

- Medium - Phase 4 - `scripts/web-watch.sh:13-26`
  Watch script claims an initial build but does not explicitly run one.

### Low Priority

- None filed; review focused bead filing on P0/P1/P2 per request.

## Findings by Category

### Architecture and Code Quality

- Route lookup normalization bug in `src/web.rs`.
- Biome formatting/import gate failure in `apps/web`.
- Manual web action metadata drift risk.

### Security

- API client error parsing obscures auth/scope/status failures.

### Performance

- Static cache-control strategy is too broad for non-hashed export artifacts.

### Testing

- Missing embedded static route tests.
- Missing API client error parser tests.

### Documentation

- `docs/WEB.md` contains stale `build.rs` guidance.
- `apps/web/README.md` stack table is stale.

### Standards and Operations

- Raw controls/imperative style mutation bypass local Aurora wrapper guidance.
- `scripts/build-web.sh` is less reproducible than Docker.
- `scripts/web-watch.sh` does not make its initial build behavior explicit.

## Recommended Fix Order

1. Fix `src/web.rs` route normalization and add regression tests for `/tools/` and `/api/`.
2. Run Biome fixes and replace raw app controls with wrapper components.
3. Harden `apiFetch` error parsing and normalize `apiBaseUrl`.
4. Leave P2 docs/build-script/cache/parity work as follow-up unless it is needed by a P1 fix.

## Residual Risks

- P2 parity and caching issues may still cause stale or inaccurate generated apps after the P1 remediation.
- Browser visual verification should be run after UI component changes because the app is static-exported and embedded rather than served by a separate Node runtime.

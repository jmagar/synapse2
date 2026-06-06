# Phase 1: Code Quality and Architecture

## Findings

- High - `src/web.rs:50-57`
  Static asset lookup does not normalize a trailing slash before building `format!("{path}/index.html")`. Next static export produces `tools/index.html` and `api/index.html`; a request for `/tools/` produces the candidate `tools//index.html`, misses the page, and falls back to root `index.html`.
  Impact: navigation links in `apps/web/app/layout.tsx:60-62` point at trailing-slash URLs and can serve the dashboard shell instead of the requested web page when embedded in the Rust binary.
  Fix by trimming trailing slashes from non-root paths before candidate generation and adding route-level tests for `/tools`, `/tools/`, `/api`, and `/api/`.

- High - `apps/web/app/api/page.tsx:3`, `apps/web/app/page.tsx:3-7`, `apps/web/app/tools/page.tsx:3-8`, `apps/web/app/layout.tsx:1-5`
  The web app does not pass its configured quality gate. `pnpm -C apps/web check` reports unsorted imports, formatting drift, and a type-only import violation in `components/api/action-card.tsx:1`.
  Impact: CI or local `pnpm validate` fails before users reach the static build, and agents cannot rely on the documented `pnpm check` gate.
  Fix by running Biome formatting/import organization and keeping the checked-in code at the configured `biome.json` style.

- Medium - `apps/web/lib/template.ts:36-149`
  Web action metadata is a hand-maintained list separate from Rust action metadata and schemas. This template intentionally gives scaffold users a visible web customization point, but there is no parity guard that catches a missing REST action or stale parameter shape.
  Impact: scaffolded application/platform servers can silently ship a Tool Runner/API Explorer that disagrees with the REST/MCP/CLI action set.
  Fix with a small generated or snapshot parity test that compares `REST_ACTIONS` with the Rust action manifest/OpenAPI contract, or document the manual-update boundary as a required scaffold step with validation.

## Critical Issues for Phase 2 Context

- The trailing-slash static asset bug affects embedded browser navigation and should be considered when reviewing caching and asset-serving behavior.

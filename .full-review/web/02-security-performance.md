# Phase 2: Security and Performance

## Findings

- High - `apps/web/lib/api.ts:39-48`
  `apiFetch` calls `res.json()` before checking status or response content type. Empty, HTML, proxy-generated, or middleware-generated error responses become JSON parse exceptions and are displayed as generic network errors, losing the HTTP status and any useful auth/authorization context.
  Impact: authenticated deployments can mislead operators during token, scope, CORS, or gateway failures; the browser UI hides whether the server returned 401/403/404/500.
  Fix by reading text first, parsing JSON only when possible, preserving `HTTP <status>` for non-OK responses, and extracting `{error}` only from JSON object bodies.

- Medium - `src/web.rs:62-67`
  Static asset caching is based only on whether the candidate ends in `index.html`. Next's static export also emits route data/text files and non-hashed route artifacts. Applying immutable one-year caching to every non-index asset can make stale route payloads hard to recover from if filenames are not content-addressed.
  Impact: users may see stale page data after a binary upgrade if a browser or proxy cached non-hashed static export artifacts.
  Fix by restricting immutable caching to clearly hashed Next assets under `_next/static/` and using `no-store` or short `max-age` for route HTML/text/data files.

- Medium - `apps/web/lib/template.ts:6`
  `NEXT_PUBLIC_EXAMPLE_API_BASE_URL` is concatenated directly in `endpoint()` without normalization. A value with a trailing slash generates URLs such as `http://localhost:3100//health`.
  Impact: most Axum routes tolerate this poorly or not at all depending on proxy/path normalization, so local development can produce false "network" failures.
  Fix by normalizing `apiBaseUrl` with trailing slash removal before concatenating paths.

## Critical Issues for Phase 3 Context

- Tests need to cover both embedded web route lookup and browser client error parsing; the current tests do not exercise either failure mode.

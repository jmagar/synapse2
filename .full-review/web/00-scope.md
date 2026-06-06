# Review Scope

## Target

Web UI surface for `rmcp-template` on branch `fix/docker-network-default`.

## Files

- `apps/web/**`
- `src/web.rs`
- `src/web_tests.rs`
- `docs/WEB.md`
- `scripts/build-web.sh`
- `scripts/web-watch.sh`
- Closely related web build/config docs and cross-surface contracts: `Justfile`, `config/Dockerfile`, `src/server/routes.rs`, `src/api.rs`

## Review Flags

- Security focus: yes
- Performance critical: no
- Strict mode: yes
- Framework: Next.js static export, React, TypeScript, Axum embedded static assets

## Review Phases

1. Code Quality and Architecture
2. Security and Performance
3. Testing and Documentation
4. Best Practices and Standards
5. Consolidated Report

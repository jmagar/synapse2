---
title: "Environment Variables"
doc_type: "guide"
status: "active"
owner: "rmcp-template"
audience:
  - "contributors"
  - "agents"
scope: "template"
source_of_truth: false
upstream_refs:
  - "src/config.rs"
last_reviewed: "2026-05-15"
---

# Environment variables

The template uses `EXAMPLE_*` variables. Rename the prefix when adapting the template.

## Upstream service

| Variable | Purpose |
|---|---|
| `EXAMPLE_API_URL` | Upstream API base URL used by `ExampleClient`. Required. |
| `EXAMPLE_API_KEY` | Upstream API key or token. Keep secret. Required. |

## MCP HTTP server

| Variable | Default | Purpose |
|---|---:|---|
| `EXAMPLE_MCP_HOST` | `127.0.0.1` | Bind host for HTTP transport. Set `0.0.0.0` only with bearer, OAuth, or trusted-gateway auth configured. |
| `EXAMPLE_MCP_PORT` | `40080` | Bind port for HTTP transport. |
| `EXAMPLE_MCP_NO_AUTH` | `false` | Disable local auth for loopback development only. |
| `EXAMPLE_NOAUTH` | `false` | Trusted-gateway no-auth mode for non-loopback deployments. |
| `EXAMPLE_MCP_TOKEN` | unset | Static bearer token. Required for bearer-only mounted HTTP. |
| `EXAMPLE_MCP_ALLOWED_HOSTS` | unset | Extra accepted Host header values (comma-separated). |
| `EXAMPLE_MCP_ALLOWED_ORIGINS` | unset | Extra CORS origins (comma-separated). |
| `EXAMPLE_MCP_PUBLIC_URL` | unset | Public URL used for OAuth metadata endpoints. |
| `EXAMPLE_MCP_AUTH_MODE` | `bearer` | `bearer` or `oauth`. |
| `EXAMPLE_MCP_AUTH_SQLITE_PATH` | `/data/auth.db` | OAuth session/client database path. |
| `EXAMPLE_MCP_AUTH_KEY_PATH` | `/data/auth-jwt.pem` | OAuth JWT signing key path. |
| `EXAMPLE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS` | `3600` | OAuth access-token TTL. |
| `EXAMPLE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS` | `2592000` | OAuth refresh-token TTL. |
| `EXAMPLE_MCP_AUTH_CODE_TTL_SECS` | `300` | OAuth authorization-code TTL. |
| `EXAMPLE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE` | `10` | OAuth dynamic-registration rate limit. |
| `EXAMPLE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE` | `60` | OAuth authorization rate limit. |
| `EXAMPLE_MCP_AUTH_DISABLE_STATIC_TOKEN_WITH_OAUTH` | `true` | Disable static bearer tokens when OAuth is active. |
| `EXAMPLE_MCP_AUTH_ALLOWED_REDIRECT_URIS` | unset | Extra OAuth redirect URI patterns (comma-separated). |

## OAuth mode

Only required when `EXAMPLE_MCP_AUTH_MODE=oauth`:

| Variable | Purpose |
|---|---|
| `EXAMPLE_MCP_GOOGLE_CLIENT_ID` | Google OAuth client ID. |
| `EXAMPLE_MCP_GOOGLE_CLIENT_SECRET` | Google OAuth client secret. |
| `EXAMPLE_MCP_AUTH_ADMIN_EMAIL` | Initial/admin email allowed by the OAuth flow. |

## Docker runtime

| Variable | Purpose |
|---|---|
| `DOCKER_GID` | Host docker group id; required when the Docker socket is mounted. |
| `DOCKER_NETWORK` | Docker network name (default: `mcp`). |
| `EXAMPLE_VERSION` | Image tag to pull (default: `latest`). |
| `EXAMPLE_MCP_HOST_PORT` | Host port published to the container MCP port. |

## Logging

| Variable | Example | Purpose |
|---|---|---|
| `RUST_LOG` | `info,rmcp=warn` | Tracing filter. |
| `NO_COLOR` | `1` | Disable ANSI color in console logs. |
| `FORCE_COLOR` | `1` | Force ANSI color even when stderr is not a TTY. |

## `.env` file structure

```bash
# .env — secrets, URLs, and deploy/runtime vars
EXAMPLE_API_URL=https://example.internal/api
EXAMPLE_API_KEY=your_api_key_here

# MCP auth
EXAMPLE_MCP_TOKEN=your_bearer_token_here

# OAuth (only when auth_mode=oauth in config.toml)
# EXAMPLE_MCP_GOOGLE_CLIENT_ID=...
# EXAMPLE_MCP_GOOGLE_CLIENT_SECRET=...

# Docker runtime
DOCKER_GID=999
DOCKER_NETWORK=mcp
RUST_LOG=info
```

## Safety

`.env` and `.env.*` are ignored by `.gitignore` and blocked by `scripts/block-env-commits.sh`. Only `.env.example` belongs in git.

Non-secret settings usually go in `config.toml`; `.env` can override them for
deploy-time secrets, logging, and runtime interpolation. Existing process env
vars beat `.env`, and the appdata `.env` beats a current-directory `.env`.

Generate a bearer token:

```bash
just gen-token
# or: openssl rand -hex 32
```

See `docs/CONFIG.md` for the config loading pattern and auth policy details.

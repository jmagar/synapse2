---
title: "Environment Variables"
doc_type: "guide"
status: "active"
owner: "synapse2"
audience:
  - "contributors"
  - "agents"
scope: "synapse2"
source_of_truth: false
upstream_refs:
  - "src/config.rs"
last_reviewed: "2026-06-12"
---

# Environment variables

Synapse2 uses `SYNAPSE_*` variables for service configuration and
`SYNAPSE_MCP_*` variables for MCP server configuration.

## MCP HTTP server

| Variable | Default | Purpose |
|---|---:|---|
| `SYNAPSE_MCP_HOST` | `127.0.0.1` | Bind host for HTTP transport. Set `0.0.0.0` only with bearer, OAuth, or trusted-gateway auth configured. |
| `SYNAPSE_MCP_PORT` | `40080` | Bind port for HTTP transport. |
| `SYNAPSE_MCP_SERVER_NAME` | `synapse2` | MCP server name advertised to clients. |
| `SYNAPSE_MCP_NO_AUTH` | `false` | Disable local auth for loopback development only. |
| `SYNAPSE_NOAUTH` | `false` | Trusted-gateway no-auth mode for non-loopback deployments. |
| `SYNAPSE_MCP_ALLOW_DESTRUCTIVE` | `false` | Skip destructive-operation confirmation prompts. Startup refuses this on non-loopback binds. |
| `SYNAPSE_MCP_TOKEN` | unset | Static bearer token. Required for bearer-only mounted HTTP. |
| `SYNAPSE_MCP_ALLOWED_HOSTS` | unset | Extra accepted Host header values (comma-separated). |
| `SYNAPSE_MCP_ALLOWED_ORIGINS` | unset | Extra CORS origins (comma-separated). |
| `SYNAPSE_MCP_PUBLIC_URL` | unset | Public URL used for OAuth metadata endpoints. |
| `SYNAPSE_MCP_AUTH_MODE` | `bearer` | `bearer` or `oauth`. |
| `SYNAPSE_MCP_AUTH_SQLITE_PATH` | `/data/auth.db` | OAuth session/client database path. |
| `SYNAPSE_MCP_AUTH_KEY_PATH` | `/data/auth-jwt.pem` | OAuth JWT signing key path. |
| `SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS` | `3600` | OAuth access-token TTL. |
| `SYNAPSE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS` | `2592000` | OAuth refresh-token TTL. |
| `SYNAPSE_MCP_AUTH_CODE_TTL_SECS` | `300` | OAuth authorization-code TTL. |
| `SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE` | `10` | OAuth dynamic-registration rate limit. |
| `SYNAPSE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE` | `60` | OAuth authorization rate limit. |
| `SYNAPSE_MCP_AUTH_DISABLE_STATIC_TOKEN_WITH_OAUTH` | `true` | Disable static bearer tokens when OAuth is active. |
| `SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS` | unset | Extra OAuth redirect URI patterns (comma-separated). |
| `SYNAPSE_MCP_MAX_CONCURRENCY` | `50` | Maximum simultaneous in-flight requests on `/mcp` and `/v1/synapse2`. Excess requests are queued (back-pressure), not rejected. Set to `0` to disable. `/health` and `/status` are exempt. This is a global cap across all clients, not a per-client rate limit. |

## Host topology

| Variable | Purpose |
|---|---|
| `SYNAPSE_HOSTS_CONFIG` | Inline host topology as a JSON array; highest priority. |
| `SYNAPSE_CONFIG_FILE` | Path to a host config file; used when inline hosts are unset. |
| `SYNAPSE_HOME` | Override appdata directory. Defaults to `~/.synapse2` outside containers and `/data` in containers. |

When no host topology variable is set, Synapse2 falls back to `~/.ssh/config`
discovery.

## OAuth mode

Only required when `SYNAPSE_MCP_AUTH_MODE=oauth`:

| Variable | Purpose |
|---|---|
| `SYNAPSE_MCP_GOOGLE_CLIENT_ID` | Google OAuth client ID. |
| `SYNAPSE_MCP_GOOGLE_CLIENT_SECRET` | Google OAuth client secret. |
| `SYNAPSE_MCP_AUTH_ADMIN_EMAIL` | Initial/admin email allowed by the OAuth flow. |

## Docker runtime

| Variable | Purpose |
|---|---|
| `DOCKER_GID` | Host docker group id; required when the Docker socket is mounted. |
| `DOCKER_NETWORK` | Docker network name (default: `mcp`). |
| `SYNAPSE2_VERSION` | Image tag to pull (default: `latest`). |
| `SYNAPSE_MCP_HOST_PORT` | Host port published to the container MCP port. |

## Logging

| Variable | Example | Purpose |
|---|---|---|
| `RUST_LOG` | `info,rmcp=warn` | Tracing filter. |
| `NO_COLOR` | `1` | Disable ANSI color in console logs. |
| `FORCE_COLOR` | `1` | Force ANSI color even when stderr is not a TTY. |

## `.env` file structure

```bash
# .env — secrets, URLs, and deploy/runtime vars

# MCP auth
SYNAPSE_MCP_TOKEN=your_bearer_token_here

# OAuth (only when auth_mode=oauth in config.toml)
# SYNAPSE_MCP_AUTH_MODE=oauth
# SYNAPSE_MCP_PUBLIC_URL=https://synapse2.example.com
# SYNAPSE_MCP_GOOGLE_CLIENT_ID=...
# SYNAPSE_MCP_GOOGLE_CLIENT_SECRET=...
# SYNAPSE_MCP_AUTH_ADMIN_EMAIL=admin@example.com

# Host topology
# SYNAPSE_CONFIG_FILE=/home/synapse/.synapse2/hosts.json

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

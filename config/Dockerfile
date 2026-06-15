# syntax=docker/dockerfile:1.7
# =============================================================================
# Dockerfile — Template for building an rmcp-based MCP server
#
# TEMPLATE: Replace all occurrences of "example" with your binary name.
#           The binary name must match Cargo.toml [[bin]] name = "example".
#
# Key substitutions:
#   "example"              → your binary name (e.g. "myservice")
#   "example-cargo-*"      → your cache mount IDs (e.g. "myservice-cargo-*")
#   "example-mcp"          → your systemd/Docker service name
#   EXPOSE 40080            → your port (must match config.toml [mcp] port)
#   HEALTHCHECK port 40080  → same as EXPOSE
#   CMD ["example", ...]   → your binary name
#
# Build stages:
#   builder  — Rust compilation with BuildKit cache mounts (fast incremental builds)
#   runtime  — Minimal Debian image with only the binary + curl for health checks
# =============================================================================

# ── Stage 1: Web assets ───────────────────────────────────────────────────────
# Build the static Next.js export inside Docker so fresh clones do not need a
# pre-existing ignored apps/web/out/ directory.
# DIGEST-PIN: Refresh with:
#   docker pull node:26-bookworm-slim
#   docker inspect --format '{{index .RepoDigests 0}}' node:26-bookworm-slim
# Then replace the tag below with:
#   FROM node@sha256:<digest> AS web  # node:26-bookworm-slim
FROM node:26.3.0-bookworm-slim AS web

WORKDIR /app/apps/web

COPY apps/web/package.json apps/web/pnpm-lock.yaml apps/web/pnpm-workspace.yaml ./
RUN npm install -g "$(node -p "require('./package.json').packageManager")" && \
    pnpm install --frozen-lockfile

COPY apps/web/ ./
RUN pnpm build

# ── Stage 2: Builder ──────────────────────────────────────────────────────────
# DIGEST-PIN: Refresh with:
#   docker pull rust:1.96-slim-bookworm
#   docker inspect --format '{{index .RepoDigests 0}}' rust:1.96-slim-bookworm
# Then replace the tag below with:
#   FROM rust@sha256:<digest> AS builder  # rust:1.96-slim-bookworm
FROM rust:1.96-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies.
# pkg-config and libssl-dev are needed for openssl-sys (used by reqwest/lab-auth).
# TEMPLATE: Add other system deps your crates need (e.g. libpq-dev for postgres).
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# ── Dependency caching layer ──────────────────────────────────────────────────
# Copy only Cargo manifests first and build a stub binary. This lets Docker/
# BuildKit cache the compiled dependencies as a separate layer — subsequent
# builds that only change src/ skip this expensive step entirely.
COPY Cargo.toml Cargo.lock ./
# Strip xtask workspace member — not needed in Docker build context
RUN sed -i 's/, \"xtask\"//g; s/\"xtask\", //g; s/\"xtask\"//g' Cargo.toml

# TEMPLATE: Replace "example" cache IDs with your service name to avoid
#           sharing cache with other projects on the same build host.
RUN --mount=type=cache,id=synapse2-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=synapse2-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=synapse2-cargo-target,target=/app/target,sharing=locked \
    mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release --locked -p synapse2 && rm -rf src

# ── Application build ─────────────────────────────────────────────────────────
# Now copy the real source and build the actual binary.
# The `touch src/main.rs` forces Cargo to re-link even when timestamps haven't
# changed (Docker layer caching can confuse Cargo's incremental build detection).
COPY src/ src/
COPY docs/generated/openapi.json docs/generated/openapi.json
# Embed the Next.js static export built in the web stage.
COPY --from=web /app/apps/web/out/ apps/web/out/
RUN --mount=type=cache,id=synapse2-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=synapse2-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=synapse2-cargo-target,target=/app/target,sharing=locked \
    touch src/main.rs && cargo build --release --locked -p synapse2 && \
    cp target/release/synapse /usr/local/bin/synapse

# ── Stage 3: Runtime ──────────────────────────────────────────────────────────
# Minimal Debian image — no Rust toolchain, no build tools.
# Only ca-certificates (for TLS) and curl (for HEALTHCHECK).
# DIGEST-PIN: Update quarterly with:
#   docker pull debian:bookworm-slim
#   docker inspect --format '{{index .RepoDigests 0}}' debian:bookworm-slim
# Then replace the tag below with:
#   FROM debian@sha256:<digest>  # debian:bookworm-slim YYYY-MM-DD
FROM debian:bookworm-slim

# Install runtime dependencies:
#   ca-certificates — TLS certificate verification for outbound HTTPS calls
#   curl            — used by HEALTHCHECK
#   gosu            — privilege drop in entrypoint.sh (exec as UID 1000:1000)
#   openssh-client  — required by the openssh crate's native-mux backend
#
# TEMPLATE: Add other runtime libraries your binary needs (e.g. libssl1.1 if not
#           statically linked). Alpine users: replace gosu with su-exec and update
#           entrypoint.sh accordingly.
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl gosu openssh-client procps iproute2 && \
    rm -rf /var/lib/apt/lists/*
# NOTE: docker.io (the Docker daemon CLI) was removed. Bollard communicates with
# the Docker daemon entirely via the mounted socket (/var/run/docker.sock) and
# does not invoke the docker binary at runtime. Installing the full daemon added
# ~100-200 MB to the image for zero benefit.

# TEMPLATE: Replace "example" with your binary name in both COPY and target path.
COPY --from=builder /usr/local/bin/synapse /usr/local/bin/synapse

# Wire in the entrypoint.
# TEMPLATE: entrypoint.sh lives at the repo root. It runs as root, fixes /data
#           permissions, validates env vars, then exec's as UID 1000:1000.
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Create a non-root user and group for the service.
# UID/GID 1000 matches the host user convention in docker-compose.yml.
# TEMPLATE: Replace "example" with your service name for the user/group.
RUN groupadd --gid 1000 synapse && \
    useradd --uid 1000 --gid synapse --no-create-home --shell /sbin/nologin synapse && \
    mkdir -p /data /home/synapse/.ssh && \
    chown -R synapse:synapse /data /home/synapse && \
    chmod 700 /home/synapse/.ssh

# HOME must be set explicitly: the service user is created with --no-create-home
# and neither gosu nor setpriv export HOME. Host topology auto-discovery in
# host_config.rs resolves ~/.ssh/config relative to HOME, so a deployment that
# bind-mounts the operator's SSH config (see docker-compose.prod.yml) lands it
# at /home/synapse/.ssh and is discovered automatically.
ENV HOME=/home/synapse

# Default log level. Override with RUST_LOG env var in docker-compose or at runtime.
ENV RUST_LOG=info

# TEMPLATE: Add other default env vars your binary needs.
# ENV EXAMPLE_MCP_HOST=0.0.0.0
# ENV EXAMPLE_MCP_PORT=3000

# NOTE: No USER directive here — entrypoint.sh drops to UID 1000:1000 via gosu.
# This allows entrypoint.sh to run as root (for chown /data) before execing the binary.
# Pattern §26 + §16 (USER 1000:1000 via entrypoint, not Dockerfile USER directive).

# TEMPLATE: Change to match your config.toml [mcp] port = 3000
EXPOSE 40080/tcp

# Health check hits /health (always unauthenticated in the rmcp-template design).
# TEMPLATE: Change port to match EXPOSE above.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -sf http://localhost:40080/health || exit 1

# MCP registry label — used by Claude Code plugin discovery.
# TEMPLATE: Replace with your registry path: "tv.tootie/yourservice-mcp"
LABEL io.modelcontextprotocol.server.name="tv.tootie/synapse2"

# Entrypoint handles permission setup and privilege drop.
# CMD is the default argument passed to entrypoint.sh.
# TEMPLATE: Replace "example" with your binary name.
ENTRYPOINT ["/entrypoint.sh"]
CMD ["serve", "mcp"]

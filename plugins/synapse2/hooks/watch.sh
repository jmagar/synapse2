#!/usr/bin/env bash
# Claude monitor entry point. Uses an installed synapse from PATH.
set -euo pipefail

binary="${SYNAPSE_MCP_BIN:-synapse}"

if ! command -v "${binary}" >/dev/null 2>&1; then
  printf 'synapse2 monitor: synapse is not installed or not on PATH.\n' >&2
  exit 0
fi

exec "${binary}" watch "$@"

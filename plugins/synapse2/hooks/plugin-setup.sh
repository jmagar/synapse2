#!/usr/bin/env bash
# SessionStart / ConfigChange hook for the Synapse2 MCP server plugin.
# Adapts plugin settings to env and delegates setup to an installed synapse binary.
set -euo pipefail

: "${CLAUDE_PLUGIN_ROOT:=$(cd "$(dirname "$0")/.." && pwd)}"
: "${CLAUDE_PLUGIN_DATA:=${HOME}/.claude/plugins/data/synapse2-jmagar-lab}"
: "${SYNAPSE_HOME:=${CLAUDE_PLUGIN_DATA}}"

reject_unsafe_value() {
  local name="$1" value="${2:-}"
  if [[ "${value}" == *$'\n'* || "${value}" == *$'\r'* ]]; then
    printf 'synapse2 plugin setup: %s must not contain newlines\n' "${name}" >&2
    exit 2
  fi
}

export_if_set() {
  local env_name="$1" option_name="$2" value
  value="$(printenv "${option_name}" || true)"
  reject_unsafe_value "${option_name}" "${value}"
  [[ -n "${value}" ]] || return 0
  export "${env_name}=${value}"
}

synapse_binary() {
  if command -v synapse >/dev/null 2>&1; then
    command -v synapse
    return 0
  fi

  printf 'synapse2 plugin setup: synapse is not installed or not on PATH.\n' >&2
  printf 'Install synapse separately, then run: synapse setup\n' >&2
  return 1
}

main() {
  local synapse_bin
  if ! synapse_bin="$(synapse_binary)"; then
    return 0
  fi

  reject_unsafe_value "CLAUDE_PLUGIN_OPTION_API_TOKEN" "${CLAUDE_PLUGIN_OPTION_API_TOKEN:-}"
  export_if_set SYNAPSE_MCP_TOKEN CLAUDE_PLUGIN_OPTION_API_TOKEN
  export_if_set SYNAPSE_SERVER_URL CLAUDE_PLUGIN_OPTION_SERVER_URL
  export_if_set SYNAPSE_HOSTS_CONFIG CLAUDE_PLUGIN_OPTION_SYNAPSE_HOSTS_CONFIG
  export_if_set SYNAPSE_CONFIG_FILE CLAUDE_PLUGIN_OPTION_SYNAPSE_CONFIG_FILE
  export_if_set SYNAPSE_MCP_AUTH_MODE CLAUDE_PLUGIN_OPTION_AUTH_MODE
  export_if_set SYNAPSE_MCP_NO_AUTH CLAUDE_PLUGIN_OPTION_NO_AUTH
  export_if_set SYNAPSE_MCP_PUBLIC_URL CLAUDE_PLUGIN_OPTION_PUBLIC_URL
  export_if_set SYNAPSE_MCP_GOOGLE_CLIENT_ID CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID
  export_if_set SYNAPSE_MCP_GOOGLE_CLIENT_SECRET CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET
  export_if_set SYNAPSE_MCP_AUTH_ADMIN_EMAIL CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL

  mkdir -p "${SYNAPSE_HOME}"
  chmod 700 "${SYNAPSE_HOME}" 2>/dev/null || true
  export SYNAPSE_HOME

  "${synapse_bin}" setup plugin-hook "$@"
}

main "$@"

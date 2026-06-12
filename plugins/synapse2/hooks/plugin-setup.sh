#!/usr/bin/env bash
# SessionStart / ConfigChange hook for the Synapse2 MCP server plugin.
# Keep setup policy in the binary; this script only adapts plugin settings to env.
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

ensure_synapse2_binary() {
  local bundled="${CLAUDE_PLUGIN_ROOT}/bin/synapse"
  if [[ ! -x "${bundled}" ]]; then
    printf 'synapse2 plugin setup: bundled binary not found at %s\n' "${bundled}" >&2
    printf '  → run: just install   (builds release binary and copies to plugins/synapse2/bin/)\n' >&2
    exit 1
  fi

  mkdir -p "${HOME}/.local/bin"
  ln -sf "${bundled}" "${HOME}/.local/bin/synapse"
  export PATH="${HOME}/.local/bin:${PATH}"
  printf '%s\n' "${bundled}"
}

main() {
  local synapse_bin
  synapse_bin="$(ensure_synapse2_binary)"

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

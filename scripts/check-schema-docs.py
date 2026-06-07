#!/usr/bin/env python3
"""Generate and verify MCP schema/action documentation drift."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCHEMAS_RS = ROOT / "src/mcp/schemas.rs"
ACTION_RS = ROOT / "src/actions.rs"
HELP_RS = ROOT / "src/mcp/help.rs"
PROMPTS_RS = ROOT / "src/mcp/prompts.rs"
RESOURCES_RS = ROOT / "src/mcp/resources.rs"
README = ROOT / "README.md"
SKILL = ROOT / "plugins/synapse2/skills/synapse2/SKILL.md"
DOC = ROOT / "docs/MCP_SCHEMA.md"

FLUX_ACTIONS = {"help", "docker", "container", "host", "compose"}
SCOUT_ACTIONS = {
    "help",
    "nodes",
    "peek",
    "find",
    "ps",
    "df",
    "delta",
    "exec",
    "emit",
    "beam",
    "zfs",
    "logs",
}


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def extract_actions() -> list[str]:
    text = read(ACTION_RS)
    return re.findall(r'name:\s*"([^"]+)"', text)


def extract_scope_for_actions() -> dict[str, str]:
    text = read(ACTION_RS)
    entries = re.findall(r"ActionSpec\s*\{(.*?)\}", text, re.S)
    scopes: dict[str, str] = {}
    for entry in entries:
        name_match = re.search(r'name:\s*"([^"]+)"', entry)
        scope_match = re.search(r"required_scope:\s*([^,\n]+)", entry)
        if not name_match or not scope_match:
            continue
        name = name_match.group(1)
        scope_expr = scope_match.group(1).strip()
        if scope_expr == "None":
            scopes[name] = "public"
        elif scope_expr == "Some(READ_SCOPE)":
            scopes[name] = "`synapse:read`"
        elif scope_expr == "Some(WRITE_SCOPE)":
            scopes[name] = "`synapse:write`"
        else:
            scopes[name] = "`synapse2:__deny__`"
    return scopes


def action_description(action: str) -> str:
    descriptions = {
        "help": "Return the in-tool action reference. Public; no scope required.",
        "docker": "Docker daemon and image operations.",
        "container": "Container read and lifecycle operations.",
        "host": "Host status, resource, service, network, mount, port, and doctor operations.",
        "compose": "Docker Compose project operations.",
        "nodes": "List configured hosts.",
        "peek": "Read a file or directory listing.",
        "find": "Find files by glob.",
        "ps": "List processes.",
        "df": "Report disk usage.",
        "delta": "Compare files or inline content.",
        "exec": "Run an allowlisted command.",
        "emit": "Run an allowlisted command across multiple targets.",
        "beam": "Transfer a file between hosts.",
        "zfs": "Read ZFS pools, datasets, and snapshots.",
        "logs": "Read syslog, journal, dmesg, and auth logs.",
    }
    return descriptions.get(action, "TEMPLATE: document this action.")


def action_tools(action: str) -> list[str]:
    tools: list[str] = []
    if action in FLUX_ACTIONS:
        tools.append("flux")
    if action in SCOUT_ACTIONS:
        tools.append("scout")
    return tools


def render() -> str:
    actions = extract_actions()
    scopes = extract_scope_for_actions()
    lines = [
        "# synapse2 MCP Schema Contract",
        "",
        "`synapse2` exposes two MCP tools: `flux` and `scout`.",
        "",
        "Run:",
        "",
        "```bash",
        "python3 scripts/check-schema-docs.py --write",
        "python3 scripts/check-schema-docs.py --check",
        "```",
        "",
        "## Tool",
        "",
        "| Tool | Dispatch parameter | Purpose |",
        "|---|---|---|",
        "| `flux` | `action` | Docker, container, host, and compose operations |",
        "| `scout` | `action` | SSH/local filesystem, process, ZFS, log, and command operations |",
        "",
        "## Actions",
        "",
        "| Tool | Action | Scope | Description |",
        "|---|---|---|---|",
    ]
    for action in actions:
        scope = scopes[action]
        for tool in action_tools(action):
            lines.append(f"| `{tool}` | `{action}` | {scope} | {action_description(action)} |")
    lines.extend(
        [
            "",
            "## Drift Rules",
            "",
            "- `ACTION_SPECS` in `src/actions.rs` is the canonical action and scope list.",
            "- `src/mcp/schemas.rs` must expose exactly the `flux` and `scout` tool schemas.",
            "- Both MCP tool schemas must reject unknown top-level parameters.",
            "- `help` is intentionally public and must have no required scope.",
            "- `README.md`, `docs/API.md`, and `plugins/synapse2/skills/synapse2/SKILL.md` must mention every shipped action.",
            "- `src/mcp/resources.rs` owns stable resources and must keep `synapse://schema/flux` and `synapse://schema/scout` wired to `tool_definitions()`.",
            "- `src/mcp/prompts.rs` owns stable prompts and must keep `quick_start` covered by prompt tests.",
            "",
            "## Resources",
            "",
            "| URI | Source | Contract |",
            "|---|---|---|",
            "| `synapse://schema/flux` | `src/mcp/resources.rs` | Returns the `flux` schema from `tool_definitions()` as `application/json`. |",
            "| `synapse://schema/scout` | `src/mcp/resources.rs` | Returns the `scout` schema from `tool_definitions()` as `application/json`. |",
            "",
            "## Prompts",
            "",
            "| Prompt | Source | Contract |",
            "|---|---|---|",
            "| `quick_start` | `src/mcp/prompts.rs` | Guides a client to call `scout` `nodes` and `flux` `host`. |",
            "",
            "## Input Validation",
            "",
            "- `action` is always required.",
            "- Unknown top-level parameters are rejected by the schema.",
            "- Destructive operations require `synapse:write` and a service-layer confirmation gate.",
            "",
        ]
    )
    return "\n".join(lines)


def check_mentions(actions: list[str]) -> list[str]:
    failures: list[str] = []
    surfaces = {
        "README.md": read(README),
        "plugins/synapse2/skills/synapse2/SKILL.md": read(SKILL),
        "src/mcp/help.rs": read(HELP_RS),
    }
    for label, text in surfaces.items():
        for action in actions:
            if action not in text:
                failures.append(f"{label} does not mention action `{action}`")
    return failures


def check_scope(actions: list[str]) -> list[str]:
    failures: list[str] = []
    scopes = extract_scope_for_actions()
    if set(scopes) != set(actions):
        failures.append("ACTION_SPECS action names and scope entries are out of sync")
    if scopes.get("help") != "public":
        failures.append("help must be public")
    for action in set(actions) - {"help"}:
        if scopes.get(action) == "public":
            failures.append(f"action `{action}` must declare a required scope")
    schema_text = read(SCHEMAS_RS)
    if '"name": "flux"' not in schema_text or '"name": "scout"' not in schema_text:
        failures.append("src/mcp/schemas.rs must expose flux and scout tool schemas")
    if schema_text.count('"additionalProperties": false') < 2:
        failures.append("src/mcp/schemas.rs must reject unknown top-level properties for both tools")
    resources_text = read(RESOURCES_RS)
    if (
        "synapse://schema/flux" not in resources_text
        or "synapse://schema/scout" not in resources_text
        or "tool_definitions()" not in resources_text
    ):
        failures.append("src/mcp/resources.rs must expose flux and scout schema resources from tool_definitions()")
    prompts_text = read(PROMPTS_RS)
    if "quick_start" not in prompts_text:
        failures.append("src/mcp/prompts.rs must expose quick_start prompt")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="Rewrite docs/MCP_SCHEMA.md.")
    parser.add_argument("--check", action="store_true", help="Fail if docs or action surfaces drift.")
    args = parser.parse_args()
    if not args.write and not args.check:
        args.check = True

    rendered = render()
    if args.write:
        DOC.write_text(rendered, encoding="utf-8")
        print(f"wrote {DOC.relative_to(ROOT)}")

    failures: list[str] = []
    if args.check:
        if not DOC.exists():
            failures.append("docs/MCP_SCHEMA.md is missing; run --write")
        elif read(DOC) != rendered:
            failures.append("docs/MCP_SCHEMA.md is stale; run --write")
        actions = extract_actions()
        failures.extend(check_mentions(actions))
        failures.extend(check_scope(actions))

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    if args.check:
        print("schema docs are current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

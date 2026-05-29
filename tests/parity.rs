//! Parity gate: assert every action listed in `synapse-mcp/docs/INVENTORY.md`
//! is reachable in synapse2.
//!
//! # What this test checks
//!
//! For each row in the INVENTORY table:
//! - The **top-level action** (e.g. `container`, `zfs`) exists in `ACTION_SPECS`
//!   via [`synapse2::actions::is_known_action`].
//! - The **subaction** (e.g. `list`, `pools`) is documented in the help map via
//!   [`synapse2::mcp::help::topic_markdown`].  Scout simple actions (no
//!   subaction) are checked at the bare topic key (e.g. `"nodes"`, `"exec"`).
//!   Help rows (`flux help`, `scout help`, `synapse_help`) collapse to the
//!   single `help` action.
//!
//! Both checks hit real production data structures — `ACTION_SPECS` is what the
//! dispatch gate uses to route or reject calls, and the help map is authoritative
//! for which topic keys (action:subaction pairs) are documented and wired. An
//! action removed from either will cause this test to fail.
//!
//! # Skip behaviour
//!
//! The test **skips gracefully** (returns early without failing) when
//! `../synapse-mcp/docs/INVENTORY.md` does not exist. This lets contributors
//! work on synapse2 without the sibling repo checked out.
//!
//! # Non-vacuity
//!
//! The test asserts that at least 55 rows are parsed from INVENTORY (guards
//! against a silent file-format mismatch that would produce an empty table).
//! It also verifies — at the bottom — that a bogus action and a bogus subaction
//! are correctly reported as missing.

use synapse2::actions::is_known_action;
use synapse2::mcp::help::topic_markdown;

/// A single entry parsed from the INVENTORY table.
#[derive(Debug, Clone)]
struct InventoryRow {
    /// INVENTORY "Action" column (e.g. "container", "zfs", "help").
    action: String,
    /// INVENTORY "Subaction" column; `None` when the column contains "--".
    subaction: Option<String>,
    /// Original "Tool" column (flux/scout/synapse_help); used only for
    /// diagnostics.
    tool: String,
}

/// Parse the MCP tools table from INVENTORY.md.
///
/// Looks for a markdown table whose header row contains columns
/// `Tool | Action | Subaction`.  Returns every data row with those three
/// fields extracted.
fn parse_inventory(content: &str) -> Vec<InventoryRow> {
    let mut rows = Vec::new();
    let mut in_table = false;
    let mut header_passed = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            if in_table {
                // We've left the table block.
                break;
            }
            continue;
        }

        // Detect the header row by looking for the three column names.
        if trimmed.contains("Tool") && trimmed.contains("Action") && trimmed.contains("Subaction") {
            in_table = true;
            header_passed = false; // next non-separator row is the separator
            continue;
        }

        if !in_table {
            continue;
        }

        // Skip the separator row (---|---|---)
        if trimmed.contains("---") {
            header_passed = true;
            continue;
        }

        if !header_passed {
            continue;
        }

        // Parse a data row: `| flux | container | list | … |`
        let cells: Vec<&str> = trimmed
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if cells.len() < 3 {
            continue;
        }

        let tool = cells[0].trim_matches('`').to_string();
        let action = cells[1].trim_matches('`').to_string();
        let subaction_raw = cells[2].trim_matches('`').to_string();
        let subaction = if subaction_raw == "--" {
            None
        } else {
            Some(subaction_raw)
        };

        rows.push(InventoryRow {
            action,
            subaction,
            tool,
        });
    }

    rows
}

/// Check whether a single INVENTORY row is covered by synapse2.
///
/// Returns `true` if the action/subaction is reachable, `false` if missing.
///
/// Mapping rules:
/// - `synapse_help` tool → maps to action `"help"` (top-level).
/// - `flux help` / `scout help` rows → maps to action `"help"` (same).
/// - Scout simples (subaction == None, not help) → `is_known_action(action)`
///   AND `topic_markdown(action)` returns Some.
/// - Flux/scout subaction rows → `is_known_action(action)` AND
///   `topic_markdown("action:subaction")` returns Some.
fn is_covered(row: &InventoryRow) -> bool {
    // synapse_help is a standalone tool in synapse-mcp but collapses to the
    // `help` action in synapse2 (no subaction needed).
    if row.tool == "synapse_help" {
        return is_known_action("help");
    }

    // flux/scout help rows → top-level "help" action.
    if row.action == "help" {
        return is_known_action("help");
    }

    // Check top-level action always.
    if !is_known_action(&row.action) {
        return false;
    }

    match &row.subaction {
        // Scout simples (no subaction) — the topic key is just the action name.
        None => topic_markdown(&row.action).is_some(),
        // Flux / scout subaction rows — the topic key is "action:subaction".
        Some(sub) => {
            let key = format!("{}:{}", row.action, sub);
            topic_markdown(&key).is_some()
        }
    }
}

#[test]
fn parity_with_synapse_mcp_inventory() {
    // ── Locate INVENTORY.md ───────────────────────────────────────────────────
    // The test runs from the cargo workspace root (synapse2/); the sibling repo
    // lives at ../synapse-mcp/.
    let inventory_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../synapse-mcp/docs/INVENTORY.md");

    if !inventory_path.exists() {
        eprintln!(
            "SKIP: {} not found — synapse-mcp sibling repo not checked out. \
             Clone it alongside synapse2 to enable full parity verification.",
            inventory_path.display()
        );
        return; // graceful skip, not a failure
    }

    let content = std::fs::read_to_string(&inventory_path).expect("failed to read INVENTORY.md");

    // ── Parse rows ────────────────────────────────────────────────────────────
    let rows = parse_inventory(&content);

    // Non-vacuity guard: if the parser is broken (format change, wrong file),
    // we'll parse zero rows and silently "pass" on an empty set. Reject that.
    assert!(
        rows.len() >= 55,
        "INVENTORY.md parsed only {} rows — expected ≥ 55. \
         The file format may have changed or the wrong file was read.",
        rows.len()
    );

    // ── Check each row ────────────────────────────────────────────────────────
    let mut matched = 0usize;
    let mut missing: Vec<String> = Vec::new();

    for row in &rows {
        if is_covered(row) {
            matched += 1;
        } else {
            let key = match &row.subaction {
                None => format!("{}/{}", row.tool, row.action),
                Some(sub) => format!("{}/{}/{}", row.tool, row.action, sub),
            };
            missing.push(key);
        }
    }

    // ── Report ────────────────────────────────────────────────────────────────
    let total = rows.len();
    println!(
        "synapse-mcp parity: {} rows parsed → {} matched, {} missing",
        total,
        matched,
        missing.len()
    );

    if !missing.is_empty() {
        panic!(
            "{} INVENTORY action(s) not covered by synapse2:\n  {}\n\n\
             Add the missing actions to ACTION_SPECS, dispatch arms, and \
             help.rs before closing this bead.",
            missing.len(),
            missing.join("\n  ")
        );
    }

    // All rows accounted for.
    assert_eq!(matched, total, "matched + missing should equal total rows");
}

// ── Negative unit checks ──────────────────────────────────────────────────────
//
// Ensure the `is_covered` helper actually rejects unknown entries (guards
// against a vacuous implementation where everything returns true).

#[test]
fn negative_unknown_action_is_not_covered() {
    let bogus_action = InventoryRow {
        tool: "flux".to_string(),
        action: "bogus_nonexistent_action".to_string(),
        subaction: None,
    };
    assert!(
        !is_covered(&bogus_action),
        "is_covered should return false for an unknown top-level action"
    );
}

#[test]
fn negative_unknown_subaction_is_not_covered() {
    let bogus_sub = InventoryRow {
        tool: "flux".to_string(),
        action: "container".to_string(),
        subaction: Some("bogus_nonexistent_subaction".to_string()),
    };
    assert!(
        !is_covered(&bogus_sub),
        "is_covered should return false for an unknown subaction even when \
         the top-level action exists"
    );
}

#[test]
fn negative_unknown_scout_subaction_is_not_covered() {
    let bogus_zfs = InventoryRow {
        tool: "scout".to_string(),
        action: "zfs".to_string(),
        subaction: Some("bogus_zfs_op".to_string()),
    };
    assert!(
        !is_covered(&bogus_zfs),
        "is_covered should return false for an unknown zfs subaction"
    );
}

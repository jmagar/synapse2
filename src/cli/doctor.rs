//! doctor — pre-flight environment validation command.
//!
//! Pattern §48: Every server binary MUST implement a `doctor` subcommand that
//! validates the environment and reports what's missing before the user tries
//! to start the server.
//!
//! # Usage
//!
//! ```text
//! synapse2 doctor           # human-readable coloured output; exit 0/1
//! synapse2 doctor --json    # machine-readable JSON; exit 0/1
//! ```
//!
//! # TEMPLATE
//!
//! Doctor validates the local environment: config file presence, writable data
//! and log directories, the binary on PATH, MCP port availability, and auth
//! configuration. Host topology (SYNAPSE_HOSTS_CONFIG / SYNAPSE_CONFIG_FILE /
//! ~/.ssh/config) is resolved lazily by flux/scout, so there is no startup
//! credential to validate here.
//!
//! Business logic for the checks belongs in the individual `check_*` functions —
//! never in `run_doctor`.

mod checks;

use checks::{
    check_auth_config, check_binary_in_path, check_config_file, check_dir_writable,
    check_port_available,
};

use anyhow::{Result, bail};
use serde::Serialize;

use crate::config::{Config, default_data_dir};

// ── Public entry point ────────────────────────────────────────────────────────

/// Run the doctor command.
///
/// Executes all pre-flight checks in order and prints a summary. Exits with
/// code 1 if any check fails; 0 if all pass.
///
/// # TEMPLATE
/// This function is the canonical §48 implementation. Add calls to new
/// `check_*` functions below to extend the set of checks for your service.
pub async fn run_doctor(config: &Config, json: bool) -> Result<()> {
    let mut checks: Vec<DoctorCheck> = Vec::new();

    // ── 1. Config and filesystem ──────────────────────────────────────────────
    //
    // TEMPLATE: The data dir is resolved via `config::default_data_dir()`.
    //           In Docker it resolves to /data; bare-metal to ~/.synapse2/.
    //           Replace ".synapse2" with your service name in config.rs.
    let data_dir = default_data_dir()?;

    checks.push(check_config_file(&data_dir));
    checks.push(check_dir_writable("Data directory", &data_dir));
    checks.push(check_dir_writable("Log directory", &data_dir.join("logs")));

    // ── 2. Binary in PATH ─────────────────────────────────────────────────────
    //
    // TEMPLATE: Replace "synapse2" with your binary name (Cargo.toml [[bin]] name).
    checks.push(check_binary_in_path("synapse"));

    // ── 3. MCP server port ────────────────────────────────────────────────────
    //
    // The host topology (SYNAPSE_HOSTS_CONFIG / SYNAPSE_CONFIG_FILE / ~/.ssh/config)
    // is resolved lazily by flux/scout at call time, so there is no startup
    // credential to validate here.
    checks.push(check_port_available(&config.mcp.host, config.mcp.port));

    // ── 4. Auth configuration ─────────────────────────────────────────────────
    //
    // TEMPLATE: The auth check inspects the combination of host / auth settings
    //           and reports which auth mode is active, or warns if 0.0.0.0 has
    //           no auth configured.
    checks.push(check_auth_config(config));

    // ── Render output ─────────────────────────────────────────────────────────

    let issues = checks.iter().filter(|c| !c.ok).count();

    if json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        print_doctor_report(&checks);
    }

    // Exit code 1 when any check fails.
    if issues > 0 {
        bail!("doctor found {issues} issue(s)");
    }
    Ok(())
}

// ── DoctorCheck struct ────────────────────────────────────────────────────────

/// A single pre-flight check result.
///
/// `ok = true`  → the check passed; `value` shows what was found.
/// `ok = false` → the check failed; `hint` explains how to fix it.
///
/// # TEMPLATE
/// Serialises directly to the `--json` output. Add fields here if you need
/// additional metadata (e.g. `severity: "warning" | "error"`, `doc_url`).
#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    /// Logical category for grouping in human output and JSON filtering.
    ///
    /// Categories emitted by the `check_*` functions:
    ///   "config" | "server" | "auth"
    pub category: &'static str,

    /// Short human-readable name for the check (shown in the left column).
    pub name: String,

    /// `true` = passed (✓), `false` = failed (✗).
    pub ok: bool,

    /// What was found — shown in the right column when ok=true.
    /// For failed checks, the hint is more useful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// How to fix the problem — only present when `ok = false`.
    ///
    /// TEMPLATE: Make hints actionable — tell the user exactly what to type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,

    /// Round-trip latency in milliseconds — only for connectivity checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

impl DoctorCheck {
    fn pass(category: &'static str, name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            category,
            name: name.into(),
            ok: true,
            value: Some(value.into()),
            hint: None,
            latency_ms: None,
        }
    }

    fn fail(category: &'static str, name: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            category,
            name: name.into(),
            ok: false,
            value: None,
            hint: Some(hint.into()),
            latency_ms: None,
        }
    }
}

// ── Human-readable report ─────────────────────────────────────────────────────

/// Print the doctor report in human-readable coloured format.
///
/// Output follows the §48 layout:
///
/// ```text
/// synapse2 v0.1.0 — environment check
///
///   Config
///   ────────────────────────────────────────────
///   ✓ Config file:  ~/.synapse2/config.toml
///   ✗ Data dir:     not writable
///     → Fix: chmod u+w ~/.synapse2
///   ...
/// ```
///
/// # TEMPLATE
/// Section headings and the version string are the main things to customise.
/// Add new sections if you add new check categories beyond the five defaults.
fn print_doctor_report(checks: &[DoctorCheck]) {
    use std::io::IsTerminal;
    let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();

    // ── ANSI helpers ──────────────────────────────────────────────────────────
    macro_rules! green {
        ($s:expr) => {
            if color {
                format!("\x1b[32m{}\x1b[0m", $s)
            } else {
                $s.to_string()
            }
        };
    }
    macro_rules! red {
        ($s:expr) => {
            if color {
                format!("\x1b[31m{}\x1b[0m", $s)
            } else {
                $s.to_string()
            }
        };
    }
    macro_rules! yellow {
        ($s:expr) => {
            if color {
                format!("\x1b[33m{}\x1b[0m", $s)
            } else {
                $s.to_string()
            }
        };
    }
    macro_rules! bold {
        ($s:expr) => {
            if color {
                format!("\x1b[1m{}\x1b[0m", $s)
            } else {
                $s.to_string()
            }
        };
    }
    macro_rules! dim {
        ($s:expr) => {
            if color {
                format!("\x1b[2m{}\x1b[0m", $s)
            } else {
                $s.to_string()
            }
        };
    }

    // TEMPLATE: Replace "synapse2" with your service name and binary name.
    println!();
    println!(
        "{}",
        bold!(format!(
            "synapse2 v{} — environment check",
            env!("CARGO_PKG_VERSION")
        ))
    );
    println!();

    // Group checks by category and print in order.
    // TEMPLATE: Reorder categories or add new ones to match your service.
    let categories: &[(&str, &str)] = &[
        ("config", "Config"),
        ("server", "MCP server"),
        ("auth", "Authentication"),
    ];

    for (cat_key, cat_label) in categories {
        let cat_checks: Vec<&DoctorCheck> =
            checks.iter().filter(|c| c.category == *cat_key).collect();
        if cat_checks.is_empty() {
            continue;
        }

        println!("  {}", bold!(cat_label));
        println!("  {}", dim!("─".repeat(44)));

        for check in &cat_checks {
            if check.ok {
                let value = check.value.as_deref().unwrap_or("");
                let latency = check
                    .latency_ms
                    .map(|ms| format!(" ({ms} ms)"))
                    .unwrap_or_default();
                println!(
                    "  {}  {:<28}  {}{}",
                    green!("✓"),
                    check.name,
                    value,
                    latency
                );
            } else {
                println!("  {}  {}", red!("✗"), check.name);
                if let Some(hint) = &check.hint {
                    for line in hint.lines() {
                        println!("    {}", yellow!(line));
                    }
                }
            }
        }

        println!();
    }

    // ── Summary line ──────────────────────────────────────────────────────────
    let issues = checks.iter().filter(|c| !c.ok).count();
    println!("  {}", dim!("━".repeat(44)));

    if issues == 0 {
        println!(
            "  {}  All checks passed. Run: {}",
            green!("✓"),
            bold!("synapse2 serve")
        );
    } else {
        // TEMPLATE: Replace "synapse2 serve" with your binary name.
        let noun = if issues == 1 { "issue" } else { "issues" };
        println!(
            "  {}  {} {noun} found. Fix before running: {}",
            red!("✗"),
            red!(issues.to_string()),
            bold!("synapse2 serve")
        );
    }
    println!();
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;

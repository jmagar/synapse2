use super::color::{CYAN_ANSI, PRIMARY_ANSI};
use super::help::{HelpRequest, classify_help, render_command, render_top_level};

const RESET: &str = "\x1b[0m";

#[test]
fn top_level_help_matches_cortex_grouped_shape() {
    let rendered = render_top_level(false);
    assert!(rendered.contains("  SYNAPSE2 CLI\n"));
    assert!(rendered.contains("  Version "));
    assert!(rendered.contains("  Usage\n"));
    assert!(rendered.contains("  synapse [options] <command> [args]\n"));
    assert!(rendered.contains("  Quick Start\n"));
    assert!(rendered.contains("  Global Options\n"));
    assert!(rendered.contains("  --color <when>"));
    assert!(rendered.contains("  Environment\n"));
    assert!(rendered.contains("  Commands\n"));
    assert!(rendered.contains("  Flux\n"));
    assert!(rendered.contains("    flux"));
    assert!(rendered.contains("  Scout\n"));
    assert!(rendered.contains("    scout"));
    assert!(rendered.contains("  Runtime & Setup\n"));
    assert!(rendered.contains("→ Run synapse <command> --help for command-specific flags"));
}

#[test]
fn colorized_help_uses_cortex_palette_codes() {
    let rendered = render_top_level(true);
    assert!(rendered.contains(CYAN_ANSI));
    assert!(rendered.contains(PRIMARY_ANSI));
    assert!(rendered.contains(&format!(
        "  {PRIMARY_ANSI}synapse flux container list --host local{RESET}\n"
    )));
    assert!(rendered.contains(&format!(
        "  {PRIMARY_ANSI}--color <when>              {RESET}"
    )));
    assert!(rendered.contains(&format!(
        "  {PRIMARY_ANSI}SYNAPSE_HOSTS_CONFIG        {RESET}"
    )));
    assert!(!rendered.contains(&format!("{CYAN_ANSI}synapse [options]")));
}

#[test]
fn command_help_renders_usage_only_for_that_command() {
    let rendered = render_command("flux container", false).unwrap();
    assert!(rendered.contains("  flux container  Container read operations"));
    assert!(rendered.contains("synapse flux container list"));
    assert!(!rendered.contains("Quick Start"));
}

#[test]
fn top_level_flux_help_groups_usage_and_uses_white_content() {
    let rendered = render_command("flux", true).unwrap();
    assert!(rendered.contains(&format!("{CYAN_ANSI}Docker{RESET}")));
    assert!(rendered.contains(&format!("{CYAN_ANSI}Containers{RESET}")));
    assert!(rendered.contains(&format!("{CYAN_ANSI}Host{RESET}")));
    assert!(rendered.contains(&format!("{CYAN_ANSI}Compose{RESET}")));
    assert!(rendered.contains(&format!(
        "{PRIMARY_ANSI}synapse flux docker images [--host H] [--dangling-only]{RESET}"
    )));
    assert!(!rendered.contains(&format!("{CYAN_ANSI}synapse flux docker images")));
}

#[test]
fn top_level_scout_help_groups_usage_and_uses_white_content() {
    let rendered = render_command("scout", true).unwrap();
    assert!(rendered.contains(&format!("{CYAN_ANSI}Inventory & Files{RESET}")));
    assert!(rendered.contains(&format!("{CYAN_ANSI}Processes & Exec{RESET}")));
    assert!(rendered.contains(&format!("{CYAN_ANSI}Transfer{RESET}")));
    assert!(rendered.contains(&format!("{CYAN_ANSI}ZFS & Logs{RESET}")));
    assert!(rendered.contains(&format!("{PRIMARY_ANSI}synapse scout nodes{RESET}")));
    assert!(!rendered.contains(&format!("{CYAN_ANSI}synapse scout nodes")));
}

#[test]
fn help_classification_is_positional() {
    assert_eq!(
        classify_help(&["flux".into(), "container".into(), "--help".into()]),
        HelpRequest::Command("flux container".into())
    );
    assert_eq!(classify_help(&["help".into()]), HelpRequest::TopLevel);
    assert_eq!(
        classify_help(&[
            "scout".into(),
            "find".into(),
            "--pattern".into(),
            "help".into()
        ]),
        HelpRequest::None
    );
}

use super::color::{CYAN_ANSI, PRIMARY_ANSI};
use catalog::{
    ENVIRONMENT, FLUX_USAGE_SECTIONS, GLOBAL_OPTIONS, QUICK_START, SCOUT_USAGE_SECTIONS, SECTIONS,
    UsageSection, lookup, nested_lookup,
};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const TAGLINE: &str = "Host, container, and SSH operations for MCP agents";

mod catalog;

fn paint(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn heading(color: bool, text: &str) -> String {
    if color {
        format!("{BOLD}{CYAN_ANSI}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn push_row(
    out: &mut String,
    color: bool,
    indent: usize,
    label_width: usize,
    label_code: &str,
    label: &str,
    desc: &str,
) {
    if label.chars().count() > label_width {
        out.push_str(&format!(
            "{:indent$}{}\n",
            "",
            paint(color, label_code, label),
            indent = indent
        ));
        out.push_str(&format!(
            "{:width$}{}\n",
            "",
            paint(color, PRIMARY_ANSI, desc),
            width = indent + label_width + 1
        ));
        return;
    }
    let padded = format!("{label:<label_width$}");
    out.push_str(&format!(
        "{:indent$}{} {}\n",
        "",
        paint(color, label_code, &padded),
        paint(color, PRIMARY_ANSI, desc),
        indent = indent
    ));
}

pub(crate) fn render_top_level(color: bool) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(&format!("  {}\n", heading(color, "SYNAPSE2 CLI")));
    out.push_str(&format!(
        "  {}\n",
        paint(color, CYAN_ANSI, "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
    ));
    out.push_str(&format!(
        "  Version {}  |  {}\n\n",
        env!("CARGO_PKG_VERSION"),
        paint(color, PRIMARY_ANSI, TAGLINE)
    ));

    out.push_str(&format!("  {}\n", heading(color, "Usage")));
    out.push_str(&format!(
        "  {}\n\n",
        paint(color, PRIMARY_ANSI, "synapse [options] <command> [args]")
    ));

    out.push_str(&format!("  {}\n", heading(color, "Quick Start")));
    for example in QUICK_START {
        out.push_str(&format!("  {}\n", paint(color, PRIMARY_ANSI, example)));
    }
    out.push('\n');

    out.push_str(&format!("  {}\n", heading(color, "Global Options")));
    for (flag, desc) in GLOBAL_OPTIONS {
        push_row(&mut out, color, 2, 28, PRIMARY_ANSI, flag, desc);
    }
    out.push('\n');

    out.push_str(&format!("  {}\n", heading(color, "Environment")));
    for (name, desc) in ENVIRONMENT {
        push_row(&mut out, color, 2, 28, PRIMARY_ANSI, name, desc);
    }
    out.push('\n');

    out.push_str(&format!("  {}\n", heading(color, "Commands")));
    for (section, names) in SECTIONS {
        out.push_str(&format!("  {}\n", paint(color, CYAN_ANSI, section)));
        for name in *names {
            if let Some(doc) = lookup(name) {
                push_row(&mut out, color, 4, 18, PRIMARY_ANSI, doc.name, doc.summary);
            }
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "  {}\n",
        paint(
            color,
            PRIMARY_ANSI,
            "→ Run synapse <command> --help for command-specific flags"
        )
    ));
    out
}

pub(crate) fn render_command(name: &str, color: bool) -> Option<String> {
    if name == "flux" {
        return Some(render_grouped_doc(
            "flux",
            "Docker, container, host, and compose operations",
            FLUX_USAGE_SECTIONS,
            color,
        ));
    }
    if name == "scout" {
        return Some(render_grouped_doc(
            "scout",
            "SSH filesystem, process, transfer, ZFS, and log operations",
            SCOUT_USAGE_SECTIONS,
            color,
        ));
    }
    if let Some(doc) = nested_lookup(name) {
        return Some(render_doc(doc.path, doc.summary, doc.usage, color));
    }
    let doc = lookup(name)?;
    Some(render_doc(doc.name, doc.summary, doc.usage, color))
}

fn render_doc(name: &str, summary: &str, usage: &[&str], color: bool) -> String {
    let mut out = String::with_capacity(512);
    out.push_str(&format!(
        "  {}  {}\n\n",
        heading(color, name),
        paint(color, PRIMARY_ANSI, summary)
    ));
    out.push_str(&format!("  {}\n", heading(color, "Usage")));
    for line in usage {
        out.push_str(&format!("  {}\n", paint(color, PRIMARY_ANSI, line)));
    }
    out
}

fn render_grouped_doc(name: &str, summary: &str, sections: &[UsageSection], color: bool) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str(&format!(
        "  {}  {}\n\n",
        heading(color, name),
        paint(color, PRIMARY_ANSI, summary)
    ));
    out.push_str(&format!("  {}\n", heading(color, "Usage")));
    for section in sections {
        out.push_str(&format!("  {}\n", heading(color, section.title)));
        for line in section.lines {
            out.push_str(&format!("  {}\n", paint(color, PRIMARY_ANSI, line)));
        }
        out.push('\n');
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HelpRequest {
    TopLevel,
    Command(String),
    None,
}

pub(crate) fn classify_help(args: &[String]) -> HelpRequest {
    let scan: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .take_while(|arg| *arg != "--")
        .collect();
    if scan.is_empty() {
        return HelpRequest::None;
    }
    let has_help =
        scan.iter().any(|arg| matches!(*arg, "-h" | "--help")) || scan.first() == Some(&"help");
    if !has_help {
        return HelpRequest::None;
    }

    let positionals: Vec<&str> = scan
        .into_iter()
        .filter(|arg| !arg.starts_with('-') && *arg != "help")
        .collect();
    if positionals.len() >= 2 {
        let nested = format!("{} {}", positionals[0], positionals[1]);
        if nested_lookup(&nested).is_some() {
            return HelpRequest::Command(nested);
        }
    }
    match positionals.first().copied() {
        Some(name) if lookup(name).is_some() => HelpRequest::Command(name.to_string()),
        _ => HelpRequest::TopLevel,
    }
}

pub(crate) fn maybe_handle_help(args: &[String]) -> bool {
    match classify_help(args) {
        HelpRequest::TopLevel => {
            print!("{}", render_top_level(super::color::color_enabled()));
            true
        }
        HelpRequest::Command(name) => {
            if let Some(rendered) = render_command(&name, super::color::color_enabled()) {
                print!("{rendered}");
            } else {
                print!("{}", render_top_level(super::color::color_enabled()));
            }
            true
        }
        HelpRequest::None => false,
    }
}

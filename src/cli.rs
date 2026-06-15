//! CLI — thin shim that parses args, calls `SynapseService`, formats output.
//!
//! The CLI uses the same service layer as the MCP server. No business logic lives here.
//!
//! # Usage
//!
//! ```text
//! synapse2 flux container list --host local
//! synapse2 scout nodes
//! synapse2 doctor [--json]
//! ```

use crate::{
    actions::{
        ContainerArgs, DockerArgs, ScoutBeamArgs, ScoutDeltaArgs, ScoutEmitArgs, ScoutExecArgs,
        ScoutFindArgs, ScoutLogsArgs, ScoutPsArgs, ScoutZfsArgs, rest_help,
    },
    app::SynapseService,
};
use anyhow::{Result, anyhow};

// TEMPLATE: The doctor module is the §48 reference implementation.
//           Import it from here and wire into run() below.
pub(crate) mod color;
pub mod doctor;
mod flux;
pub(crate) mod help;
mod scout;
pub mod setup;
pub mod watch;

pub use setup::{SetupCommand, run_setup};

pub fn usage() -> String {
    help::render_top_level(false)
}

pub fn install_color_from_args(args: &mut Vec<String>) -> Result<()> {
    color::install_color_from_args(args)
}

pub fn maybe_handle_help(args: &[String]) -> bool {
    help::maybe_handle_help(args)
}

pub fn print_top_level_help_stderr() {
    eprint!("{}", help::render_top_level(color::color_enabled_stderr()));
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Docker subactions. Params boxed in [`DockerArgs`] (shared with
    /// [`crate::actions::SynapseAction`]) so the enum stays small.
    FluxDocker(Box<DockerArgs>),
    /// Container read-only subactions. Params are boxed in [`ContainerArgs`]
    /// (shared with [`crate::actions::SynapseAction`]) so the enum stays small.
    FluxContainer(Box<ContainerArgs>),
    FluxHost(Box<crate::actions::HostArgs>),
    FluxCompose(Box<crate::actions::ComposeArgs>),
    ScoutNodes {
        response_format: Option<String>,
    },
    ScoutPeek {
        response_format: Option<String>,
        host: String,
        path: String,
        tree: bool,
        depth: u8,
    },
    ScoutFind(Box<ScoutFindArgs>),
    ScoutPs(Box<ScoutPsArgs>),
    ScoutDf {
        response_format: Option<String>,
        host: String,
        path: Option<String>,
    },
    ScoutDelta(Box<ScoutDeltaArgs>),
    ScoutExec(Box<ScoutExecArgs>),
    ScoutEmit(Box<ScoutEmitArgs>),
    ScoutBeam(Box<ScoutBeamArgs>),
    /// B15: ZFS subactions via CLI.
    ScoutZfs(Box<ScoutZfsArgs>),
    /// B15: Log subactions via CLI.
    ScoutLogs(Box<ScoutLogsArgs>),
    Help {
        response_format: Option<String>,
    },
    /// Pre-flight environment validation (§48).
    ///
    /// TEMPLATE: Always keep this command. It is the operator's first stop
    /// when setting up or debugging the service.
    Doctor {
        /// Output JSON instead of human-readable text.
        json: bool,
    },
    /// Poll the MCP server health endpoint and emit a line on every state change.
    ///
    /// Designed to be run as a plugin monitor — stdout is the event stream,
    /// stderr is debug output. Exits only on CTRL+C.
    Watch {
        /// Base URL of the MCP server (default: http://localhost:{SYNAPSE_MCP_PORT}).
        url: Option<String>,
        /// Poll interval in seconds (default: 10).
        interval: u64,
    },
    Setup(SetupCommand),
}

/// Parse CLI arguments from `std::env::args()`.
///
/// Returns `None` if the first argument is not a known subcommand.
/// **Template**: extend this to use clap or another arg parser for a real CLI.
/// This is intentionally minimal so the template compiles without extra deps.
///
/// # TEMPLATE: Adding a new subcommand
///
/// 1. Add a variant to `Command` above.
/// 2. Add a match arm here to construct it from args.
/// 3. Add a dispatch arm in `run()` below.
/// 4. Update `src/cli/help.rs`.
pub fn parse_args() -> Result<Option<Command>> {
    parse_args_from(std::env::args().skip(1))
}

pub fn parse_args_from<I, S>(args: I) -> Result<Option<Command>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let command = match args.as_slice() {
        [] => None,
        [subcommand, rest @ ..] => match subcommand.as_str() {
            "flux" => Some(flux::parse_flux(rest)?),
            "scout" => Some(scout::parse_scout(rest)?),
            "help" => Some(Command::Help {
                response_format: parse_output_format_flag(rest, "help")?,
            }),
            // §48: doctor is always parsed here, dispatched via run_cli in main.rs.
            // TEMPLATE: Keep this arm. It routes to doctor::run_doctor() which needs
            //           the full Config (not just SynapseConfig), so main.rs handles it.
            "doctor" => {
                let json = parse_bool_flag(rest, "doctor", "--json")?;
                Some(Command::Doctor { json })
            }
            "watch" => {
                let (url, interval_arg) = parse_watch_flags(rest)?;
                let interval = match interval_arg {
                    Some(v) => v.parse().map_err(|_| {
                        anyhow!("watch --interval must be a positive integer number of seconds")
                    })?,
                    None => 10,
                };
                if interval == 0 {
                    return Err(anyhow!(
                        "watch --interval must be a positive integer number of seconds"
                    ));
                }
                Some(Command::Watch { url, interval })
            }
            "setup" => match rest {
                [action, flags @ ..] if action == "check" => {
                    reject_args(flags, "setup check")?;
                    Some(Command::Setup(SetupCommand::Check))
                }
                [action, flags @ ..] if action == "repair" => {
                    reject_args(flags, "setup repair")?;
                    Some(Command::Setup(SetupCommand::Repair))
                }
                [action, flags @ ..] if action == "install" => {
                    reject_args(flags, "setup install")?;
                    Some(Command::Setup(SetupCommand::Install))
                }
                [action, flags @ ..] if action == "plugin-hook" => {
                    let no_repair = parse_bool_flag(flags, "setup plugin-hook", "--no-repair")?;
                    Some(Command::Setup(SetupCommand::PluginHook { no_repair }))
                }
                _ => None,
            },
            _ => None,
        },
    };
    Ok(command)
}

/// Run a CLI command, print the result, and exit.
///
/// `Doctor`, `Watch`, and `Setup` are handled directly in `main.rs::run_cli`
/// because they need full `Config` fields; everything else flows through here.
pub async fn run(cmd: Command) -> Result<()> {
    let service = SynapseService::new();

    // Delegation marker for the surface checker: command arms pass through to
    // helpers that invoke service.* methods, keeping business logic out of CLI.
    // The CLI is human-driven: the operator running the command IS the
    // confirmation gate. `CliStderrWarn` prints a single warning line for
    // destructive ops and proceeds (B5 design).
    let confirmer = crate::elicitation_gate::CliStderrWarn;

    let result = match &cmd {
        Command::FluxDocker(args) => flux::run_docker(args, &service, &confirmer).await?,
        Command::FluxContainer(args) => flux::run_container(args, &service, &confirmer).await?,
        Command::FluxHost(args) => flux::run_host(args, &service).await?,
        Command::FluxCompose(args) => flux::run_compose(args, &service, &confirmer).await?,
        Command::ScoutNodes { .. }
        | Command::ScoutPeek { .. }
        | Command::ScoutFind(_)
        | Command::ScoutPs(_)
        | Command::ScoutDf { .. }
        | Command::ScoutDelta(_)
        | Command::ScoutExec(_)
        | Command::ScoutEmit(_)
        | Command::ScoutBeam(_)
        | Command::ScoutZfs(_)
        | Command::ScoutLogs(_) => scout::run_scout(&cmd, &service, &confirmer).await?,
        Command::Help { .. } => rest_help(),
        // Doctor, Watch, and Setup are never dispatched via this function — main.rs
        // handles them directly because they need config.mcp fields.
        Command::Doctor { .. } | Command::Watch { .. } | Command::Setup(_) => {
            unreachable!("dispatched directly in main.rs::run_cli")
        }
    };

    println!("{}", render_cli_output(&cmd, &result)?);
    Ok(())
}

pub(crate) fn render_cli_output(cmd: &Command, result: &serde_json::Value) -> Result<String> {
    let (tool, action, subaction, response_format) = match cmd {
        Command::FluxDocker(args) => (
            "flux",
            "docker",
            Some(args.subaction.as_str()),
            args.response_format.as_deref(),
        ),
        Command::FluxContainer(args) => (
            "flux",
            "container",
            Some(args.subaction.as_str()),
            args.response_format.as_deref(),
        ),
        Command::FluxHost(args) => (
            "flux",
            "host",
            Some(args.subaction.as_str()),
            args.response_format.as_deref(),
        ),
        Command::FluxCompose(args) => (
            "flux",
            "compose",
            Some(args.subaction.as_str()),
            args.response_format.as_deref(),
        ),
        Command::ScoutNodes { response_format } => {
            ("scout", "nodes", None, response_format.as_deref())
        }
        Command::ScoutPeek {
            response_format, ..
        } => ("scout", "peek", None, response_format.as_deref()),
        Command::ScoutFind(args) => ("scout", "find", None, args.response_format.as_deref()),
        Command::ScoutPs(args) => ("scout", "ps", None, args.response_format.as_deref()),
        Command::ScoutDf {
            response_format, ..
        } => ("scout", "df", None, response_format.as_deref()),
        Command::ScoutDelta(args) => ("scout", "delta", None, args.response_format.as_deref()),
        Command::ScoutExec(args) => ("scout", "exec", None, args.response_format.as_deref()),
        Command::ScoutEmit(args) => ("scout", "emit", None, args.response_format.as_deref()),
        Command::ScoutBeam(args) => ("scout", "beam", None, args.response_format.as_deref()),
        Command::ScoutZfs(args) => (
            "scout",
            "zfs",
            Some(args.subaction.as_str()),
            args.response_format.as_deref(),
        ),
        Command::ScoutLogs(args) => (
            "scout",
            "logs",
            Some(args.subaction.as_str()),
            args.response_format.as_deref(),
        ),
        Command::Help { response_format } => ("flux", "help", None, response_format.as_deref()),
        Command::Doctor { .. } | Command::Watch { .. } | Command::Setup(_) => {
            unreachable!("dispatched directly in main.rs::run_cli")
        }
    };
    crate::formatters::render_action_output(tool, action, subaction, response_format, result)
        .map_err(anyhow::Error::msg)
}

// ── arg parsing helpers ───────────────────────────────────────────────────────

fn reject_args(args: &[String], command: &str) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("{command} does not accept argument `{}`", args[0]))
    }
}

pub(super) fn parse_required_named_value(args: &[String], flag: &str) -> Result<String> {
    parse_optional_named_value(args, flag)?.ok_or_else(|| anyhow!("missing required {flag}"))
}

pub(super) fn parse_optional_named_value(args: &[String], flag: &str) -> Result<Option<String>> {
    let mut value = None;
    let mut index = 0;
    while index < args.len() {
        let found_flag = args[index].as_str();
        if !found_flag.starts_with("--") {
            return Err(anyhow!("unexpected argument `{found_flag}`"));
        }
        let Some(found_value) = args.get(index + 1) else {
            return Err(anyhow!("missing value after {found_flag}"));
        };
        if found_value.starts_with("--") {
            return Err(anyhow!("missing value after {found_flag}"));
        }
        if found_flag == flag {
            if value.is_some() {
                return Err(anyhow!("duplicate {flag}"));
            }
            value = Some(found_value.clone());
        }
        index += 2;
    }
    Ok(value)
}

fn parse_bool_flag(args: &[String], command: &str, flag: &str) -> Result<bool> {
    let mut found = false;
    for arg in args {
        if arg == flag {
            if found {
                return Err(anyhow!("{command} received duplicate {flag}"));
            }
            found = true;
        } else {
            return Err(anyhow!("{command} does not accept argument `{arg}`"));
        }
    }
    Ok(found)
}

pub(super) fn parse_output_format_flag(args: &[String], command: &str) -> Result<Option<String>> {
    match parse_optional_response_format(args)? {
        Some(value) => Ok(Some(value)),
        None => {
            reject_args(args, command)?;
            Ok(None)
        }
    }
}

pub(super) fn parse_optional_response_format(args: &[String]) -> Result<Option<String>> {
    let value = parse_optional_named_value(args, "--response-format")?;
    if let Some(value) = value.as_deref() {
        crate::formatters::ResponseFormat::parse(Some(value)).map_err(anyhow::Error::msg)?;
    }
    Ok(value)
}

fn parse_watch_flags(args: &[String]) -> Result<(Option<String>, Option<String>)> {
    let mut url = None;
    let mut interval = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let target = match flag {
            "--url" => &mut url,
            "--interval" => &mut interval,
            _ => return Err(anyhow!("watch does not accept argument `{flag}`")),
        };
        if target.is_some() {
            return Err(anyhow!("watch received duplicate {flag}"));
        }
        let Some(value) = args.get(index + 1) else {
            return Err(anyhow!("watch requires a value after {flag}"));
        };
        if value.starts_with("--") {
            return Err(anyhow!("watch requires a value after {flag}"));
        }
        *target = Some(value.clone());
        index += 2;
    }
    Ok((url, interval))
}

#[cfg(test)]
mod flux_tests;

#[cfg(test)]
mod help_tests;

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;

//! CLI — thin shim that parses args, calls `SynapseService`, formats output.
//!
//! The CLI uses the same service layer as the MCP server. No business logic lives here.
//!
//! **Template**: add subcommands to match your service's operations.
//!
//! # Usage
//!
//! ```text
//! synapse2 greet --name Alice
//! synapse2 echo --message "Hello!"
//! synapse2 status
//! synapse2 doctor [--json]
//! ```

use crate::{
    actions::{rest_help, ContainerArgs},
    app::SynapseService,
    config::SynapseConfig,
    synapse2::SynapseClient,
};
use anyhow::{anyhow, Result};

// TEMPLATE: The doctor module is the §48 reference implementation.
//           Import it from here and wire into run() below.
pub mod doctor;
pub mod setup;
pub mod watch;

pub use setup::{run_setup, SetupCommand};

pub const USAGE: &str = "Usage:
  synapse2 [serve]          Start MCP HTTP server (default)
  synapse2 mcp              Start MCP stdio transport

  synapse2 flux docker info|images|networks|volumes
  synapse2 flux container list [--host H] [--state S] [--name-filter N] [--image-filter I] [--label-filter K=V]
  synapse2 flux container inspect --container-id ID [--host H] [--summary]
  synapse2 flux container logs --container-id ID [--host H] [--lines N] [--since T] [--until T] [--grep S] [--stream stdout|stderr|both]
  synapse2 flux container stats [--container-id ID] [--host H]
  synapse2 flux container top --container-id ID [--host H]
  synapse2 flux container search --query Q [--host H]
    (all flux container subactions also accept [--response-format markdown|json])
  synapse2 flux host status [--host HOST]
  synapse2 scout nodes
  synapse2 scout peek --host HOST --path PATH
  synapse2 scout exec --host HOST --path PATH --command CMD
  synapse2 help                      Show JSON action reference
  synapse2 doctor [--json]           Run environment pre-flight checks
  synapse2 watch [--url URL] [--interval N]  Poll /health and emit on state change
  synapse2 setup check               Check plugin setup without mutating appdata
  synapse2 setup repair              Create missing appdata/env setup files
  synapse2 setup plugin-hook [--no-repair]  Plugin hook JSON contract

  synapse2 --help                    Show this help
  synapse2 --version                 Show version

Environment:
  SYNAPSE_API_URL          Upstream service URL
  SYNAPSE_API_KEY          Upstream service API key
  SYNAPSE_MCP_HOST         Bind host (default 127.0.0.1)
  SYNAPSE_MCP_PORT         Bind port (default 40060)
  SYNAPSE_MCP_NO_AUTH      Disable auth (loopback only)
  SYNAPSE_MCP_TOKEN        Static bearer token
  RUST_LOG                 Log filter (e.g. info,rmcp=warn)";

pub fn usage() -> &'static str {
    USAGE
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    FluxDocker {
        subaction: String,
    },
    /// Container read-only subactions. Params are boxed in [`ContainerArgs`]
    /// (shared with [`crate::actions::SynapseAction`]) so the enum stays small.
    FluxContainer(Box<ContainerArgs>),
    FluxHost {
        subaction: String,
        host: Option<String>,
    },
    ScoutNodes,
    ScoutPeek {
        host: String,
        path: String,
    },
    ScoutExec {
        host: String,
        path: String,
        command: String,
    },
    Help,
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
/// 4. Update `USAGE` above.
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
            "flux" => Some(parse_flux(rest)?),
            "scout" => Some(parse_scout(rest)?),
            "help" => {
                reject_args(rest, "help")?;
                Some(Command::Help)
            }
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
/// # TEMPLATE
/// - `Doctor` is handled specially in `main.rs::run_cli` (needs full `Config`).
/// - All other commands get only `SynapseConfig`; keep it that way.
/// - Add `--json` support to each new command by forwarding a `json` flag.
pub async fn run(cmd: Command, cfg: &SynapseConfig) -> Result<()> {
    let client = SynapseClient::new(cfg)?;
    let service = SynapseService::new(client);

    let result = match &cmd {
        Command::FluxDocker { subaction } => match subaction.as_str() {
            "info" => service.flux().docker_info().await?,
            "images" => service.flux().docker_images().await?,
            "networks" => service.flux().docker_networks().await?,
            "volumes" => service.flux().docker_volumes().await?,
            other => return Err(anyhow!("unknown flux docker subaction `{other}`")),
        },
        Command::FluxContainer(args) => {
            use crate::flux_service::container_read::{ListFilters, LogOptions, DEFAULT_LOG_LINES};
            let ContainerArgs {
                subaction,
                container_id,
                host,
                lines,
                state,
                name_filter,
                image_filter,
                label_filter,
                since,
                until,
                grep,
                stream,
                summary,
                query,
            } = args.as_ref();
            let flux = service.flux();
            let host = host.as_deref();
            match subaction.as_str() {
                "list" => {
                    let filters = ListFilters {
                        state: state.clone(),
                        name_filter: name_filter.clone(),
                        image_filter: image_filter.clone(),
                        label_filter: label_filter.clone(),
                    };
                    flux.container_list(host, filters).await?
                }
                "search" => {
                    let q = query
                        .as_deref()
                        .ok_or_else(|| anyhow!("container search requires --query"))?;
                    flux.container_search(host, q).await?
                }
                "stats" => flux.container_stats(host, container_id.as_deref()).await?,
                "inspect" => {
                    let id = container_id
                        .as_deref()
                        .ok_or_else(|| anyhow!("container inspect requires --container-id"))?;
                    flux.container_inspect(host, id, summary.unwrap_or(false))
                        .await?
                }
                "top" => {
                    let id = container_id
                        .as_deref()
                        .ok_or_else(|| anyhow!("container top requires --container-id"))?;
                    flux.container_top(host, id).await?
                }
                "logs" => {
                    let id = container_id
                        .as_deref()
                        .ok_or_else(|| anyhow!("container logs requires --container-id"))?;
                    let opts = LogOptions {
                        lines: lines.unwrap_or(DEFAULT_LOG_LINES),
                        since: since.clone(),
                        until: until.clone(),
                        grep: grep.clone(),
                        stream: stream.clone().unwrap_or_else(|| "both".to_owned()),
                    };
                    flux.container_logs(host, id, opts).await?
                }
                other => return Err(anyhow!("unknown flux container subaction `{other}`")),
            }
        }
        Command::FluxHost { subaction, host } => match subaction.as_str() {
            "status" => service.flux().host_status(host.as_deref()).await?,
            other => return Err(anyhow!("unknown flux host subaction `{other}`")),
        },
        Command::ScoutNodes => service.scout().nodes().await?,
        Command::ScoutPeek { host, path } => service.scout().peek(host, path).await?,
        Command::ScoutExec {
            host,
            path,
            command,
        } => service.scout().exec(host, path, command).await?,
        Command::Help => rest_help(),
        // Doctor, Watch, and Setup are never dispatched via this function — main.rs
        // handles them directly because they need config.mcp fields.
        Command::Doctor { .. } | Command::Watch { .. } | Command::Setup(_) => {
            unreachable!("dispatched directly in main.rs::run_cli")
        }
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// ── arg parsing helpers ───────────────────────────────────────────────────────

fn reject_args(args: &[String], command: &str) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("{command} does not accept argument `{}`", args[0]))
    }
}

fn parse_flux(args: &[String]) -> Result<Command> {
    match args {
        [group, subaction] if group == "docker" => Ok(Command::FluxDocker {
            subaction: subaction.clone(),
        }),
        [group, subaction, rest @ ..] if group == "container" => {
            // `--summary` is a valueless bool flag; split it out before the
            // value-pair parser (which requires a value after every flag).
            let summary = rest.iter().any(|a| a == "--summary");
            let value_args: Vec<String> =
                rest.iter().filter(|a| *a != "--summary").cloned().collect();
            let container_id = parse_optional_named_value(&value_args, "--container-id")?;
            let lines = parse_optional_named_value(&value_args, "--lines")?
                .map(|value| value.parse())
                .transpose()
                .map_err(|_| anyhow!("--lines must be an integer"))?;
            // Validate `--response-format` for MCP/CLI parity (output stays JSON
            // for the CLI today; an invalid value is still a hard error).
            if let Some(rf) = parse_optional_named_value(&value_args, "--response-format")? {
                crate::formatters::ResponseFormat::parse(Some(&rf)).map_err(|e| anyhow!(e))?;
            }
            Ok(Command::FluxContainer(Box::new(ContainerArgs {
                subaction: subaction.clone(),
                container_id,
                host: parse_optional_named_value(&value_args, "--host")?,
                lines,
                state: parse_optional_named_value(&value_args, "--state")?,
                name_filter: parse_optional_named_value(&value_args, "--name-filter")?,
                image_filter: parse_optional_named_value(&value_args, "--image-filter")?,
                label_filter: parse_optional_named_value(&value_args, "--label-filter")?,
                since: parse_optional_named_value(&value_args, "--since")?,
                until: parse_optional_named_value(&value_args, "--until")?,
                grep: parse_optional_named_value(&value_args, "--grep")?,
                stream: parse_optional_named_value(&value_args, "--stream")?,
                summary: summary.then_some(true),
                query: parse_optional_named_value(&value_args, "--query")?,
            })))
        }
        [group, subaction, rest @ ..] if group == "host" => Ok(Command::FluxHost {
            subaction: subaction.clone(),
            host: parse_optional_value_flag(rest, "flux host", "--host")?,
        }),
        _ => Err(anyhow!("unknown flux command")),
    }
}

fn parse_scout(args: &[String]) -> Result<Command> {
    match args {
        [action] if action == "nodes" => Ok(Command::ScoutNodes),
        [action, rest @ ..] if action == "peek" => Ok(Command::ScoutPeek {
            host: parse_required_named_value(rest, "--host")?,
            path: parse_required_named_value(rest, "--path")?,
        }),
        [action, rest @ ..] if action == "exec" => Ok(Command::ScoutExec {
            host: parse_required_named_value(rest, "--host")?,
            path: parse_required_named_value(rest, "--path")?,
            command: parse_required_named_value(rest, "--command")?,
        }),
        _ => Err(anyhow!("unknown scout command")),
    }
}

fn parse_required_named_value(args: &[String], flag: &str) -> Result<String> {
    parse_optional_named_value(args, flag)?.ok_or_else(|| anyhow!("missing required {flag}"))
}

fn parse_optional_named_value(args: &[String], flag: &str) -> Result<Option<String>> {
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

fn parse_optional_value_flag(args: &[String], command: &str, flag: &str) -> Result<Option<String>> {
    match args {
        [] => Ok(None),
        [found_flag, value] if found_flag == flag => {
            if value.starts_with("--") {
                Err(anyhow!("{command} requires a value after {flag}"))
            } else {
                Ok(Some(value.clone()))
            }
        }
        [found_flag] if found_flag == flag => {
            Err(anyhow!("{command} requires a value after {flag}"))
        }
        [found_flag, value, rest @ ..] if found_flag == flag => {
            if value.starts_with("--") {
                Err(anyhow!("{command} requires a value after {flag}"))
            } else if rest.iter().any(|arg| arg == flag) {
                Err(anyhow!("{command} received duplicate {flag}"))
            } else {
                Err(anyhow!("{command} does not accept argument `{}`", rest[0]))
            }
        }
        [unexpected, ..] => Err(anyhow!("{command} does not accept argument `{unexpected}`")),
    }
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
#[path = "cli_tests.rs"]
mod tests;

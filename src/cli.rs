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
    actions::{
        rest_help, ContainerArgs, DockerArgs, ScoutBeamArgs, ScoutDeltaArgs, ScoutEmitArgs,
        ScoutExecArgs, ScoutFindArgs, ScoutPsArgs,
    },
    app::SynapseService,
    config::SynapseConfig,
    synapse2::SynapseClient,
};
use anyhow::{anyhow, Result};

// TEMPLATE: The doctor module is the §48 reference implementation.
//           Import it from here and wire into run() below.
pub mod doctor;
mod flux;
mod scout;
pub mod setup;
pub mod watch;

pub use setup::{run_setup, SetupCommand};

pub const USAGE: &str = "Usage:
  synapse2 [serve]          Start MCP HTTP server (default)
  synapse2 mcp              Start MCP stdio transport

  synapse2 flux docker info|df|networks|volumes [--host H]
  synapse2 flux docker images [--host H] [--dangling-only]
  synapse2 flux docker pull --host H --image IMG
  synapse2 flux docker build --host H --context /abs/path --tag TAG [--dockerfile REL] [--no-cache]
  synapse2 flux docker rmi --host H --image IMG --force
  synapse2 flux docker prune --host H --target containers|images|volumes|networks|buildcache|all --force
  synapse2 flux container list [--host H] [--state S] [--name-filter N] [--image-filter I] [--label-filter K=V]
  synapse2 flux container inspect --container-id ID [--host H] [--summary]
  synapse2 flux container logs --container-id ID [--host H] [--lines N] [--since T] [--until T] [--grep S] [--stream stdout|stderr|both]
  synapse2 flux container stats [--container-id ID] [--host H]
  synapse2 flux container top --container-id ID [--host H]
  synapse2 flux container search --query Q [--host H]
    (all flux container subactions also accept [--response-format markdown|json])
  synapse2 flux host status [--host HOST]
  synapse2 flux host info [--host HOST]
  synapse2 flux host uptime [--host HOST]
  synapse2 flux host resources [--host HOST]
  synapse2 flux host services --host HOST [--state STATE] [--service NAME]
  synapse2 flux host network [--host HOST]
  synapse2 flux host mounts --host HOST
  synapse2 flux host ports --host HOST [--protocol tcp|udp] [--limit N] [--offset N]
  synapse2 flux host doctor --host HOST [--checks c1,c2,...]
  synapse2 flux compose list --host HOST
  synapse2 flux compose status --host HOST --project P [--service SVC]
  synapse2 flux compose up --host HOST --project P
  synapse2 flux compose down --host HOST --project P [--remove-volumes --force]
  synapse2 flux compose restart --host HOST --project P
  synapse2 flux compose recreate --host HOST --project P
  synapse2 flux compose logs --host HOST --project P [--lines N] [--since T] [--service SVC]
  synapse2 flux compose build --host HOST --project P [--service SVC]
  synapse2 flux compose pull --host HOST --project P [--service SVC]
  synapse2 flux compose refresh --host HOST
  synapse2 scout nodes
  synapse2 scout peek --host HOST --path PATH [--tree] [--depth N]
  synapse2 scout find --host HOST --path PATH --pattern GLOB [--depth N] [--limit N]
  synapse2 scout ps --host HOST [--sort cpu|mem|pid|time] [--grep S] [--user U] [--limit N]
  synapse2 scout df --host HOST [--path PATH]
  synapse2 scout delta --source-host H --source-path P (--target-host H --target-path P | --content STR)
  synapse2 scout exec --host HOST --command CMD [--path PATH] [--args A1 A2...]
  synapse2 scout emit --command CMD --target HOST:PATH[,HOST:PATH...] [--timeout S]
  synapse2 scout beam --source-host H --source-path P --dest-host H --dest-path P
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
    /// Docker subactions. Params boxed in [`DockerArgs`] (shared with
    /// [`crate::actions::SynapseAction`]) so the enum stays small.
    FluxDocker(Box<DockerArgs>),
    /// Container read-only subactions. Params are boxed in [`ContainerArgs`]
    /// (shared with [`crate::actions::SynapseAction`]) so the enum stays small.
    FluxContainer(Box<ContainerArgs>),
    FluxHost(Box<crate::actions::HostArgs>),
    FluxCompose(Box<crate::actions::ComposeArgs>),
    ScoutNodes,
    ScoutPeek {
        host: String,
        path: String,
        tree: bool,
        depth: u8,
    },
    ScoutFind(Box<ScoutFindArgs>),
    ScoutPs(Box<ScoutPsArgs>),
    ScoutDf {
        host: String,
        path: Option<String>,
    },
    ScoutDelta(Box<ScoutDeltaArgs>),
    ScoutExec(Box<ScoutExecArgs>),
    ScoutEmit(Box<ScoutEmitArgs>),
    ScoutBeam(Box<ScoutBeamArgs>),
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
            "flux" => Some(flux::parse_flux(rest)?),
            "scout" => Some(scout::parse_scout(rest)?),
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

    // The CLI is human-driven: the operator running the command IS the
    // confirmation gate. `CliStderrWarn` prints a single warning line for
    // destructive ops and proceeds (B5 design).
    let confirmer = crate::elicitation_gate::CliStderrWarn;

    let result = match &cmd {
        Command::FluxDocker(args) => flux::run_docker(args, &service, &confirmer).await?,
        Command::FluxContainer(args) => flux::run_container(args, &service).await?,
        Command::FluxHost(args) => flux::run_host(args, &service).await?,
        Command::FluxCompose(args) => flux::run_compose(args, &service, &confirmer).await?,
        Command::ScoutNodes
        | Command::ScoutPeek { .. }
        | Command::ScoutFind(_)
        | Command::ScoutPs(_)
        | Command::ScoutDf { .. }
        | Command::ScoutDelta(_)
        | Command::ScoutExec(_)
        | Command::ScoutEmit(_)
        | Command::ScoutBeam(_) => scout::run_scout(&cmd, &service, &confirmer).await?,
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

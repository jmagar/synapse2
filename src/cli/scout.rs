//! CLI scout subtree — parse and run helpers for `scout *`.
//!
//! `parse_scout` builds the `Command` variant; `run_scout` executes it.
//! All calls delegate to `ScoutService` via the thin shim.

use crate::{
    actions::{
        ScoutBeamArgs, ScoutDeltaArgs, ScoutEmitArgs, ScoutEmitTarget, ScoutExecArgs,
        ScoutFindArgs, ScoutLogsArgs, ScoutPsArgs, ScoutZfsArgs,
    },
    app::SynapseService,
    elicitation_gate::CliStderrWarn,
    scout_service::logs::{DEFAULT_LINES, MAX_LINES},
};
use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use super::Command;

// ── parse ─────────────────────────────────────────────────────────────────────

pub(super) fn parse_scout(args: &[String]) -> Result<Command> {
    match args {
        [action] if action == "nodes" => Ok(Command::ScoutNodes),
        [action, rest @ ..] if action == "peek" => {
            let tree = rest.iter().any(|a| a == "--tree");
            let value_args: Vec<String> = rest.iter().filter(|a| *a != "--tree").cloned().collect();
            let depth = super::parse_optional_named_value(&value_args, "--depth")?
                .map(|v| v.parse::<u8>().unwrap_or(3).clamp(1, 10))
                .unwrap_or(3);
            Ok(Command::ScoutPeek {
                host: super::parse_required_named_value(&value_args, "--host")?,
                path: super::parse_required_named_value(&value_args, "--path")?,
                tree,
                depth,
            })
        }
        [action, rest @ ..] if action == "find" => {
            let depth = super::parse_optional_named_value(rest, "--depth")?
                .map(|v| v.parse::<u8>().unwrap_or(10).clamp(1, 20));
            let limit = super::parse_optional_named_value(rest, "--limit")?
                .map(|v| v.parse::<u32>().unwrap_or(500));
            Ok(Command::ScoutFind(Box::new(ScoutFindArgs {
                host: super::parse_required_named_value(rest, "--host")?,
                path: super::parse_required_named_value(rest, "--path")?,
                pattern: super::parse_required_named_value(rest, "--pattern")?,
                depth,
                limit,
            })))
        }
        [action, rest @ ..] if action == "ps" => {
            let limit = super::parse_optional_named_value(rest, "--limit")?
                .map(|v| v.parse::<u32>().unwrap_or(50));
            Ok(Command::ScoutPs(Box::new(ScoutPsArgs {
                host: super::parse_required_named_value(rest, "--host")?,
                sort: super::parse_optional_named_value(rest, "--sort")?,
                grep: super::parse_optional_named_value(rest, "--grep")?,
                user: super::parse_optional_named_value(rest, "--user")?,
                limit,
            })))
        }
        [action, rest @ ..] if action == "df" => Ok(Command::ScoutDf {
            host: super::parse_required_named_value(rest, "--host")?,
            path: super::parse_optional_named_value(rest, "--path")?,
        }),
        [action, rest @ ..] if action == "delta" => {
            Ok(Command::ScoutDelta(Box::new(ScoutDeltaArgs {
                source_host: super::parse_required_named_value(rest, "--source-host")?,
                source_path: super::parse_required_named_value(rest, "--source-path")?,
                target_host: super::parse_optional_named_value(rest, "--target-host")?,
                target_path: super::parse_optional_named_value(rest, "--target-path")?,
                content: super::parse_optional_named_value(rest, "--content")?,
            })))
        }
        [action, rest @ ..] if action == "exec" => {
            let timeout_secs = super::parse_optional_named_value(rest, "--timeout")?
                .map(|v| v.parse::<u64>().unwrap_or(30));
            // Collect remaining non-flag args as positional args (simplified).
            Ok(Command::ScoutExec(Box::new(ScoutExecArgs {
                host: super::parse_required_named_value(rest, "--host")?,
                path: super::parse_optional_named_value(rest, "--path")?,
                command: super::parse_required_named_value(rest, "--command")?,
                args: Vec::new(), // CLI doesn't support extra args for simplicity
                timeout_secs,
            })))
        }
        [action, rest @ ..] if action == "emit" => {
            // --target HOST:PATH[,HOST:PATH,...] (comma-separated)
            let raw_targets = super::parse_required_named_value(rest, "--target")?;
            let targets: Vec<ScoutEmitTarget> = raw_targets
                .split(',')
                .map(|s| {
                    let s = s.trim();
                    if let Some((host, path)) = s.split_once(':') {
                        ScoutEmitTarget {
                            host: host.to_owned(),
                            path: Some(path.to_owned()),
                        }
                    } else {
                        ScoutEmitTarget {
                            host: s.to_owned(),
                            path: None,
                        }
                    }
                })
                .collect();
            let timeout_secs = super::parse_optional_named_value(rest, "--timeout")?
                .map(|v| v.parse::<u64>().unwrap_or(30));
            Ok(Command::ScoutEmit(Box::new(ScoutEmitArgs {
                targets,
                command: super::parse_required_named_value(rest, "--command")?,
                args: Vec::new(),
                timeout_secs,
            })))
        }
        [action, rest @ ..] if action == "beam" => {
            Ok(Command::ScoutBeam(Box::new(ScoutBeamArgs {
                source_host: super::parse_required_named_value(rest, "--source-host")?,
                source_path: super::parse_required_named_value(rest, "--source-path")?,
                dest_host: super::parse_required_named_value(rest, "--dest-host")?,
                dest_path: super::parse_required_named_value(rest, "--dest-path")?,
            })))
        }
        [action, subaction, rest @ ..] if action == "zfs" => parse_scout_zfs(subaction, rest),
        [action, subaction, rest @ ..] if action == "logs" => parse_scout_logs(subaction, rest),
        _ => Err(anyhow!("unknown scout command")),
    }
}

fn parse_scout_zfs(subaction: &str, rest: &[String]) -> Result<Command> {
    let host = super::parse_required_named_value(rest, "--host")?;
    match subaction {
        "pools" => Ok(Command::ScoutZfs(Box::new(ScoutZfsArgs {
            host,
            subaction: "pools".to_owned(),
            pool: super::parse_optional_named_value(rest, "--pool")?,
            ..Default::default()
        }))),
        "datasets" => {
            let recursive = rest.iter().any(|a| a == "--recursive");
            let value_args: Vec<String> = rest
                .iter()
                .filter(|a| *a != "--recursive")
                .cloned()
                .collect();
            Ok(Command::ScoutZfs(Box::new(ScoutZfsArgs {
                host,
                subaction: "datasets".to_owned(),
                pool: super::parse_optional_named_value(&value_args, "--pool")?,
                dataset_type: super::parse_optional_named_value(&value_args, "--type")?,
                recursive,
                ..Default::default()
            })))
        }
        "snapshots" => {
            let limit = super::parse_optional_named_value(rest, "--limit")?
                .map(|v| v.parse::<u32>().unwrap_or(0));
            Ok(Command::ScoutZfs(Box::new(ScoutZfsArgs {
                host,
                subaction: "snapshots".to_owned(),
                pool: super::parse_optional_named_value(rest, "--pool")?,
                dataset: super::parse_optional_named_value(rest, "--dataset")?,
                limit,
                ..Default::default()
            })))
        }
        other => {
            bail!("unknown zfs subaction `{other}`; must be one of: pools, datasets, snapshots")
        }
    }
}

fn parse_scout_logs(subaction: &str, rest: &[String]) -> Result<Command> {
    let host = super::parse_required_named_value(rest, "--host")?;
    let lines = super::parse_optional_named_value(rest, "--lines")?
        .map(|v| v.parse::<u32>().unwrap_or(DEFAULT_LINES))
        .unwrap_or(DEFAULT_LINES)
        .clamp(1, MAX_LINES);
    let grep = super::parse_optional_named_value(rest, "--grep")?;

    match subaction {
        "syslog" => Ok(Command::ScoutLogs(Box::new(ScoutLogsArgs {
            host,
            subaction: "syslog".to_owned(),
            lines,
            grep,
            ..Default::default()
        }))),
        "journal" => Ok(Command::ScoutLogs(Box::new(ScoutLogsArgs {
            host,
            subaction: "journal".to_owned(),
            lines,
            grep,
            unit: super::parse_optional_named_value(rest, "--unit")?,
            priority: super::parse_optional_named_value(rest, "--priority")?,
            since: super::parse_optional_named_value(rest, "--since")?,
            until: super::parse_optional_named_value(rest, "--until")?,
        }))),
        "dmesg" => Ok(Command::ScoutLogs(Box::new(ScoutLogsArgs {
            host,
            subaction: "dmesg".to_owned(),
            lines,
            grep,
            ..Default::default()
        }))),
        "auth" => Ok(Command::ScoutLogs(Box::new(ScoutLogsArgs {
            host,
            subaction: "auth".to_owned(),
            lines,
            grep,
            ..Default::default()
        }))),
        other => {
            bail!("unknown logs subaction `{other}`; must be one of: syslog, journal, dmesg, auth")
        }
    }
}

// ── run helpers ───────────────────────────────────────────────────────────────

pub(super) async fn run_scout(
    cmd: &Command,
    service: &SynapseService,
    confirmer: &CliStderrWarn,
) -> Result<Value> {
    let result = match cmd {
        Command::ScoutNodes => service.scout().nodes().await?,
        Command::ScoutPeek {
            host,
            path,
            tree,
            depth,
        } => service.scout().peek(host, path, *tree, *depth).await?,
        Command::ScoutFind(a) => {
            service
                .scout()
                .find(&a.host, &a.path, &a.pattern, a.depth, a.limit)
                .await?
        }
        Command::ScoutPs(a) => {
            service
                .scout()
                .ps(
                    &a.host,
                    a.sort.as_deref(),
                    a.grep.as_deref(),
                    a.user.as_deref(),
                    a.limit,
                )
                .await?
        }
        Command::ScoutDf { host, path } => service.scout().df(host, path.as_deref()).await?,
        Command::ScoutDelta(a) => {
            service
                .scout()
                .delta(
                    &a.source_host,
                    &a.source_path,
                    a.target_host.as_deref(),
                    a.target_path.as_deref(),
                    a.content.as_deref(),
                )
                .await?
        }
        Command::ScoutExec(a) => {
            service
                .scout()
                .exec(&a.host, a.path.as_deref(), &a.command, &a.args, confirmer)
                .await?
        }
        Command::ScoutEmit(a) => {
            let targets = service.scout().resolve_emit_targets(
                &a.targets
                    .iter()
                    .map(|t| (t.host.clone(), t.path.clone()))
                    .collect::<Vec<_>>(),
            )?;
            service
                .scout()
                .emit(&targets, &a.command, &a.args, a.timeout_secs, confirmer)
                .await?
        }
        Command::ScoutBeam(a) => {
            service
                .scout()
                .beam(
                    &a.source_host,
                    &a.source_path,
                    &a.dest_host,
                    &a.dest_path,
                    confirmer,
                )
                .await?
        }
        Command::ScoutZfs(a) => crate::actions::scout::dispatch_scout_zfs(service, a).await?,
        Command::ScoutLogs(a) => crate::actions::scout::dispatch_scout_logs(service, a).await?,
        _ => unreachable!("run_scout called with non-scout command"),
    };
    Ok(result)
}

//! CLI scout subtree — parse and run helpers for `scout *`.
//!
//! `parse_scout` builds the `Command` variant; `run_scout` executes it.
//! All calls delegate to `ScoutService` via the thin shim.

use crate::{
    actions::{
        ScoutBeamArgs, ScoutDeltaArgs, ScoutEmitArgs, ScoutEmitTarget, ScoutExecArgs,
        ScoutFindArgs, ScoutPsArgs,
    },
    app::SynapseService,
    elicitation_gate::CliStderrWarn,
};
use anyhow::{anyhow, Result};
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
        _ => Err(anyhow!("unknown scout command")),
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
        _ => unreachable!("run_scout called with non-scout command"),
    };
    Ok(result)
}

//! Scout-domain arg structs, `from_scout_args`, and dispatch helpers.
//!
//! All items here are re-exported from the parent [`crate::actions`] module so
//! call sites need no changes.

use anyhow::{bail, Result};
use serde_json::Value;

use crate::app::SynapseService;

use super::{
    optional_string_array_param, optional_string_param, optional_u32_param, required_string_param,
    ValidationError,
};

// ── Arg structs ───────────────────────────────────────────────────────────────

/// Parsed parameters for `scout find` (B14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutFindArgs {
    pub host: String,
    pub path: String,
    pub pattern: String,
    pub depth: Option<u8>,
    pub limit: Option<u32>,
}

/// Parsed parameters for `scout ps` (B14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutPsArgs {
    pub host: String,
    pub sort: Option<String>,
    pub grep: Option<String>,
    pub user: Option<String>,
    pub limit: Option<u32>,
}

/// Parsed parameters for `scout delta` (B14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutDeltaArgs {
    /// Source `{host, path}`.
    pub source_host: String,
    pub source_path: String,
    /// Target `{host, path}` (mutually exclusive with `content`).
    pub target_host: Option<String>,
    pub target_path: Option<String>,
    /// Inline content to compare against (capped at 1 MB).
    pub content: Option<String>,
}

/// Parsed parameters for `scout exec` (B14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutExecArgs {
    pub host: String,
    /// Optional working directory (local only; ignored for SSH).
    pub path: Option<String>,
    pub command: String,
    /// Additional positional arguments (execvp-style, no shell).
    pub args: Vec<String>,
    pub timeout_secs: Option<u64>,
}

/// A single target for `scout emit`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutEmitTarget {
    pub host: String,
    pub path: Option<String>,
}

/// Parsed parameters for `scout emit` (B14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutEmitArgs {
    pub targets: Vec<ScoutEmitTarget>,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_secs: Option<u64>,
}

/// Parsed parameters for `scout beam` (B14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutBeamArgs {
    pub source_host: String,
    pub source_path: String,
    pub dest_host: String,
    pub dest_path: String,
}

/// Parsed parameters for `scout zfs` subactions (B15).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutZfsArgs {
    pub host: String,
    pub subaction: String,
    // pools: optional pool name filter
    pub pool: Option<String>,
    // datasets: optional dataset type filter
    pub dataset_type: Option<String>,
    // datasets: recursive flag
    pub recursive: bool,
    // snapshots: optional dataset filter
    pub dataset: Option<String>,
    // snapshots: max rows
    pub limit: Option<u32>,
}

/// Parsed parameters for `scout logs` subactions (B15).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoutLogsArgs {
    pub host: String,
    pub subaction: String,
    /// Line count (1–500, default 100).
    pub lines: u32,
    // grep applied locally after retrieval (injection-safe)
    pub grep: Option<String>,
    // journal: unit filter (-u)
    pub unit: Option<String>,
    // journal: priority filter (-p)
    pub priority: Option<String>,
    // journal: time range filters
    pub since: Option<String>,
    pub until: Option<String>,
}

// ── from_scout_args ───────────────────────────────────────────────────────────

impl super::SynapseAction {
    pub fn from_scout_args(args: &Value) -> Result<Self> {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .ok_or(ValidationError::MissingAction)?;
        match action {
            "help" => Ok(Self::ScoutHelp),
            "nodes" => Ok(Self::ScoutNodes),
            "peek" => Ok(Self::ScoutPeek {
                host: required_string_param(args, "host")?,
                path: required_string_param(args, "path")?,
                tree: super::optional_bool_param(args, "tree")?.unwrap_or(false),
                depth: optional_u32_param(args, "depth")?
                    .map(|d| d.clamp(1, 10) as u8)
                    .unwrap_or(3),
            }),
            "find" => Ok(Self::ScoutFind(Box::new(ScoutFindArgs {
                host: required_string_param(args, "host")?,
                path: required_string_param(args, "path")?,
                pattern: required_string_param(args, "pattern")?,
                depth: optional_u32_param(args, "depth")?.map(|d| d.clamp(1, 20) as u8),
                limit: optional_u32_param(args, "limit")?,
            }))),
            "ps" => Ok(Self::ScoutPs(Box::new(ScoutPsArgs {
                host: required_string_param(args, "host")?,
                sort: optional_string_param(args, "sort")?,
                grep: optional_string_param(args, "grep")?,
                user: optional_string_param(args, "user")?,
                limit: optional_u32_param(args, "limit")?,
            }))),
            "df" => Ok(Self::ScoutDf {
                host: required_string_param(args, "host")?,
                path: optional_string_param(args, "path")?,
            }),
            "delta" => Ok(Self::ScoutDelta(Box::new(ScoutDeltaArgs {
                source_host: required_string_param(args, "source_host")?,
                source_path: required_string_param(args, "source_path")?,
                target_host: optional_string_param(args, "target_host")?,
                target_path: optional_string_param(args, "target_path")?,
                content: optional_string_param(args, "content")?,
            }))),
            "exec" => Ok(Self::ScoutExec(Box::new(ScoutExecArgs {
                host: required_string_param(args, "host")?,
                path: optional_string_param(args, "path")?,
                command: required_string_param(args, "command")?,
                args: optional_string_array_param(args, "args")?,
                timeout_secs: optional_u32_param(args, "timeout_secs")?.map(|v| v as u64),
            }))),
            "emit" => {
                let raw_targets =
                    args.get("targets")
                        .and_then(Value::as_array)
                        .ok_or_else(|| ValidationError::MissingField {
                            field: "targets".into(),
                        })?;
                let targets: Result<Vec<ScoutEmitTarget>> = raw_targets
                    .iter()
                    .map(|t| {
                        Ok(ScoutEmitTarget {
                            host: t
                                .get("host")
                                .and_then(Value::as_str)
                                .ok_or_else(|| ValidationError::MissingField {
                                    field: "targets[].host".into(),
                                })?
                                .to_owned(),
                            path: t.get("path").and_then(Value::as_str).map(|s| s.to_owned()),
                        })
                    })
                    .collect();
                Ok(Self::ScoutEmit(Box::new(ScoutEmitArgs {
                    targets: targets?,
                    command: required_string_param(args, "command")?,
                    args: optional_string_array_param(args, "args")?,
                    timeout_secs: optional_u32_param(args, "timeout_secs")?.map(|v| v as u64),
                })))
            }
            "beam" => Ok(Self::ScoutBeam(Box::new(ScoutBeamArgs {
                source_host: required_string_param(args, "source_host")?,
                source_path: required_string_param(args, "source_path")?,
                dest_host: required_string_param(args, "dest_host")?,
                dest_path: required_string_param(args, "dest_path")?,
            }))),
            "zfs" => Ok(Self::ScoutZfs(Box::new(ScoutZfsArgs {
                host: required_string_param(args, "host")?,
                subaction: required_string_param(args, "subaction")?,
                pool: optional_string_param(args, "pool")?,
                dataset_type: optional_string_param(args, "dataset_type")?,
                recursive: super::optional_bool_param(args, "recursive")?.unwrap_or(false),
                dataset: optional_string_param(args, "dataset")?,
                limit: optional_u32_param(args, "limit")?,
            }))),
            "logs" => {
                let lines = optional_u32_param(args, "lines")?
                    .unwrap_or(crate::scout_service::logs::DEFAULT_LINES)
                    .clamp(1, crate::scout_service::logs::MAX_LINES);
                Ok(Self::ScoutLogs(Box::new(ScoutLogsArgs {
                    host: required_string_param(args, "host")?,
                    subaction: required_string_param(args, "subaction")?,
                    lines,
                    grep: optional_string_param(args, "grep")?,
                    unit: optional_string_param(args, "unit")?,
                    priority: optional_string_param(args, "priority")?,
                    since: optional_string_param(args, "since")?,
                    until: optional_string_param(args, "until")?,
                })))
            }
            other => Err(ValidationError::UnknownAction {
                action: other.to_owned(),
            }
            .into()),
        }
    }
}

// ── Dispatch helpers (called from dispatch.rs) ────────────────────────────────

/// Dispatch `scout zfs` subactions to the appropriate `ScoutService` method.
pub(crate) async fn dispatch_scout_zfs(
    service: &SynapseService,
    args: &ScoutZfsArgs,
) -> Result<Value> {
    match args.subaction.as_str() {
        "pools" => {
            service
                .scout()
                .zfs_pools(&args.host, args.pool.as_deref())
                .await
        }
        "datasets" => {
            service
                .scout()
                .zfs_datasets(
                    &args.host,
                    args.pool.as_deref(),
                    args.dataset_type.as_deref(),
                    args.recursive,
                )
                .await
        }
        "snapshots" => {
            service
                .scout()
                .zfs_snapshots(
                    &args.host,
                    args.pool.as_deref(),
                    args.dataset.as_deref(),
                    args.limit,
                )
                .await
        }
        other => {
            bail!("unknown zfs subaction `{other}`; must be one of: pools, datasets, snapshots")
        }
    }
}

/// Dispatch `scout logs` subactions to the appropriate `ScoutService` method.
pub(crate) async fn dispatch_scout_logs(
    service: &SynapseService,
    args: &ScoutLogsArgs,
) -> Result<Value> {
    match args.subaction.as_str() {
        "syslog" => {
            service
                .scout()
                .logs_syslog(&args.host, args.lines, args.grep.as_deref())
                .await
        }
        "journal" => {
            service
                .scout()
                .logs_journal(
                    &args.host,
                    args.lines,
                    args.unit.as_deref(),
                    args.priority.as_deref(),
                    args.since.as_deref(),
                    args.until.as_deref(),
                    args.grep.as_deref(),
                )
                .await
        }
        "dmesg" => {
            service
                .scout()
                .logs_dmesg(&args.host, args.lines, args.grep.as_deref())
                .await
        }
        "auth" => {
            service
                .scout()
                .logs_auth(&args.host, args.lines, args.grep.as_deref())
                .await
        }
        other => {
            bail!("unknown logs subaction `{other}`; must be one of: syslog, journal, dmesg, auth")
        }
    }
}

//! Flux-domain arg structs, `from_flux_args`, and dispatch helpers.
//!
//! All items here are re-exported from the parent [`crate::actions`] module so
//! call sites need no changes.

use anyhow::Result;
use serde_json::{json, Value};

use crate::app::SynapseService;

use super::{
    optional_bool_param, optional_string_array_param, optional_string_param, optional_u32_param,
    optional_u64_param, require_container_id, require_field, required_string_param,
    ValidationError,
};

// ── Arg structs ───────────────────────────────────────────────────────────────

/// Parsed parameters for `flux container` subactions.
///
/// Boxed inside [`super::SynapseAction::FluxContainer`] (and mirrored by the
/// CLI `Command`) so the enum stays small — every read-only container
/// subaction's params live here. Extraction stays in the shim; logic lives in
/// `FluxService`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerArgs {
    pub subaction: String,
    pub container_id: Option<String>,
    pub host: Option<String>,
    pub lines: Option<u32>,
    // list filters
    pub state: Option<String>,
    pub name_filter: Option<String>,
    pub image_filter: Option<String>,
    pub label_filter: Option<String>,
    // logs params
    pub since: Option<String>,
    pub until: Option<String>,
    pub grep: Option<String>,
    pub stream: Option<String>,
    // inspect param
    pub summary: Option<bool>,
    // search param
    pub query: Option<String>,
    // B9: lifecycle params
    /// exec: command as argv (index 0 = binary, no shell). Required for exec.
    /// Empty when not provided.
    pub command: Vec<String>,
    /// exec: optional user to run as.
    pub exec_user: Option<String>,
    /// exec: optional working directory inside container.
    pub exec_workdir: Option<String>,
    /// exec: timeout in ms, clamped [1000, 300000], default 30000.
    pub exec_timeout_ms: Option<u64>,
    /// recreate: whether to pull the image before recreating (default true).
    pub pull: Option<bool>,
}

/// Parsed parameters for `flux docker` subactions.
///
/// Boxed inside [`super::SynapseAction::FluxDocker`] (and mirrored by the
/// CLI) so the enum stays small. Extraction stays in the shim; all logic
/// (validation, fanout, gating) lives in `FluxService` / the `docker`
/// submodule.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerArgs {
    pub subaction: String,
    pub host: Option<String>,
    // images
    pub dangling_only: Option<bool>,
    // pull / rmi / build
    pub image: Option<String>,
    pub force: Option<bool>,
    pub context: Option<String>,
    pub tag: Option<String>,
    pub dockerfile: Option<String>,
    pub no_cache: Option<bool>,
    // prune
    pub prune_target: Option<String>,
}

/// Parsed parameters for `flux host` subactions (B11).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostArgs {
    pub subaction: String,
    /// Target host name (None = fan out to all hosts).
    pub host: Option<String>,
    // services params
    pub state: Option<String>,
    pub service: Option<String>,
    // ports params
    pub protocol: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    // doctor params
    pub checks: Option<String>, // comma-separated check names
}

/// Parsed parameters for `flux compose` subactions (B13).
///
/// Boxed inside [`super::SynapseAction::FluxCompose`] so the enum stays
/// small. Extraction lives in the shim; all logic lives in `FluxService`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposeArgs {
    /// Subaction: list|status|up|down|restart|recreate|logs|build|pull|refresh.
    pub subaction: String,
    /// Target host name. Required for all subactions except `list` (where it
    /// is also required — compose ops are always single-host).
    pub host: Option<String>,
    /// Compose project name. Required for all subactions except
    /// `list`/`refresh`.
    pub project: Option<String>,
    // down params
    pub remove_volumes: Option<bool>,
    pub force: Option<bool>,
    // logs params
    pub lines: Option<u32>,
    pub since: Option<String>,
    /// Single service filter for `logs`/`status`.
    pub service: Option<String>,
    // build/pull: same `service` field above
}

// ── from_flux_args ─────────────────────────────────────────────────────────

impl super::SynapseAction {
    pub fn from_flux_args(args: &Value) -> Result<Self> {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .ok_or(ValidationError::MissingAction)?;
        match action {
            "help" => Ok(Self::FluxHelp),
            "docker" => Ok(Self::FluxDocker(Box::new(DockerArgs {
                subaction: required_string_param(args, "subaction")?,
                host: optional_string_param(args, "host")?,
                dangling_only: optional_bool_param(args, "dangling_only")?,
                image: optional_string_param(args, "image")?,
                force: optional_bool_param(args, "force")?,
                context: optional_string_param(args, "context")?,
                tag: optional_string_param(args, "tag")?,
                dockerfile: optional_string_param(args, "dockerfile")?,
                no_cache: optional_bool_param(args, "no_cache")?,
                prune_target: optional_string_param(args, "prune_target")?,
            }))),
            "container" => {
                // Validate `response_format` at the shim per B4 contract (no-op
                // on output shape today; full rendering wiring is a separate
                // codebase-wide concern). Invalid value → hard error.
                if let Some(rf) = optional_string_param(args, "response_format")? {
                    crate::formatters::ResponseFormat::parse(Some(&rf))
                        .map_err(|e| anyhow::anyhow!(e))?;
                }
                Ok(Self::FluxContainer(Box::new(ContainerArgs {
                    subaction: required_string_param(args, "subaction")?,
                    container_id: optional_string_param(args, "container_id")?,
                    host: optional_string_param(args, "host")?,
                    lines: optional_u32_param(args, "lines")?,
                    state: optional_string_param(args, "state")?,
                    name_filter: optional_string_param(args, "name_filter")?,
                    image_filter: optional_string_param(args, "image_filter")?,
                    label_filter: optional_string_param(args, "label_filter")?,
                    since: optional_string_param(args, "since")?,
                    until: optional_string_param(args, "until")?,
                    grep: optional_string_param(args, "grep")?,
                    stream: optional_string_param(args, "stream")?,
                    summary: optional_bool_param(args, "summary")?,
                    query: optional_string_param(args, "query")?,
                    // B9 lifecycle params
                    command: optional_string_array_param(args, "command")?,
                    exec_user: optional_string_param(args, "exec_user")?,
                    exec_workdir: optional_string_param(args, "exec_workdir")?,
                    exec_timeout_ms: optional_u64_param(args, "exec_timeout_ms")?,
                    pull: optional_bool_param(args, "pull")?,
                })))
            }
            "host" => Ok(Self::FluxHost(Box::new(HostArgs {
                subaction: required_string_param(args, "subaction")?,
                host: optional_string_param(args, "host")?,
                state: optional_string_param(args, "state")?,
                service: optional_string_param(args, "service")?,
                protocol: optional_string_param(args, "protocol")?,
                limit: optional_u32_param(args, "limit")?,
                offset: optional_u32_param(args, "offset")?,
                checks: optional_string_param(args, "checks")?,
            }))),
            "compose" => Ok(Self::FluxCompose(Box::new(ComposeArgs {
                subaction: required_string_param(args, "subaction")?,
                host: optional_string_param(args, "host")?,
                project: optional_string_param(args, "project")?,
                remove_volumes: optional_bool_param(args, "remove_volumes")?,
                force: optional_bool_param(args, "force")?,
                lines: optional_u32_param(args, "lines")?,
                since: optional_string_param(args, "since")?,
                service: optional_string_param(args, "service")?,
            }))),
            other => Err(ValidationError::UnknownAction {
                action: other.to_owned(),
            }
            .into()),
        }
    }
}

// ── dispatch helpers ──────────────────────────────────────────────────────────

/// Dispatch a `flux docker` subaction to the [`FluxService`].
///
/// Thin: validate/extract params and call the matching service method. The
/// destructive gate (`build`/`rmi`/`prune`) is enforced INSIDE the service
/// method via the supplied `confirmer` — never here.
pub(super) async fn dispatch_flux_docker(
    service: &SynapseService,
    args: &DockerArgs,
    confirmer: &dyn crate::elicitation_gate::Confirmer,
) -> Result<Value> {
    use crate::flux_service::docker::{build_args, PruneTarget};
    let flux = service.flux();
    let host = args.host.as_deref();
    match args.subaction.as_str() {
        "info" => flux.docker_info(host).await,
        "df" => flux.docker_df(host).await,
        "images" => {
            flux.docker_images(host, args.dangling_only.unwrap_or(false))
                .await
        }
        "networks" => flux.docker_networks(host).await,
        "volumes" => flux.docker_volumes(host).await,
        "pull" => {
            let image = require_field(&args.image, "image")?;
            flux.docker_pull(require_field(&args.host, "host")?, image)
                .await
        }
        "build" => {
            let context = require_field(&args.context, "context")?;
            let tag = require_field(&args.tag, "tag")?;
            let built = build_args(
                context,
                tag,
                args.dockerfile.as_deref(),
                args.no_cache.unwrap_or(false),
            )?;
            flux.docker_build(require_field(&args.host, "host")?, built, confirmer)
                .await
        }
        "rmi" => {
            let image = require_field(&args.image, "image")?;
            let force = args.force.unwrap_or(false);
            if !force {
                return Err(ValidationError::MissingField {
                    field: "force (rmi requires force=true)".into(),
                }
                .into());
            }
            flux.docker_rmi(require_field(&args.host, "host")?, image, force, confirmer)
                .await
        }
        "prune" => {
            let target_str = require_field(&args.prune_target, "prune_target")?;
            let target = PruneTarget::parse(target_str)?;
            if !args.force.unwrap_or(false) {
                return Err(ValidationError::MissingField {
                    field: "force (prune requires force=true)".into(),
                }
                .into());
            }
            flux.docker_prune(require_field(&args.host, "host")?, target, confirmer)
                .await
        }
        other => Err(ValidationError::UnknownAction {
            action: format!("docker:{other}"),
        }
        .into()),
    }
}

/// Dispatch a `flux container` subaction to the [`FluxService`].
///
/// Thin: extracts the parsed [`ContainerArgs`] and calls the matching service
/// method. All filtering/fanout logic lives in `FluxService` /
/// `container_read` (read-only) and `container_lifecycle` (B9 lifecycle).
/// Destructive gate (`stop`/`recreate`/`exec`) is enforced INSIDE the service
/// method via the supplied `confirmer` — never here.
pub(super) async fn dispatch_flux_container(
    service: &SynapseService,
    args: &ContainerArgs,
    confirmer: &dyn crate::elicitation_gate::Confirmer,
) -> Result<Value> {
    use crate::flux_service::container_lifecycle::{
        ExecParams, RecreateParams, EXEC_TIMEOUT_DEFAULT_MS,
    };
    use crate::flux_service::container_read::{ListFilters, LogOptions, DEFAULT_LOG_LINES};
    let flux = service.flux();
    let host = args.host.as_deref();
    match args.subaction.as_str() {
        "list" => {
            let filters = ListFilters {
                state: args.state.clone(),
                name_filter: args.name_filter.clone(),
                image_filter: args.image_filter.clone(),
                label_filter: args.label_filter.clone(),
            };
            flux.container_list(host, filters).await
        }
        "search" => {
            let q = args.query.as_deref().ok_or(ValidationError::MissingField {
                field: "query".into(),
            })?;
            flux.container_search(host, q).await
        }
        "stats" => {
            flux.container_stats(host, args.container_id.as_deref())
                .await
        }
        "inspect" => {
            flux.container_inspect(
                host,
                require_container_id(&args.container_id)?,
                args.summary.unwrap_or(false),
            )
            .await
        }
        "top" => {
            flux.container_top(host, require_container_id(&args.container_id)?)
                .await
        }
        "logs" => {
            let opts = LogOptions {
                lines: args.lines.unwrap_or(DEFAULT_LOG_LINES),
                since: args.since.clone(),
                until: args.until.clone(),
                grep: args.grep.clone(),
                stream: args.stream.clone().unwrap_or_else(|| "both".to_owned()),
            };
            flux.container_logs(host, require_container_id(&args.container_id)?, opts)
                .await
        }
        // B9: simple lifecycle (start/stop/restart/pause/resume)
        sa @ ("start" | "stop" | "restart" | "pause" | "resume") => {
            flux.container_lifecycle(
                host,
                require_container_id(&args.container_id)?,
                sa,
                confirmer,
            )
            .await
        }
        // B9: pull container image
        "pull" => {
            flux.container_pull(host, require_container_id(&args.container_id)?)
                .await
        }
        // B9: recreate
        "recreate" => {
            let params = RecreateParams {
                pull: args.pull.unwrap_or(true),
            };
            flux.container_recreate(
                host,
                require_container_id(&args.container_id)?,
                params,
                confirmer,
            )
            .await
        }
        // B9: exec
        "exec" => {
            if args.command.is_empty() {
                return Err(ValidationError::MissingField {
                    field: "command".into(),
                }
                .into());
            }
            let params = ExecParams {
                container_id: require_container_id(&args.container_id)?.to_owned(),
                command: args.command.clone(),
                user: args.exec_user.clone(),
                workdir: args.exec_workdir.clone(),
                timeout_ms: args.exec_timeout_ms.unwrap_or(EXEC_TIMEOUT_DEFAULT_MS),
            };
            flux.container_exec(host, params, confirmer).await
        }
        other => Err(ValidationError::UnknownAction {
            action: format!("container:{other}"),
        }
        .into()),
    }
}

/// Dispatch a `flux host` subaction to the [`FluxService`].
///
/// Thin: extracts the parsed [`HostArgs`] and calls the matching service
/// method. All shell execution / fanout logic lives in `FluxService` / the
/// `host` submodule.
pub(super) async fn dispatch_flux_host(service: &SynapseService, args: &HostArgs) -> Result<Value> {
    let flux = service.flux();
    let host = args.host.as_deref();
    match args.subaction.as_str() {
        "status" => flux.host_status(host).await,
        "info" => flux.host_info(host).await,
        "uptime" => flux.host_uptime(host).await,
        "resources" => flux.host_resources(host).await,
        "services" => {
            let h = require_field(&args.host, "host")?;
            flux.host_services(h, args.state.as_deref(), args.service.as_deref())
                .await
        }
        "network" => flux.host_network(host).await,
        "mounts" => {
            let h = require_field(&args.host, "host")?;
            flux.host_mounts(h).await
        }
        "ports" => {
            let h = require_field(&args.host, "host")?;
            let limit = args.limit.map(|v| v as usize);
            let offset = args.offset.map(|v| v as usize);
            flux.host_ports(h, args.protocol.as_deref(), limit, offset)
                .await
        }
        "doctor" => {
            let h = require_field(&args.host, "host")?;
            let checks: Vec<String> = match &args.checks {
                Some(s) if !s.is_empty() => s.split(',').map(|c| c.trim().to_owned()).collect(),
                _ => crate::flux_service::host::DEFAULT_DOCTOR_CHECKS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            };
            flux.host_doctor(h, checks).await
        }
        other => Err(ValidationError::UnknownAction {
            action: format!("host:{other}"),
        }
        .into()),
    }
}

/// Dispatch a `flux compose` subaction to the [`FluxService`].
///
/// Thin: validates required params and calls the matching service method.
/// Gating (`down`/`restart`/`recreate`) is enforced INSIDE the service
/// methods via the supplied `confirmer` — not here.
pub(super) async fn dispatch_flux_compose(
    service: &SynapseService,
    args: &ComposeArgs,
    confirmer: &dyn crate::elicitation_gate::Confirmer,
) -> Result<Value> {
    use crate::flux_service::compose_ops::{ComposeLogOptions, DownArgs};
    let flux = service.flux();
    let host = require_field(&args.host, "host")?;
    match args.subaction.as_str() {
        "list" => {
            let projects = flux.compose_list(host).await?;
            let items: Vec<Value> = projects
                .iter()
                .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
                .collect();
            Ok(json!({
                "host": host,
                "count": items.len(),
                "projects": items,
            }))
        }
        "refresh" => {
            flux.compose_refresh(Some(host));
            Ok(json!({ "host": host, "refreshed": true }))
        }
        "status" => {
            let project = require_field(&args.project, "project")?;
            flux.compose_status(host, project, args.service.as_deref())
                .await
        }
        "up" => {
            let project = require_field(&args.project, "project")?;
            flux.compose_up(host, project).await
        }
        "down" => {
            let project = require_field(&args.project, "project")?;
            let down_args = DownArgs {
                remove_volumes: args.remove_volumes.unwrap_or(false),
                force: args.force.unwrap_or(false),
            };
            flux.compose_down(host, project, down_args, confirmer).await
        }
        "restart" => {
            let project = require_field(&args.project, "project")?;
            flux.compose_restart(host, project, confirmer).await
        }
        "recreate" => {
            let project = require_field(&args.project, "project")?;
            flux.compose_recreate(host, project, confirmer).await
        }
        "logs" => {
            let project = require_field(&args.project, "project")?;
            let opts = ComposeLogOptions {
                lines: args.lines,
                since: args.since.clone(),
                service: args.service.clone(),
            };
            flux.compose_logs(host, project, opts).await
        }
        "build" => {
            let project = require_field(&args.project, "project")?;
            flux.compose_build(host, project, args.service.as_deref())
                .await
        }
        "pull" => {
            let project = require_field(&args.project, "project")?;
            flux.compose_pull(host, project, args.service.as_deref())
                .await
        }
        other => Err(ValidationError::UnknownAction {
            action: format!("compose:{other}"),
        }
        .into()),
    }
}

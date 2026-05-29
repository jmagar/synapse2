//! CLI flux subtree — parse and run helpers for `flux docker|container|host|compose`.
//!
//! `parse_flux` builds the `Command` variant; `run_docker`, `run_container`,
//! `run_host`, and `run_compose` execute it. All call into `FluxService` via
//! the thin shim; no business logic lives here.

use crate::{
    actions::{ComposeArgs, ContainerArgs, DockerArgs, HostArgs},
    app::SynapseService,
    elicitation_gate::CliStderrWarn,
};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::Command;

// ── parse ─────────────────────────────────────────────────────────────────────

pub(super) fn parse_flux(args: &[String]) -> Result<Command> {
    match args {
        [group, subaction, rest @ ..] if group == "docker" => parse_flux_docker(subaction, rest),
        [group, subaction, rest @ ..] if group == "container" => {
            parse_flux_container(subaction, rest)
        }
        [group, subaction, rest @ ..] if group == "host" => parse_flux_host(subaction, rest),
        [group, subaction, rest @ ..] if group == "compose" => parse_flux_compose(subaction, rest),
        _ => Err(anyhow!("unknown flux command")),
    }
}

fn parse_flux_docker(subaction: &str, rest: &[String]) -> Result<Command> {
    // Split out valueless bool flags before the value-pair parser.
    const BOOL_FLAGS: &[&str] = &["--dangling-only", "--no-cache", "--force"];
    let has_bool = |flag: &str| rest.iter().any(|a| a == flag);
    let dangling_only = has_bool("--dangling-only");
    let no_cache = has_bool("--no-cache");
    let force = has_bool("--force");
    let value_args: Vec<String> = rest
        .iter()
        .filter(|a| !BOOL_FLAGS.contains(&a.as_str()))
        .cloned()
        .collect();
    Ok(Command::FluxDocker(Box::new(DockerArgs {
        subaction: subaction.to_owned(),
        host: super::parse_optional_named_value(&value_args, "--host")?,
        dangling_only: dangling_only.then_some(true),
        image: super::parse_optional_named_value(&value_args, "--image")?,
        force: force.then_some(true),
        context: super::parse_optional_named_value(&value_args, "--context")?,
        tag: super::parse_optional_named_value(&value_args, "--tag")?,
        dockerfile: super::parse_optional_named_value(&value_args, "--dockerfile")?,
        no_cache: no_cache.then_some(true),
        prune_target: super::parse_optional_named_value(&value_args, "--target")?,
    })))
}

fn parse_flux_container(subaction: &str, rest: &[String]) -> Result<Command> {
    // `--summary` and `--pull` are valueless bool flags; split them out before the
    // value-pair parser (which requires a value after every flag).
    const BOOL_FLAGS: &[&str] = &["--summary", "--no-pull"];
    let summary = rest.iter().any(|a| a == "--summary");
    // `--no-pull` maps to pull=false for recreate; absent → pull=true (default).
    let no_pull = rest.iter().any(|a| a == "--no-pull");
    let value_args: Vec<String> = rest
        .iter()
        .filter(|a| !BOOL_FLAGS.contains(&a.as_str()))
        .cloned()
        .collect();
    let container_id = super::parse_optional_named_value(&value_args, "--container-id")?;
    let lines = super::parse_optional_named_value(&value_args, "--lines")?
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| anyhow!("--lines must be an integer"))?;
    let exec_timeout_ms = super::parse_optional_named_value(&value_args, "--timeout")?
        .map(|v: String| v.parse::<u64>())
        .transpose()
        .map_err(|_| anyhow!("--timeout must be a positive integer (milliseconds)"))?;
    // `--command` collects everything after `--command` as argv tokens.
    let command = parse_command_argv(&value_args);
    // Validate `--response-format` for MCP/CLI parity.
    if let Some(rf) = super::parse_optional_named_value(&value_args, "--response-format")? {
        crate::formatters::ResponseFormat::parse(Some(&rf)).map_err(|e| anyhow!(e))?;
    }
    Ok(Command::FluxContainer(Box::new(ContainerArgs {
        subaction: subaction.to_owned(),
        container_id,
        host: super::parse_optional_named_value(&value_args, "--host")?,
        lines,
        state: super::parse_optional_named_value(&value_args, "--state")?,
        name_filter: super::parse_optional_named_value(&value_args, "--name-filter")?,
        image_filter: super::parse_optional_named_value(&value_args, "--image-filter")?,
        label_filter: super::parse_optional_named_value(&value_args, "--label-filter")?,
        since: super::parse_optional_named_value(&value_args, "--since")?,
        until: super::parse_optional_named_value(&value_args, "--until")?,
        grep: super::parse_optional_named_value(&value_args, "--grep")?,
        stream: super::parse_optional_named_value(&value_args, "--stream")?,
        summary: summary.then_some(true),
        query: super::parse_optional_named_value(&value_args, "--query")?,
        // B9 lifecycle params
        command,
        exec_user: super::parse_optional_named_value(&value_args, "--user")?,
        exec_workdir: super::parse_optional_named_value(&value_args, "--workdir")?,
        exec_timeout_ms,
        pull: if no_pull { Some(false) } else { None },
    })))
}

/// Extract `--command ARG1 ARG2 ...` from the arg list.
///
/// Finds the index of `--command`, then collects all following tokens as the
/// command argv. Tokens that look like other flags (start with `--`) but appear
/// after `--command` are still collected — the user has quoted them or passed them
/// as literal argv items to be exec'd inside the container.
fn parse_command_argv(args: &[String]) -> Vec<String> {
    let idx = args.iter().position(|a| a == "--command");
    match idx {
        Some(i) => args[i + 1..].to_vec(),
        None => vec![],
    }
}

fn parse_flux_host(subaction: &str, rest: &[String]) -> Result<Command> {
    Ok(Command::FluxHost(Box::new(HostArgs {
        subaction: subaction.to_owned(),
        host: super::parse_optional_named_value(rest, "--host")?,
        state: super::parse_optional_named_value(rest, "--state")?,
        service: super::parse_optional_named_value(rest, "--service")?,
        protocol: super::parse_optional_named_value(rest, "--protocol")?,
        limit: super::parse_optional_named_value(rest, "--limit")?
            .map(|v| v.parse::<u32>())
            .transpose()
            .map_err(|_| anyhow!("--limit must be an integer"))?,
        offset: super::parse_optional_named_value(rest, "--offset")?
            .map(|v| v.parse::<u32>())
            .transpose()
            .map_err(|_| anyhow!("--offset must be an integer"))?,
        checks: super::parse_optional_named_value(rest, "--checks")?,
    })))
}

fn parse_flux_compose(subaction: &str, rest: &[String]) -> Result<Command> {
    // Valueless bool flags for compose.
    const BOOL_FLAGS: &[&str] = &["--remove-volumes", "--force"];
    let has_bool = |flag: &str| rest.iter().any(|a| a == flag);
    let remove_volumes = has_bool("--remove-volumes");
    let force = has_bool("--force");
    let value_args: Vec<String> = rest
        .iter()
        .filter(|a| !BOOL_FLAGS.contains(&a.as_str()))
        .cloned()
        .collect();
    let lines = super::parse_optional_named_value(&value_args, "--lines")?
        .map(|v| v.parse::<u32>())
        .transpose()
        .map_err(|_| anyhow!("--lines must be an integer"))?;
    Ok(Command::FluxCompose(Box::new(ComposeArgs {
        subaction: subaction.to_owned(),
        host: super::parse_optional_named_value(&value_args, "--host")?,
        project: super::parse_optional_named_value(&value_args, "--project")?,
        remove_volumes: remove_volumes.then_some(true),
        force: force.then_some(true),
        lines,
        since: super::parse_optional_named_value(&value_args, "--since")?,
        service: super::parse_optional_named_value(&value_args, "--service")?,
    })))
}

// ── run helpers ───────────────────────────────────────────────────────────────

pub(super) async fn run_docker(
    args: &DockerArgs,
    service: &SynapseService,
    confirmer: &CliStderrWarn,
) -> Result<Value> {
    let DockerArgs {
        subaction,
        host,
        dangling_only,
        image,
        force,
        context,
        tag,
        dockerfile,
        no_cache,
        prune_target,
    } = args;
    let flux = service.flux();
    let host_opt = host.as_deref();
    let result = match subaction.as_str() {
        "info" => flux.docker_info(host_opt).await?,
        "df" => flux.docker_df(host_opt).await?,
        "images" => {
            flux.docker_images(host_opt, dangling_only.unwrap_or(false))
                .await?
        }
        "networks" => flux.docker_networks(host_opt).await?,
        "volumes" => flux.docker_volumes(host_opt).await?,
        "pull" => {
            let h = host
                .as_deref()
                .ok_or_else(|| anyhow!("docker pull requires --host"))?;
            let img = image
                .as_deref()
                .ok_or_else(|| anyhow!("docker pull requires --image"))?;
            flux.docker_pull(h, img).await?
        }
        "build" => {
            use crate::flux_service::docker::build_args;
            let h = host
                .as_deref()
                .ok_or_else(|| anyhow!("docker build requires --host"))?;
            let ctx = context
                .as_deref()
                .ok_or_else(|| anyhow!("docker build requires --context"))?;
            let t = tag
                .as_deref()
                .ok_or_else(|| anyhow!("docker build requires --tag"))?;
            let built = build_args(ctx, t, dockerfile.as_deref(), no_cache.unwrap_or(false))?;
            flux.docker_build(h, built, confirmer).await?
        }
        "rmi" => {
            let h = host
                .as_deref()
                .ok_or_else(|| anyhow!("docker rmi requires --host"))?;
            let img = image
                .as_deref()
                .ok_or_else(|| anyhow!("docker rmi requires --image"))?;
            if !force.unwrap_or(false) {
                return Err(anyhow!("docker rmi requires --force"));
            }
            flux.docker_rmi(h, img, true, confirmer).await?
        }
        "prune" => {
            use crate::flux_service::docker::PruneTarget;
            let h = host
                .as_deref()
                .ok_or_else(|| anyhow!("docker prune requires --host"))?;
            let target_str = prune_target
                .as_deref()
                .ok_or_else(|| anyhow!("docker prune requires --target"))?;
            let target = PruneTarget::parse(target_str)?;
            if !force.unwrap_or(false) {
                return Err(anyhow!("docker prune requires --force"));
            }
            flux.docker_prune(h, target, confirmer).await?
        }
        other => return Err(anyhow!("unknown flux docker subaction `{other}`")),
    };
    Ok(result)
}

pub(super) async fn run_container(
    args: &ContainerArgs,
    service: &SynapseService,
    confirmer: &CliStderrWarn,
) -> Result<Value> {
    use crate::flux_service::container_lifecycle::{
        ExecParams, RecreateParams, EXEC_TIMEOUT_DEFAULT_MS,
    };
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
        command,
        exec_user,
        exec_workdir,
        exec_timeout_ms,
        pull,
    } = args;
    let flux = service.flux();
    let host = host.as_deref();
    let result = match subaction.as_str() {
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
        // B9: simple lifecycle (start/stop/restart/pause/resume)
        sa @ ("start" | "stop" | "restart" | "pause" | "resume") => {
            let id = container_id
                .as_deref()
                .ok_or_else(|| anyhow!("container {sa} requires --container-id"))?;
            flux.container_lifecycle(host, id, sa, confirmer).await?
        }
        // B9: pull container image
        "pull" => {
            let id = container_id
                .as_deref()
                .ok_or_else(|| anyhow!("container pull requires --container-id"))?;
            flux.container_pull(host, id).await?
        }
        // B9: recreate
        "recreate" => {
            let id = container_id
                .as_deref()
                .ok_or_else(|| anyhow!("container recreate requires --container-id"))?;
            let params = RecreateParams {
                pull: pull.unwrap_or(true),
            };
            flux.container_recreate(host, id, params, confirmer).await?
        }
        // B9: exec
        "exec" => {
            if command.is_empty() {
                return Err(anyhow!(
                    "container exec requires --command BINARY [ARGS...]"
                ));
            }
            let id = container_id
                .as_deref()
                .ok_or_else(|| anyhow!("container exec requires --container-id"))?;
            let params = ExecParams {
                container_id: id.to_owned(),
                command: command.clone(),
                user: exec_user.clone(),
                workdir: exec_workdir.clone(),
                timeout_ms: exec_timeout_ms.unwrap_or(EXEC_TIMEOUT_DEFAULT_MS),
            };
            flux.container_exec(host, params, confirmer).await?
        }
        other => return Err(anyhow!("unknown flux container subaction `{other}`")),
    };
    Ok(result)
}

pub(super) async fn run_host(args: &HostArgs, service: &SynapseService) -> Result<Value> {
    use crate::flux_service::host::DEFAULT_DOCTOR_CHECKS;
    let flux = service.flux();
    let host = args.host.as_deref();
    let result = match args.subaction.as_str() {
        "status" => flux.host_status(host).await?,
        "info" => flux.host_info(host).await?,
        "uptime" => flux.host_uptime(host).await?,
        "resources" => flux.host_resources(host).await?,
        "services" => {
            let h = host.ok_or_else(|| anyhow!("host services requires --host"))?;
            flux.host_services(h, args.state.as_deref(), args.service.as_deref())
                .await?
        }
        "network" => flux.host_network(host).await?,
        "mounts" => {
            let h = host.ok_or_else(|| anyhow!("host mounts requires --host"))?;
            flux.host_mounts(h).await?
        }
        "ports" => {
            let h = host.ok_or_else(|| anyhow!("host ports requires --host"))?;
            flux.host_ports(
                h,
                args.protocol.as_deref(),
                args.limit.map(|v| v as usize),
                args.offset.map(|v| v as usize),
            )
            .await?
        }
        "doctor" => {
            let h = host.ok_or_else(|| anyhow!("host doctor requires --host"))?;
            let checks: Vec<String> = match &args.checks {
                Some(s) if !s.is_empty() => s.split(',').map(|c| c.trim().to_owned()).collect(),
                _ => DEFAULT_DOCTOR_CHECKS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            };
            flux.host_doctor(h, checks).await?
        }
        other => return Err(anyhow!("unknown flux host subaction `{other}`")),
    };
    Ok(result)
}

pub(super) async fn run_compose(
    args: &ComposeArgs,
    service: &SynapseService,
    confirmer: &CliStderrWarn,
) -> Result<Value> {
    use crate::flux_service::compose_ops::{ComposeLogOptions, DownArgs};
    let flux = service.flux();
    let host = args
        .host
        .as_deref()
        .ok_or_else(|| anyhow!("flux compose requires --host"))?;
    let result = match args.subaction.as_str() {
        "list" => {
            let projects = flux.compose_list(host).await?;
            let items: Vec<Value> = projects
                .iter()
                .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
                .collect();
            json!({ "host": host, "count": items.len(), "projects": items })
        }
        "refresh" => {
            flux.compose_refresh(Some(host));
            json!({ "host": host, "refreshed": true })
        }
        "status" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose status requires --project"))?;
            flux.compose_status(host, project, args.service.as_deref())
                .await?
        }
        "up" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose up requires --project"))?;
            flux.compose_up(host, project).await?
        }
        "down" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose down requires --project"))?;
            let down_args = DownArgs {
                remove_volumes: args.remove_volumes.unwrap_or(false),
                force: args.force.unwrap_or(false),
            };
            flux.compose_down(host, project, down_args, confirmer)
                .await?
        }
        "restart" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose restart requires --project"))?;
            flux.compose_restart(host, project, confirmer).await?
        }
        "recreate" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose recreate requires --project"))?;
            flux.compose_recreate(host, project, confirmer).await?
        }
        "logs" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose logs requires --project"))?;
            let opts = ComposeLogOptions {
                lines: args.lines,
                since: args.since.clone(),
                service: args.service.clone(),
            };
            flux.compose_logs(host, project, opts).await?
        }
        "build" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose build requires --project"))?;
            flux.compose_build(host, project, args.service.as_deref())
                .await?
        }
        "pull" => {
            let project = args
                .project
                .as_deref()
                .ok_or_else(|| anyhow!("compose pull requires --project"))?;
            flux.compose_pull(host, project, args.service.as_deref())
                .await?
        }
        other => return Err(anyhow!("unknown flux compose subaction `{other}`")),
    };
    Ok(result)
}

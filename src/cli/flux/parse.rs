use crate::actions::{ComposeArgs, ContainerArgs, DockerArgs, HostArgs};
use anyhow::{Result, anyhow};

use super::super::Command;

pub(in crate::cli) fn parse_flux(args: &[String]) -> Result<Command> {
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
        response_format: super::super::parse_optional_response_format(&value_args)?,
        subaction: subaction.to_owned(),
        host: super::super::parse_optional_named_value(&value_args, "--host")?,
        dangling_only: dangling_only.then_some(true),
        image: super::super::parse_optional_named_value(&value_args, "--image")?,
        force: force.then_some(true),
        context: super::super::parse_optional_named_value(&value_args, "--context")?,
        tag: super::super::parse_optional_named_value(&value_args, "--tag")?,
        dockerfile: super::super::parse_optional_named_value(&value_args, "--dockerfile")?,
        no_cache: no_cache.then_some(true),
        prune_target: super::super::parse_optional_named_value(&value_args, "--target")?,
    })))
}

fn parse_flux_container(subaction: &str, rest: &[String]) -> Result<Command> {
    const BOOL_FLAGS: &[&str] = &["--summary", "--no-pull"];
    let summary = rest.iter().any(|a| a == "--summary");
    let no_pull = rest.iter().any(|a| a == "--no-pull");
    let value_args: Vec<String> = rest
        .iter()
        .filter(|a| !BOOL_FLAGS.contains(&a.as_str()))
        .cloned()
        .collect();
    let (option_args, command) = split_command_argv(&value_args);
    let container_id = super::super::parse_optional_named_value(&option_args, "--container-id")?;
    let lines = super::super::parse_optional_named_value(&option_args, "--lines")?
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| anyhow!("--lines must be an integer"))?;
    let exec_timeout_ms = super::super::parse_optional_named_value(&option_args, "--timeout")?
        .map(|v: String| v.parse::<u64>())
        .transpose()
        .map_err(|_| anyhow!("--timeout must be a positive integer (milliseconds)"))?;
    let response_format = super::super::parse_optional_response_format(&option_args)?;
    Ok(Command::FluxContainer(Box::new(ContainerArgs {
        response_format,
        subaction: subaction.to_owned(),
        container_id,
        host: super::super::parse_optional_named_value(&option_args, "--host")?,
        lines,
        state: super::super::parse_optional_named_value(&option_args, "--state")?,
        name_filter: super::super::parse_optional_named_value(&option_args, "--name-filter")?,
        image_filter: super::super::parse_optional_named_value(&option_args, "--image-filter")?,
        label_filter: super::super::parse_optional_named_value(&option_args, "--label-filter")?,
        since: super::super::parse_optional_named_value(&option_args, "--since")?,
        until: super::super::parse_optional_named_value(&option_args, "--until")?,
        grep: super::super::parse_optional_named_value(&option_args, "--grep")?,
        stream: super::super::parse_optional_named_value(&option_args, "--stream")?,
        summary: summary.then_some(true),
        query: super::super::parse_optional_named_value(&option_args, "--query")?,
        command,
        exec_user: super::super::parse_optional_named_value(&option_args, "--user")?,
        exec_workdir: super::super::parse_optional_named_value(&option_args, "--workdir")?,
        exec_timeout_ms,
        pull: if no_pull { Some(false) } else { None },
    })))
}

fn split_command_argv(args: &[String]) -> (Vec<String>, Vec<String>) {
    match args.iter().position(|a| a == "--command") {
        Some(i) => (args[..i].to_vec(), args[i + 1..].to_vec()),
        None => (args.to_vec(), Vec::new()),
    }
}

fn parse_flux_host(subaction: &str, rest: &[String]) -> Result<Command> {
    Ok(Command::FluxHost(Box::new(HostArgs {
        response_format: super::super::parse_optional_response_format(rest)?,
        subaction: subaction.to_owned(),
        host: super::super::parse_optional_named_value(rest, "--host")?,
        state: super::super::parse_optional_named_value(rest, "--state")?,
        service: super::super::parse_optional_named_value(rest, "--service")?,
        protocol: super::super::parse_optional_named_value(rest, "--protocol")?,
        limit: super::super::parse_optional_named_value(rest, "--limit")?
            .map(|v| v.parse::<u32>())
            .transpose()
            .map_err(|_| anyhow!("--limit must be an integer"))?,
        offset: super::super::parse_optional_named_value(rest, "--offset")?
            .map(|v| v.parse::<u32>())
            .transpose()
            .map_err(|_| anyhow!("--offset must be an integer"))?,
        checks: super::super::parse_optional_named_value(rest, "--checks")?,
    })))
}

fn parse_flux_compose(subaction: &str, rest: &[String]) -> Result<Command> {
    const BOOL_FLAGS: &[&str] = &["--remove-volumes", "--force"];
    let has_bool = |flag: &str| rest.iter().any(|a| a == flag);
    let remove_volumes = has_bool("--remove-volumes");
    let force = has_bool("--force");
    let value_args: Vec<String> = rest
        .iter()
        .filter(|a| !BOOL_FLAGS.contains(&a.as_str()))
        .cloned()
        .collect();
    let lines = super::super::parse_optional_named_value(&value_args, "--lines")?
        .map(|v| v.parse::<u32>())
        .transpose()
        .map_err(|_| anyhow!("--lines must be an integer"))?;
    Ok(Command::FluxCompose(Box::new(ComposeArgs {
        response_format: super::super::parse_optional_response_format(&value_args)?,
        subaction: subaction.to_owned(),
        host: super::super::parse_optional_named_value(&value_args, "--host")?,
        project: super::super::parse_optional_named_value(&value_args, "--project")?,
        remove_volumes: remove_volumes.then_some(true),
        force: force.then_some(true),
        lines,
        since: super::super::parse_optional_named_value(&value_args, "--since")?,
        service: super::super::parse_optional_named_value(&value_args, "--service")?,
    })))
}

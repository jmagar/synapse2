//! Container lifecycle operations (B9): `start`, `stop`, `restart`, `pause`,
//! `resume`, `pull`, `recreate`, `exec`.
//!
//! # Architecture seam
//!
//! The **pure** per-host functions in this module operate on `&dyn ContainerOps`
//! (and `&dyn ImageOps`) so they are fully unit-testable with
//! [`MockDockerClient`](crate::docker_client::MockDockerClient) — no live docker
//! daemon required.
//!
//! # Destructive gating (B5)
//!
//! The service driver (`container_driver.rs`) enforces the B5 elicitation gate
//! **before** calling these functions. The pure ops here perform IO directly;
//! it is the caller's responsibility to gate.
//!
//! Subactions gated (synapse-mcp parity — `requireConfirmation` before any IO):
//! - `stop`, `recreate`, `exec`
//!
//! Subactions ungated:
//! - `start`, `restart`, `pause`, `resume`, `pull`

use anyhow::Result;
use bollard::exec::{StartExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, ContainerInspectResponse, ExecConfig, NetworkingConfig,
};
use bollard::query_parameters::CreateContainerOptions;
use bollard::query_parameters::CreateImageOptions;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;

use crate::docker_client::{ContainerAction, ContainerOps, ImageOps};

// ── Exec params ────────────────────────────────────────────────────────────────

/// Minimum exec timeout in milliseconds (1 second).
pub const EXEC_TIMEOUT_MIN_MS: u64 = 1_000;
/// Maximum exec timeout in milliseconds (5 minutes).
pub const EXEC_TIMEOUT_MAX_MS: u64 = 300_000;
/// Default exec timeout in milliseconds (30 seconds).
pub const EXEC_TIMEOUT_DEFAULT_MS: u64 = 30_000;

/// Parameters for the `exec` subaction.
#[derive(Debug, Clone)]
pub struct ExecParams {
    /// Container id or name.
    pub container_id: String,
    /// Command argv: `command[0]` is the binary, rest are args.
    /// NEVER passed to `sh -c` — pure execvp semantics.
    pub command: Vec<String>,
    /// Optional user to run as (e.g. `"root"`).
    pub user: Option<String>,
    /// Optional working directory inside the container.
    pub workdir: Option<String>,
    /// Timeout in milliseconds, clamped to `[1000, 300000]`, default 30000.
    pub timeout_ms: u64,
}

/// Result of an `exec` call.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Combined stdout collected from the exec stream.
    pub stdout: String,
    /// Combined stderr collected from the exec stream.
    pub stderr: String,
    /// Exit code from `inspect_exec`. `None` if inspection could not determine it.
    pub exit_code: Option<i64>,
}

// ── Recreate params ────────────────────────────────────────────────────────────

/// Parameters for the `recreate` subaction.
#[derive(Debug, Clone)]
pub struct RecreateParams {
    /// Whether to pull the latest image before recreating. Default: true.
    pub pull: bool,
}

impl Default for RecreateParams {
    fn default() -> Self {
        Self { pull: true }
    }
}

// ── lifecycle action ────────────────────────────────────────────────────────────

/// Perform a simple lifecycle action (start/stop/restart/pause/resume) on a
/// single container, returning a host-tagged result value.
///
/// `subaction` maps user-facing strings to [`ContainerAction`] verbs:
/// - `"start"` → `Start`
/// - `"stop"` → `Stop`
/// - `"restart"` → `Restart`
/// - `"pause"` → `Pause`
/// - `"resume"` → `Unpause` (Docker API uses `unpause`)
pub async fn lifecycle_action_on_host(
    client: &dyn ContainerOps,
    host_name: &str,
    container_id: &str,
    subaction: &str,
) -> Result<Value, bollard::errors::Error> {
    let verb = parse_lifecycle_verb(subaction)?;
    client.container_action(container_id, verb).await?;
    Ok(json!({
        "host": host_name,
        "container": container_id,
        "action": subaction,
        "ok": true,
    }))
}

/// Map the user-facing subaction string to a [`ContainerAction`] variant.
///
/// Returns `DockerResponseServerError(400)` for unrecognised verbs so errors
/// propagate via the same error type as all other bollard errors.
fn parse_lifecycle_verb(subaction: &str) -> Result<ContainerAction, bollard::errors::Error> {
    match subaction {
        "start" => Ok(ContainerAction::Start),
        "stop" => Ok(ContainerAction::Stop),
        "restart" => Ok(ContainerAction::Restart),
        "pause" => Ok(ContainerAction::Pause),
        "resume" => Ok(ContainerAction::Unpause),
        other => Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 400,
            message: format!("unknown lifecycle subaction: {other}"),
        }),
    }
}

// ── pull (container-image pull) ────────────────────────────────────────────────

/// Pull the latest image for `image_ref` on a single host, returning a
/// host-tagged progress summary.
///
/// This is "pull for this container's image" (B9) — distinct from
/// `docker pull` (B10). Both use the same bollard stream.
pub async fn pull_image_on_host(
    client: &dyn ImageOps,
    host_name: &str,
    image_ref: &str,
) -> Result<Value, bollard::errors::Error> {
    let (from_image, tag) = split_image_ref(image_ref);
    let opts = CreateImageOptions {
        from_image: Some(from_image),
        tag,
        ..Default::default()
    };
    let mut stream = client.pull_image(Some(opts));
    let mut event_count: usize = 0;
    while let Some(item) = stream.next().await {
        item?;
        event_count += 1;
    }
    Ok(json!({
        "host": host_name,
        "image": image_ref,
        "pulled": true,
        "events": event_count,
    }))
}

/// Split a docker image reference into (`from_image`, optional `tag`).
/// Only treats the final `:segment` as a tag when it contains no `/`.
pub(crate) fn split_image_ref(image: &str) -> (String, Option<String>) {
    match image.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') && !tag.is_empty() => {
            (repo.to_owned(), Some(tag.to_owned()))
        }
        _ => (image.to_owned(), None),
    }
}

// ── recreate ──────────────────────────────────────────────────────────────────

/// Recreate a container: inspect → (optionally pull) → stop → remove → create
/// with the same config → start.
///
/// Preserves volumes and networks by reusing the `HostConfig`/`NetworkingConfig`
/// from the inspection. Returns a host-tagged result.
///
/// # Gate
///
/// The **caller** (service driver) must run the B5 Confirmer gate **before**
/// calling this function.
pub async fn recreate_on_host<C>(
    client: &C,
    host_name: &str,
    container_id: &str,
    params: &RecreateParams,
) -> Result<Value, bollard::errors::Error>
where
    C: ContainerOps + ImageOps,
{
    // Step 1: Inspect to capture config.
    let inspect = client.inspect_container(container_id, None).await?;
    let image_ref = extract_image_ref(&inspect);

    // Step 2: Optionally pull the latest image.
    if params.pull {
        let (from_image, tag) = split_image_ref(&image_ref);
        let opts = CreateImageOptions {
            from_image: Some(from_image),
            tag,
            ..Default::default()
        };
        let mut stream = client.pull_image(Some(opts));
        while let Some(item) = stream.next().await {
            item?;
        }
    }

    // Step 3: Stop the container (idempotent — if already stopped, ignore error).
    let _ = client
        .container_action(container_id, ContainerAction::Stop)
        .await;

    // Step 4: Remove the container (keep anonymous volumes handled by HostConfig).
    client
        .container_action(container_id, ContainerAction::Remove)
        .await?;

    // Step 5: Create a new container from the captured config.
    let name = extract_container_name(&inspect, container_id);
    let body = build_create_body(&inspect, &image_ref);
    let opts = CreateContainerOptions {
        name: Some(name.clone()),
        platform: String::new(),
    };
    let create_resp = client.create_container(Some(opts), body).await?;
    let new_id = create_resp.id;

    // Step 6: Start the new container.
    client
        .container_action(&new_id, ContainerAction::Start)
        .await?;

    Ok(json!({
        "host": host_name,
        "original_container": container_id,
        "new_container": new_id,
        "name": name,
        "image": image_ref,
        "pulled": params.pull,
        "status": "recreated",
    }))
}

/// Extract the image reference from an inspection response.
fn extract_image_ref(inspect: &ContainerInspectResponse) -> String {
    inspect
        .config
        .as_ref()
        .and_then(|c| c.image.clone())
        .unwrap_or_default()
}

/// Extract the container name (leading `/` stripped) from an inspection response.
fn extract_container_name(inspect: &ContainerInspectResponse, fallback: &str) -> String {
    inspect
        .name
        .as_deref()
        .map(|n| n.trim_start_matches('/').to_owned())
        .unwrap_or_else(|| fallback.to_owned())
}

/// Build a [`ContainerCreateBody`] from an inspection response, preserving env,
/// cmd, entrypoint, labels, volumes, host config, and networking config.
fn build_create_body(inspect: &ContainerInspectResponse, image_ref: &str) -> ContainerCreateBody {
    let cfg = inspect.config.as_ref();
    let host_cfg = inspect.host_config.as_ref();
    let net_settings = inspect.network_settings.as_ref();

    // Reconstruct NetworkingConfig from the connected networks.
    let networking_config = net_settings
        .and_then(|ns| ns.networks.as_ref())
        .map(|nets| NetworkingConfig {
            endpoints_config: Some(nets.clone()),
        });

    ContainerCreateBody {
        image: Some(image_ref.to_owned()),
        env: cfg.and_then(|c| c.env.clone()),
        cmd: cfg.and_then(|c| c.cmd.clone()),
        entrypoint: cfg.and_then(|c| c.entrypoint.clone()),
        labels: cfg.and_then(|c| c.labels.clone()),
        working_dir: cfg.and_then(|c| c.working_dir.clone()),
        user: cfg.and_then(|c| c.user.clone()),
        volumes: cfg.and_then(|c| c.volumes.clone()),
        host_config: host_cfg.cloned(),
        networking_config,
        ..Default::default()
    }
}

// ── exec ──────────────────────────────────────────────────────────────────────

/// Execute a command inside a container (3-step: create_exec → start_exec →
/// inspect_exec). One-shot — no interactive TTY. Combined stdout+stderr
/// collected from the stream; exit code from the inspection.
///
/// # Gate
///
/// The **caller** (service driver) must run the B5 Confirmer gate **before**
/// calling this function. NEVER passes `command` through `sh -c`.
///
/// # Timeout
///
/// The entire exec call is wrapped in `tokio::time::timeout` using
/// `params.timeout_ms` (clamped to `[1000, 300000]`).
pub async fn exec_on_host(
    client: &dyn ContainerOps,
    host_name: &str,
    params: &ExecParams,
) -> Result<Value, bollard::errors::Error> {
    let timeout_ms = params
        .timeout_ms
        .clamp(EXEC_TIMEOUT_MIN_MS, EXEC_TIMEOUT_MAX_MS);
    let timeout = Duration::from_millis(timeout_ms);

    let result = tokio::time::timeout(timeout, exec_inner(client, params))
        .await
        .map_err(|_elapsed| bollard::errors::Error::RequestTimeoutError)?;

    let result = result?;
    Ok(json!({
        "host": host_name,
        "container": params.container_id,
        "command": params.command,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "exit_code": result.exit_code,
        "ok": result.exit_code.map(|c| c == 0).unwrap_or(false),
    }))
}

/// Inner exec — no timeout wrapper. Called by [`exec_on_host`].
async fn exec_inner(
    client: &dyn ContainerOps,
    params: &ExecParams,
) -> Result<ExecResult, bollard::errors::Error> {
    if params.command.is_empty() {
        return Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 400,
            message: "exec: command must not be empty".to_owned(),
        });
    }

    // Step 1: Create exec instance.
    let exec_config = ExecConfig {
        cmd: Some(params.command.clone()),
        user: params.user.clone(),
        working_dir: params.workdir.clone(),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        // No TTY — one-shot output capture.
        tty: Some(false),
        ..Default::default()
    };
    let create_result = client
        .create_exec(&params.container_id, exec_config)
        .await?;
    let exec_id = create_result.id;

    // Step 2: Start exec (attached, non-detached) and drain the output stream.
    let start_opts = StartExecOptions {
        detach: false,
        tty: false,
        ..Default::default()
    };
    let start_result = client.start_exec(&exec_id, Some(start_opts)).await?;

    let (mut stdout_parts, mut stderr_parts): (Vec<String>, Vec<String>) = (vec![], vec![]);
    if let StartExecResults::Attached { mut output, .. } = start_result {
        use bollard::container::LogOutput;
        while let Some(frame) = output.next().await {
            match frame? {
                LogOutput::StdOut { message } => {
                    stdout_parts.push(String::from_utf8_lossy(&message).into_owned());
                }
                LogOutput::StdErr { message } => {
                    stderr_parts.push(String::from_utf8_lossy(&message).into_owned());
                }
                _ => {}
            }
        }
    }

    // Step 3: Inspect exec to get the exit code.
    let inspect = client.inspect_exec(&exec_id).await?;
    let exit_code = inspect.exit_code;

    Ok(ExecResult {
        stdout: stdout_parts.join(""),
        stderr: stderr_parts.join(""),
        exit_code,
    })
}

#[cfg(test)]
#[path = "container_lifecycle_tests.rs"]
mod tests;

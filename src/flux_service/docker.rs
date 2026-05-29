//! Docker system + image operations (B10): `info`, `df`, `images`, `networks`,
//! `volumes`, `pull`, `build`, `rmi`, `prune`.
//!
//! # Architecture seam
//!
//! Mirrors [`container_read`](super::container_read): the **pure** per-host
//! functions here operate on the segregated `&dyn …Ops` trait objects so they are
//! fully unit-testable with [`MockDockerClient`](crate::docker_client::MockDockerClient).
//! [`FluxService`](super::FluxService) resolves hosts, acquires the cached bollard
//! client, drives fanout for the read-only ops, and enforces the destructive gate
//! for `pull`/`build`/`rmi`/`prune` **before** calling these functions.
//!
//! # Read vs destructive
//!
//! - **Read-only** (`info`, `df`, `images`, `networks`, `volumes`) fan out across
//!   all target hosts when `host` is unspecified (B6).
//! - **Destructive / mutating** (`pull`, `build`, `rmi`, `prune`) are single-host
//!   only (host required) and pass through the B5 elicitation gate at the service
//!   layer. `pull` mutates (writes an image) but is non-gated by convention,
//!   matching synapse-mcp; `build`, `rmi`, `prune` are gated.
//!
//! # `build`
//!
//! bollard's `build_image` requires a streamed tar of the build context, which is
//! substantially more code than the value it adds for a homelab tool. Per the
//! bead's locked decision, `build` shells out to `docker build` (the gate still
//! runs at the service layer before the subprocess; context validation is
//! schema-level). All other ops use bollard.

use std::collections::HashMap;

use anyhow::{bail, Result};
use bollard::query_parameters::{
    CreateImageOptions, ListImagesOptions, PruneBuildOptions, PruneContainersOptions,
    PruneImagesOptions, PruneNetworksOptions, PruneVolumesOptions, RemoveImageOptions,
};
use futures_util::StreamExt;
use serde_json::{json, Map, Value};

use crate::docker_client::{ContainerOps, ImageOps, NetworkOps, SystemOps, VolumeOps};

// ───────────────────────────── read-only ─────────────────────────────

/// System-wide docker `info`, host-tagged.
pub async fn info_on_host(
    client: &dyn SystemOps,
    host_name: &str,
) -> Result<Value, bollard::errors::Error> {
    let info = client.info().await?;
    let body = serde_json::to_value(&info).unwrap_or(Value::Null);
    Ok(json!({ "host": host_name, "info": body }))
}

/// Disk usage (`docker system df`), host-tagged.
pub async fn df_on_host(
    client: &dyn SystemOps,
    host_name: &str,
) -> Result<Value, bollard::errors::Error> {
    let df = client.df(None).await?;
    let body = serde_json::to_value(&df).unwrap_or(Value::Null);
    Ok(json!({ "host": host_name, "df": body }))
}

/// List images on a single host. `dangling_only` adds the server-side
/// `dangling=true` filter (parity with synapse-mcp). Returns host-tagged
/// per-image summaries.
pub async fn images_on_host(
    client: &dyn ImageOps,
    host_name: &str,
    dangling_only: bool,
) -> Result<Vec<Value>, bollard::errors::Error> {
    let mut opts = ListImagesOptions {
        all: false,
        ..Default::default()
    };
    if dangling_only {
        let mut filters = HashMap::new();
        filters.insert("dangling".to_owned(), vec!["true".to_owned()]);
        opts.filters = Some(filters);
    }
    let images = client.list_images(Some(opts)).await?;
    Ok(images
        .iter()
        .map(|img| image_summary_value(img, host_name))
        .collect())
}

/// Render a [`bollard::models::ImageSummary`] into a host-tagged shape.
fn image_summary_value(img: &bollard::models::ImageSummary, host_name: &str) -> Value {
    json!({
        "id": img.id,
        "tags": img.repo_tags,
        "digests": img.repo_digests,
        "size": img.size,
        "created": img.created,
        "containers": img.containers,
        "labels": img.labels,
        "host": host_name,
    })
}

/// List networks on a single host, host-tagged.
pub async fn networks_on_host(
    client: &dyn NetworkOps,
    host_name: &str,
) -> Result<Vec<Value>, bollard::errors::Error> {
    let networks = client.list_networks(None).await?;
    Ok(networks
        .iter()
        .map(|n| {
            let mut v = serde_json::to_value(n).unwrap_or_else(|_| json!({}));
            if let Some(obj) = v.as_object_mut() {
                obj.insert("host".into(), json!(host_name));
            }
            v
        })
        .collect())
}

/// List volumes on a single host, host-tagged.
pub async fn volumes_on_host(
    client: &dyn VolumeOps,
    host_name: &str,
) -> Result<Vec<Value>, bollard::errors::Error> {
    let resp = client.list_volumes(None).await?;
    let volumes = resp.volumes.unwrap_or_default();
    Ok(volumes
        .iter()
        .map(|vol| {
            let mut v = serde_json::to_value(vol).unwrap_or_else(|_| json!({}));
            if let Some(obj) = v.as_object_mut() {
                obj.insert("host".into(), json!(host_name));
            }
            v
        })
        .collect())
}

// ───────────────────────────── pull ─────────────────────────────

/// Pull an image on a single host, draining the bollard `create_image` stream
/// one-shot and collecting progress frames. `image` is the full reference
/// (`repo:tag`); the tag (if any) is split out for the `fromImage`/`tag` params.
pub async fn pull_on_host(
    client: &dyn ImageOps,
    host_name: &str,
    image: &str,
) -> Result<Value, bollard::errors::Error> {
    let (from_image, tag) = split_image_ref(image);
    let opts = CreateImageOptions {
        from_image: Some(from_image.clone()),
        tag: tag.clone(),
        ..Default::default()
    };
    let mut stream = client.pull_image(Some(opts));
    let mut frames: Vec<Value> = Vec::new();
    while let Some(item) = stream.next().await {
        let info = item?;
        frames.push(serde_json::to_value(&info).unwrap_or(Value::Null));
    }
    Ok(json!({
        "host": host_name,
        "image": image,
        "pulled": true,
        "events": frames.len(),
        "progress": frames,
    }))
}

/// Split a docker image reference into (`fromImage`, optional `tag`).
///
/// Only treats the final `:segment` as a tag when it contains no `/` (so a
/// registry port like `host:5000/repo` is left intact as the image part).
fn split_image_ref(image: &str) -> (String, Option<String>) {
    match image.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') && !tag.is_empty() => {
            (repo.to_owned(), Some(tag.to_owned()))
        }
        _ => (image.to_owned(), None),
    }
}

// ───────────────────────────── rmi ─────────────────────────────

/// Remove an image on a single host. `force` is required by the schema layer;
/// the destructive gate runs before this is called.
pub async fn rmi_on_host(
    client: &dyn ImageOps,
    host_name: &str,
    image: &str,
    force: bool,
) -> Result<Value, bollard::errors::Error> {
    let opts = RemoveImageOptions {
        force,
        ..Default::default()
    };
    let removed = client.remove_image(image, Some(opts)).await?;
    let body = serde_json::to_value(&removed).unwrap_or(Value::Null);
    Ok(json!({
        "host": host_name,
        "image": image,
        "removed": body,
    }))
}

// ───────────────────────────── prune ─────────────────────────────

/// A `docker prune` target. `All` fans out to every prune API (same scope as
/// synapse-mcp's `prune_target=all` — NOT a "danger nuke").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneTarget {
    Containers,
    Images,
    Volumes,
    Networks,
    BuildCache,
    All,
}

impl PruneTarget {
    /// Parse the `prune_target` enum string. Unknown value → hard error.
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "containers" => Ok(Self::Containers),
            "images" => Ok(Self::Images),
            "volumes" => Ok(Self::Volumes),
            "networks" => Ok(Self::Networks),
            "buildcache" | "build" | "build_cache" => Ok(Self::BuildCache),
            "all" => Ok(Self::All),
            other => bail!(
                "unknown prune_target {other:?}; expected one of containers, images, volumes, \
                 networks, buildcache, all"
            ),
        }
    }

    /// The human-readable confirmation scope shown at the gate. The `All` variant
    /// is explicit per the security review — a generic "are you sure" invites
    /// yes-clicking.
    pub fn confirmation_details(&self) -> &'static str {
        match self {
            Self::Containers => "delete all stopped containers",
            Self::Images => "delete all dangling images",
            Self::Volumes => "delete all unused volumes (DATA LOSS)",
            Self::Networks => "delete all unused networks",
            Self::BuildCache => "delete all build cache",
            Self::All => {
                "This will delete ALL unused images, stopped containers, unused networks, unused \
                 volumes, AND build cache."
            }
        }
    }
}

/// Run a prune on a single host for the given target. `All` calls every prune
/// API and aggregates per-target results. The gate runs before this is called.
pub async fn prune_on_host(
    client: &dyn FullDocker,
    host_name: &str,
    target: PruneTarget,
) -> Result<Value, bollard::errors::Error> {
    let mut results = Map::new();
    let targets: &[PruneTarget] = match target {
        PruneTarget::All => &[
            PruneTarget::Containers,
            PruneTarget::Images,
            PruneTarget::Volumes,
            PruneTarget::Networks,
            PruneTarget::BuildCache,
        ],
        PruneTarget::Containers => &[PruneTarget::Containers],
        PruneTarget::Images => &[PruneTarget::Images],
        PruneTarget::Volumes => &[PruneTarget::Volumes],
        PruneTarget::Networks => &[PruneTarget::Networks],
        PruneTarget::BuildCache => &[PruneTarget::BuildCache],
    };
    for t in targets {
        let (key, value) = prune_single(client, *t).await?;
        results.insert(key.into(), value);
    }
    Ok(json!({
        "host": host_name,
        "target": prune_target_label(target),
        "pruned": Value::Object(results),
    }))
}

fn prune_target_label(t: PruneTarget) -> &'static str {
    match t {
        PruneTarget::Containers => "containers",
        PruneTarget::Images => "images",
        PruneTarget::Volumes => "volumes",
        PruneTarget::Networks => "networks",
        PruneTarget::BuildCache => "buildcache",
        PruneTarget::All => "all",
    }
}

/// Run one prune API and return its (label, json) result.
async fn prune_single(
    client: &dyn FullDocker,
    target: PruneTarget,
) -> Result<(&'static str, Value), bollard::errors::Error> {
    let value = match target {
        PruneTarget::Containers => {
            let r = client
                .prune_containers(None::<PruneContainersOptions>)
                .await?;
            serde_json::to_value(&r).unwrap_or(Value::Null)
        }
        PruneTarget::Images => {
            let r = client.prune_images(None::<PruneImagesOptions>).await?;
            serde_json::to_value(&r).unwrap_or(Value::Null)
        }
        PruneTarget::Volumes => {
            let r = client.prune_volumes(None::<PruneVolumesOptions>).await?;
            serde_json::to_value(&r).unwrap_or(Value::Null)
        }
        PruneTarget::Networks => {
            let r = client.prune_networks(None::<PruneNetworksOptions>).await?;
            serde_json::to_value(&r).unwrap_or(Value::Null)
        }
        PruneTarget::BuildCache => {
            let r = client.prune_build(None::<PruneBuildOptions>).await?;
            serde_json::to_value(&r).unwrap_or(Value::Null)
        }
        PruneTarget::All => unreachable!("prune_single is never called with All"),
    };
    Ok((prune_target_label(target), value))
}

/// Trait alias for the full prune surface — a single bound that covers every
/// prune API. The bollard client and the mock both satisfy it.
pub trait FullDocker: ContainerOps + ImageOps + NetworkOps + VolumeOps + SystemOps {}
impl<T> FullDocker for T where T: ContainerOps + ImageOps + NetworkOps + VolumeOps + SystemOps {}

// ───────────────────────────── build ─────────────────────────────

/// Parsed, validated arguments for `docker build`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildArgs {
    /// Absolute build context directory (validated: absolute, no `..`, no `~`/`$`).
    pub context: String,
    /// Image tag (`-t`).
    pub tag: String,
    /// Dockerfile path relative to context (optional).
    pub dockerfile: Option<String>,
    /// `--no-cache`.
    pub no_cache: bool,
}

/// Validate a `docker build` context path.
///
/// Reuses [`validate_safe_path`](crate::synapse::validate_safe_path) (absolute,
/// no `..`, char allowlist that already excludes `~` and `$`) and additionally
/// rejects paths inside the docker socket directory (security review). The
/// allowlist rejection of `~`/`$` is asserted in tests so a future loosening of
/// `validate_safe_path` cannot silently weaken this boundary.
pub fn validate_build_context(context: &str) -> Result<()> {
    crate::synapse::validate_safe_path(context)?;
    // Defense in depth: reject the docker socket directory explicitly.
    if context == "/var/run" || context.starts_with("/var/run/docker") {
        bail!("build context must not be inside the docker socket directory");
    }
    Ok(())
}

/// Validate a Dockerfile path: relative (no leading `/`), no `..`, no `~`/`$`.
fn validate_dockerfile(dockerfile: &str) -> Result<()> {
    if dockerfile.is_empty() {
        bail!("dockerfile must not be empty");
    }
    if dockerfile.starts_with('/') {
        bail!("dockerfile must be relative to the build context");
    }
    if dockerfile.split('/').any(|p| p == "..") {
        bail!("dockerfile path traversal is not allowed");
    }
    if dockerfile.contains('~') || dockerfile.contains('$') {
        bail!("dockerfile must not contain ~ or $ expansion");
    }
    Ok(())
}

/// Parse + validate build arguments from already-extracted strings (shim has
/// pulled them out of JSON/CLI). Validation lives here, not in the shim.
pub fn build_args(
    context: &str,
    tag: &str,
    dockerfile: Option<&str>,
    no_cache: bool,
) -> Result<BuildArgs> {
    validate_build_context(context)?;
    if tag.is_empty() {
        bail!("build requires a tag");
    }
    if let Some(df) = dockerfile {
        validate_dockerfile(df)?;
    }
    Ok(BuildArgs {
        context: context.to_owned(),
        tag: tag.to_owned(),
        dockerfile: dockerfile.map(str::to_owned),
        no_cache,
    })
}

/// Run `docker build` as a subprocess on the local host (locked bead decision:
/// bollard's build API needs a streamed tar; subprocess is the sanctioned
/// fallback). The destructive gate has already run at the service layer.
pub async fn build_subprocess(host_name: &str, args: &BuildArgs) -> Result<Value> {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("build").arg("-t").arg(&args.tag);
    if args.no_cache {
        cmd.arg("--no-cache");
    }
    if let Some(df) = &args.dockerfile {
        // Absolute Dockerfile path = context + relative dockerfile.
        cmd.arg("-f").arg(format!("{}/{}", args.context, df));
    }
    cmd.arg(&args.context);
    let output = cmd.output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok(json!({
        "host": host_name,
        "tag": args.tag,
        "context": args.context,
        "succeeded": output.status.success(),
        "exit_code": output.status.code(),
        "stdout": stdout,
        "stderr": stderr,
    }))
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;

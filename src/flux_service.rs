//! Flux domain service — Docker / container / host / compose operations.
//!
//! Extracted from the `SynapseService` god-object so flux concerns live in one
//! focused module. Owns the per-host bollard Docker client cache (B2) and the
//! compose discovery engine + TTL cache (B12). Resolves hosts through the
//! injected `HostRepository`.
//!
//! All flux business logic lives here. CLI (`cli.rs`) and MCP (via `actions.rs`)
//! are thin shims that call into these methods.

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::sync::Arc;

use crate::compose::{ComposeDiscovery, ComposeProject};
use crate::docker_client::DockerClientCache;
use crate::elicitation_gate::Confirmer;
use crate::fanout::{fanout, FanoutOutcome};
use crate::host_config::HostRepository;
use crate::scout;
use crate::synapse::HostConfig;

pub mod container_read;
pub mod docker;

use container_read::{ListFilters, LogOptions};
use docker::{BuildArgs, PruneTarget};

#[cfg(test)]
#[path = "flux_service_tests.rs"]
mod tests;

/// Flux domain service. Cheap to clone — all fields are `Arc`-shared.
#[derive(Clone)]
pub struct FluxService {
    /// Host configuration repository — shared with the facade and scout so an
    /// injected repo (tests / DI) resolves the same hosts everywhere.
    pub(crate) host_repo: Arc<dyn HostRepository>,
    /// Compose project discovery engine + per-host TTL cache (B12). Held behind
    /// `Arc` so the shared cache survives clones.
    pub(crate) compose: Arc<ComposeDiscovery>,
    /// Per-host bollard Docker client cache (B2). One client per `HostConfig`,
    /// reused; remote hosts connect via B1's SSH-forwarded unix socket.
    pub(crate) docker_clients: Arc<DockerClientCache>,
}

impl FluxService {
    /// Construct with the supplied host repository and default discovery / client caches.
    pub fn new(host_repo: Arc<dyn HostRepository>) -> Self {
        Self {
            host_repo,
            compose: Arc::new(ComposeDiscovery::new(Arc::new(crate::ssh::SshPool::new()))),
            docker_clients: Arc::new(DockerClientCache::new()),
        }
    }

    pub async fn help(&self) -> Result<Value> {
        Ok(json!({
            "tool": "flux",
            "actions": {
                "docker": [
                    "info", "df", "images", "networks", "volumes",
                    "pull", "build", "rmi", "prune"
                ],
                "container": ["list", "inspect", "logs", "stats", "top", "search"],
                "host": ["status"],
                "help": []
            },
            "destructive": ["docker build", "docker rmi", "docker prune"],
            "deferred": ["compose", "destructive container lifecycle"],
        }))
    }

    // ── docker read-only ops (B10) ───────────────────────────────────────
    //
    // These fan out across target host(s) when `host` is unset, mirroring the
    // container read-only pattern. The per-host logic lives in the pure `docker`
    // submodule for unit-testability with a `MockDockerClient`.

    /// System info across target host(s), fanning out when `host` is unset.
    pub async fn docker_info(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| async move {
            let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
            docker::info_on_host(client.as_ref(), &h.name)
                .await
                .map_err(|e| e.to_string())
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "info"))
    }

    /// Disk usage (`docker system df`) across target host(s).
    pub async fn docker_df(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| async move {
            let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
            docker::df_on_host(client.as_ref(), &h.name)
                .await
                .map_err(|e| e.to_string())
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "df"))
    }

    /// List images across target host(s); `dangling_only` adds a server filter.
    pub async fn docker_images(&self, host: Option<&str>, dangling_only: bool) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| async move {
            let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
            docker::images_on_host(client.as_ref(), &h.name, dangling_only)
                .await
                .map_err(|e| e.to_string())
        })
        .await;
        Ok(flatten_list_outcome(outcome, "images"))
    }

    /// List networks across target host(s).
    pub async fn docker_networks(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| async move {
            let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
            docker::networks_on_host(client.as_ref(), &h.name)
                .await
                .map_err(|e| e.to_string())
        })
        .await;
        Ok(flatten_list_outcome(outcome, "networks"))
    }

    /// List volumes across target host(s).
    pub async fn docker_volumes(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| async move {
            let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
            docker::volumes_on_host(client.as_ref(), &h.name)
                .await
                .map_err(|e| e.to_string())
        })
        .await;
        Ok(flatten_list_outcome(outcome, "volumes"))
    }

    // ── docker mutating ops (B10) ────────────────────────────────────────
    //
    // Single-host only (host required). `pull` mutates but is non-gated by
    // convention (parity with synapse-mcp). `build`, `rmi`, `prune` are
    // destructive and pass through the B5 confirmation gate BEFORE any IO.

    /// Pull an image on a single host. Non-gated (writes an image but is the
    /// standard provisioning path).
    pub async fn docker_pull(&self, host: &str, image: &str) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        let client = self.docker_clients.client_for(&h).await?;
        docker::pull_on_host(client.as_ref(), &h.name, image)
            .await
            .map_err(Into::into)
    }

    /// Build an image from a context on a single host (subprocess; locked bead
    /// decision). DESTRUCTIVE-adjacent: gated before the subprocess runs.
    pub async fn docker_build(
        &self,
        host: &str,
        args: BuildArgs,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        // Resolve host first so an unknown host is a validation error, not a gate.
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        confirmer
            .require(
                "docker build",
                &format!("build image {} on {}", args.tag, h.name),
            )
            .await?;
        docker::build_subprocess(&h.name, &args).await
    }

    /// Remove an image on a single host. DESTRUCTIVE: gated before IO.
    pub async fn docker_rmi(
        &self,
        host: &str,
        image: &str,
        force: bool,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        confirmer
            .require("docker rmi", &format!("remove image {image} on {}", h.name))
            .await?;
        let client = self.docker_clients.client_for(&h).await?;
        docker::rmi_on_host(client.as_ref(), &h.name, image, force)
            .await
            .map_err(Into::into)
    }

    /// Prune docker resources on a single host. DESTRUCTIVE: gated before IO.
    /// The confirmation details spell out the scope (security review).
    pub async fn docker_prune(
        &self,
        host: &str,
        target: PruneTarget,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        confirmer
            .require("docker prune", target.confirmation_details())
            .await?;
        let client = self.docker_clients.client_for(&h).await?;
        docker::prune_on_host(client.as_ref(), &h.name, target)
            .await
            .map_err(Into::into)
    }

    // ── container read-only ops (B8) ─────────────────────────────────────
    //
    // These drive host resolution + bollard client acquisition + multi-host
    // fanout. The per-host logic lives in the pure `container_read` submodule
    // so it stays unit-testable with a `MockDockerClient`.

    /// Resolve the target host slice: the named host, or all configured hosts.
    fn target_hosts(&self, host: Option<&str>) -> Result<Vec<HostConfig>> {
        match host {
            Some(name) => Ok(vec![scout::resolve_host(self.host_repo.as_ref(), name)?]),
            None => Ok(self.host_repo.load_hosts()?),
        }
    }

    /// List containers across target host(s), fanning out when `host` is unset.
    /// Returns a flat host-tagged container list with a `partial`/`errors` block.
    pub async fn container_list(&self, host: Option<&str>, filters: ListFilters) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| {
            let filters = filters.clone();
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                container_read::list_on_host(client.as_ref(), &h.name, &filters)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        Ok(flatten_list_outcome(outcome, "containers"))
    }

    /// Full-text search containers (name + image + labels) across target host(s).
    pub async fn container_search(&self, host: Option<&str>, query: &str) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let filters = ListFilters::default();
        let outcome = fanout(&hosts, |h| {
            let filters = filters.clone();
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                container_read::list_on_host(client.as_ref(), &h.name, &filters)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        let mut result = flatten_list_outcome(outcome, "containers");
        if let Some(arr) = result.get("containers").and_then(Value::as_array) {
            let matches: Vec<Value> = arr
                .iter()
                .filter(|c| container_read::search_matches(c, query))
                .cloned()
                .collect();
            let obj = result.as_object_mut().expect("flatten produces an object");
            obj.insert("count".into(), json!(matches.len()));
            obj.insert("containers".into(), json!(matches));
            obj.insert("query".into(), json!(query));
        }
        Ok(result)
    }

    /// One-shot stats for one container, or every container on the host(s) when
    /// `container_id` is `None`.
    pub async fn container_stats(
        &self,
        host: Option<&str>,
        container_id: Option<&str>,
    ) -> Result<Value> {
        if let Some(id) = container_id {
            // Single container: find-host then one-shot stats.
            return self
                .find_host_op(host, id, |client, host_name, id| {
                    Box::pin(container_read::stats_on_host(client, host_name, id))
                })
                .await;
        }
        // No id: fan out, collect per-host all-container stats.
        let hosts = self.target_hosts(host)?;
        let clients = &self.docker_clients;
        let outcome = fanout(&hosts, |h| async move {
            let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
            let containers =
                container_read::list_on_host(client.as_ref(), &h.name, &ListFilters::default())
                    .await
                    .map_err(|e| e.to_string())?;
            let mut stats = Vec::new();
            for c in &containers {
                if let Some(id) = c.get("id").and_then(Value::as_str) {
                    if let Ok(s) = container_read::stats_on_host(client.as_ref(), &h.name, id).await
                    {
                        stats.push(s);
                    }
                }
            }
            Ok::<_, String>(stats)
        })
        .await;
        Ok(flatten_list_outcome(outcome, "stats"))
    }

    /// Inspect a single container (full or `summary`), resolving its host.
    pub async fn container_inspect(
        &self,
        host: Option<&str>,
        container_id: &str,
        summary: bool,
    ) -> Result<Value> {
        self.find_host_op(host, container_id, move |client, host_name, id| {
            Box::pin(container_read::inspect_on_host(
                client, host_name, id, summary,
            ))
        })
        .await
    }

    /// Show running processes (`top`) in a single container, resolving its host.
    pub async fn container_top(&self, host: Option<&str>, container_id: &str) -> Result<Value> {
        self.find_host_op(host, container_id, |client, host_name, id| {
            Box::pin(container_read::top_on_host(client, host_name, id))
        })
        .await
    }

    /// Fetch one-shot logs for a single container, resolving its host.
    pub async fn container_logs(
        &self,
        host: Option<&str>,
        container_id: &str,
        opts: LogOptions,
    ) -> Result<Value> {
        let bollard_opts = container_read::build_logs_options(&opts)?;
        let grep = opts.grep.clone();
        let id = container_id.to_owned();
        self.find_host_op(host, container_id, move |client, host_name, _| {
            let bollard_opts = bollard_opts.clone();
            let grep = grep.clone();
            let id = id.clone();
            let host_name = host_name.to_owned();
            Box::pin(async move {
                let lines = container_read::collect_log_lines(client, &id, bollard_opts).await?;
                let lines = container_read::grep_lines(lines, grep.as_deref());
                Ok(container_read::logs_value(&host_name, &id, lines))
            })
        })
        .await
    }

    /// Run a single-container op against the named host, or fan out to find the
    /// owning host (first match wins) when `host` is unspecified.
    async fn find_host_op<F>(&self, host: Option<&str>, container_id: &str, op: F) -> Result<Value>
    where
        F: for<'a> Fn(
            &'a dyn crate::docker_client::ContainerOps,
            &'a str,
            &'a str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<Value, bollard::errors::Error>> + Send + 'a,
            >,
        >,
    {
        let hosts = self.target_hosts(host)?;
        // Named host → target directly (surface its error verbatim).
        if host.is_some() {
            let h = &hosts[0];
            let client = self.docker_clients.client_for(h).await?;
            return op(client.as_ref(), &h.name, container_id)
                .await
                .map_err(Into::into);
        }
        // Unspecified → probe hosts, first that has the container wins.
        let mut errors: Vec<String> = Vec::new();
        for h in &hosts {
            match self.docker_clients.client_for(h).await {
                Ok(client) => match op(client.as_ref(), &h.name, container_id).await {
                    Ok(value) => return Ok(value),
                    Err(e) => errors.push(format!("{}: {e}", h.name)),
                },
                Err(e) => errors.push(format!("{}: {e}", h.name)),
            }
        }
        Err(anyhow::anyhow!(
            "container {container_id} not found on any host ({})",
            errors.join("; ")
        ))
    }

    pub async fn host_status(&self, host: Option<&str>) -> Result<Value> {
        Ok(json!({
            "host": host.unwrap_or("local"),
            "docker": self.docker_info(host).await?,
        }))
    }

    /// Discover compose projects on `host_name`, merging `docker compose ls`
    /// with a filesystem scan (cache-aware). Thin delegation to the discovery
    /// engine; resolves the host via the injected repository.
    pub async fn compose_list(&self, host_name: &str) -> Result<Vec<ComposeProject>> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        self.compose.list(&host).await
    }

    /// Invalidate the compose discovery cache for `host_name` (or all hosts when
    /// `None`), forcing the next `compose_list` to re-scan.
    pub fn compose_refresh(&self, host_name: Option<&str>) {
        self.compose.refresh(host_name);
    }
}

/// Flatten a fanout outcome whose per-host value is a `Vec<Value>` into one
/// host-tagged collection under `key`, with a `partial` flag and a per-host
/// `errors` map. Each item already carries its `host` tag (injected by the
/// per-host op), so the ok results concatenate into a flat array.
fn flatten_list_outcome(outcome: FanoutOutcome<Vec<Value>, String>, key: &str) -> Value {
    let mut items: Vec<Value> = Vec::new();
    for (_host, vec) in outcome.ok_results() {
        items.extend(vec.iter().cloned());
    }
    let errors: Map<String, Value> = outcome
        .err_results()
        .iter()
        .map(|(host, err)| (host.clone(), json!(err)))
        .collect();

    let mut obj = Map::new();
    obj.insert("count".into(), json!(items.len()));
    obj.insert(key.into(), json!(items));
    obj.insert("partial".into(), json!(outcome.is_partial()));
    if !errors.is_empty() {
        obj.insert("errors".into(), Value::Object(errors));
    }
    Value::Object(obj)
}

/// Flatten a fanout outcome whose per-host value is a single `Value` (e.g.
/// `info`, `df`) into a host-keyed map under `key`, with a `partial` flag and a
/// per-host `errors` map. Each per-host value already carries its `host` tag.
fn flatten_scalar_outcome(outcome: FanoutOutcome<Value, String>, key: &str) -> Value {
    let results: Vec<Value> = outcome
        .ok_results()
        .iter()
        .map(|(_host, v)| v.clone())
        .collect();
    let errors: Map<String, Value> = outcome
        .err_results()
        .iter()
        .map(|(host, err)| (host.clone(), json!(err)))
        .collect();

    let mut obj = Map::new();
    obj.insert("count".into(), json!(results.len()));
    obj.insert(key.into(), json!(results));
    obj.insert("partial".into(), json!(outcome.is_partial()));
    if !errors.is_empty() {
        obj.insert("errors".into(), Value::Object(errors));
    }
    Value::Object(obj)
}

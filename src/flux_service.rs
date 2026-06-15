//! Flux domain service — Docker / container / host / compose operations.
//!
//! Extracted from the `SynapseService` god-object so flux concerns live in one
//! focused module. Owns the per-host bollard Docker client cache (B2) and the
//! compose discovery engine + TTL cache (B12). Resolves hosts through the
//! injected `HostRepository`.
//!
//! All flux business logic lives here (and in the driver submodules below). CLI
//! (`cli.rs`) and MCP (via `actions.rs`) are thin shims that call into these
//! methods.
//!
//! # Module layout
//!
//! - `flux_service.rs` — struct, constructors, shared helpers (`target_hosts`,
//!   `exec_for_host`, `help`, `flatten_*`).
//! - `flux_service/container_driver.rs` — `impl FluxService` for container ops.
//! - `flux_service/docker_driver.rs`    — `impl FluxService` for docker ops.
//! - `flux_service/host_driver.rs`      — `impl FluxService` for host ops.
//! - `flux_service/compose_driver.rs`   — `impl FluxService` for compose ops.
//! - `flux_service/{container_read,docker,host,compose_ops}.rs` — pure per-host fns.

use anyhow::Result;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::compose::ComposeDiscovery;
use crate::docker_client::DockerClientCache;
use crate::fanout::{FanoutOutcome, fanout};
use crate::host_config::HostRepository;
use crate::mcp::help as help_module;
use crate::scout;
use crate::ssh::SshPool;
use crate::synapse::HostConfig;

pub mod compose_driver;
pub mod compose_ops;
pub mod container_driver;
pub mod container_lifecycle;
pub mod container_read;
pub mod docker;
pub mod docker_driver;
pub mod host;
pub mod host_driver;

// Re-export the types callers need when going through the parent module path.
pub use compose_ops::{ComposeLogOptions, DownArgs};
pub use container_read::{ListFilters, LogOptions};
pub use docker::{BuildArgs, PruneTarget};
pub use host::{
    CheckResult, CheckStatus, LocalExec, RemoteExec, info_on_host, is_local_host, mounts_on_host,
    network_on_host, resources_on_host, services_on_host, uptime_on_host,
};

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
    /// SSH session pool for host shell commands (B11). Shared across clones so
    /// ControlMaster connections are reused.
    pub(crate) ssh_pool: Arc<SshPool>,
}

impl FluxService {
    /// Construct with the supplied host repository and default discovery / client caches.
    ///
    /// A single [`SshPool`] is created and shared by the compose discovery engine,
    /// the Docker client cache (for SSH-forwarded remote sockets), and the host-exec
    /// seam — collapsing what were three independent pools into one (`C-1`/`P-C1`).
    pub fn new(host_repo: Arc<dyn HostRepository>) -> Self {
        let ssh_pool = Arc::new(SshPool::new());
        Self {
            host_repo,
            compose: Arc::new(ComposeDiscovery::new(
                Arc::clone(&ssh_pool) as Arc<dyn crate::ssh::SshExecutor>
            )),
            docker_clients: Arc::new(DockerClientCache::with_pool(Arc::clone(&ssh_pool))),
            ssh_pool,
        }
    }

    /// Expose the shared SSH pool so callers (e.g. `SynapseService::new`) can
    /// pass it to the scout service, completing the single-pool topology.
    pub fn ssh_pool(&self) -> Arc<SshPool> {
        Arc::clone(&self.ssh_pool)
    }

    /// Return help for the flux tool.
    ///
    /// - `topic=None` → topic index (backwards-compatible shape extended with `topics`).
    /// - `topic=Some(t)` → per-subaction markdown for topic `t`; error if unknown.
    /// - `format=Some("json")` → wrap result in `{topic, text}` JSON envelope.
    pub async fn help(&self, topic: Option<&str>, format: Option<&str>) -> Result<Value> {
        match topic {
            None => Ok(help_module::legacy_flux_help()),
            Some(_) => help_module::help_response("flux", topic, format),
        }
    }

    // ── shared private helpers (used by multiple driver submodules) ───────

    /// Resolve the target host slice: the named host, or all configured hosts.
    pub(crate) fn target_hosts(&self, host: Option<&str>) -> Result<Vec<HostConfig>> {
        match host {
            Some(name) => Ok(vec![scout::resolve_host(self.host_repo.as_ref(), name)?]),
            None => Ok(self.host_repo.load_hosts()?),
        }
    }

    /// Resolve Docker operation targets, deduping all-host fanout by Docker
    /// daemon ID. This keeps aliases such as an SSH host name and the built-in
    /// `local` fallback from reporting the same Docker device twice.
    ///
    /// Explicit `--host` requests are preserved exactly: `--host local` still
    /// targets local even if another alias points at the same daemon.
    pub(crate) async fn target_docker_hosts(&self, host: Option<&str>) -> Result<Vec<HostConfig>> {
        let hosts = self.target_hosts(host)?;
        if host.is_some() {
            return Ok(hosts);
        }

        let clients = Arc::clone(&self.docker_clients);
        let discovery = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                docker::daemon_id(client.as_ref())
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        let daemon_ids = daemon_discovery_results(discovery);
        Ok(dedupe_hosts_by_daemon_id(hosts, &daemon_ids))
    }

    /// Build a `HostExec` impl for the given host: `LocalExec` for local
    /// protocol / localhost, `RemoteExec` (SSH pool) for everything else.
    /// The returned value holds a reference into `&self.ssh_pool`, hence the
    /// lifetime annotation tying it to `&self`.
    pub(crate) fn exec_for_host<'a>(
        &'a self,
        host: &'a HostConfig,
    ) -> Box<dyn host::HostExec + 'a> {
        if host::is_local_host(host) {
            Box::new(host::LocalExec)
        } else {
            Box::new(host::RemoteExec {
                executor: self.ssh_pool.as_ref(),
                host,
            })
        }
    }
}

type DaemonDiscoveryResult = (String, Result<Option<String>, String>);

fn daemon_discovery_results(
    discovery: FanoutOutcome<Option<String>, String>,
) -> Vec<DaemonDiscoveryResult> {
    let mut results = Vec::new();
    match discovery {
        FanoutOutcome::AllOk(ok) => {
            results.extend(ok.into_iter().map(|(host, id)| (host, Ok(id))));
        }
        FanoutOutcome::PartialSuccess { ok, errors } => {
            results.extend(ok.into_iter().map(|(host, id)| (host, Ok(id))));
            results.extend(errors.into_iter().map(|(host, err)| (host, Err(err))));
        }
        FanoutOutcome::AllFailed(errors) => {
            results.extend(errors.into_iter().map(|(host, err)| (host, Err(err))));
        }
    }
    results
}

fn dedupe_hosts_by_daemon_id(
    hosts: Vec<HostConfig>,
    daemon_ids: &[DaemonDiscoveryResult],
) -> Vec<HostConfig> {
    let daemon_ids_by_host: HashMap<&str, &Result<Option<String>, String>> = daemon_ids
        .iter()
        .map(|(host, result)| (host.as_str(), result))
        .collect();
    let mut seen_daemons = HashSet::new();
    let mut deduped = Vec::with_capacity(hosts.len());

    for host in hosts {
        match daemon_ids_by_host
            .get(host.name.as_str())
            .and_then(|result| result.as_ref().ok())
            .and_then(|id| id.as_deref())
        {
            Some(id) if seen_daemons.insert(id.to_owned()) => deduped.push(host),
            Some(_) => {}
            None => deduped.push(host),
        }
    }

    deduped
}

/// Flatten a fanout outcome whose per-host value is a `Vec<Value>` into one
/// host-tagged collection under `key`, with a `partial` flag and a per-host
/// `errors` map. Each item already carries its `host` tag (injected by the
/// per-host op), so the ok results concatenate into a flat array.
pub(crate) fn flatten_list_outcome(outcome: FanoutOutcome<Vec<Value>, String>, key: &str) -> Value {
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
pub(crate) fn flatten_scalar_outcome(outcome: FanoutOutcome<Value, String>, key: &str) -> Value {
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

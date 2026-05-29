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
use serde_json::{json, Map, Value};
use std::sync::Arc;

use crate::compose::ComposeDiscovery;
use crate::docker_client::DockerClientCache;
use crate::fanout::FanoutOutcome;
use crate::host_config::HostRepository;
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
    info_on_host, is_local_host, mounts_on_host, network_on_host, resources_on_host,
    services_on_host, uptime_on_host, CheckResult, CheckStatus, LocalExec, RemoteExec,
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
    pub fn new(host_repo: Arc<dyn HostRepository>) -> Self {
        let ssh_pool = Arc::new(SshPool::new());
        Self {
            host_repo,
            compose: Arc::new(ComposeDiscovery::new(
                Arc::clone(&ssh_pool) as Arc<dyn crate::ssh::SshExecutor>
            )),
            docker_clients: Arc::new(DockerClientCache::new()),
            ssh_pool,
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
                "container": [
                    "list", "inspect", "logs", "stats", "top", "search",
                    "start", "stop", "restart", "pause", "resume", "pull", "recreate", "exec"
                ],
                "host": [
                    "status", "info", "uptime", "resources",
                    "services", "network", "mounts", "ports", "doctor"
                ],
                "compose": [
                    "list", "status", "up", "down", "restart",
                    "recreate", "logs", "build", "pull", "refresh"
                ],
                "help": []
            },
            "destructive": [
                "docker build", "docker rmi", "docker prune",
                "compose down", "compose restart", "compose recreate",
                "container stop", "container recreate", "container exec"
            ],
        }))
    }

    // ── shared private helpers (used by multiple driver submodules) ───────

    /// Resolve the target host slice: the named host, or all configured hosts.
    pub(crate) fn target_hosts(&self, host: Option<&str>) -> Result<Vec<HostConfig>> {
        match host {
            Some(name) => Ok(vec![scout::resolve_host(self.host_repo.as_ref(), name)?]),
            None => Ok(self.host_repo.load_hosts()?),
        }
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

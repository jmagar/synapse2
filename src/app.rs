//! Business service layer — thin facade over focused domain services.
//!
//! **All business logic lives in the domain services.** CLI and MCP are thin
//! shims that call into this facade, which delegates to the sub-services.
//!
//! `SynapseService` owns:
//! - `flux: FluxService` — Docker / container / host / compose operations
//! - `scout: ScoutService` — node discovery, filesystem peek, remote exec
//!
//! Reach domain methods through the accessors: `service.flux().docker_info()`,
//! `service.scout().nodes()`. If you need caching, retries, data transformation,
//! or validation, do it in the relevant domain service — never in `cli.rs` or
//! `mcp/tools.rs`.

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

use crate::compose::ComposeDiscovery;
use crate::docker_client::DockerClientCache;
use crate::flux_service::FluxService;
use crate::host_config::{FileHostRepository, HostRepository};
use crate::scout_service::ScoutService;

// Re-export the scaffold contract types so existing callers that import them
// from `crate::app` (e.g. actions.rs's downcast, app_tests.rs) keep compiling.
pub use crate::scaffold::{ScaffoldIntent, ScaffoldIntentValidationError};

// Unit tests live in a sidecar file — see src/app_tests.rs for the pattern.
#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

/// The service layer — a thin facade wiring together the focused domain
/// services (flux + scout) over the shared host-topology repository.
#[derive(Clone)]
pub struct SynapseService {
    flux: FluxService,
    scout: ScoutService,
}

impl Default for SynapseService {
    fn default() -> Self {
        Self::new()
    }
}

impl SynapseService {
    /// Create a new `SynapseService` with the production host repository.
    ///
    /// The host repository resolves the real host topology (`SYNAPSE_HOSTS_CONFIG`
    /// → `SYNAPSE_CONFIG_FILE` → `~/.ssh/config`) shared by both flux and scout.
    ///
    /// A single [`SshPool`] is threaded from flux through to the scout service and
    /// the Docker client cache, so all three consumers share ControlMaster
    /// connections rather than opening independent pools (`C-1`/`P-C1`).
    pub fn new() -> Self {
        let host_repo: Arc<dyn HostRepository> = Arc::new(FileHostRepository::default());
        let flux = FluxService::new(Arc::clone(&host_repo));
        // Share flux's ssh_pool with scout so the whole process uses one SSH pool.
        // The cast to `Arc<dyn SshExecutor>` is required because ScoutService
        // holds the executor as a trait object (C-1/P-C1).
        let shared_pool = flux.ssh_pool() as Arc<dyn crate::ssh::SshExecutor>;
        let scout = ScoutService::new(host_repo).with_ssh_executor(shared_pool);
        Self { flux, scout }
    }

    /// Inject a custom `HostRepository` (for testing or future DI).
    ///
    /// Propagates to **both** the flux and scout sub-services so they resolve
    /// the same hosts.
    pub fn with_host_repo(mut self, repo: Arc<dyn HostRepository>) -> Self {
        self.flux.host_repo = Arc::clone(&repo);
        self.scout.host_repo = repo;
        self
    }

    /// Inject a custom compose discovery engine (for testing or future DI).
    pub fn with_compose_discovery(mut self, compose: Arc<ComposeDiscovery>) -> Self {
        self.flux.compose = compose;
        self
    }

    /// Inject a custom `DockerClientCache` (e.g. one sharing an `SshPool` with
    /// scout, or a cache primed for tests).
    pub fn with_docker_clients(mut self, cache: Arc<DockerClientCache>) -> Self {
        self.flux.docker_clients = cache;
        self
    }

    /// Access the flux domain service (Docker / container / host / compose).
    pub fn flux(&self) -> &FluxService {
        &self.flux
    }

    /// Access the scout domain service (nodes / peek / exec).
    pub fn scout(&self) -> &ScoutService {
        &self.scout
    }

    /// Convert elicited scaffold requirements into the handoff contract consumed
    /// by the skill. Thin delegation to the `scaffold` module.
    pub fn scaffold_intent(&self, input: ScaffoldIntent) -> Result<Value> {
        crate::scaffold::scaffold_intent(input)
    }
}

//! `FluxService` driver methods for Docker system + image ops (B10).
//!
//! This module holds `impl FluxService` blocks that drive host resolution,
//! bollard client acquisition, and fanout for docker operations. The per-host
//! logic (pure functions) lives in the `docker` sibling module.

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

use super::{
    FluxService,
    docker::{self, BuildArgs, PruneTarget},
    flatten_list_outcome, flatten_scalar_outcome,
};
use crate::docker_client::is_transport_dead;
use crate::elicitation_gate::Confirmer;
use crate::fanout::fanout;
use crate::scout;

#[cfg(test)]
#[path = "docker_driver_tests.rs"]
mod tests;

impl FluxService {
    // ── docker read-only ops (B10) ───────────────────────────────────────
    //
    // These fan out across target host(s) when `host` is unset, mirroring the
    // container read-only pattern. The per-host logic lives in the pure `docker`
    // submodule for unit-testability with a `MockDockerClient`.

    /// System info across target host(s), fanning out when `host` is unset.
    pub async fn docker_info(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_docker_hosts(host).await?;
        let clients = Arc::clone(&self.docker_clients);
        let outcome = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                docker::info_on_host(client.as_ref(), &h.name)
                    .await
                    .map_err(|e| {
                        if is_transport_dead(&e) {
                            clients.invalidate(&h);
                        }
                        e.to_string()
                    })
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "info"))
    }

    /// Disk usage (`docker system df`) across target host(s).
    pub async fn docker_df(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_docker_hosts(host).await?;
        let clients = Arc::clone(&self.docker_clients);
        let outcome = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                docker::df_on_host(client.as_ref(), &h.name)
                    .await
                    .map_err(|e| {
                        if is_transport_dead(&e) {
                            clients.invalidate(&h);
                        }
                        e.to_string()
                    })
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "df"))
    }

    /// List images across target host(s); `dangling_only` adds a server filter.
    pub async fn docker_images(&self, host: Option<&str>, dangling_only: bool) -> Result<Value> {
        let hosts = self.target_docker_hosts(host).await?;
        let clients = Arc::clone(&self.docker_clients);
        let outcome = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                docker::images_on_host(client.as_ref(), &h.name, dangling_only)
                    .await
                    .map_err(|e| {
                        if is_transport_dead(&e) {
                            clients.invalidate(&h);
                        }
                        e.to_string()
                    })
            }
        })
        .await;
        Ok(flatten_list_outcome(outcome, "images"))
    }

    /// List networks across target host(s).
    pub async fn docker_networks(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_docker_hosts(host).await?;
        let clients = Arc::clone(&self.docker_clients);
        let outcome = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                docker::networks_on_host(client.as_ref(), &h.name)
                    .await
                    .map_err(|e| {
                        if is_transport_dead(&e) {
                            clients.invalidate(&h);
                        }
                        e.to_string()
                    })
            }
        })
        .await;
        Ok(flatten_list_outcome(outcome, "networks"))
    }

    /// List volumes across target host(s).
    pub async fn docker_volumes(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_docker_hosts(host).await?;
        let clients = Arc::clone(&self.docker_clients);
        let outcome = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                docker::volumes_on_host(client.as_ref(), &h.name)
                    .await
                    .map_err(|e| {
                        if is_transport_dead(&e) {
                            clients.invalidate(&h);
                        }
                        e.to_string()
                    })
            }
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
        let exec = self.exec_for_host(&h);
        docker::build_on_host(exec.as_ref(), &h.name, &args).await
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
}

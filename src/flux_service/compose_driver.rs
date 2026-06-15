//! `FluxService` driver methods for compose operations (B13).
//!
//! This module holds `impl FluxService` blocks that drive host resolution,
//! compose project discovery, exec seam routing, and the destructive gate for
//! compose operations. The per-host pure functions live in `compose_ops`.

use anyhow::Result;
use serde_json::Value;

use super::{
    FluxService,
    compose_ops::{self, ComposeLogOptions, DownArgs},
};
use crate::compose::ComposeProject;
use crate::elicitation_gate::Confirmer;
use crate::scout;

#[cfg(test)]
#[path = "compose_driver_tests.rs"]
mod tests;

impl FluxService {
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

    /// Resolve a compose project on `host_name` by name and return its
    /// primary config file path. Errors clearly on project-not-found and on
    /// projects discovered without a config file (can't run compose without it).
    pub(super) async fn resolve_compose_project(
        &self,
        host: &crate::synapse::HostConfig,
        project_name: &str,
    ) -> Result<String> {
        let projects = self.compose.list(host).await?;
        let project = projects
            .iter()
            .find(|p| p.name == project_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "compose project {:?} not found on host {}; run `compose list` to see \
                     available projects",
                    project_name,
                    host.name
                )
            })?;
        project
            .primary_config_file()
            .map(str::to_owned)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "compose project {:?} on host {} has no config file path; \
                     it may be running but was not discovered via the filesystem scan",
                    project_name,
                    host.name
                )
            })
    }

    /// Show `docker compose ps` status for a project. Read-only.
    pub async fn compose_status(
        &self,
        host_name: &str,
        project_name: &str,
        service_filter: Option<&str>,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        let exec = self.exec_for_host(&h);
        compose_ops::status_on_host(
            exec.as_ref(),
            &h.name,
            project_name,
            &config_file,
            service_filter,
        )
        .await
    }

    /// Bring a compose project up (`docker compose up -d`). Not gated.
    pub async fn compose_up(&self, host_name: &str, project_name: &str) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        let exec = self.exec_for_host(&h);
        compose_ops::up_on_host(exec.as_ref(), &h.name, project_name, &config_file).await
    }

    /// Bring a compose project down. DESTRUCTIVE: gated before IO.
    ///
    /// `remove_volumes=true` requires `force=true` — validated here at the
    /// service layer so both CLI and MCP inherit the check.
    pub async fn compose_down(
        &self,
        host_name: &str,
        project_name: &str,
        args: DownArgs,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        // Validate before resolving the host so the error is schema-level.
        compose_ops::validate_down_args(&args)?;
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        let detail = if args.remove_volumes {
            format!(
                "stop and remove project {:?} (including volumes) on {}",
                project_name, h.name
            )
        } else {
            format!("stop and remove project {:?} on {}", project_name, h.name)
        };
        confirmer
            .require("compose down", &detail)
            .await
            .map_err(anyhow::Error::from)?;
        let exec = self.exec_for_host(&h);
        compose_ops::down_on_host(
            exec.as_ref(),
            &h.name,
            project_name,
            &config_file,
            args.remove_volumes,
        )
        .await
    }

    /// Restart a compose project. DESTRUCTIVE: gated before IO.
    pub async fn compose_restart(
        &self,
        host_name: &str,
        project_name: &str,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        confirmer
            .require(
                "compose restart",
                &format!("restart project {:?} on {}", project_name, h.name),
            )
            .await
            .map_err(anyhow::Error::from)?;
        let exec = self.exec_for_host(&h);
        compose_ops::restart_on_host(exec.as_ref(), &h.name, project_name, &config_file).await
    }

    /// Force-recreate a compose project. DESTRUCTIVE: gated before IO.
    pub async fn compose_recreate(
        &self,
        host_name: &str,
        project_name: &str,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        confirmer
            .require(
                "compose recreate",
                &format!("force-recreate project {:?} on {}", project_name, h.name),
            )
            .await
            .map_err(anyhow::Error::from)?;
        let exec = self.exec_for_host(&h);
        compose_ops::recreate_on_host(exec.as_ref(), &h.name, project_name, &config_file).await
    }

    /// Fetch logs for a compose project. Read-only, not gated.
    pub async fn compose_logs(
        &self,
        host_name: &str,
        project_name: &str,
        opts: ComposeLogOptions,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        let exec = self.exec_for_host(&h);
        compose_ops::logs_on_host(exec.as_ref(), &h.name, project_name, &config_file, &opts).await
    }

    /// Build images for a compose project. Not gated (parity: doesn't destroy state).
    pub async fn compose_build(
        &self,
        host_name: &str,
        project_name: &str,
        service: Option<&str>,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        let exec = self.exec_for_host(&h);
        compose_ops::build_on_host(exec.as_ref(), &h.name, project_name, &config_file, service)
            .await
    }

    /// Pull images for a compose project. Not gated (parity: doesn't destroy state).
    pub async fn compose_pull(
        &self,
        host_name: &str,
        project_name: &str,
        service: Option<&str>,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        let config_file = self.resolve_compose_project(&h, project_name).await?;
        let exec = self.exec_for_host(&h);
        compose_ops::pull_on_host(exec.as_ref(), &h.name, project_name, &config_file, service).await
    }
}

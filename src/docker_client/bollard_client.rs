//! [`BollardClient`] — the real [`DockerClient`](super::DockerClient) implementation.
//!
//! Owns the transport guard (SSH-forwarded socket + session for remote hosts,
//! nothing extra for local). The unit of caching in [`DockerClientCache`](super::DockerClientCache).

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecResults, StartExecOptions, StartExecResults};
use bollard::models::{
    BuildPruneResponse, ContainerInspectResponse, ContainerPruneResponse, ContainerStatsResponse,
    ContainerSummary, ContainerTopResponse, CreateImageInfo, ExecConfig, ExecInspectResponse,
    ImageDeleteResponseItem, ImagePruneResponse, ImageSummary, Network, NetworkPruneResponse,
    SystemDataUsageResponse, SystemInfo, VolumeListResponse, VolumePruneResponse,
};
use bollard::query_parameters::{
    CreateImageOptions, DataUsageOptions, InspectContainerOptions, ListContainersOptions,
    ListImagesOptions, ListNetworksOptions, ListVolumesOptions, LogsOptions, PruneBuildOptions,
    PruneContainersOptions, PruneImagesOptions, PruneNetworksOptions, PruneVolumesOptions,
    RemoveImageOptions, StatsOptions, TopOptions,
};
use bollard::{Docker, API_DEFAULT_VERSION};

use crate::ssh::{forward_socket_path, ForwardedSocket, PooledSession, SshPool};
use crate::synapse::HostConfig;

use super::traits::{
    BoxStream, ContainerAction, ContainerOps, ImageOps, NetworkOps, SystemOps, VolumeOps,
};
use super::CLIENT_TIMEOUT_SECS;

// ---------------------------------------------------------------------------
// BollardClient — the real implementation + its owned transport guard.
// ---------------------------------------------------------------------------

/// A cached, live Docker client for a single host.
///
/// For a **remote** host the bundle owns the [`ForwardedSocket`] guard and the
/// `Arc<PooledSession>` that keep the unix socket alive — bollard's `Docker` is
/// only valid while they live, so they are dropped together when the cache entry
/// is evicted. For a **local** host both are `None`.
pub struct BollardClient {
    docker: Docker,
    /// Held for the client's lifetime (remote only). On drop, the forward is
    /// torn down. Prefer explicit teardown via [`BollardClient::close`].
    forward: Option<ForwardedSocket>,
    /// Keeps the SSH session alive for the duration of the forward (remote only).
    _session: Option<Arc<PooledSession>>,
}

impl BollardClient {
    /// Connect to a **local** docker daemon. An explicit `docker_socket_path`
    /// wins (mirrors the TS factory's socket-path-first check); otherwise
    /// bollard's unix defaults (`DOCKER_HOST` / `/var/run/docker.sock`).
    pub fn connect_local(host: &HostConfig) -> Result<Self> {
        let docker = match host.docker_socket_path.as_deref() {
            Some(path) => {
                Docker::connect_with_socket(path, CLIENT_TIMEOUT_SECS, API_DEFAULT_VERSION)
                    .with_context(|| {
                        format!(
                            "connect bollard to local socket {path} for host {}",
                            host.name
                        )
                    })?
            }
            None => Docker::connect_with_unix_defaults().with_context(|| {
                format!("connect bollard to local docker for host {}", host.name)
            })?,
        };
        Ok(Self {
            docker,
            forward: None,
            _session: None,
        })
    }

    /// Connect to a **remote** docker daemon via a B1 SSH-forwarded unix socket.
    ///
    /// Checks out the shared SSH session, opens a 0600 forward to the remote
    /// `/var/run/docker.sock`, and points bollard at the local socket path. The
    /// forward + session are held inside the returned bundle for its lifetime.
    pub async fn connect_remote(pool: &SshPool, host: &HostConfig) -> Result<Self> {
        let pooled = pool.checkout(host).await?;
        let session = pooled.session();
        let forward = ForwardedSocket::open(session, forward_socket_path(host))
            .await
            .with_context(|| format!("forward docker socket for host {}", host.name))?;

        let path = forward.path().to_string_lossy().into_owned();
        let docker = Docker::connect_with_socket(&path, CLIENT_TIMEOUT_SECS, API_DEFAULT_VERSION)
            .with_context(|| {
            format!(
                "connect bollard to forwarded socket {path} for host {}",
                host.name
            )
        })?;

        Ok(Self {
            docker,
            forward: Some(forward),
            _session: Some(pooled),
        })
    }

    /// Borrow the underlying bollard `Docker` (cheap to clone if a caller needs
    /// an owned handle for streaming).
    pub fn docker(&self) -> &Docker {
        &self.docker
    }

    /// Explicit async teardown of the forwarded socket (remote only). Preferred
    /// over relying on `Drop` so the port-forward is closed deterministically.
    pub async fn close(self) -> Result<()> {
        if let Some(forward) = self.forward {
            forward.close().await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ContainerOps for BollardClient {
    async fn list_containers(
        &self,
        options: Option<ListContainersOptions>,
    ) -> Result<Vec<ContainerSummary>, bollard::errors::Error> {
        self.docker.list_containers(options).await
    }

    async fn inspect_container(
        &self,
        name: &str,
        options: Option<InspectContainerOptions>,
    ) -> Result<ContainerInspectResponse, bollard::errors::Error> {
        self.docker.inspect_container(name, options).await
    }

    async fn top_processes(
        &self,
        name: &str,
        options: Option<TopOptions>,
    ) -> Result<ContainerTopResponse, bollard::errors::Error> {
        self.docker.top_processes(name, options).await
    }

    fn logs(&self, name: &str, options: Option<LogsOptions>) -> BoxStream<LogOutput> {
        Box::pin(self.docker.logs(name, options))
    }

    fn stats(
        &self,
        name: &str,
        options: Option<StatsOptions>,
    ) -> BoxStream<ContainerStatsResponse> {
        Box::pin(self.docker.stats(name, options))
    }

    async fn container_action(
        &self,
        name: &str,
        action: ContainerAction,
    ) -> Result<(), bollard::errors::Error> {
        use bollard::query_parameters as q;
        match action {
            ContainerAction::Start => {
                self.docker
                    .start_container(name, None::<q::StartContainerOptions>)
                    .await
            }
            ContainerAction::Stop => {
                self.docker
                    .stop_container(name, None::<q::StopContainerOptions>)
                    .await
            }
            ContainerAction::Restart => {
                self.docker
                    .restart_container(name, None::<q::RestartContainerOptions>)
                    .await
            }
            ContainerAction::Pause => self.docker.pause_container(name).await,
            ContainerAction::Unpause => self.docker.unpause_container(name).await,
            ContainerAction::Kill => {
                self.docker
                    .kill_container(name, None::<q::KillContainerOptions>)
                    .await
            }
            ContainerAction::Remove => {
                self.docker
                    .remove_container(name, None::<q::RemoveContainerOptions>)
                    .await
            }
        }
    }

    async fn create_exec(
        &self,
        name: &str,
        config: ExecConfig,
    ) -> Result<CreateExecResults, bollard::errors::Error> {
        self.docker.create_exec(name, config).await
    }

    async fn start_exec(
        &self,
        exec_id: &str,
        options: Option<StartExecOptions>,
    ) -> Result<StartExecResults, bollard::errors::Error> {
        self.docker.start_exec(exec_id, options).await
    }

    async fn inspect_exec(
        &self,
        exec_id: &str,
    ) -> Result<ExecInspectResponse, bollard::errors::Error> {
        self.docker.inspect_exec(exec_id).await
    }

    async fn prune_containers(
        &self,
        options: Option<PruneContainersOptions>,
    ) -> Result<ContainerPruneResponse, bollard::errors::Error> {
        self.docker.prune_containers(options).await
    }
}

#[async_trait]
impl ImageOps for BollardClient {
    async fn list_images(
        &self,
        options: Option<ListImagesOptions>,
    ) -> Result<Vec<ImageSummary>, bollard::errors::Error> {
        self.docker.list_images(options).await
    }

    fn pull_image(&self, options: Option<CreateImageOptions>) -> BoxStream<CreateImageInfo> {
        Box::pin(self.docker.create_image(options, None, None))
    }

    async fn remove_image(
        &self,
        image_name: &str,
        options: Option<RemoveImageOptions>,
    ) -> Result<Vec<ImageDeleteResponseItem>, bollard::errors::Error> {
        self.docker.remove_image(image_name, options, None).await
    }

    async fn prune_images(
        &self,
        options: Option<PruneImagesOptions>,
    ) -> Result<ImagePruneResponse, bollard::errors::Error> {
        self.docker.prune_images(options).await
    }
}

#[async_trait]
impl NetworkOps for BollardClient {
    async fn list_networks(
        &self,
        options: Option<ListNetworksOptions>,
    ) -> Result<Vec<Network>, bollard::errors::Error> {
        self.docker.list_networks(options).await
    }

    async fn prune_networks(
        &self,
        options: Option<PruneNetworksOptions>,
    ) -> Result<NetworkPruneResponse, bollard::errors::Error> {
        self.docker.prune_networks(options).await
    }
}

#[async_trait]
impl VolumeOps for BollardClient {
    async fn list_volumes(
        &self,
        options: Option<ListVolumesOptions>,
    ) -> Result<VolumeListResponse, bollard::errors::Error> {
        self.docker.list_volumes(options).await
    }

    async fn prune_volumes(
        &self,
        options: Option<PruneVolumesOptions>,
    ) -> Result<VolumePruneResponse, bollard::errors::Error> {
        self.docker.prune_volumes(options).await
    }
}

#[async_trait]
impl SystemOps for BollardClient {
    async fn info(&self) -> Result<SystemInfo, bollard::errors::Error> {
        self.docker.info().await
    }

    async fn df(
        &self,
        options: Option<DataUsageOptions>,
    ) -> Result<SystemDataUsageResponse, bollard::errors::Error> {
        self.docker.df(options).await
    }

    async fn ping(&self) -> Result<String, bollard::errors::Error> {
        self.docker.ping().await
    }

    async fn prune_build(
        &self,
        options: Option<PruneBuildOptions>,
    ) -> Result<BuildPruneResponse, bollard::errors::Error> {
        self.docker.prune_build(options).await
    }
}

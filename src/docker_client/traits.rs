//! Segregated operation traits (ISP) and the composed [`DockerClient`] super-trait.
//!
//! Each sub-trait covers a distinct Docker resource domain. Mocks implement only
//! the sub-traits they exercise. The composed [`DockerClient`] is the surface that
//! action beads (B8–B13) depend on — object-safe so it can be used as
//! `&dyn DockerClient` in free-function action seams.

use std::pin::Pin;

use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecResults, StartExecOptions, StartExecResults};
use bollard::models::{
    BuildPruneResponse, ContainerCreateBody, ContainerCreateResponse, ContainerInspectResponse,
    ContainerPruneResponse, ContainerStatsResponse, ContainerSummary, ContainerTopResponse,
    CreateImageInfo, ExecConfig, ExecInspectResponse, ImageDeleteResponseItem, ImagePruneResponse,
    ImageSummary, Network, NetworkPruneResponse, SystemDataUsageResponse, SystemInfo,
    VolumeListResponse, VolumePruneResponse,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, DataUsageOptions, InspectContainerOptions,
    ListContainersOptions, ListImagesOptions, ListNetworksOptions, ListVolumesOptions, LogsOptions,
    PruneBuildOptions, PruneContainersOptions, PruneImagesOptions, PruneNetworksOptions,
    PruneVolumesOptions, RemoveImageOptions, StatsOptions, TopOptions,
};
use futures_util::Stream;

use anyhow::Result;

/// A boxed, `Send` stream — the return type for the streaming Docker surfaces
/// (`logs`, `stats`, attached exec output). `dyn DockerClient` requires a
/// concrete (boxed) return type rather than `impl Stream`.
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, bollard::errors::Error>> + Send>>;

// ---------------------------------------------------------------------------
// Segregated operation traits (ISP) — composed into `DockerClient`.
// Mocks implement only the sub-traits they exercise.
// ---------------------------------------------------------------------------

/// Container lifecycle, inspection, and streaming operations.
///
/// Read ops (`list`, `inspect`, `top`) are awaited single calls. `logs`/`stats`
/// return **streams** (bollard 0.21) — consumers (B8) drive them, applying their
/// own bounded backpressure. Exec is the 3-step `create_exec` → `start_exec`
/// (stream) → `inspect_exec` flow (B9); this trait exposes each primitive.
#[async_trait]
pub trait ContainerOps: Send + Sync {
    async fn list_containers(
        &self,
        options: Option<ListContainersOptions>,
    ) -> Result<Vec<ContainerSummary>, bollard::errors::Error>;

    async fn inspect_container(
        &self,
        name: &str,
        options: Option<InspectContainerOptions>,
    ) -> Result<ContainerInspectResponse, bollard::errors::Error>;

    async fn top_processes(
        &self,
        name: &str,
        options: Option<TopOptions>,
    ) -> Result<ContainerTopResponse, bollard::errors::Error>;

    /// Container logs as a stream. Unbounded at the source — B8 applies a
    /// bounded mpsc buffer for backpressure.
    fn logs(&self, name: &str, options: Option<LogsOptions>) -> BoxStream<LogOutput>;

    /// Live resource stats as a stream.
    fn stats(&self, name: &str, options: Option<StatsOptions>)
    -> BoxStream<ContainerStatsResponse>;

    /// Lifecycle action by container `name` (start/stop/restart/pause/unpause/
    /// kill/remove). `action` is the bollard endpoint verb; B9 maps user actions
    /// to these. Implemented as a thin passthrough.
    async fn container_action(
        &self,
        name: &str,
        action: ContainerAction,
    ) -> Result<(), bollard::errors::Error>;

    // --- exec, 3-step (B9) ---
    async fn create_exec(
        &self,
        name: &str,
        config: ExecConfig,
    ) -> Result<CreateExecResults, bollard::errors::Error>;

    async fn start_exec(
        &self,
        exec_id: &str,
        options: Option<StartExecOptions>,
    ) -> Result<StartExecResults, bollard::errors::Error>;

    async fn inspect_exec(
        &self,
        exec_id: &str,
    ) -> Result<ExecInspectResponse, bollard::errors::Error>;

    /// Prune stopped containers (B10 `docker prune` target `containers`).
    async fn prune_containers(
        &self,
        options: Option<PruneContainersOptions>,
    ) -> Result<ContainerPruneResponse, bollard::errors::Error>;

    /// Create a new container from the supplied config (B9 `recreate`).
    async fn create_container(
        &self,
        options: Option<CreateContainerOptions>,
        config: ContainerCreateBody,
    ) -> Result<ContainerCreateResponse, bollard::errors::Error>;
}

/// Lifecycle verbs for [`ContainerOps::container_action`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    Kill,
    Remove,
}

/// Image management and inspection.
#[async_trait]
pub trait ImageOps: Send + Sync {
    async fn list_images(
        &self,
        options: Option<ListImagesOptions>,
    ) -> Result<Vec<ImageSummary>, bollard::errors::Error>;

    /// Pull an image (`docker pull`). Drives bollard's `create_image` stream to
    /// completion and returns the collected progress frames (B10 `pull`).
    fn pull_image(&self, options: Option<CreateImageOptions>) -> BoxStream<CreateImageInfo>;

    /// Remove an image by name/id (B10 destructive `rmi`).
    async fn remove_image(
        &self,
        image_name: &str,
        options: Option<RemoveImageOptions>,
    ) -> Result<Vec<ImageDeleteResponseItem>, bollard::errors::Error>;

    /// Prune unused images (B10 destructive `prune` target `images`).
    async fn prune_images(
        &self,
        options: Option<PruneImagesOptions>,
    ) -> Result<ImagePruneResponse, bollard::errors::Error>;
}

/// Network resource operations.
#[async_trait]
pub trait NetworkOps: Send + Sync {
    async fn list_networks(
        &self,
        options: Option<ListNetworksOptions>,
    ) -> Result<Vec<Network>, bollard::errors::Error>;

    /// Prune unused networks (B10 destructive `prune` target `networks`).
    async fn prune_networks(
        &self,
        options: Option<PruneNetworksOptions>,
    ) -> Result<NetworkPruneResponse, bollard::errors::Error>;
}

/// Volume resource operations.
#[async_trait]
pub trait VolumeOps: Send + Sync {
    async fn list_volumes(
        &self,
        options: Option<ListVolumesOptions>,
    ) -> Result<VolumeListResponse, bollard::errors::Error>;

    /// Prune unused volumes (B10 destructive `prune` target `volumes`).
    async fn prune_volumes(
        &self,
        options: Option<PruneVolumesOptions>,
    ) -> Result<VolumePruneResponse, bollard::errors::Error>;
}

/// System-level information and health.
#[async_trait]
pub trait SystemOps: Send + Sync {
    async fn info(&self) -> Result<SystemInfo, bollard::errors::Error>;

    async fn df(
        &self,
        options: Option<DataUsageOptions>,
    ) -> Result<SystemDataUsageResponse, bollard::errors::Error>;

    /// Liveness probe. Used by [`DockerClientCache`](crate::docker_client::DockerClientCache)
    /// for explicit health checks and by B8/B9 to detect a dead tunnel.
    async fn ping(&self) -> Result<String, bollard::errors::Error>;

    /// Prune the build cache (B10 destructive `prune` target `buildcache`).
    async fn prune_build(
        &self,
        options: Option<PruneBuildOptions>,
    ) -> Result<BuildPruneResponse, bollard::errors::Error>;
}

/// The composed Docker client surface that action beads (B8–B13) depend on.
///
/// Object-safe (`Send + Sync`, boxed streams) so it can be used as
/// `&dyn DockerClient` in free-function action seams (mirroring scout's
/// `repo: &dyn HostRepository` pattern) and mocked without live docker.
pub trait DockerClient: ContainerOps + ImageOps + NetworkOps + VolumeOps + SystemOps {}

// Blanket impl: anything satisfying every sub-trait is a `DockerClient`.
impl<T> DockerClient for T where T: ContainerOps + ImageOps + NetworkOps + VolumeOps + SystemOps {}

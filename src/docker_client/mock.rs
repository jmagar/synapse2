//! Hand-written test double for the [`DockerClient`](super::DockerClient) trait surface.
//!
//! Lives behind `test-support` (not bare `cfg(test)`) so the integration-test
//! crate (B8/B9/B10/B13 in `tests/`) can reuse it — `tests/` is a separate crate
//! and cannot see `#[cfg(test)]`-only items. Re-exported via `lib::testing`.

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

use anyhow::Result;

use super::traits::{
    BoxStream, ContainerAction, ContainerOps, ImageOps, NetworkOps, SystemOps, VolumeOps,
};

/// A destructive mutating op recorded by [`MockDockerClient`] for gate-assertion
/// tests (B10). The `String` payload is the operative target (image name for
/// `rmi`, prune target for `prune_*`, empty otherwise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutatingOp {
    PullImage,
    RemoveImage(String),
    PruneContainers,
    PruneImages,
    PruneNetworks,
    PruneVolumes,
    PruneBuild,
}

/// A scriptable in-memory [`DockerClient`](super::DockerClient) for tests. Each
/// field holds the canned response for the corresponding operation; streaming/exec
/// surfaces default to empty/error so consumers can override only what they exercise.
#[derive(Default)]
pub struct MockDockerClient {
    pub containers: Vec<ContainerSummary>,
    pub images: Vec<ImageSummary>,
    pub networks: Vec<Network>,
    pub volumes: VolumeListResponse,
    pub info: SystemInfo,
    pub df: SystemDataUsageResponse,
    pub ping: String,
    /// Optional canned container inspection keyed by name.
    pub inspect: std::collections::HashMap<String, ContainerInspectResponse>,
    /// Optional canned top output keyed by name.
    pub top: std::collections::HashMap<String, ContainerTopResponse>,
    /// Optional canned log frames replayed by [`ContainerOps::logs`] (B8 tests).
    /// Each entry is one `LogOutput` frame; the default empty stream is used
    /// when this is empty.
    pub log_frames: Vec<LogOutput>,
    /// Optional canned stats frames replayed by [`ContainerOps::stats`].
    pub stats_frames: Vec<ContainerStatsResponse>,
    /// When true, [`ContainerOps::logs`] yields a single 404 error frame
    /// (simulates a missing container so find-host advances). B8 tests.
    pub logs_error: bool,
    /// Records every lifecycle action requested, for assertions.
    pub actions: std::sync::Mutex<Vec<(String, ContainerAction)>>,
    /// Optional canned pull progress frames replayed by [`ImageOps::pull_image`].
    pub pull_frames: Vec<CreateImageInfo>,
    /// Canned `rmi` response (image delete items).
    pub removed_images: Vec<ImageDeleteResponseItem>,
    /// Canned prune responses, keyed by op.
    pub image_prune: ImagePruneResponse,
    pub container_prune: ContainerPruneResponse,
    pub network_prune: NetworkPruneResponse,
    pub volume_prune: VolumePruneResponse,
    pub build_prune: BuildPruneResponse,
    /// Records every destructive mutating op requested, for gate assertions
    /// (B10). The gate-decline test asserts this stays empty; the
    /// allow-destructive test asserts it is populated.
    pub mutations: std::sync::Mutex<Vec<MutatingOp>>,
    /// Canned response for `create_container` (B9 recreate). If `None`, returns
    /// a default with id `"new-container"`.
    pub create_container_response: Option<ContainerCreateResponse>,
}

impl MockDockerClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recorded `(name, action)` lifecycle calls.
    pub fn recorded_actions(&self) -> Vec<(String, ContainerAction)> {
        self.actions.lock().expect("mock action log").clone()
    }

    /// Recorded destructive mutating ops, for gate assertions (B10).
    pub fn recorded_mutations(&self) -> Vec<MutatingOp> {
        self.mutations.lock().expect("mock mutation log").clone()
    }

    fn record_mutation(&self, op: MutatingOp) {
        self.mutations.lock().expect("mock mutation log").push(op);
    }
}

#[async_trait]
impl ContainerOps for MockDockerClient {
    async fn list_containers(
        &self,
        _options: Option<ListContainersOptions>,
    ) -> Result<Vec<ContainerSummary>, bollard::errors::Error> {
        Ok(self.containers.clone())
    }

    async fn inspect_container(
        &self,
        name: &str,
        _options: Option<InspectContainerOptions>,
    ) -> Result<ContainerInspectResponse, bollard::errors::Error> {
        Ok(self.inspect.get(name).cloned().unwrap_or_default())
    }

    async fn top_processes(
        &self,
        name: &str,
        _options: Option<TopOptions>,
    ) -> Result<ContainerTopResponse, bollard::errors::Error> {
        Ok(self.top.get(name).cloned().unwrap_or_default())
    }

    fn logs(&self, _name: &str, _options: Option<LogsOptions>) -> BoxStream<LogOutput> {
        if self.logs_error {
            let err = bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                message: "no such container".into(),
            };
            return Box::pin(futures_util::stream::iter(vec![Err(err)]));
        }
        let frames: Vec<Result<LogOutput, bollard::errors::Error>> =
            self.log_frames.iter().cloned().map(Ok).collect();
        Box::pin(futures_util::stream::iter(frames))
    }

    fn stats(
        &self,
        _name: &str,
        _options: Option<StatsOptions>,
    ) -> BoxStream<ContainerStatsResponse> {
        let frames: Vec<Result<ContainerStatsResponse, bollard::errors::Error>> =
            self.stats_frames.iter().cloned().map(Ok).collect();
        Box::pin(futures_util::stream::iter(frames))
    }

    async fn container_action(
        &self,
        name: &str,
        action: ContainerAction,
    ) -> Result<(), bollard::errors::Error> {
        self.actions
            .lock()
            .expect("mock action log")
            .push((name.to_string(), action));
        Ok(())
    }

    async fn create_exec(
        &self,
        _name: &str,
        _config: ExecConfig,
    ) -> Result<CreateExecResults, bollard::errors::Error> {
        Ok(CreateExecResults {
            id: "mock-exec".to_string(),
        })
    }

    async fn start_exec(
        &self,
        _exec_id: &str,
        _options: Option<StartExecOptions>,
    ) -> Result<StartExecResults, bollard::errors::Error> {
        Ok(StartExecResults::Detached)
    }

    async fn inspect_exec(
        &self,
        _exec_id: &str,
    ) -> Result<ExecInspectResponse, bollard::errors::Error> {
        Ok(ExecInspectResponse::default())
    }

    async fn prune_containers(
        &self,
        _options: Option<PruneContainersOptions>,
    ) -> Result<ContainerPruneResponse, bollard::errors::Error> {
        self.record_mutation(MutatingOp::PruneContainers);
        Ok(self.container_prune.clone())
    }

    async fn create_container(
        &self,
        _options: Option<CreateContainerOptions>,
        _config: ContainerCreateBody,
    ) -> Result<ContainerCreateResponse, bollard::errors::Error> {
        Ok(self
            .create_container_response
            .clone()
            .unwrap_or(ContainerCreateResponse {
                id: "new-container".to_owned(),
                warnings: vec![],
            }))
    }
}

#[async_trait]
impl ImageOps for MockDockerClient {
    async fn list_images(
        &self,
        _options: Option<ListImagesOptions>,
    ) -> Result<Vec<ImageSummary>, bollard::errors::Error> {
        Ok(self.images.clone())
    }

    fn pull_image(&self, _options: Option<CreateImageOptions>) -> BoxStream<CreateImageInfo> {
        self.record_mutation(MutatingOp::PullImage);
        let frames: Vec<Result<CreateImageInfo, bollard::errors::Error>> =
            self.pull_frames.iter().cloned().map(Ok).collect();
        Box::pin(futures_util::stream::iter(frames))
    }

    async fn remove_image(
        &self,
        image_name: &str,
        _options: Option<RemoveImageOptions>,
    ) -> Result<Vec<ImageDeleteResponseItem>, bollard::errors::Error> {
        self.record_mutation(MutatingOp::RemoveImage(image_name.to_owned()));
        Ok(self.removed_images.clone())
    }

    async fn prune_images(
        &self,
        _options: Option<PruneImagesOptions>,
    ) -> Result<ImagePruneResponse, bollard::errors::Error> {
        self.record_mutation(MutatingOp::PruneImages);
        Ok(self.image_prune.clone())
    }
}

#[async_trait]
impl NetworkOps for MockDockerClient {
    async fn list_networks(
        &self,
        _options: Option<ListNetworksOptions>,
    ) -> Result<Vec<Network>, bollard::errors::Error> {
        Ok(self.networks.clone())
    }

    async fn prune_networks(
        &self,
        _options: Option<PruneNetworksOptions>,
    ) -> Result<NetworkPruneResponse, bollard::errors::Error> {
        self.record_mutation(MutatingOp::PruneNetworks);
        Ok(self.network_prune.clone())
    }
}

#[async_trait]
impl VolumeOps for MockDockerClient {
    async fn list_volumes(
        &self,
        _options: Option<ListVolumesOptions>,
    ) -> Result<VolumeListResponse, bollard::errors::Error> {
        Ok(self.volumes.clone())
    }

    async fn prune_volumes(
        &self,
        _options: Option<PruneVolumesOptions>,
    ) -> Result<VolumePruneResponse, bollard::errors::Error> {
        self.record_mutation(MutatingOp::PruneVolumes);
        Ok(self.volume_prune.clone())
    }
}

#[async_trait]
impl SystemOps for MockDockerClient {
    async fn info(&self) -> Result<SystemInfo, bollard::errors::Error> {
        Ok(self.info.clone())
    }

    async fn df(
        &self,
        _options: Option<DataUsageOptions>,
    ) -> Result<SystemDataUsageResponse, bollard::errors::Error> {
        Ok(self.df.clone())
    }

    async fn ping(&self) -> Result<String, bollard::errors::Error> {
        Ok(self.ping.clone())
    }

    async fn prune_build(
        &self,
        _options: Option<PruneBuildOptions>,
    ) -> Result<BuildPruneResponse, bollard::errors::Error> {
        self.record_mutation(MutatingOp::PruneBuild);
        Ok(self.build_prune.clone())
    }
}

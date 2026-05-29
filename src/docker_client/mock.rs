//! Hand-written test double for the [`DockerClient`](super::DockerClient) trait surface.
//!
//! Lives behind `test-support` (not bare `cfg(test)`) so the integration-test
//! crate (B8/B9/B10/B13 in `tests/`) can reuse it — `tests/` is a separate crate
//! and cannot see `#[cfg(test)]`-only items. Re-exported via `lib::testing`.

use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecResults, StartExecOptions, StartExecResults};
use bollard::models::{
    ContainerInspectResponse, ContainerStatsResponse, ContainerSummary, ContainerTopResponse,
    ExecConfig, ExecInspectResponse, ImageSummary, Network, SystemDataUsageResponse, SystemInfo,
    VolumeListResponse,
};
use bollard::query_parameters::{
    DataUsageOptions, InspectContainerOptions, ListContainersOptions, ListImagesOptions,
    ListNetworksOptions, ListVolumesOptions, LogsOptions, StatsOptions, TopOptions,
};

use anyhow::Result;

use super::traits::{
    BoxStream, ContainerAction, ContainerOps, ImageOps, NetworkOps, SystemOps, VolumeOps,
};

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
}

impl MockDockerClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recorded `(name, action)` lifecycle calls.
    pub fn recorded_actions(&self) -> Vec<(String, ContainerAction)> {
        self.actions.lock().expect("mock action log").clone()
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
}

#[async_trait]
impl ImageOps for MockDockerClient {
    async fn list_images(
        &self,
        _options: Option<ListImagesOptions>,
    ) -> Result<Vec<ImageSummary>, bollard::errors::Error> {
        Ok(self.images.clone())
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
}

#[async_trait]
impl VolumeOps for MockDockerClient {
    async fn list_volumes(
        &self,
        _options: Option<ListVolumesOptions>,
    ) -> Result<VolumeListResponse, bollard::errors::Error> {
        Ok(self.volumes.clone())
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
}

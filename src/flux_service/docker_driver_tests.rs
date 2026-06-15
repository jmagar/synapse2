//! Sidecar tests for `flux_service/docker_driver.rs`.
//!
//! Covers the `FluxService` docker driver orchestration layer:
//! empty-host fanout shapes, unknown-host error paths for single-host ops,
//! and confirmation gate behavior for destructive ops.

use std::sync::Arc;

use async_trait::async_trait;

use crate::elicitation_gate::{ConfirmationDenied, Confirmer};
use crate::flux_service::{
    FluxService,
    docker::{BuildArgs, PruneTarget},
};
use crate::host_config::HostRepository;
use crate::synapse::{HostConfig, HostProtocol};

// ── helpers ───────────────────────────────────────────────────────────────────

fn ssh_host(name: &str) -> HostConfig {
    HostConfig {
        name: name.to_owned(),
        host: format!("{name}.example.test"),
        port: None,
        protocol: HostProtocol::Ssh,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: Vec::new(),
        scout_read_roots: Vec::new(),
        exec_allowlist: Vec::new(),
    }
}

struct StubRepo {
    hosts: Vec<HostConfig>,
}

impl HostRepository for StubRepo {
    fn load_hosts(&self) -> anyhow::Result<Vec<HostConfig>> {
        Ok(self.hosts.clone())
    }
}

fn flux_with_hosts(hosts: Vec<HostConfig>) -> FluxService {
    FluxService::new(Arc::new(StubRepo { hosts }))
}

struct DenyConfirmer;

#[async_trait]
impl Confirmer for DenyConfirmer {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Err(ConfirmationDenied::Declined)
    }
}

// ── empty fanout returns correct shapes ───────────────────────────────────────

#[tokio::test]
async fn docker_info_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.docker_info(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
    assert!(result.get("errors").is_none());
    assert!(result["info"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn docker_df_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.docker_df(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
    assert!(result["df"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn docker_images_empty_hosts_returns_empty_list_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.docker_images(None, false).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
    assert!(result["images"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn docker_networks_empty_hosts_returns_empty_list_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.docker_networks(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
    assert!(result["networks"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn docker_volumes_empty_hosts_returns_empty_list_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.docker_volumes(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
    assert!(result["volumes"].as_array().unwrap().is_empty());
}

// ── single-host ops reject unknown host before IO ────────────────────────────

#[tokio::test]
async fn docker_pull_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .docker_pull("missing", "alpine:latest")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn docker_build_rejects_unknown_host_before_gate() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let args = BuildArgs {
        context: "/srv/app".to_owned(),
        tag: "app:test".to_owned(),
        dockerfile: None,
        no_cache: false,
    };
    let err = flux
        .docker_build("missing", args, &DenyConfirmer)
        .await
        .unwrap_err();
    // Host resolution is before the gate — "unknown host" is the error.
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn docker_rmi_rejects_unknown_host_before_gate() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .docker_rmi("missing", "alpine:latest", false, &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn docker_prune_rejects_unknown_host_before_gate() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .docker_prune("missing", PruneTarget::Images, &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

// ── confirmation gate blocks destructive ops ──────────────────────────────────

#[tokio::test]
async fn docker_build_gate_decline_on_known_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let args = BuildArgs {
        context: "/srv/app".to_owned(),
        tag: "app:test".to_owned(),
        dockerfile: None,
        no_cache: false,
    };
    // Known host, but gate denies — should fail with gate error, not host error.
    // (The actual Docker op will also fail since it's a fake host, but the gate
    //  runs first so the error is "declined".)
    let err = flux
        .docker_build("alpha", args, &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

#[tokio::test]
async fn docker_rmi_gate_decline_on_known_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .docker_rmi("alpha", "alpine:latest", false, &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

#[tokio::test]
async fn docker_prune_gate_decline_on_known_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .docker_prune("alpha", PruneTarget::Containers, &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

// ── shape invariants: partial=false when empty ───────────────────────────────

/// Verify that AllOk (empty) is correctly represented — this exercises the
/// aggregation logic rather than real Docker connections.
#[tokio::test]
async fn docker_info_and_images_empty_produce_independent_shapes() {
    let flux = flux_with_hosts(Vec::new());

    let info = flux.docker_info(None).await.unwrap();
    let images = flux.docker_images(None, true).await.unwrap();

    // Both are empty AllOk — shape must be consistent.
    assert_eq!(info["count"], 0);
    assert_eq!(images["count"], 0);
    assert_eq!(info["partial"], false);
    assert_eq!(images["partial"], false);
    // dangling_only=true still produces same shape
    assert!(images["images"].as_array().unwrap().is_empty());
}

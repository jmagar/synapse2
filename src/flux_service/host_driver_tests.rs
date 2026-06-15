//! Sidecar tests for `flux_service/host_driver.rs`.
//!
//! Covers the `FluxService` host driver orchestration layer:
//! empty-host fanout shapes, unknown-host single-host errors, local-host
//! execution through the real exec seam, and the `host_doctor` partitioning.

use std::sync::Arc;

use crate::flux_service::FluxService;
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

fn local_host(name: &str) -> HostConfig {
    HostConfig {
        name: name.to_owned(),
        host: "localhost".to_owned(),
        port: None,
        protocol: HostProtocol::Local,
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

// ── empty fanout shapes ───────────────────────────────────────────────────────

#[tokio::test]
async fn host_status_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.host_status(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
    assert!(result.get("errors").is_none());
}

#[tokio::test]
async fn host_info_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.host_info(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
}

#[tokio::test]
async fn host_uptime_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.host_uptime(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
}

#[tokio::test]
async fn host_resources_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.host_resources(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
}

#[tokio::test]
async fn host_network_empty_hosts_returns_empty_scalar_shape() {
    let flux = flux_with_hosts(Vec::new());
    let result = flux.host_network(None).await.unwrap();
    assert_eq!(result["count"], 0);
    assert_eq!(result["partial"], false);
}

// ── single-host ops reject unknown host ──────────────────────────────────────

#[tokio::test]
async fn host_services_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux.host_services("missing", None, None).await.unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn host_mounts_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux.host_mounts("missing").await.unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn host_ports_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .host_ports("missing", None, None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn host_doctor_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux
        .host_doctor("missing", vec!["docker".to_owned()])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

// ── named-host fanout rejects unknown host ───────────────────────────────────

#[tokio::test]
async fn host_status_named_unknown_host_fails() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux.host_status(Some("missing")).await.unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn host_info_named_unknown_host_fails() {
    let flux = flux_with_hosts(vec![ssh_host("alpha")]);
    let err = flux.host_info(Some("missing")).await.unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

// ── local exec seam ───────────────────────────────────────────────────────────

/// `host_info` with the local protocol uses `LocalExec` (no SSH). Verify
/// the response has the right shape and actually ran on this machine.
#[tokio::test]
async fn host_info_local_host_returns_uname_output() {
    let flux = flux_with_hosts(vec![local_host("local")]);
    let result = flux.host_info(Some("local")).await.unwrap();
    assert_eq!(result["count"], 1, "one local host");
    let entry = &result["info"][0];
    assert_eq!(entry["host"], "local");
    // The uname output should be a non-empty string on any host.
    let info_str = entry["info"].as_str().unwrap_or("");
    assert!(!info_str.is_empty(), "uname output should be non-empty");
}

#[tokio::test]
async fn host_uptime_local_host_returns_uptime_output() {
    let flux = flux_with_hosts(vec![local_host("local")]);
    let result = flux.host_uptime(Some("local")).await.unwrap();
    assert_eq!(result["count"], 1);
    let entry = &result["uptime"][0];
    assert_eq!(entry["host"], "local");
    assert!(
        entry.get("uptime").is_some(),
        "uptime field should be present"
    );
}

#[tokio::test]
async fn host_mounts_local_host_returns_df_output() {
    let flux = flux_with_hosts(vec![local_host("local")]);
    let result = flux.host_mounts("local").await.unwrap();
    assert_eq!(result["host"], "local");
    let mounts = result["mounts"].as_str().unwrap_or("");
    // `df -h` header line is always present on a running system.
    assert!(
        mounts.contains("Filesystem") || mounts.contains("filesystem") || !mounts.is_empty(),
        "df output should be non-empty"
    );
}

/// `host_doctor` with exec-based checks (resources/network/processes) should
/// run on the local host and return all checks in the result.
#[tokio::test]
async fn host_doctor_local_exec_checks_return_results() {
    let flux = flux_with_hosts(vec![local_host("local")]);
    let checks = vec![
        "resources".to_owned(),
        "network".to_owned(),
        "processes".to_owned(),
    ];
    let result = flux.host_doctor("local", checks).await.unwrap();
    assert_eq!(result["host"], "local");
    let check_arr = result["checks"]
        .as_array()
        .expect("checks must be an array");
    assert_eq!(check_arr.len(), 3, "all 3 checks must be present");
    // Every check must have a status field.
    for check in check_arr {
        assert!(
            check.get("status").is_some(),
            "check {check:?} must have a status"
        );
    }
}

// ── multi-host fanout shape with local host ───────────────────────────────────

/// One local host in a multi-host fanout. The local host succeeds; verifying
/// that the outcome aggregation counts correctly (no partial, count=1).
#[tokio::test]
async fn host_info_one_local_host_gives_count_one_all_ok() {
    let flux = flux_with_hosts(vec![local_host("local")]);
    let result = flux.host_info(None).await.unwrap();
    assert_eq!(result["partial"], false, "single success is AllOk");
    assert_eq!(result["count"], 1);
    assert!(result.get("errors").is_none(), "no errors for local host");
}

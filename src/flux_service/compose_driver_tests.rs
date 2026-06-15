//! Sidecar tests for `flux_service/compose_driver.rs`.
//!
//! Covers the `FluxService` compose driver orchestration layer:
//! unknown-host rejection, project-not-found errors, confirmation gate behavior,
//! and the validation before host resolution (volume-removal guard).

use std::sync::Arc;

use async_trait::async_trait;

use crate::compose::ComposeDiscovery;
use crate::elicitation_gate::{ConfirmationDenied, Confirmer};
use crate::flux_service::{
    FluxService,
    compose_ops::{ComposeLogOptions, DownArgs},
};
use crate::host_config::HostRepository;
use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::{HostConfig, HostProtocol};

// ── helpers ───────────────────────────────────────────────────────────────────

fn compose_host(name: &str) -> HostConfig {
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
        compose_search_paths: vec!["/compose".to_owned()],
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

/// An executor that returns empty results — simulates a host with no compose projects.
struct EmptyExec;

#[async_trait]
impl SshExecutor for EmptyExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        _program: &str,
        _args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

/// An executor that returns a single compose project with a config file.
struct ProjectExec {
    project: String,
    config_file: String,
}

#[async_trait]
impl SshExecutor for ProjectExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        program: &str,
        args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        let stdout = match program {
            "docker" => format!(
                r#"[{{"Name":"{}","Status":"running(1)","ConfigFiles":"{}"}}]"#,
                self.project, self.config_file
            ),
            "find" => format!("{}\n", self.config_file),
            "cat" if args.last().is_some() => format!("name: {}\n", self.project),
            _ => String::new(),
        };
        Ok(CommandOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

struct DenyConfirmer;

#[async_trait]
impl Confirmer for DenyConfirmer {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Err(ConfirmationDenied::Declined)
    }
}

// ── unknown host rejection ────────────────────────────────────────────────────

#[tokio::test]
async fn compose_list_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![compose_host("alpha")]);
    let err = flux.compose_list("missing").await.unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn compose_status_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![compose_host("alpha")]);
    let err = flux
        .compose_status("missing", "myapp", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

#[tokio::test]
async fn compose_up_rejects_unknown_host() {
    let flux = flux_with_hosts(vec![compose_host("alpha")]);
    let err = flux.compose_up("missing", "myapp").await.unwrap_err();
    assert!(err.to_string().contains("unknown host"), "{err}");
}

// ── project not found ─────────────────────────────────────────────────────────

#[tokio::test]
async fn compose_status_missing_project_returns_not_found_error() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(EmptyExec)));

    let err = flux
        .compose_status("alpha", "missing", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("missing\" not found"), "{err}");
}

#[tokio::test]
async fn compose_up_missing_project_returns_not_found_error() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(EmptyExec)));

    let err = flux.compose_up("alpha", "ghost").await.unwrap_err();
    assert!(err.to_string().contains("ghost\" not found"), "{err}");
}

#[tokio::test]
async fn compose_logs_missing_project_returns_not_found_error() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(EmptyExec)));

    let err = flux
        .compose_logs(
            "alpha",
            "ghost",
            ComposeLogOptions {
                lines: Some(50),
                since: None,
                service: None,
            },
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("ghost\" not found"), "{err}");
}

// ── confirmation gate ─────────────────────────────────────────────────────────

#[tokio::test]
async fn compose_down_decline_blocks_before_exec() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(ProjectExec {
        project: "myapp".to_owned(),
        config_file: "/compose/myapp/docker-compose.yml".to_owned(),
    })));

    let err = flux
        .compose_down(
            "alpha",
            "myapp",
            DownArgs {
                remove_volumes: false,
                force: false,
            },
            &DenyConfirmer,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

#[tokio::test]
async fn compose_restart_decline_blocks_before_exec() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(ProjectExec {
        project: "myapp".to_owned(),
        config_file: "/compose/myapp/docker-compose.yml".to_owned(),
    })));

    let err = flux
        .compose_restart("alpha", "myapp", &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

#[tokio::test]
async fn compose_recreate_decline_blocks_before_exec() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(ProjectExec {
        project: "myapp".to_owned(),
        config_file: "/compose/myapp/docker-compose.yml".to_owned(),
    })));

    let err = flux
        .compose_recreate("alpha", "myapp", &DenyConfirmer)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

// ── validation before host resolution ────────────────────────────────────────

/// `compose_down` with `remove_volumes=true` and `force=false` should fail
/// at validation — before hitting discovery or host resolution.
#[tokio::test]
async fn compose_down_remove_volumes_without_force_is_a_validation_error() {
    let flux = flux_with_hosts(vec![compose_host("alpha")]);

    let err = flux
        .compose_down(
            "alpha",
            "myapp",
            DownArgs {
                remove_volumes: true,
                force: false,
            },
            &DenyConfirmer,
        )
        .await
        .unwrap_err();
    // The error must mention `force` — it's a validation error, not a gate denial.
    assert!(err.to_string().contains("force"), "{err}");
}

// ── happy path: project discovery + list ─────────────────────────────────────

#[tokio::test]
async fn compose_list_returns_discovered_projects() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(ProjectExec {
        project: "myapp".to_owned(),
        config_file: "/compose/myapp/docker-compose.yml".to_owned(),
    })));

    let projects = flux.compose_list("alpha").await.unwrap();
    assert!(!projects.is_empty(), "should discover at least one project");
    assert_eq!(projects[0].name, "myapp");
}

/// `compose_refresh` invalidates the discovery cache for a host.
#[tokio::test]
async fn compose_refresh_invalidates_host_cache() {
    let mut flux = flux_with_hosts(vec![compose_host("alpha")]);
    flux.compose = Arc::new(ComposeDiscovery::new(Arc::new(ProjectExec {
        project: "myapp".to_owned(),
        config_file: "/compose/myapp/docker-compose.yml".to_owned(),
    })));

    // First list — populates cache.
    let _ = flux.compose_list("alpha").await.unwrap();
    // Refresh — should not panic and should clear the cache.
    flux.compose_refresh(Some("alpha"));
    // Second list — should still work (re-discovers).
    let projects = flux.compose_list("alpha").await.unwrap();
    assert!(!projects.is_empty());
}

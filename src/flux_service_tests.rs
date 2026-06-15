//! Unit tests for FluxService — sidecar for src/flux_service.rs.
//!
//! Verifies the help contract and host_status shape without requiring a live
//! Docker daemon (help and the static fields of host_status don't shell out).

use super::*;
use crate::compose::{ComposeDiscovery, DiscoveredFrom};
use crate::elicitation_gate::{ConfirmationDenied, Confirmer};
use crate::host_config::FileHostRepository;
use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::{HostConfig, HostProtocol};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

fn stub_flux() -> FluxService {
    FluxService::new(Arc::new(FileHostRepository::default()))
}

struct StubHostRepository {
    hosts: Vec<HostConfig>,
}

impl crate::host_config::HostRepository for StubHostRepository {
    fn load_hosts(&self) -> Result<Vec<HostConfig>> {
        Ok(self.hosts.clone())
    }
}

fn flux_with_hosts(hosts: Vec<HostConfig>) -> FluxService {
    FluxService::new(Arc::new(StubHostRepository { hosts }))
}

struct MockComposeExec {
    ls_stdout: String,
    find_stdout: String,
    cat_stdout: String,
    calls: AtomicUsize,
}

impl MockComposeExec {
    fn empty() -> Self {
        Self {
            ls_stdout: String::new(),
            find_stdout: String::new(),
            cat_stdout: String::new(),
            calls: AtomicUsize::new(0),
        }
    }

    fn with_project_without_config(project: &str) -> Self {
        Self {
            ls_stdout: format!(
                r#"[{{"Name":"{project}","Status":"running(1)","ConfigFiles":""}}]"#
            ),
            find_stdout: String::new(),
            cat_stdout: String::new(),
            calls: AtomicUsize::new(0),
        }
    }

    fn with_scanned_project(project: &str, config_file: &str) -> Self {
        Self {
            ls_stdout: String::new(),
            find_stdout: format!("{config_file}\n"),
            cat_stdout: format!("name: {project}\n"),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SshExecutor for MockComposeExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        program: &str,
        args: &[&str],
    ) -> Result<CommandOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let stdout = match program {
            "docker" => self.ls_stdout.clone(),
            "find" => self.find_stdout.clone(),
            "cat" => {
                if args.last().is_some() {
                    self.cat_stdout.clone()
                } else {
                    String::new()
                }
            }
            other => return Err(anyhow::anyhow!("unexpected program {other}")),
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

#[tokio::test]
async fn test_flux_help_shape() {
    let flux = stub_flux();
    let result = flux.help(None, None).await.expect("help should succeed");

    assert_eq!(result["tool"], "flux");
    assert_eq!(
        result["actions"]["docker"],
        serde_json::json!([
            "info", "df", "images", "networks", "volumes", "pull", "build", "rmi", "prune"
        ])
    );
    assert_eq!(
        result["actions"]["container"],
        serde_json::json!([
            "list", "inspect", "logs", "stats", "top", "search", "start", "stop", "restart",
            "pause", "resume", "pull", "recreate", "exec"
        ])
    );
    assert_eq!(
        result["actions"]["host"],
        serde_json::json!([
            "status",
            "info",
            "uptime",
            "resources",
            "services",
            "network",
            "mounts",
            "ports",
            "doctor"
        ])
    );
}

#[test]
fn flatten_list_outcome_partial_success() {
    use crate::fanout::FanoutOutcome;
    let outcome: FanoutOutcome<Vec<serde_json::Value>, String> = FanoutOutcome::PartialSuccess {
        ok: vec![(
            "dookie".to_owned(),
            vec![serde_json::json!({"name": "nginx", "host": "dookie"})],
        )],
        errors: vec![("tootie".to_owned(), "connection refused".to_owned())],
    };
    let out = flatten_list_outcome(outcome, "containers");
    assert_eq!(out["count"], 1);
    assert_eq!(out["partial"], true);
    assert_eq!(out["containers"][0]["name"], "nginx");
    assert_eq!(out["errors"]["tootie"], "connection refused");
}

#[test]
fn flatten_list_outcome_all_ok_has_no_errors() {
    use crate::fanout::FanoutOutcome;
    let outcome: FanoutOutcome<Vec<serde_json::Value>, String> = FanoutOutcome::AllOk(vec![(
        "dookie".to_owned(),
        vec![serde_json::json!({"name": "nginx"})],
    )]);
    let out = flatten_list_outcome(outcome, "containers");
    assert_eq!(out["partial"], false);
    assert!(out.get("errors").is_none());
}

fn test_host(name: &str) -> HostConfig {
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

fn local_test_host(name: &str) -> HostConfig {
    let mut host = test_host(name);
    host.host = "localhost".to_owned();
    host.protocol = HostProtocol::Local;
    host
}

fn compose_host(name: &str) -> HostConfig {
    let mut host = test_host(name);
    host.compose_search_paths = vec!["/compose".to_owned()];
    host
}

#[test]
fn target_hosts_returns_named_host_from_injected_repo() {
    let flux = flux_with_hosts(vec![test_host("alpha"), test_host("beta")]);

    let hosts = flux.target_hosts(Some("beta")).unwrap();

    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].name, "beta");
}

#[test]
fn target_hosts_reports_unknown_host_from_injected_repo() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);

    let err = flux.target_hosts(Some("missing")).unwrap_err();

    assert!(err.to_string().contains("unknown host"));
}

#[tokio::test]
async fn compose_list_resolves_host_via_injected_repo() {
    let mock = Arc::new(MockComposeExec::with_scanned_project(
        "myapp",
        "/compose/myapp/docker-compose.yml",
    ));
    let mut flux = flux_with_hosts(vec![compose_host("tootie")]);
    flux.compose = Arc::new(ComposeDiscovery::new(mock.clone()));

    let projects = flux.compose_list("tootie").await.unwrap();

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "myapp");
    assert_eq!(
        projects[0].primary_config_file(),
        Some("/compose/myapp/docker-compose.yml")
    );
    assert_eq!(projects[0].discovered_from, DiscoveredFrom::Scan);
    assert!(
        mock.calls() >= 3,
        "compose list should use discovery executor"
    );
}

#[tokio::test]
async fn resolve_compose_project_reports_missing_project() {
    let mock = Arc::new(MockComposeExec::empty());
    let mut flux = flux_with_hosts(vec![compose_host("tootie")]);
    flux.compose = Arc::new(ComposeDiscovery::new(mock));
    let host = compose_host("tootie");

    let err = flux
        .resolve_compose_project(&host, "missing")
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("compose project \"missing\" not found")
    );
}

#[tokio::test]
async fn resolve_compose_project_reports_project_without_config_file() {
    let mock = Arc::new(MockComposeExec::with_project_without_config("orphan"));
    let mut flux = flux_with_hosts(vec![compose_host("tootie")]);
    flux.compose = Arc::new(ComposeDiscovery::new(mock));
    let host = compose_host("tootie");

    let err = flux
        .resolve_compose_project(&host, "orphan")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("has no config file path"));
}

#[tokio::test]
async fn compose_operations_report_missing_project_before_exec() {
    let mock = Arc::new(MockComposeExec::empty());
    let mut flux = flux_with_hosts(vec![compose_host("tootie")]);
    flux.compose = Arc::new(ComposeDiscovery::new(mock.clone()));

    let status = flux
        .compose_status("tootie", "missing", Some("web"))
        .await
        .unwrap_err();
    let up = flux.compose_up("tootie", "missing").await.unwrap_err();
    let down = flux
        .compose_down(
            "tootie",
            "missing",
            DownArgs {
                remove_volumes: false,
                force: false,
            },
            &DenyConfirmer,
        )
        .await
        .unwrap_err();
    let restart = flux
        .compose_restart("tootie", "missing", &DenyConfirmer)
        .await
        .unwrap_err();
    let recreate = flux
        .compose_recreate("tootie", "missing", &DenyConfirmer)
        .await
        .unwrap_err();
    let logs = flux
        .compose_logs(
            "tootie",
            "missing",
            ComposeLogOptions {
                lines: Some(10),
                since: Some("1h".to_owned()),
                service: Some("web".to_owned()),
            },
        )
        .await
        .unwrap_err();
    let build = flux
        .compose_build("tootie", "missing", Some("web"))
        .await
        .unwrap_err();
    let pull = flux
        .compose_pull("tootie", "missing", Some("web"))
        .await
        .unwrap_err();

    for err in [status, up, down, restart, recreate, logs, build, pull] {
        assert!(
            err.to_string()
                .contains("compose project \"missing\" not found")
        );
    }
    assert!(
        mock.calls() > 0,
        "project resolution should query discovery"
    );
}

#[tokio::test]
async fn compose_down_rejects_remove_volumes_without_force_before_discovery() {
    let mock = Arc::new(MockComposeExec::empty());
    let mut flux = flux_with_hosts(vec![compose_host("tootie")]);
    flux.compose = Arc::new(ComposeDiscovery::new(mock.clone()));

    let err = flux
        .compose_down(
            "tootie",
            "missing",
            DownArgs {
                remove_volumes: true,
                force: false,
            },
            &DenyConfirmer,
        )
        .await
        .unwrap_err();

    assert!(err.to_string().contains("force=true"));
    assert_eq!(mock.calls(), 0, "validation should run before discovery");
}

#[tokio::test]
async fn compose_refresh_invalidates_cached_discovery_for_one_host() {
    let mock = Arc::new(MockComposeExec::with_scanned_project(
        "myapp",
        "/compose/myapp/docker-compose.yml",
    ));
    let mut flux = flux_with_hosts(vec![compose_host("tootie")]);
    flux.compose = Arc::new(ComposeDiscovery::new(mock.clone()));

    let first = flux.compose_list("tootie").await.unwrap();
    let calls_after_first = mock.calls();
    let second = flux.compose_list("tootie").await.unwrap();
    assert_eq!(first, second);
    assert_eq!(
        mock.calls(),
        calls_after_first,
        "second list should hit cache"
    );

    flux.compose_refresh(Some("tootie"));
    let third = flux.compose_list("tootie").await.unwrap();

    assert_eq!(third, first);
    assert!(
        mock.calls() > calls_after_first,
        "refresh should force rediscovery"
    );
}

#[tokio::test]
async fn host_driver_unknown_host_fails_before_exec_or_docker_client() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);

    let err = flux.host_mounts("missing").await.unwrap_err();

    assert!(err.to_string().contains("unknown host"));
}

#[tokio::test]
async fn host_driver_empty_fanout_returns_empty_shapes_without_io() {
    let flux = flux_with_hosts(Vec::new());

    let status = flux.host_status(None).await.unwrap();
    let info = flux.host_info(None).await.unwrap();
    let uptime = flux.host_uptime(None).await.unwrap();
    let resources = flux.host_resources(None).await.unwrap();
    let network = flux.host_network(None).await.unwrap();

    for value in [status, info, uptime, resources, network] {
        assert_eq!(value["count"], 0);
        assert_eq!(value["partial"], false);
        assert!(value.get("errors").is_none());
    }
}

#[tokio::test]
async fn host_driver_single_host_ops_reject_unknown_host_before_io() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);

    let services = flux.host_services("missing", None, None).await.unwrap_err();
    let ports = flux
        .host_ports("missing", None, None, None)
        .await
        .unwrap_err();
    let doctor = flux
        .host_doctor("missing", vec!["docker".to_owned()])
        .await
        .unwrap_err();

    assert!(services.to_string().contains("unknown host"));
    assert!(ports.to_string().contains("unknown host"));
    assert!(doctor.to_string().contains("unknown host"));
}

#[tokio::test]
async fn docker_driver_empty_fanout_returns_empty_shapes_without_io() {
    let flux = flux_with_hosts(Vec::new());

    let info = flux.docker_info(None).await.unwrap();
    let df = flux.docker_df(None).await.unwrap();
    let images = flux.docker_images(None, false).await.unwrap();
    let networks = flux.docker_networks(None).await.unwrap();
    let volumes = flux.docker_volumes(None).await.unwrap();

    for value in [info, df, images, networks, volumes] {
        assert_eq!(value["count"], 0);
        assert_eq!(value["partial"], false);
        assert!(value.get("errors").is_none());
    }
}

#[tokio::test]
async fn docker_driver_single_host_ops_reject_unknown_host_before_io() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);
    let build_args = docker::BuildArgs {
        context: "/srv/app".to_owned(),
        tag: "app:test".to_owned(),
        dockerfile: None,
        no_cache: false,
    };

    let pull = flux
        .docker_pull("missing", "alpine:latest")
        .await
        .unwrap_err();
    let build = flux
        .docker_build("missing", build_args, &DenyConfirmer)
        .await
        .unwrap_err();
    let rmi = flux
        .docker_rmi("missing", "alpine:latest", false, &DenyConfirmer)
        .await
        .unwrap_err();
    let prune = flux
        .docker_prune("missing", docker::PruneTarget::Images, &DenyConfirmer)
        .await
        .unwrap_err();

    assert!(pull.to_string().contains("unknown host"));
    assert!(build.to_string().contains("unknown host"));
    assert!(rmi.to_string().contains("unknown host"));
    assert!(prune.to_string().contains("unknown host"));
}

#[tokio::test]
async fn container_driver_unknown_named_host_fails_before_docker_client() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);

    let err = flux
        .container_inspect(Some("missing"), "container-id", true)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("unknown host"));
}

#[tokio::test]
async fn container_driver_empty_fanout_returns_empty_shapes_without_io() {
    let flux = flux_with_hosts(Vec::new());

    let list = flux
        .container_list(None, ListFilters::default())
        .await
        .unwrap();
    let search = flux.container_search(None, "nginx").await.unwrap();
    let stats = flux.container_stats(None, None).await.unwrap();

    for value in [list, search, stats] {
        assert_eq!(value["count"], 0);
        assert_eq!(value["partial"], false);
        assert!(value.get("errors").is_none());
    }
}

#[tokio::test]
async fn container_stop_confirmation_decline_blocks_before_host_resolution() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);

    let err = flux
        .container_lifecycle(Some("missing"), "container-id", "stop", &DenyConfirmer)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("declined"));
}

#[tokio::test]
async fn container_exec_confirmation_decline_blocks_before_host_resolution() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);
    let params = container_lifecycle::ExecParams {
        container_id: "container-id".to_owned(),
        command: vec!["true".to_owned()],
        user: None,
        workdir: None,
        timeout_ms: container_lifecycle::EXEC_TIMEOUT_DEFAULT_MS,
    };

    let err = flux
        .container_exec(Some("missing"), params, &DenyConfirmer)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("declined"));
}

#[tokio::test]
async fn container_recreate_without_host_reports_not_found_before_confirmation() {
    let flux = flux_with_hosts(Vec::new());
    let params = container_lifecycle::RecreateParams { pull: true };

    let err = flux
        .container_recreate(None, "container-id", params, &DenyConfirmer)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("not found on any host"));
}

#[tokio::test]
async fn container_find_host_ops_aggregate_empty_host_errors_consistently() {
    let flux = flux_with_hosts(Vec::new());

    let inspect = flux
        .container_inspect(None, "abc", false)
        .await
        .unwrap_err();
    let top = flux.container_top(None, "abc").await.unwrap_err();
    let logs = flux
        .container_logs(
            None,
            "abc",
            LogOptions {
                lines: 10,
                since: None,
                until: None,
                grep: None,
                stream: "both".to_owned(),
            },
        )
        .await
        .unwrap_err();
    let pull = flux.container_pull(None, "abc").await.unwrap_err();

    for err in [inspect, top, logs, pull] {
        let text = err.to_string();
        assert!(text.contains("container abc not found on any host"));
    }
}

#[tokio::test]
async fn host_driver_named_fanout_ops_reject_unknown_host() {
    let flux = flux_with_hosts(vec![test_host("alpha")]);

    let status = flux.host_status(Some("missing")).await.unwrap_err();
    let info = flux.host_info(Some("missing")).await.unwrap_err();
    let uptime = flux.host_uptime(Some("missing")).await.unwrap_err();
    let resources = flux.host_resources(Some("missing")).await.unwrap_err();
    let network = flux.host_network(Some("missing")).await.unwrap_err();

    for err in [status, info, uptime, resources, network] {
        assert!(err.to_string().contains("unknown host"));
    }
}

#[tokio::test]
async fn host_driver_local_host_executes_non_docker_ops_through_local_seam() {
    let flux = flux_with_hosts(vec![local_test_host("local")]);

    let info = flux.host_info(Some("local")).await.unwrap();
    let uptime = flux.host_uptime(Some("local")).await.unwrap();
    let resources = flux.host_resources(Some("local")).await.unwrap();
    let network = flux.host_network(Some("local")).await.unwrap();
    let mounts = flux.host_mounts("local").await.unwrap();
    let doctor = flux
        .host_doctor(
            "local",
            vec![
                "resources".to_owned(),
                "network".to_owned(),
                "processes".to_owned(),
            ],
        )
        .await
        .unwrap();

    assert_eq!(info["info"][0]["host"], "local");
    assert!(
        info["info"][0]["info"]
            .as_str()
            .unwrap_or("")
            .contains("Linux")
    );
    assert_eq!(uptime["uptime"][0]["host"], "local");
    assert_eq!(resources["resources"][0]["host"], "local");
    assert_eq!(network["network"][0]["host"], "local");
    assert_eq!(mounts["host"], "local");
    assert!(
        mounts["mounts"]
            .as_str()
            .unwrap_or("")
            .contains("Filesystem")
    );
    assert_eq!(doctor["host"], "local");
    assert_eq!(doctor["checks"].as_array().unwrap().len(), 3);
}

#[test]
fn dedupe_docker_hosts_keeps_first_host_for_duplicate_daemon_id() {
    let hosts = vec![
        test_host("alias-a"),
        test_host("alias-b"),
        test_host("other"),
    ];
    let ids = vec![
        ("alias-a".to_owned(), Ok(Some("daemon-1".to_owned()))),
        ("alias-b".to_owned(), Ok(Some("daemon-1".to_owned()))),
        ("other".to_owned(), Ok(Some("daemon-2".to_owned()))),
    ];

    let deduped = dedupe_hosts_by_daemon_id(hosts, &ids);

    assert_eq!(
        deduped.into_iter().map(|h| h.name).collect::<Vec<_>>(),
        ["alias-a", "other"]
    );
}

#[test]
fn dedupe_docker_hosts_keeps_hosts_when_daemon_discovery_fails_or_has_no_id() {
    let hosts = vec![
        test_host("first"),
        test_host("unknown"),
        test_host("failed"),
    ];
    let ids = vec![
        ("first".to_owned(), Ok(Some("daemon-1".to_owned()))),
        ("unknown".to_owned(), Ok(None)),
        ("failed".to_owned(), Err("connection refused".to_owned())),
    ];

    let deduped = dedupe_hosts_by_daemon_id(hosts, &ids);

    assert_eq!(
        deduped.into_iter().map(|h| h.name).collect::<Vec<_>>(),
        ["first", "unknown", "failed"]
    );
}

//! Unit tests for FluxService — sidecar for src/flux_service.rs.
//!
//! Verifies the help contract and host_status shape without requiring a live
//! Docker daemon (help and the static fields of host_status don't shell out).

use super::*;
use crate::host_config::FileHostRepository;
use crate::synapse::{HostConfig, HostProtocol};

fn stub_flux() -> FluxService {
    FluxService::new(Arc::new(FileHostRepository::default()))
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
        exec_allowlist: Vec::new(),
    }
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

//! Unit tests for the Docker client layer.
//!
//! These exercise the cache logic, the transport-death classifier, and the
//! `MockDockerClient` trait surface — none require a live docker daemon. The
//! live-daemon path is covered by `tests/docker_client.rs`.

use super::*;
use crate::synapse::{HostConfig, HostProtocol};
use bollard::exec::StartExecResults;
use bollard::models::{ContainerSummary, ImageSummary, Network};

fn local_host(name: &str) -> HostConfig {
    HostConfig {
        name: name.to_string(),
        host: "localhost".to_string(),
        port: None,
        protocol: HostProtocol::Local,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: vec![],
        compose_search_paths: vec![],
        scout_read_roots: vec![],
        exec_allowlist: vec![],
    }
}

fn remote_host(name: &str) -> HostConfig {
    HostConfig {
        name: name.to_string(),
        host: "10.0.0.5".to_string(),
        port: None,
        protocol: HostProtocol::Ssh,
        ssh_user: Some("deploy".to_string()),
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: vec![],
        compose_search_paths: vec![],
        scout_read_roots: vec![],
        exec_allowlist: vec![],
    }
}

// --- cache classification ---

#[test]
fn is_local_matches_protocol_and_localhost() {
    assert!(DockerClientCache::is_local(&local_host("a")));

    let mut h = remote_host("b");
    assert!(!DockerClientCache::is_local(&h));

    // localhost host string forces local even if protocol is non-local.
    h.host = "localhost".to_string();
    assert!(DockerClientCache::is_local(&h));
}

#[tokio::test]
async fn cache_starts_empty() {
    let cache = DockerClientCache::new();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
}

#[tokio::test]
async fn invalidate_missing_host_is_noop() {
    let cache = DockerClientCache::new();
    // Should not panic on an unknown host.
    cache.invalidate(&remote_host("never-cached"));
    assert!(cache.is_empty());
}

#[tokio::test]
async fn clear_empties_the_cache() {
    let cache = DockerClientCache::new();
    cache.clear();
    assert!(cache.is_empty());
}

// --- transport-death classifier (BrokenPipe eviction, HIGH) ---

#[test]
fn broken_pipe_is_transport_dead() {
    let err = bollard::errors::Error::IOError {
        err: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe gone"),
    };
    assert!(is_transport_dead(&err));
}

#[test]
fn connection_refused_is_transport_dead() {
    let err = bollard::errors::Error::IOError {
        err: std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused"),
    };
    assert!(is_transport_dead(&err));
}

#[test]
fn request_timeout_is_transport_dead() {
    assert!(is_transport_dead(
        &bollard::errors::Error::RequestTimeoutError
    ));
}

#[test]
fn not_found_io_error_is_not_transport_dead() {
    let err = bollard::errors::Error::IOError {
        err: std::io::Error::new(std::io::ErrorKind::NotFound, "no socket"),
    };
    assert!(!is_transport_dead(&err));
}

#[test]
fn api_404_is_not_transport_dead() {
    let err = bollard::errors::Error::DockerResponseServerError {
        status_code: 404,
        message: "no such container".to_string(),
    };
    assert!(!is_transport_dead(&err));
}

// --- mock trait surface: every op the trait exposes is reachable ---

#[tokio::test]
async fn mock_covers_read_ops() {
    let mut mock = MockDockerClient::new();
    mock.ping = "OK".to_string();
    mock.containers = vec![ContainerSummary::default()];
    mock.images = vec![ImageSummary::default()];
    mock.networks = vec![Network::default()];

    // Drive through `&dyn DockerClient` to prove object safety.
    let client: &dyn DockerClient = &mock;

    assert_eq!(client.ping().await.unwrap(), "OK");
    assert_eq!(client.list_containers(None).await.unwrap().len(), 1);
    assert_eq!(client.list_images(None).await.unwrap().len(), 1);
    assert_eq!(client.list_networks(None).await.unwrap().len(), 1);
    // info / df / volumes return defaults without panicking.
    let _ = client.info().await.unwrap();
    let _ = client.df(None).await.unwrap();
    let _ = client.list_volumes(None).await.unwrap();
}

#[tokio::test]
async fn mock_records_lifecycle_actions() {
    let mock = MockDockerClient::new();
    let client: &dyn DockerClient = &mock;

    client
        .container_action("web", ContainerAction::Start)
        .await
        .unwrap();
    client
        .container_action("web", ContainerAction::Stop)
        .await
        .unwrap();

    assert_eq!(
        mock.recorded_actions(),
        vec![
            ("web".to_string(), ContainerAction::Start),
            ("web".to_string(), ContainerAction::Stop),
        ]
    );
}

#[tokio::test]
async fn mock_exec_three_step_flow() {
    let mock = MockDockerClient::new();
    let client: &dyn DockerClient = &mock;

    let created = client
        .create_exec("web", bollard::models::ExecConfig::default())
        .await
        .unwrap();
    assert_eq!(created.id, "mock-exec");

    let started = client.start_exec(&created.id, None).await.unwrap();
    assert!(matches!(started, StartExecResults::Detached));

    let _inspect = client.inspect_exec(&created.id).await.unwrap();
}

#[tokio::test]
async fn mock_streams_are_consumable() {
    use futures_util::StreamExt;
    let mock = MockDockerClient::new();
    let client: &dyn DockerClient = &mock;

    let logs: Vec<_> = client.logs("web", None).collect().await;
    assert!(logs.is_empty());

    let stats: Vec<_> = client.stats("web", None).collect().await;
    assert!(stats.is_empty());
}

//! Unit tests for the pure docker system/image operations (B10).
//!
//! Pure helpers (`split_image_ref`, `PruneTarget::parse`, build/context
//! validation) need no daemon; the `*_on_host` ops run against a
//! [`MockDockerClient`](crate::docker_client::MockDockerClient).

use super::*;
use crate::docker_client::MockDockerClient;
use crate::ssh::CommandOutput;
use async_trait::async_trait;
use bollard::models::{ImageDeleteResponseItem, ImageSummary, Network};
use std::sync::Mutex;

// ───────────────────────────── split_image_ref ─────────────────────────────

#[test]
fn split_image_ref_no_tag() {
    assert_eq!(split_image_ref("nginx"), ("nginx".to_owned(), None));
}

#[test]
fn split_image_ref_with_tag() {
    assert_eq!(
        split_image_ref("nginx:1.25"),
        ("nginx".to_owned(), Some("1.25".to_owned()))
    );
}

#[test]
fn split_image_ref_registry_port_is_not_a_tag() {
    // A registry host:port must NOT be mistaken for a tag.
    assert_eq!(
        split_image_ref("registry.local:5000/team/app"),
        ("registry.local:5000/team/app".to_owned(), None)
    );
}

#[test]
fn split_image_ref_registry_port_with_tag() {
    assert_eq!(
        split_image_ref("registry.local:5000/team/app:v2"),
        (
            "registry.local:5000/team/app".to_owned(),
            Some("v2".to_owned())
        )
    );
}

#[test]
fn split_image_ref_trailing_colon_is_not_a_tag() {
    assert_eq!(split_image_ref("repo:"), ("repo:".to_owned(), None));
}

// ───────────────────────────── PruneTarget ─────────────────────────────

#[test]
fn prune_target_parses_all_variants_case_insensitive() {
    assert_eq!(
        PruneTarget::parse("containers").unwrap(),
        PruneTarget::Containers
    );
    assert_eq!(PruneTarget::parse("IMAGES").unwrap(), PruneTarget::Images);
    assert_eq!(
        PruneTarget::parse(" Volumes ").unwrap(),
        PruneTarget::Volumes
    );
    assert_eq!(
        PruneTarget::parse("networks").unwrap(),
        PruneTarget::Networks
    );
    assert_eq!(PruneTarget::parse("all").unwrap(), PruneTarget::All);
}

#[test]
fn prune_target_buildcache_aliases() {
    for s in ["buildcache", "build", "build_cache"] {
        assert_eq!(
            PruneTarget::parse(s).unwrap(),
            PruneTarget::BuildCache,
            "{s}"
        );
    }
}

#[test]
fn prune_target_unknown_is_error() {
    assert!(PruneTarget::parse("everything").is_err());
    assert!(PruneTarget::parse("").is_err());
}

#[test]
fn prune_target_all_confirmation_is_explicit() {
    // Security review: the `all` confirmation must spell out the scope, not be a
    // generic "are you sure".
    let details = PruneTarget::All.confirmation_details();
    assert!(details.contains("ALL"));
    assert!(
        PruneTarget::Volumes
            .confirmation_details()
            .contains("DATA LOSS")
    );
}

// ───────────────────────────── build validation ─────────────────────────────

#[test]
fn validate_build_context_requires_absolute() {
    assert!(validate_build_context("relative/path").is_err());
    assert!(validate_build_context("/home/user/project").is_ok());
}

#[test]
fn validate_build_context_rejects_traversal_and_socket_dir() {
    assert!(validate_build_context("/home/../etc").is_err());
    assert!(validate_build_context("/var/run").is_err());
    assert!(validate_build_context("/var/run/docker").is_err());
}

#[test]
fn validate_build_context_rejects_shell_expansion() {
    // Guards against a future loosening of validate_safe_path silently weakening this.
    assert!(validate_build_context("/home/$USER/proj").is_err());
    assert!(validate_build_context("/home/~bob/proj").is_err());
}

#[test]
fn validate_dockerfile_rules() {
    assert!(validate_dockerfile("Dockerfile").is_ok());
    assert!(validate_dockerfile("docker/Dockerfile.prod").is_ok());
    assert!(validate_dockerfile("").is_err());
    assert!(validate_dockerfile("/abs/Dockerfile").is_err());
    assert!(validate_dockerfile("../escape/Dockerfile").is_err());
    assert!(validate_dockerfile("$HOME/Dockerfile").is_err());
}

#[test]
fn build_args_happy_path() {
    let args = build_args("/srv/app", "myapp:latest", Some("Dockerfile"), true).unwrap();
    assert_eq!(args.context, "/srv/app");
    assert_eq!(args.tag, "myapp:latest");
    assert_eq!(args.dockerfile.as_deref(), Some("Dockerfile"));
    assert!(args.no_cache);
}

#[test]
fn build_args_requires_tag() {
    assert!(build_args("/srv/app", "", None, false).is_err());
}

#[test]
fn build_args_rejects_bad_context() {
    assert!(build_args("relative", "tag:1", None, false).is_err());
}

// ───────────────────────────── op tests (mock) ─────────────────────────────

#[tokio::test]
async fn info_on_host_is_host_tagged() {
    let mock = MockDockerClient::default();
    let v = info_on_host(&mock, "dookie").await.unwrap();
    assert_eq!(v["host"], "dookie");
    assert!(v.get("info").is_some());
}

#[tokio::test]
async fn daemon_id_reads_typed_system_info_id() {
    let mock = MockDockerClient {
        info: bollard::models::SystemInfo {
            id: Some("daemon-123".to_owned()),
            ..Default::default()
        },
        ..Default::default()
    };

    assert_eq!(
        daemon_id(&mock).await.unwrap().as_deref(),
        Some("daemon-123")
    );
}

#[tokio::test]
async fn daemon_id_preserves_missing_id_as_none() {
    let mock = MockDockerClient {
        info: bollard::models::SystemInfo {
            id: None,
            ..Default::default()
        },
        ..Default::default()
    };

    assert_eq!(daemon_id(&mock).await.unwrap(), None);
}

#[tokio::test]
async fn df_on_host_is_host_tagged() {
    let mock = MockDockerClient::default();
    let v = df_on_host(&mock, "tootie").await.unwrap();
    assert_eq!(v["host"], "tootie");
    assert!(v.get("df").is_some());
}

#[tokio::test]
async fn images_on_host_tags_each_image() {
    let mock = MockDockerClient {
        images: vec![
            ImageSummary {
                id: "sha256:aaa".to_owned(),
                repo_tags: vec!["nginx:1.25".to_owned()],
                ..Default::default()
            },
            ImageSummary {
                id: "sha256:bbb".to_owned(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let out = images_on_host(&mock, "dookie", false).await.unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["host"], "dookie");
    assert_eq!(out[0]["id"], "sha256:aaa");
}

#[tokio::test]
async fn images_on_host_dangling_filter_does_not_error() {
    let mock = MockDockerClient {
        images: vec![ImageSummary {
            id: "x".to_owned(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(images_on_host(&mock, "h", true).await.is_ok());
}

#[tokio::test]
async fn networks_on_host_tags_each() {
    let mock = MockDockerClient {
        networks: vec![Network {
            name: Some("bridge".to_owned()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let out = networks_on_host(&mock, "dookie").await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["host"], "dookie");
}

#[tokio::test]
async fn volumes_on_host_tags_each() {
    let resp = bollard::models::VolumeListResponse {
        volumes: Some(vec![bollard::models::Volume {
            name: "data".to_owned(),
            ..Default::default()
        }]),
        ..Default::default()
    };
    let mock = MockDockerClient {
        volumes: resp,
        ..Default::default()
    };
    let out = volumes_on_host(&mock, "dookie").await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["host"], "dookie");
}

#[tokio::test]
async fn rmi_on_host_returns_removed() {
    let mock = MockDockerClient {
        removed_images: vec![ImageDeleteResponseItem {
            untagged: Some("nginx:1.25".to_owned()),
            deleted: None,
        }],
        ..Default::default()
    };
    let v = rmi_on_host(&mock, "dookie", "nginx:1.25", true)
        .await
        .unwrap();
    assert_eq!(v["host"], "dookie");
    assert_eq!(v["image"], "nginx:1.25");
    assert!(v.get("removed").is_some());
}

#[tokio::test]
async fn prune_on_host_single_target() {
    let mock = MockDockerClient::default();
    let v = prune_on_host(&mock, "dookie", PruneTarget::Images)
        .await
        .unwrap();
    assert_eq!(v["host"], "dookie");
    assert_eq!(v["target"], "images");
    assert!(v["pruned"]["images"].is_object() || v["pruned"].get("images").is_some());
}

#[tokio::test]
async fn prune_on_host_all_aggregates_every_target() {
    let mock = MockDockerClient::default();
    let v = prune_on_host(&mock, "dookie", PruneTarget::All)
        .await
        .unwrap();
    assert_eq!(v["target"], "all");
    let pruned = v["pruned"].as_object().unwrap();
    for key in ["containers", "images", "volumes", "networks", "buildcache"] {
        assert!(pruned.contains_key(key), "missing prune result for {key}");
    }
}

struct RecordingExec {
    calls: Mutex<Vec<(String, Vec<String>)>>,
    output: CommandOutput,
}

impl Default for RecordingExec {
    fn default() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            output: CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
            },
        }
    }
}

#[async_trait]
impl HostExec for RecordingExec {
    async fn run(&self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput> {
        self.calls.lock().expect("call log").push((
            program.to_owned(),
            args.iter().map(|arg| (*arg).to_owned()).collect(),
        ));
        Ok(self.output.clone())
    }
}

#[tokio::test]
async fn build_on_host_runs_docker_build_through_exec_seam() {
    let exec = RecordingExec {
        output: CommandOutput {
            stdout: "built".to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
        },
        ..Default::default()
    };
    let args = build_args("/srv/app", "app:test", Some("docker/Dockerfile"), true).unwrap();

    let value = build_on_host(&exec, "remote", &args).await.unwrap();

    assert_eq!(value["host"], "remote");
    assert_eq!(value["succeeded"], true);
    assert_eq!(value["stdout"], "built");
    assert_eq!(
        exec.calls.lock().expect("call log").as_slice(),
        [(
            "docker".to_owned(),
            vec![
                "build".to_owned(),
                "-t".to_owned(),
                "app:test".to_owned(),
                "--no-cache".to_owned(),
                "-f".to_owned(),
                "/srv/app/docker/Dockerfile".to_owned(),
                "/srv/app".to_owned(),
            ],
        )]
    );
}

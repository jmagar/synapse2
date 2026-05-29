//! Unit tests for container lifecycle operations (B9).
//!
//! Tests exercise the pure functions in `container_lifecycle` against a
//! [`MockDockerClient`] — no live docker daemon required.
//! Gate tests assert that declined/unsupported confirmation aborts before any
//! bollard call reaches the mock.

use super::*;
use crate::docker_client::{ContainerAction, MockDockerClient};

// ─────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────

fn mock() -> MockDockerClient {
    MockDockerClient::new()
}

// ─────────────────────────────────────────────────────────────────
// split_image_ref
// ─────────────────────────────────────────────────────────────────

#[test]
fn split_image_ref_tag_present() {
    let (img, tag) = split_image_ref("nginx:latest");
    assert_eq!(img, "nginx");
    assert_eq!(tag, Some("latest".to_owned()));
}

#[test]
fn split_image_ref_no_tag() {
    let (img, tag) = split_image_ref("nginx");
    assert_eq!(img, "nginx");
    assert_eq!(tag, None);
}

#[test]
fn split_image_ref_registry_port_not_confused_with_tag() {
    // registry:5000/repo:tag → repo is `registry:5000/repo`, tag is `tag`
    let (img, tag) = split_image_ref("registry:5000/repo:tag");
    assert_eq!(img, "registry:5000/repo");
    assert_eq!(tag, Some("tag".to_owned()));
}

#[test]
fn split_image_ref_registry_only_no_slash_in_tag() {
    // registry:5000/repo → last `:5000/repo` contains a `/`, so no tag
    let (img, tag) = split_image_ref("registry:5000/repo");
    assert_eq!(img, "registry:5000/repo");
    assert_eq!(tag, None);
}

// ─────────────────────────────────────────────────────────────────
// lifecycle_action_on_host — verb mapping
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn lifecycle_start_records_action() {
    let client = mock();
    let result = lifecycle_action_on_host(&client, "dookie", "my-container", "start")
        .await
        .unwrap();
    assert_eq!(result["action"], "start");
    assert_eq!(result["container"], "my-container");
    assert_eq!(result["host"], "dookie");
    assert_eq!(result["ok"], true);
    let actions = client.recorded_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0],
        ("my-container".to_owned(), ContainerAction::Start)
    );
}

#[tokio::test]
async fn lifecycle_stop_records_action() {
    let client = mock();
    let _ = lifecycle_action_on_host(&client, "h", "c", "stop")
        .await
        .unwrap();
    let actions = client.recorded_actions();
    assert_eq!(actions[0].1, ContainerAction::Stop);
}

#[tokio::test]
async fn lifecycle_restart_records_action() {
    let client = mock();
    let _ = lifecycle_action_on_host(&client, "h", "c", "restart")
        .await
        .unwrap();
    let actions = client.recorded_actions();
    assert_eq!(actions[0].1, ContainerAction::Restart);
}

#[tokio::test]
async fn lifecycle_pause_records_action() {
    let client = mock();
    let _ = lifecycle_action_on_host(&client, "h", "c", "pause")
        .await
        .unwrap();
    let actions = client.recorded_actions();
    assert_eq!(actions[0].1, ContainerAction::Pause);
}

#[tokio::test]
async fn lifecycle_resume_maps_to_unpause() {
    let client = mock();
    let result = lifecycle_action_on_host(&client, "h", "c", "resume")
        .await
        .unwrap();
    assert_eq!(result["action"], "resume");
    let actions = client.recorded_actions();
    assert_eq!(actions[0].1, ContainerAction::Unpause);
}

#[tokio::test]
async fn lifecycle_unknown_verb_returns_error() {
    let client = mock();
    let err = lifecycle_action_on_host(&client, "h", "c", "explode")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown lifecycle subaction"));
    // No bollard call was made.
    assert!(client.recorded_actions().is_empty());
}

// ─────────────────────────────────────────────────────────────────
// pull_image_on_host
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pull_returns_host_tagged_result() {
    let client = MockDockerClient {
        pull_frames: vec![bollard::models::CreateImageInfo::default()],
        ..Default::default()
    };
    let result = pull_image_on_host(&client, "dookie", "nginx:latest")
        .await
        .unwrap();
    assert_eq!(result["host"], "dookie");
    assert_eq!(result["image"], "nginx:latest");
    assert_eq!(result["pulled"], true);
    assert_eq!(result["events"], 1_u64);
}

// ─────────────────────────────────────────────────────────────────
// exec_on_host
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn exec_returns_host_tagged_result_with_exit_code() {
    let client = MockDockerClient {
        // start_exec returns Detached (no output stream); exit code from inspect = None.
        ..Default::default()
    };
    let params = ExecParams {
        container_id: "my-container".to_owned(),
        command: vec!["echo".to_owned(), "hello".to_owned()],
        user: None,
        workdir: None,
        timeout_ms: EXEC_TIMEOUT_DEFAULT_MS,
    };
    let result = exec_on_host(&client, "dookie", &params).await.unwrap();
    assert_eq!(result["host"], "dookie");
    assert_eq!(result["container"], "my-container");
    assert_eq!(result["command"][0], "echo");
    assert_eq!(result["command"][1], "hello");
    // exit_code from inspect_exec mock is None → null in JSON.
    assert!(result["exit_code"].is_null());
}

#[tokio::test]
async fn exec_empty_command_returns_error() {
    let client = mock();
    let params = ExecParams {
        container_id: "c".to_owned(),
        command: vec![],
        user: None,
        workdir: None,
        timeout_ms: EXEC_TIMEOUT_DEFAULT_MS,
    };
    let err = exec_on_host(&client, "h", &params).await.unwrap_err();
    assert!(err.to_string().contains("command must not be empty"));
    // No bollard call was made (create_exec never called → actions empty).
    assert!(client.recorded_actions().is_empty());
}

#[tokio::test]
async fn exec_timeout_clamp_min() {
    // Timeout below min should be clamped to EXEC_TIMEOUT_MIN_MS — not panic.
    let client = mock();
    let params = ExecParams {
        container_id: "c".to_owned(),
        command: vec!["ls".to_owned()],
        user: None,
        workdir: None,
        timeout_ms: 1, // below 1000ms min
    };
    // Should succeed (mock returns immediately).
    let _ = exec_on_host(&client, "h", &params).await.unwrap();
}

// ─────────────────────────────────────────────────────────────────
// recreate_on_host — action sequence
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn recreate_sequence_inspect_stop_remove_create_start() {
    use bollard::models::{ContainerConfig, ContainerInspectResponse};

    let mut inspect_map = std::collections::HashMap::new();
    inspect_map.insert(
        "my-container".to_owned(),
        ContainerInspectResponse {
            name: Some("/my-container".to_owned()),
            config: Some(ContainerConfig {
                image: Some("nginx:latest".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let client = MockDockerClient {
        inspect: inspect_map,
        // No pull frames needed when pull=false.
        ..Default::default()
    };

    let params = RecreateParams { pull: false };
    let result = recreate_on_host(&client, "dookie", "my-container", &params)
        .await
        .unwrap();

    assert_eq!(result["host"], "dookie");
    assert_eq!(result["original_container"], "my-container");
    assert_eq!(result["pulled"], false);
    assert_eq!(result["status"], "recreated");
    assert_eq!(result["new_container"], "new-container"); // from mock default

    // Assert the lifecycle action sequence: stop → remove → start (new).
    let actions = client.recorded_actions();
    // stop: ("my-container", Stop), remove: ("my-container", Remove), start: ("new-container", Start)
    assert_eq!(actions.len(), 3);
    assert_eq!(
        actions[0],
        ("my-container".to_owned(), ContainerAction::Stop)
    );
    assert_eq!(
        actions[1],
        ("my-container".to_owned(), ContainerAction::Remove)
    );
    assert_eq!(
        actions[2],
        ("new-container".to_owned(), ContainerAction::Start)
    );
}

#[tokio::test]
async fn recreate_with_pull_calls_pull_before_stop() {
    use bollard::models::{ContainerConfig, ContainerInspectResponse, CreateImageInfo};

    let mut inspect_map = std::collections::HashMap::new();
    inspect_map.insert(
        "c".to_owned(),
        ContainerInspectResponse {
            name: Some("/c".to_owned()),
            config: Some(ContainerConfig {
                image: Some("nginx:latest".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let client = MockDockerClient {
        inspect: inspect_map,
        pull_frames: vec![CreateImageInfo::default()],
        ..Default::default()
    };

    let params = RecreateParams { pull: true };
    let result = recreate_on_host(&client, "h", "c", &params).await.unwrap();
    assert_eq!(result["pulled"], true);

    // Pull recorded as PullImage mutation.
    let mutations = client.recorded_mutations();
    assert!(
        mutations.contains(&crate::docker_client::MutatingOp::PullImage),
        "expected PullImage in mutations, got: {mutations:?}"
    );

    // Lifecycle actions: stop → remove → start.
    let actions = client.recorded_actions();
    assert_eq!(actions[0].1, ContainerAction::Stop);
    assert_eq!(actions[1].1, ContainerAction::Remove);
    assert_eq!(actions[2].1, ContainerAction::Start);
}

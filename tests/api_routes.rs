//! Route-level smoke tests for public status and optional REST compatibility.

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use synapse2::{server, testing::loopback_state};
use tower::ServiceExt;

async fn request_json(
    app: axum::Router,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    request_json_with_auth(app, method, path, body, None).await
}

async fn request_json_with_auth(
    app: axum::Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    bearer_token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer_token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        builder.body(Body::from(body.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    (status, value)
}

#[tokio::test]
async fn rest_help_returns_synapse_actions() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/synapse2",
        Some(json!({"action": "help", "params": {}})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tool"], "flux");
    assert!(body["actions"]["docker"].is_array());
}

#[tokio::test]
async fn rest_scout_nodes_works_without_auth_on_loopback_state() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/synapse2",
        Some(json!({"action": "scout.nodes", "params": {}})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["hosts"].is_array());
}

#[tokio::test]
async fn mounted_rest_read_scoped_dotted_actions_pass_scope_checks() {
    let app = server::router(synapse2::testing::bearer_state("read-token"));

    let (status, body) = request_json_with_auth(
        app.clone(),
        Method::POST,
        "/v1/synapse2",
        Some(json!({"action": "scout.nodes", "params": {}})),
        Some("read-token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["hosts"].is_array());

    let (status, body) = request_json_with_auth(
        app,
        Method::POST,
        "/v1/synapse2",
        Some(json!({"action": "flux.docker.info", "params": {}})),
        Some("read-token"),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "flux.docker.info should pass REST scope checks; body={body}"
    );
}

#[tokio::test]
async fn mounted_rest_dotted_write_actions_require_write_scope() {
    let app = server::router(synapse2::testing::bearer_state("read-token"));
    let (status, body) = request_json_with_auth(
        app,
        Method::POST,
        "/v1/synapse2",
        Some(json!({
            "action": "flux.docker.prune",
            "params": {
                "host": "local",
                "prune_target": "images",
                "force": true
            }
        })),
        Some("read-token"),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap().contains("synapse:write"));
}

#[tokio::test]
async fn rest_unknown_action_is_bad_request() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/synapse2",
        Some(json!({"action": "missing", "params": {}})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("unknown synapse2 action"));
}

#[tokio::test]
async fn status_returns_only_local_redacted_metadata() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/status", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["server"], "synapse2");
    assert_eq!(body["transport"], "http");
    assert!(body.get("version").is_some());
    assert!(body.get("api_key").is_none(), "{body}");
}

#[tokio::test]
async fn oversized_body_returns_413() {
    let app = server::router(loopback_state());
    let oversized_body = vec![b'x'; 65_537];
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/synapse2")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(oversized_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

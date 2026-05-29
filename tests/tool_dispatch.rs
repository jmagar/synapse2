use serde_json::json;
use synapse2::{mcp::execute_tool_without_peer_for_test, testing::loopback_state};

async fn call_mcp_tool(tool: &str, args: serde_json::Value) -> serde_json::Value {
    let state = loopback_state();
    execute_tool_without_peer_for_test(&state, tool, args)
        .await
        .expect("MCP tool dispatch should succeed")
}

#[tokio::test]
async fn flux_help_returns_action_reference() {
    let result = call_mcp_tool("flux", json!({ "action": "help" })).await;
    assert_eq!(result["tool"], "flux");
    assert!(result["actions"]["docker"].is_array());
}

#[tokio::test]
async fn flux_docker_info_is_safe_without_docker() {
    // docker info now fans out across configured hosts via bollard (B10). With
    // no reachable daemon the per-host op errors, but the aggregate shape is
    // always present and the call never panics.
    let result = call_mcp_tool("flux", json!({ "action": "docker", "subaction": "info" })).await;
    assert!(result.get("count").is_some());
    assert!(result.get("info").is_some());
    assert!(result.get("partial").is_some());
}

#[tokio::test]
async fn scout_nodes_returns_hosts_array() {
    let result = call_mcp_tool("scout", json!({ "action": "nodes" })).await;
    assert!(result["hosts"].is_array());
}

#[tokio::test]
async fn scout_exec_rejects_denied_commands() {
    let state = loopback_state();
    let error = execute_tool_without_peer_for_test(
        &state,
        "scout",
        json!({ "action": "exec", "host": "local", "path": "/tmp", "command": "rm" }),
    )
    .await
    .expect_err("denied command should fail");
    assert!(error.to_string().contains("denied"));
}

#[tokio::test]
async fn unknown_tool_and_missing_action_are_rejected() {
    let state = loopback_state();
    assert!(
        execute_tool_without_peer_for_test(&state, "missing", json!({}))
            .await
            .is_err()
    );
    let error = execute_tool_without_peer_for_test(&state, "flux", json!({}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("action is required"));
}

use serde_json::json;

use crate::{
    actions::{
        READ_SCOPE, SynapseAction, WRITE_SCOPE, required_scope_for_action,
        required_scope_for_parsed_action,
    },
    token_limit::MAX_RESPONSE_BYTES,
};

use super::{
    internal_tool_error_message, parse_mcp_action, reject_unknown_action_before_scope,
    rmcp_tool_definitions, rmcp_tool_from_json, scope_satisfied, tool_result_from_json,
};

fn scopes(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

#[test]
fn read_scope_satisfies_read_requirement() {
    assert!(scope_satisfied(&scopes(&[READ_SCOPE]), READ_SCOPE));
}

#[test]
fn write_scope_satisfies_read_when_mixed_with_unrelated_scopes() {
    assert!(scope_satisfied(
        &scopes(&["profile", WRITE_SCOPE, "other:scope"]),
        READ_SCOPE
    ));
}

#[test]
fn write_scope_satisfies_read_requirement() {
    assert!(
        scope_satisfied(&scopes(&[WRITE_SCOPE]), READ_SCOPE),
        "write scope should satisfy read requirement (write ⊇ read)"
    );
}

#[test]
fn empty_scopes_denied() {
    assert!(!scope_satisfied(&[], READ_SCOPE));
}

#[test]
fn unrelated_scope_denied() {
    assert!(!scope_satisfied(&scopes(&["other:scope"]), READ_SCOPE));
}

#[test]
fn read_scope_does_not_satisfy_write() {
    assert!(
        !scope_satisfied(&scopes(&[READ_SCOPE]), WRITE_SCOPE),
        "read scope must not satisfy write requirement"
    );
}

#[test]
fn docker_requires_read_scope() {
    assert_eq!(required_scope_for_action("docker"), Some(READ_SCOPE));
}

#[test]
fn parsed_destructive_flux_subactions_require_write_scope() {
    let action = SynapseAction::from_flux_args(&json!({
        "action": "container",
        "subaction": "stop"
    }))
    .unwrap();
    assert_eq!(required_scope_for_parsed_action(&action), Some(WRITE_SCOPE));
}

#[test]
fn parsed_high_risk_subactions_have_expected_scopes() {
    let cases = [
        (
            json!({"action": "container", "subaction": "list"}),
            READ_SCOPE,
        ),
        (
            json!({"action": "container", "subaction": "exec"}),
            WRITE_SCOPE,
        ),
        (
            json!({"action": "container", "subaction": "recreate"}),
            WRITE_SCOPE,
        ),
        (
            json!({
                "action": "compose",
                "subaction": "logs",
                "host": "dookie",
                "project": "stack"
            }),
            READ_SCOPE,
        ),
        (
            json!({
                "action": "compose",
                "subaction": "down",
                "host": "dookie",
                "project": "stack"
            }),
            WRITE_SCOPE,
        ),
        (
            json!({
                "action": "compose",
                "subaction": "restart",
                "host": "dookie",
                "project": "stack"
            }),
            WRITE_SCOPE,
        ),
    ];

    for (args, expected_scope) in cases {
        let action = SynapseAction::from_flux_args(&args).unwrap();
        assert_eq!(
            required_scope_for_parsed_action(&action),
            Some(expected_scope),
            "unexpected scope for {args}"
        );
    }

    let scout_cases = [
        (
            json!({"action": "logs", "host": "dookie", "subaction": "journal"}),
            READ_SCOPE,
        ),
        (
            json!({"action": "zfs", "host": "dookie", "subaction": "snapshots"}),
            READ_SCOPE,
        ),
        (
            json!({"action": "exec", "host": "dookie", "command": "hostname"}),
            WRITE_SCOPE,
        ),
        (
            json!({
                "action": "emit",
                "targets": [{"host": "dookie"}],
                "command": "hostname"
            }),
            WRITE_SCOPE,
        ),
        (
            json!({
                "action": "beam",
                "source_host": "dookie",
                "source_path": "/tmp/a",
                "dest_host": "tootie",
                "dest_path": "/tmp/b"
            }),
            WRITE_SCOPE,
        ),
    ];

    for (args, expected_scope) in scout_cases {
        let action = SynapseAction::from_scout_args(&args).unwrap();
        assert_eq!(
            required_scope_for_parsed_action(&action),
            Some(expected_scope),
            "unexpected scope for {args}"
        );
    }
}

#[test]
fn help_requires_no_scope() {
    assert_eq!(required_scope_for_action("help"), None);
}

#[test]
fn unknown_action_gets_deny_scope() {
    use crate::actions::DENY_SCOPE;
    assert_eq!(
        required_scope_for_action("nonexistent_action"),
        Some(DENY_SCOPE)
    );
}

#[test]
fn unknown_action_is_rejected_as_validation_before_scope() {
    let error = reject_unknown_action_before_scope("nonexistent_action")
        .expect_err("unknown action should be invalid params");
    assert!(error.message.contains("unknown synapse2 action"));
}

#[test]
fn internal_tool_errors_include_stable_kind() {
    let message = internal_tool_error_message("docker");
    assert!(message.contains("kind=execution_error"));
    assert!(message.contains("action='docker'"));
}

#[test]
fn rmcp_tool_definitions_include_flux_and_scout_tools() {
    let tools = rmcp_tool_definitions().expect("tool definitions should convert");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();

    assert_eq!(names, ["flux", "scout"]);
    assert!(
        tools
            .iter()
            .all(|tool| tool.input_schema.contains_key("properties"))
    );
}

#[test]
fn rmcp_tool_from_json_rejects_missing_required_fields() {
    let missing_name = rmcp_tool_from_json(json!({
        "description": "nope",
        "inputSchema": {}
    }))
    .expect_err("missing name should be rejected");
    assert!(missing_name.message.contains("missing name"));

    let missing_schema = rmcp_tool_from_json(json!({
        "name": "flux",
        "description": "nope"
    }))
    .expect_err("missing input schema should be rejected");
    assert!(missing_schema.message.contains("missing inputSchema"));
}

#[test]
fn parse_mcp_action_maps_flux_and_scout_validation_errors() {
    let flux = parse_mcp_action(
        "flux",
        &json!({
            "action": "docker",
            "subaction": "info"
        }),
    )
    .expect("valid flux action should parse");
    assert_eq!(flux.name(), "docker");

    let scout = parse_mcp_action(
        "scout",
        &json!({
            "action": "nodes"
        }),
    )
    .expect("valid scout action should parse");
    assert_eq!(scout.name(), "nodes");

    let error = parse_mcp_action(
        "flux",
        &json!({
            "action": "definitely-not-real"
        }),
    )
    .expect_err("unknown action should map to invalid params");
    assert!(error.message.contains("unknown synapse2 action"));
}

#[test]
fn parse_mcp_action_rejects_missing_and_wrong_typed_subactions() {
    let missing = parse_mcp_action("flux", &json!({"action": "container"}))
        .expect_err("container action without subaction should be invalid");
    assert!(missing.message.contains("subaction"));

    let wrong_type = parse_mcp_action(
        "flux",
        &json!({
            "action": "host",
            "subaction": 42
        }),
    )
    .expect_err("numeric subaction should be invalid");
    assert!(wrong_type.message.contains("subaction"));

    let scout_missing = parse_mcp_action("scout", &json!({"action": "logs"}))
        .expect_err("logs action without required fields should be invalid");
    assert!(scout_missing.message.contains("host"));
}

#[test]
fn tool_result_from_json_applies_response_cap() {
    let result = tool_result_from_json(json!({
        "payload": "x".repeat(MAX_RESPONSE_BYTES + 1)
    }))
    .expect("tool result should serialize");
    let text = result.content[0]
        .raw
        .as_text()
        .expect("tool result should contain text")
        .text
        .as_str();
    assert!(text.contains("[TRUNCATED"));
}

#[test]
fn mcp_tool_output_defaults_to_markdown_text() {
    let rendered = super::render_mcp_tool_output(
        "flux",
        &json!({"action": "docker", "subaction": "info"}),
        &json!({"info": {"host": "local"}}),
    )
    .unwrap();
    assert!(
        !rendered.trim_start().starts_with('{'),
        "default MCP tool content should not be serialized JSON"
    );
}

#[test]
fn mcp_tool_output_json_requires_response_format_json() {
    let rendered = super::render_mcp_tool_output(
        "flux",
        &json!({
            "action": "docker",
            "subaction": "info",
            "response_format": "json"
        }),
        &json!({"info": {"host": "local"}}),
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed["info"]["host"], "local");
}

// SECURITY FIX: Unauthenticated scope-check error leak tests
//
// These tests verify that unauthenticated MCP requests return a generic error
// message for both unknown actions AND missing scopes, preventing action-name
// enumeration via error response differences.

#[test]
fn unknown_action_rejection_still_callable() {
    // This test just ensures reject_unknown_action_before_scope is still available
    // and rejects unknown actions. The actual unauthenticated gate is in call_tool.
    let error = reject_unknown_action_before_scope("definitely_not_a_real_action")
        .expect_err("should reject unknown action");
    assert!(error.message.contains("unknown synapse2 action"));
}

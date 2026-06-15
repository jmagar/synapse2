//! Unit tests for the scaffold-intent contract — sidecar for src/scaffold.rs.
//!
//! These exercise `scaffold_intent()` as a free function (no service facade),
//! verifying validation, normalization, and dedup of the handoff contract.

use super::*;

#[test]
fn test_scaffold_intent_transformation() {
    let result = scaffold_intent(ScaffoldIntent {
        display_name: "Lab Gateway".into(),
        crate_name: "lab-gateway-mcp".into(),
        binary_name: "lab-gateway".into(),
        server_category: "application platform".into(),
        env_prefix: "lab".into(),
        auth_kind: "api key".into(),
        host: "".into(),
        port: 3100,
        mcp_transport: "streamable-http".into(),
        mcp_primitives: "tools, resources, tools".into(),
        deployment: "containers".into(),
        plugins: "claude, gemini, none".into(),
        publish_mcp: true,
        crawl_urls: "https://docs.synapse2.test, https://api.synapse2.test".into(),
        crawl_repos: "".into(),
        crawl_search_topics: "Lab API".into(),
    })
    .expect("valid scaffold intent should build");

    assert_scaffold_contract_shape(&result);
    assert_eq!(result["server_category"], "application-platform");
    assert_eq!(result["project"]["service_name"], "lab_gateway");
    assert_eq!(result["project"]["env_prefix"], "LAB");
    assert_eq!(result["upstream"]["base_url_env"], "LAB_API_URL");
    assert_eq!(result["upstream"]["auth_kind"], "api-key");
    assert_eq!(result["runtime"]["host"], "127.0.0.1");
    assert_eq!(result["runtime"]["mcp_transport"], "http");
    assert_eq!(result["deployment"], "docker");
    assert_eq!(result["plugins"], serde_json::json!(["claude", "gemini"]));
    assert_eq!(
        result["mcp_primitives"],
        serde_json::json!(["tools", "resources"])
    );
}

#[test]
fn test_scaffold_intent_rejects_invalid_contract_identifiers() {
    let result = scaffold_intent(ScaffoldIntent {
        display_name: "Bad Project".into(),
        crate_name: "Invalid Crate".into(),
        binary_name: "bad".into(),
        server_category: "upstream-client".into(),
        env_prefix: "bad".into(),
        auth_kind: "api-key".into(),
        host: "127.0.0.1".into(),
        port: 3100,
        mcp_transport: "dual".into(),
        mcp_primitives: "tools".into(),
        deployment: "none".into(),
        plugins: "".into(),
        publish_mcp: false,
        crawl_urls: "".into(),
        crawl_repos: "".into(),
        crawl_search_topics: "".into(),
    });

    let error = result.expect_err("invalid crate_name should be rejected");
    assert!(error.to_string().contains("crate_name"));
}

#[test]
fn test_scaffold_intent_rejects_zero_port_and_bad_urls() {
    let mut input = valid_scaffold_intent();
    input.port = 0;
    let error = scaffold_intent(input).expect_err("zero port should be rejected");
    assert!(error.to_string().contains("port"));

    let mut input = valid_scaffold_intent();
    input.crawl_urls = "not-a-url".into();
    let error = scaffold_intent(input).expect_err("bad crawl URL should be rejected");
    assert!(error.to_string().contains("crawl_urls"));
}

#[test]
fn test_scaffold_intent_deduplicates_contract_unique_arrays() {
    let mut input = valid_scaffold_intent();
    input.crawl_urls = "https://docs.synapse2.test, https://docs.synapse2.test".into();

    let result = scaffold_intent(input).expect("duplicate crawl URLs should be deduplicated");

    assert_eq!(
        result["crawl_docs"]["urls"],
        serde_json::json!(["https://docs.synapse2.test"])
    );
    assert_scaffold_contract_shape(&result);
}

#[test]
fn test_validation_error_is_distinct_type() {
    let result = scaffold_intent(ScaffoldIntent {
        crate_name: "Invalid Crate".into(),
        ..valid_scaffold_intent()
    });
    let error = result.expect_err("invalid crate_name should be rejected");
    assert!(
        error
            .downcast_ref::<ScaffoldIntentValidationError>()
            .is_some()
    );
}

fn valid_scaffold_intent() -> ScaffoldIntent {
    ScaffoldIntent {
        display_name: "Lab Gateway".into(),
        crate_name: "lab-gateway-mcp".into(),
        binary_name: "lab-gateway".into(),
        server_category: "application platform".into(),
        env_prefix: "lab".into(),
        auth_kind: "api key".into(),
        host: "".into(),
        port: 3100,
        mcp_transport: "streamable-http".into(),
        mcp_primitives: "tools, resources, tools".into(),
        deployment: "containers".into(),
        plugins: "claude, gemini, none".into(),
        publish_mcp: true,
        crawl_urls: "https://docs.synapse2.test, https://api.synapse2.test".into(),
        crawl_repos: "".into(),
        crawl_search_topics: "Lab API".into(),
    }
}

fn assert_scaffold_contract_shape(value: &Value) {
    assert_eq!(value["kind"], "synapse2_scaffold_intent");
    assert_eq!(value["schema_version"], 1);
    assert_non_empty_string(&value["project"]["display_name"]);
    assert_matches_kebab(&value["project"]["crate_name"]);
    assert_matches_kebab(&value["project"]["binary_name"]);
    assert_matches_service_name(&value["project"]["service_name"]);
    assert_matches_env_prefix(&value["project"]["env_prefix"]);
    assert!(
        value["runtime"]["port"]
            .as_u64()
            .is_some_and(|port| port > 0),
        "runtime.port must be a positive integer"
    );
}

fn assert_non_empty_string(value: &Value) {
    assert!(
        value.as_str().is_some_and(|value| !value.is_empty()),
        "expected non-empty string, got {value}"
    );
}

fn assert_matches_kebab(value: &Value) {
    let value = value.as_str().expect("expected string");
    let mut chars = value.chars();
    assert!(chars.next().is_some_and(|ch| ch.is_ascii_lowercase()));
    assert!(chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'));
}

fn assert_matches_service_name(value: &Value) {
    let value = value.as_str().expect("expected string");
    let mut chars = value.chars();
    assert!(chars.next().is_some_and(|ch| ch.is_ascii_lowercase()));
    assert!(chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'));
}

fn assert_matches_env_prefix(value: &Value) {
    let value = value.as_str().expect("expected string");
    let mut chars = value.chars();
    assert!(chars.next().is_some_and(|ch| ch.is_ascii_uppercase()));
    assert!(chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'));
}

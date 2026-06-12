use super::{help_response, legacy_flux_help, legacy_scout_help, topic_markdown};

#[test]
fn known_topic_returns_markdown() {
    let text = topic_markdown("container:list").expect("container:list should be in the map");
    assert!(
        text.contains("List containers"),
        "topic text should describe the action"
    );
}

#[test]
fn unknown_topic_returns_none() {
    assert!(
        topic_markdown("nonexistent:action").is_none(),
        "unknown topic should return None"
    );
}

#[test]
fn help_response_no_topic_returns_index() {
    let result = help_response("flux", None, None).expect("no-topic help should succeed");
    assert!(
        result["topics"].is_array(),
        "index should include topics array"
    );
    assert_eq!(result["tool"], "flux");
}

#[test]
fn help_response_known_topic_returns_text() {
    let result =
        help_response("flux", Some("container:list"), None).expect("known topic should succeed");
    let text = result.as_str().expect("markdown result should be a string");
    assert!(
        text.contains("container:list"),
        "should include the topic key header"
    );
    assert!(
        text.contains("List containers"),
        "should include the help text"
    );
}

#[test]
fn help_response_unknown_topic_returns_error() {
    let err = help_response("flux", Some("nonsense:action"), None)
        .expect_err("unknown topic should return error");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown help topic"),
        "error should describe the unknown topic, got: {msg}"
    );
    assert!(
        msg.contains("nonsense:action"),
        "error should name the bad topic"
    );
}

#[test]
fn help_response_json_format_wraps_text() {
    let result = help_response("flux", Some("container:list"), Some("json"))
        .expect("json format should succeed");
    assert_eq!(result["topic"], "container:list");
    assert!(
        result["text"].is_string(),
        "json format should have a text field"
    );
    let text = result["text"].as_str().unwrap();
    assert!(text.contains("List containers"));
}

#[test]
fn help_response_json_format_index_when_no_topic() {
    let result = help_response("scout", None, Some("json")).expect("json index should succeed");
    // In json format with no topic, we return {topic: null, index: {...}}
    assert!(
        result["index"].is_object(),
        "json no-topic should have an index field"
    );
}

#[test]
fn legacy_flux_help_shape_unchanged() {
    let result = legacy_flux_help();
    // The existing test asserts flux["tool"] == "flux"
    assert_eq!(result["tool"], "flux");
    assert!(result["actions"].is_object(), "actions should be an object");
    assert!(
        result["destructive"].is_array(),
        "destructive should be an array"
    );
    // B16 additions
    assert!(result["topics"].is_array(), "B16: topics should be present");
    assert!(result["hint"].is_string(), "B16: hint should be present");
}

#[test]
fn legacy_scout_help_shape_unchanged() {
    let result = legacy_scout_help();
    assert_eq!(result["tool"], "scout");
    assert!(result["actions"].is_array(), "actions should be an array");
    assert!(result["topics"].is_array(), "B16: topics should be present");
}

#[test]
fn scout_exec_topic_exists() {
    let text = topic_markdown("exec").expect("scout exec topic should be in map");
    assert!(
        text.contains("allowlist"),
        "exec help should mention the allowlist"
    );
}

#[test]
fn flux_topic_keys_sorted() {
    let keys = super::flux_topic_keys();
    assert!(!keys.is_empty(), "flux should have topics");
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "flux_topic_keys() should return sorted keys");
}

#[test]
fn scout_topic_keys_sorted() {
    let keys = super::scout_topic_keys();
    assert!(!keys.is_empty(), "scout should have topics");
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "scout_topic_keys() should return sorted keys");
}

#[test]
fn flux_help_matches_required_host_contract() {
    for topic in [
        "host:services",
        "host:mounts",
        "host:ports",
        "host:doctor",
        "compose:list",
    ] {
        let text = topic_markdown(topic).expect("topic should exist");
        assert!(
            text.contains("`host` (required)"),
            "{topic} should document host as required"
        );
    }
}

use super::tool_definitions;

#[test]
fn defines_flux_and_scout_tools() {
    let tools = tool_definitions();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["flux", "scout"]);
}

#[test]
fn schemas_disallow_unknown_top_level_properties() {
    for tool in tool_definitions() {
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(tool["inputSchema"]["properties"]["action"]["enum"].is_array());
    }
}

#[test]
fn flux_schema_includes_host_parser_fields() {
    let tools = tool_definitions();
    let flux = tools
        .iter()
        .find(|tool| tool["name"] == "flux")
        .expect("flux schema should exist");
    let props = &flux["inputSchema"]["properties"];

    for field in ["protocol", "offset", "checks"] {
        assert!(
            props[field].is_object(),
            "flux schema should expose parser-supported field {field}"
        );
    }

    let host_description = props["host"]["description"].as_str().unwrap_or_default();
    assert!(host_description.contains("host services/mounts/ports/doctor"));
    assert!(host_description.contains("compose ops including list"));
}

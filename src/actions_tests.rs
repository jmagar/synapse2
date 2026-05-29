use super::{
    is_read_only, required_scope_for_action, scopes_satisfy, ActionSpec, ActionTransport,
    SynapseAction, ACTION_SPECS, READ_SCOPE, WRITE_SCOPE,
};
use serde_json::json;

#[test]
fn all_current_actions_are_non_destructive_and_read_only() {
    // First parity slice ships only read-only actions; destructive ones arrive
    // in B9/B10/B13. This guards against accidentally flipping a flag.
    for spec in ACTION_SPECS {
        assert!(!spec.destructive, "{} should not be destructive", spec.name);
        assert!(is_read_only(spec), "{} should be read-only", spec.name);
    }
}

#[test]
fn destructive_action_is_not_read_only() {
    let spec = ActionSpec {
        name: "rm",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        destructive: true,
    };
    assert!(!is_read_only(&spec));
}

#[test]
fn write_scoped_non_destructive_action_is_not_read_only() {
    let spec = ActionSpec {
        name: "label",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    };
    assert!(!is_read_only(&spec));
}

#[test]
fn read_scope_and_write_implies_read() {
    assert_eq!(required_scope_for_action("docker"), Some(READ_SCOPE));
    assert_eq!(required_scope_for_action("nodes"), Some(READ_SCOPE));
    assert!(scopes_satisfy(&[WRITE_SCOPE.into()], READ_SCOPE));
}

#[test]
fn parses_flux_actions() {
    assert_eq!(
        SynapseAction::from_flux_args(&json!({"action":"docker","subaction":"info"})).unwrap(),
        SynapseAction::FluxDocker {
            subaction: "info".into()
        }
    );
    let logs = SynapseAction::from_flux_args(&json!({
        "action":"container",
        "subaction":"logs",
        "container_id":"abc",
        "lines":20
    }))
    .unwrap();
    match logs {
        SynapseAction::FluxContainer(args) => {
            assert_eq!(args.subaction, "logs");
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.lines, Some(20));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn parses_container_list_filters() {
    let action = SynapseAction::from_flux_args(&json!({
        "action": "container",
        "subaction": "list",
        "host": "dookie",
        "state": "running",
        "name_filter": "nginx",
        "image_filter": "nginx",
        "label_filter": "tier=edge",
        "response_format": "json"
    }))
    .unwrap();
    match action {
        SynapseAction::FluxContainer(args) => {
            assert_eq!(args.subaction, "list");
            assert_eq!(args.host.as_deref(), Some("dookie"));
            assert_eq!(args.state.as_deref(), Some("running"));
            assert_eq!(args.name_filter.as_deref(), Some("nginx"));
            assert_eq!(args.image_filter.as_deref(), Some("nginx"));
            assert_eq!(args.label_filter.as_deref(), Some("tier=edge"));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn rejects_invalid_response_format_on_container() {
    let err = SynapseAction::from_flux_args(&json!({
        "action": "container",
        "subaction": "list",
        "response_format": "xml"
    }))
    .unwrap_err();
    assert!(err.to_string().contains("response_format") || err.to_string().contains("xml"));
}

#[test]
fn parses_scout_actions_and_rejects_missing_fields() {
    assert_eq!(
        SynapseAction::from_scout_args(&json!({"action":"nodes"})).unwrap(),
        SynapseAction::ScoutNodes
    );
    let error =
        SynapseAction::from_scout_args(&json!({"action":"exec","host":"local"})).unwrap_err();
    assert!(error.to_string().contains("path"));
}

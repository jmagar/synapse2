use super::{Command, parse_args_from};
use serde_json::json;

#[test]
fn parses_flux_and_scout_commands() {
    match parse_args_from(["flux", "docker", "images"]).unwrap() {
        Some(Command::FluxDocker(args)) => assert_eq!(args.subaction, "images"),
        other => panic!("expected FluxDocker, got {other:?}"),
    }
    match parse_args_from(["flux", "host", "status", "--host", "local"]).unwrap() {
        Some(Command::FluxHost(args)) => {
            assert_eq!(args.subaction, "status");
            assert_eq!(args.host.as_deref(), Some("local"));
        }
        other => panic!("expected FluxHost, got {other:?}"),
    }
    assert_eq!(
        parse_args_from(["scout", "nodes"]).unwrap(),
        Some(Command::ScoutNodes {
            response_format: None
        })
    );
}

#[test]
fn rejects_malformed_synapse_commands() {
    assert!(parse_args_from(["flux"]).is_err());
    assert!(parse_args_from(["scout", "peek", "--host", "local"]).is_err());
    assert!(parse_args_from(["scout", "exec", "--host", "local"]).is_err());
}

#[test]
fn cli_output_defaults_to_markdown() {
    let cmd = parse_args_from(["flux", "docker", "info"])
        .unwrap()
        .unwrap();
    let rendered = super::render_cli_output(&cmd, &json!({"info": {"host": "local"}})).unwrap();
    assert!(
        !rendered.trim_start().starts_with('{'),
        "default CLI output should not be raw JSON"
    );
}

#[test]
fn cli_output_json_requires_response_format_json() {
    let cmd = parse_args_from(["flux", "docker", "info", "--response-format", "json"])
        .unwrap()
        .unwrap();
    let rendered = super::render_cli_output(&cmd, &json!({"info": {"host": "local"}})).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed["info"]["host"], "local");
}

#[test]
fn strips_global_color_flags_before_command_parsing() {
    let mut args = vec![
        "--color=never".to_string(),
        "flux".to_string(),
        "container".to_string(),
        "list".to_string(),
    ];
    super::install_color_from_args(&mut args).unwrap();
    assert_eq!(args, ["flux", "container", "list"]);
    assert!(matches!(
        parse_args_from(args).unwrap(),
        Some(Command::FluxContainer(_))
    ));
}

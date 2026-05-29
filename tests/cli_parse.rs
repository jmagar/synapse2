use synapse2::cli::{parse_args_from, Command, SetupCommand};

#[test]
fn flux_docker_info_parsed() {
    let cmd = parse_args_from(["flux", "docker", "info"]).unwrap();
    match cmd {
        Some(Command::FluxDocker(args)) => assert_eq!(args.subaction, "info"),
        other => panic!("expected FluxDocker, got {other:?}"),
    }
}

#[test]
fn flux_container_logs_parsed() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "logs",
        "--container-id",
        "abc",
        "--lines",
        "20",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.subaction, "logs");
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.lines, Some(20));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_list_filters_parse() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "list",
        "--host",
        "dookie",
        "--state",
        "running",
        "--name-filter",
        "nginx",
        "--image-filter",
        "nginx",
        "--label-filter",
        "tier=edge",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
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
fn container_inspect_summary_flag_parses() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "inspect",
        "--container-id",
        "abc",
        "--summary",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.subaction, "inspect");
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.summary, Some(true), "--summary must set summary=true");
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_rejects_invalid_response_format() {
    let err =
        parse_args_from(["flux", "container", "list", "--response-format", "xml"]).unwrap_err();
    assert!(
        err.to_string().contains("response_format")
            || err.to_string().to_lowercase().contains("xml")
            || err.to_string().contains("markdown"),
        "got: {err}"
    );
}

#[test]
fn container_accepts_valid_response_format() {
    let cmd = parse_args_from(["flux", "container", "list", "--response-format", "json"]).unwrap();
    assert!(matches!(cmd, Some(Command::FluxContainer(_))));
}

#[test]
fn container_search_query_parses() {
    let cmd = parse_args_from(["flux", "container", "search", "--query", "web"]).unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.subaction, "search");
            assert_eq!(args.query.as_deref(), Some("web"));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn scout_commands_parse() {
    assert_eq!(
        parse_args_from(["scout", "nodes"]).unwrap(),
        Some(Command::ScoutNodes)
    );
    assert_eq!(
        parse_args_from(["scout", "peek", "--host", "local", "--path", "/tmp"]).unwrap(),
        Some(Command::ScoutPeek {
            host: "local".into(),
            path: "/tmp".into(),
        })
    );
    assert_eq!(
        parse_args_from([
            "scout",
            "exec",
            "--host",
            "local",
            "--path",
            "/tmp",
            "--command",
            "ls"
        ])
        .unwrap(),
        Some(Command::ScoutExec {
            host: "local".into(),
            path: "/tmp".into(),
            command: "ls".into(),
        })
    );
}

#[test]
fn setup_and_doctor_still_parse() {
    assert_eq!(
        parse_args_from(["setup", "plugin-hook", "--no-repair"]).unwrap(),
        Some(Command::Setup(SetupCommand::PluginHook { no_repair: true }))
    );
    assert_eq!(
        parse_args_from(["doctor", "--json"]).unwrap(),
        Some(Command::Doctor { json: true })
    );
}

#[test]
fn malformed_args_are_rejected() {
    for args in [
        &["flux", "container", "logs", "--container-id"][..],
        &["scout", "exec", "--host", "local", "--path", "/tmp"],
        &["watch", "--interval", "0"],
        &["setup", "plugin-hook", "--no-reapir"],
    ] {
        assert!(
            parse_args_from(args.iter().copied()).is_err(),
            "{args:?} should be rejected"
        );
    }
}

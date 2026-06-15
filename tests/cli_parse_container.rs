use synapse2::cli::{Command, parse_args_from};

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
fn container_logs_filters_parse() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "logs",
        "--container-id",
        "abc",
        "--since",
        "30m",
        "--until",
        "now",
        "--grep",
        "ERROR",
        "--stream",
        "stderr",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.since.as_deref(), Some("30m"));
            assert_eq!(args.until.as_deref(), Some("now"));
            assert_eq!(args.grep.as_deref(), Some("ERROR"));
            assert_eq!(args.stream.as_deref(), Some("stderr"));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_lifecycle_and_recreate_flags_parse() {
    let cmd = parse_args_from(["flux", "container", "restart", "--container-id", "abc"]).unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.subaction, "restart");
            assert_eq!(args.container_id.as_deref(), Some("abc"));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }

    let cmd = parse_args_from([
        "flux",
        "container",
        "recreate",
        "--container-id",
        "abc",
        "--no-pull",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.subaction, "recreate");
            assert_eq!(args.pull, Some(false));
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_exec_command_accepts_flags_after_command() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "exec",
        "--container-id",
        "abc",
        "--command",
        "sh",
        "-c",
        "echo ok",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.subaction, "exec");
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.command, ["sh", "-c", "echo ok"]);
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_exec_command_accepts_double_dash_flag_after_command() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "exec",
        "--container-id",
        "abc",
        "--command",
        "tool",
        "--flag",
        "value",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.command, ["tool", "--flag", "value"]);
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_exec_options_before_command_are_parsed_as_synapse_options() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "exec",
        "--host",
        "dookie",
        "--container-id",
        "abc",
        "--timeout",
        "5000",
        "--command",
        "printenv",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.host.as_deref(), Some("dookie"));
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.exec_timeout_ms, Some(5000));
            assert_eq!(args.command, ["printenv"]);
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_exec_user_and_workdir_parse() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "exec",
        "--container-id",
        "abc",
        "--user",
        "root",
        "--workdir",
        "/app",
        "--command",
        "pwd",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.exec_user.as_deref(), Some("root"));
            assert_eq!(args.exec_workdir.as_deref(), Some("/app"));
            assert_eq!(args.command, ["pwd"]);
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

#[test]
fn container_exec_synapse_options_after_command_are_container_argv() {
    let cmd = parse_args_from([
        "flux",
        "container",
        "exec",
        "--container-id",
        "abc",
        "--command",
        "tool",
        "--timeout",
        "5000",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxContainer(args)) => {
            assert_eq!(args.container_id.as_deref(), Some("abc"));
            assert_eq!(args.exec_timeout_ms, None);
            assert_eq!(args.command, ["tool", "--timeout", "5000"]);
        }
        other => panic!("expected FluxContainer, got {other:?}"),
    }
}

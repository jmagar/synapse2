use synapse2::cli::{Command, parse_args_from};

#[test]
fn flux_host_commands_parse_options() {
    let cmd = parse_args_from([
        "flux",
        "host",
        "ports",
        "--host",
        "dookie",
        "--protocol",
        "tcp",
        "--limit",
        "25",
        "--offset",
        "50",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxHost(args)) => {
            assert_eq!(args.subaction, "ports");
            assert_eq!(args.host.as_deref(), Some("dookie"));
            assert_eq!(args.protocol.as_deref(), Some("tcp"));
            assert_eq!(args.limit, Some(25));
            assert_eq!(args.offset, Some(50));
        }
        other => panic!("expected FluxHost, got {other:?}"),
    }

    let cmd = parse_args_from([
        "flux",
        "host",
        "doctor",
        "--host",
        "dookie",
        "--checks",
        "docker,logs",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxHost(args)) => {
            assert_eq!(args.subaction, "doctor");
            assert_eq!(args.checks.as_deref(), Some("docker,logs"));
        }
        other => panic!("expected FluxHost, got {other:?}"),
    }
}

#[test]
fn flux_compose_commands_parse_options() {
    let cmd = parse_args_from([
        "flux",
        "compose",
        "down",
        "--host",
        "tootie",
        "--project",
        "media",
        "--remove-volumes",
        "--force",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxCompose(args)) => {
            assert_eq!(args.subaction, "down");
            assert_eq!(args.host.as_deref(), Some("tootie"));
            assert_eq!(args.project.as_deref(), Some("media"));
            assert_eq!(args.remove_volumes, Some(true));
            assert_eq!(args.force, Some(true));
        }
        other => panic!("expected FluxCompose, got {other:?}"),
    }

    let cmd = parse_args_from([
        "flux",
        "compose",
        "logs",
        "--host",
        "tootie",
        "--project",
        "media",
        "--service",
        "plex",
        "--lines",
        "150",
        "--since",
        "1h",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxCompose(args)) => {
            assert_eq!(args.subaction, "logs");
            assert_eq!(args.service.as_deref(), Some("plex"));
            assert_eq!(args.lines, Some(150));
            assert_eq!(args.since.as_deref(), Some("1h"));
        }
        other => panic!("expected FluxCompose, got {other:?}"),
    }
}

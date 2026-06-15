use synapse2::cli::{Command, parse_args_from};

#[test]
fn flux_docker_info_parsed() {
    let cmd = parse_args_from(["flux", "docker", "info"]).unwrap();
    match cmd {
        Some(Command::FluxDocker(args)) => assert_eq!(args.subaction, "info"),
        other => panic!("expected FluxDocker, got {other:?}"),
    }
}

#[test]
fn flux_docker_flags_and_values_parse() {
    let cmd = parse_args_from([
        "flux",
        "docker",
        "build",
        "--host",
        "dookie",
        "--context",
        "/srv/app",
        "--tag",
        "app:test",
        "--dockerfile",
        "Dockerfile.dev",
        "--no-cache",
        "--response-format",
        "json",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxDocker(args)) => {
            assert_eq!(args.subaction, "build");
            assert_eq!(args.host.as_deref(), Some("dookie"));
            assert_eq!(args.context.as_deref(), Some("/srv/app"));
            assert_eq!(args.tag.as_deref(), Some("app:test"));
            assert_eq!(args.dockerfile.as_deref(), Some("Dockerfile.dev"));
            assert_eq!(args.no_cache, Some(true));
            assert_eq!(args.response_format.as_deref(), Some("json"));
        }
        other => panic!("expected FluxDocker, got {other:?}"),
    }

    let cmd = parse_args_from([
        "flux", "docker", "prune", "--host", "dookie", "--target", "images", "--force",
    ])
    .unwrap();
    match cmd {
        Some(Command::FluxDocker(args)) => {
            assert_eq!(args.subaction, "prune");
            assert_eq!(args.prune_target.as_deref(), Some("images"));
            assert_eq!(args.force, Some(true));
        }
        other => panic!("expected FluxDocker, got {other:?}"),
    }

    let cmd = parse_args_from(["flux", "docker", "images", "--dangling-only"]).unwrap();
    match cmd {
        Some(Command::FluxDocker(args)) => assert_eq!(args.dangling_only, Some(true)),
        other => panic!("expected FluxDocker, got {other:?}"),
    }
}

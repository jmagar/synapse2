use synapse2::cli::{Command, parse_args_from};

#[test]
fn scout_commands_parse() {
    assert_eq!(
        parse_args_from(["scout", "nodes"]).unwrap(),
        Some(Command::ScoutNodes {
            response_format: None
        })
    );
    // peek with defaults
    assert_eq!(
        parse_args_from(["scout", "peek", "--host", "local", "--path", "/tmp"]).unwrap(),
        Some(Command::ScoutPeek {
            response_format: None,
            host: "local".into(),
            path: "/tmp".into(),
            tree: false,
            depth: 3,
        })
    );
    // exec with new boxed args shape
    let cmd = parse_args_from([
        "scout",
        "exec",
        "--host",
        "local",
        "--path",
        "/tmp",
        "--command",
        "ls",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutExec(a)) => {
            assert_eq!(a.host, "local");
            assert_eq!(a.path.as_deref(), Some("/tmp"));
            assert_eq!(a.command, "ls");
        }
        other => panic!("expected ScoutExec, got {other:?}"),
    }
    // find
    let cmd = parse_args_from([
        "scout",
        "find",
        "--host",
        "local",
        "--path",
        "/etc",
        "--pattern",
        "*.conf",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutFind(a)) => {
            assert_eq!(a.host, "local");
            assert_eq!(a.pattern, "*.conf");
        }
        other => panic!("expected ScoutFind, got {other:?}"),
    }
}

#[test]
fn scout_remaining_commands_parse() {
    let cmd = parse_args_from([
        "scout", "ps", "--host", "dookie", "--sort", "mem", "--grep", "nginx", "--user", "root",
        "--limit", "10",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutPs(args)) => {
            assert_eq!(args.host, "dookie");
            assert_eq!(args.sort.as_deref(), Some("mem"));
            assert_eq!(args.grep.as_deref(), Some("nginx"));
            assert_eq!(args.user.as_deref(), Some("root"));
            assert_eq!(args.limit, Some(10));
        }
        other => panic!("expected ScoutPs, got {other:?}"),
    }

    let cmd = parse_args_from(["scout", "df", "--host", "dookie", "--path", "/srv"]).unwrap();
    match cmd {
        Some(Command::ScoutDf { host, path, .. }) => {
            assert_eq!(host, "dookie");
            assert_eq!(path.as_deref(), Some("/srv"));
        }
        other => panic!("expected ScoutDf, got {other:?}"),
    }

    let cmd = parse_args_from([
        "scout",
        "delta",
        "--source-host",
        "a",
        "--source-path",
        "/etc/hosts",
        "--target-host",
        "b",
        "--target-path",
        "/etc/hosts",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutDelta(args)) => {
            assert_eq!(args.source_host, "a");
            assert_eq!(args.target_host.as_deref(), Some("b"));
            assert_eq!(args.target_path.as_deref(), Some("/etc/hosts"));
        }
        other => panic!("expected ScoutDelta, got {other:?}"),
    }

    let cmd = parse_args_from([
        "scout",
        "emit",
        "--target",
        "a:/srv,b",
        "--command",
        "hostname",
        "--timeout",
        "5",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutEmit(args)) => {
            assert_eq!(args.targets.len(), 2);
            assert_eq!(args.targets[0].host, "a");
            assert_eq!(args.targets[0].path.as_deref(), Some("/srv"));
            assert_eq!(args.targets[1].host, "b");
            assert_eq!(args.targets[1].path, None);
            assert_eq!(args.timeout_secs, Some(5));
        }
        other => panic!("expected ScoutEmit, got {other:?}"),
    }

    let cmd = parse_args_from([
        "scout",
        "beam",
        "--source-host",
        "a",
        "--source-path",
        "/tmp/a",
        "--dest-host",
        "b",
        "--dest-path",
        "/tmp/b",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutBeam(args)) => {
            assert_eq!(args.source_host, "a");
            assert_eq!(args.dest_host, "b");
            assert_eq!(args.dest_path, "/tmp/b");
        }
        other => panic!("expected ScoutBeam, got {other:?}"),
    }
}

#[test]
fn scout_logs_and_zfs_variants_parse() {
    let cmd = parse_args_from([
        "scout",
        "logs",
        "journal",
        "--host",
        "dookie",
        "--lines",
        "200",
        "--grep",
        "failed",
        "--unit",
        "docker.service",
        "--priority",
        "err",
        "--since",
        "-1h",
        "--until",
        "now",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutLogs(args)) => {
            assert_eq!(args.subaction, "journal");
            assert_eq!(args.lines, 200);
            assert_eq!(args.grep.as_deref(), Some("failed"));
            assert_eq!(args.unit.as_deref(), Some("docker.service"));
            assert_eq!(args.priority.as_deref(), Some("err"));
            assert_eq!(args.since.as_deref(), Some("-1h"));
            assert_eq!(args.until.as_deref(), Some("now"));
        }
        other => panic!("expected ScoutLogs, got {other:?}"),
    }

    for subaction in ["syslog", "dmesg", "auth"] {
        let cmd = parse_args_from(["scout", "logs", subaction, "--host", "dookie"]).unwrap();
        match cmd {
            Some(Command::ScoutLogs(args)) => assert_eq!(args.subaction, subaction),
            other => panic!("expected ScoutLogs, got {other:?}"),
        }
    }

    let cmd = parse_args_from([
        "scout", "zfs", "pools", "--host", "dookie", "--pool", "tank",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutZfs(args)) => {
            assert_eq!(args.subaction, "pools");
            assert_eq!(args.pool.as_deref(), Some("tank"));
        }
        other => panic!("expected ScoutZfs, got {other:?}"),
    }

    let cmd = parse_args_from([
        "scout",
        "zfs",
        "snapshots",
        "--host",
        "dookie",
        "--dataset",
        "tank/data",
        "--limit",
        "25",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutZfs(args)) => {
            assert_eq!(args.subaction, "snapshots");
            assert_eq!(args.dataset.as_deref(), Some("tank/data"));
            assert_eq!(args.limit, Some(25));
        }
        other => panic!("expected ScoutZfs, got {other:?}"),
    }
}

#[test]
fn scout_zfs_recursive_flag_parses_without_value() {
    let cmd = parse_args_from([
        "scout",
        "zfs",
        "datasets",
        "--host",
        "local",
        "--pool",
        "tank",
        "--type",
        "filesystem",
        "--recursive",
    ])
    .unwrap();
    match cmd {
        Some(Command::ScoutZfs(args)) => {
            assert_eq!(args.subaction, "datasets");
            assert_eq!(args.host, "local");
            assert_eq!(args.pool.as_deref(), Some("tank"));
            assert_eq!(args.dataset_type.as_deref(), Some("filesystem"));
            assert!(args.recursive);
        }
        other => panic!("expected ScoutZfs, got {other:?}"),
    }
}

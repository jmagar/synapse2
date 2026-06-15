pub(super) struct CommandDoc {
    pub(super) name: &'static str,
    pub(super) summary: &'static str,
    pub(super) usage: &'static [&'static str],
}

pub(super) struct NestedCommandDoc {
    pub(super) path: &'static str,
    pub(super) summary: &'static str,
    pub(super) usage: &'static [&'static str],
}

pub(super) struct UsageSection {
    pub(super) title: &'static str,
    pub(super) lines: &'static [&'static str],
}

pub(super) const SECTIONS: &[(&str, &[&str])] = &[
    ("Flux", &["flux"]),
    ("Scout", &["scout"]),
    (
        "Runtime & Setup",
        &["serve", "mcp", "doctor", "watch", "setup", "help"],
    ),
];

const CATALOG: &[CommandDoc] = &[
    CommandDoc {
        name: "flux",
        summary: "Docker, container, host, and compose operations",
        usage: &[
            "synapse flux docker info|df|networks|volumes [--host H]",
            "synapse flux docker images [--host H] [--dangling-only]",
            "synapse flux docker pull --host H --image IMG",
            "synapse flux docker build --host H --context /abs/path --tag TAG [--dockerfile REL] [--no-cache]",
            "synapse flux docker rmi --host H --image IMG --force",
            "synapse flux docker prune --host H --target containers|images|volumes|networks|buildcache|all --force",
            "synapse flux container list [--host H] [--state S] [--name-filter N] [--image-filter I] [--label-filter K=V]",
            "synapse flux container inspect --container-id ID [--host H] [--summary]",
            "synapse flux container logs --container-id ID [--host H] [--lines N] [--since T] [--until T] [--grep S] [--stream stdout|stderr|both]",
            "synapse flux container stats [--container-id ID] [--host H]",
            "synapse flux container top --container-id ID [--host H]",
            "synapse flux container search --query Q [--host H]",
            "synapse flux host status|info|uptime|resources|network [--host HOST]",
            "synapse flux host services --host HOST [--state STATE] [--service NAME]",
            "synapse flux host mounts --host HOST",
            "synapse flux host ports --host HOST [--protocol tcp|udp] [--limit N] [--offset N]",
            "synapse flux host doctor --host HOST [--checks c1,c2,...]",
            "synapse flux compose list --host HOST",
            "synapse flux compose status|up|down|restart|recreate|logs|build|pull|refresh --host HOST --project P [--service SVC]",
            "All flux actions accept [--response-format markdown|json].",
        ],
    },
    CommandDoc {
        name: "scout",
        summary: "SSH filesystem, process, transfer, ZFS, and log operations",
        usage: &[
            "synapse scout nodes",
            "synapse scout peek --host HOST --path PATH [--tree] [--depth N]",
            "synapse scout find --host HOST --path PATH --pattern GLOB [--depth N] [--limit N]",
            "synapse scout ps --host HOST [--sort cpu|mem|pid|time] [--grep S] [--user U] [--limit N]",
            "synapse scout df --host HOST [--path PATH]",
            "synapse scout delta --source-host H --source-path P (--target-host H --target-path P | --content STR)",
            "synapse scout exec --host HOST --command CMD [--path PATH] [--args A1 A2...]",
            "synapse scout emit --command CMD --target HOST:PATH[,HOST:PATH...] [--timeout S]",
            "synapse scout beam --source-host H --source-path P --dest-host H --dest-path P",
            "synapse scout zfs pools|datasets|snapshots --host HOST [--pool POOL]",
            "synapse scout logs syslog|journal|dmesg|auth --host HOST [--lines N] [--grep STR]",
            "All scout actions accept [--response-format markdown|json].",
        ],
    },
    CommandDoc {
        name: "serve",
        summary: "Start the MCP HTTP server",
        usage: &["synapse serve", "synapse serve mcp"],
    },
    CommandDoc {
        name: "mcp",
        summary: "Start the MCP stdio transport",
        usage: &["synapse mcp"],
    },
    CommandDoc {
        name: "doctor",
        summary: "Run environment pre-flight checks",
        usage: &["synapse doctor [--json]"],
    },
    CommandDoc {
        name: "watch",
        summary: "Poll /health and emit state changes",
        usage: &["synapse watch [--url URL] [--interval N]"],
    },
    CommandDoc {
        name: "setup",
        summary: "Initialize, check, and repair plugin setup",
        usage: &[
            "synapse setup check",
            "synapse setup repair",
            "synapse setup install",
            "synapse setup plugin-hook [--no-repair]",
        ],
    },
    CommandDoc {
        name: "help",
        summary: "Show the action reference",
        usage: &["synapse help [--response-format markdown|json]"],
    },
];

const NESTED_CATALOG: &[NestedCommandDoc] = &[
    NestedCommandDoc {
        path: "flux docker",
        summary: "Docker engine operations",
        usage: &[
            "synapse flux docker info|df|networks|volumes [--host H]",
            "synapse flux docker images [--host H] [--dangling-only]",
            "synapse flux docker pull --host H --image IMG",
            "synapse flux docker build --host H --context /abs/path --tag TAG [--dockerfile REL] [--no-cache]",
            "synapse flux docker rmi --host H --image IMG --force",
            "synapse flux docker prune --host H --target containers|images|volumes|networks|buildcache|all --force",
        ],
    },
    NestedCommandDoc {
        path: "flux container",
        summary: "Container read operations",
        usage: &[
            "synapse flux container list [--host H] [--state S] [--name-filter N] [--image-filter I] [--label-filter K=V]",
            "synapse flux container inspect --container-id ID [--host H] [--summary]",
            "synapse flux container logs --container-id ID [--host H] [--lines N] [--since T] [--until T] [--grep S] [--stream stdout|stderr|both]",
            "synapse flux container stats [--container-id ID] [--host H]",
            "synapse flux container top --container-id ID [--host H]",
            "synapse flux container search --query Q [--host H]",
        ],
    },
    NestedCommandDoc {
        path: "flux host",
        summary: "Host status and inventory operations",
        usage: &[
            "synapse flux host status|info|uptime|resources|network [--host HOST]",
            "synapse flux host services --host HOST [--state STATE] [--service NAME]",
            "synapse flux host mounts --host HOST",
            "synapse flux host ports --host HOST [--protocol tcp|udp] [--limit N] [--offset N]",
            "synapse flux host doctor --host HOST [--checks c1,c2,...]",
        ],
    },
    NestedCommandDoc {
        path: "flux compose",
        summary: "Docker Compose project operations",
        usage: &[
            "synapse flux compose list --host HOST",
            "synapse flux compose status --host HOST --project P [--service SVC]",
            "synapse flux compose up|down|restart|recreate|logs|build|pull|refresh --host HOST --project P [--service SVC]",
        ],
    },
    NestedCommandDoc {
        path: "scout zfs",
        summary: "ZFS pool, dataset, and snapshot inspection",
        usage: &["synapse scout zfs pools|datasets|snapshots --host HOST [--pool POOL]"],
    },
    NestedCommandDoc {
        path: "scout logs",
        summary: "Remote syslog, journal, dmesg, and auth log reads",
        usage: &[
            "synapse scout logs syslog|journal|dmesg|auth --host HOST [--lines N] [--grep STR]",
        ],
    },
    NestedCommandDoc {
        path: "setup plugin-hook",
        summary: "Run plugin setup hook repair or audit mode",
        usage: &["synapse setup plugin-hook [--no-repair]"],
    },
];

pub(super) const GLOBAL_OPTIONS: &[(&str, &str)] = &[
    ("-h, --help", "Display help (top-level or per-command)"),
    ("--version", "Print version and exit"),
    ("--color <when>", "Colorize output: always, never, or auto"),
    (
        "--no-color",
        "Disable colored output (alias for --color=never)",
    ),
];

pub(super) const ENVIRONMENT: &[(&str, &str)] = &[
    ("SYNAPSE_HOSTS_CONFIG", "Host topology as a JSON array"),
    (
        "SYNAPSE_CONFIG_FILE",
        "Host config file path (falls back to ~/.ssh/config)",
    ),
    ("SYNAPSE_MCP_HOST", "Bind host (default 127.0.0.1)"),
    ("SYNAPSE_MCP_PORT", "Bind port (default 40080)"),
    ("SYNAPSE_MCP_NO_AUTH", "Disable auth (loopback only)"),
    ("SYNAPSE_MCP_TOKEN", "Static bearer token"),
    ("RUST_LOG", "Log filter; stdio logs always go to stderr"),
];

pub(super) const QUICK_START: &[&str] = &[
    "synapse flux container list --host local",
    "synapse scout nodes",
    "synapse doctor",
];

pub(super) const FLUX_USAGE_SECTIONS: &[UsageSection] = &[
    UsageSection {
        title: "Docker",
        lines: &[
            "synapse flux docker info|df|networks|volumes [--host H]",
            "synapse flux docker images [--host H] [--dangling-only]",
            "synapse flux docker pull --host H --image IMG",
            "synapse flux docker build --host H --context /abs/path --tag TAG [--dockerfile REL] [--no-cache]",
            "synapse flux docker rmi --host H --image IMG --force",
            "synapse flux docker prune --host H --target containers|images|volumes|networks|buildcache|all --force",
        ],
    },
    UsageSection {
        title: "Containers",
        lines: &[
            "synapse flux container list [--host H] [--state S] [--name-filter N] [--image-filter I] [--label-filter K=V]",
            "synapse flux container inspect --container-id ID [--host H] [--summary]",
            "synapse flux container logs --container-id ID [--host H] [--lines N] [--since T] [--until T] [--grep S] [--stream stdout|stderr|both]",
            "synapse flux container stats [--container-id ID] [--host H]",
            "synapse flux container top --container-id ID [--host H]",
            "synapse flux container search --query Q [--host H]",
        ],
    },
    UsageSection {
        title: "Host",
        lines: &[
            "synapse flux host status|info|uptime|resources|network [--host HOST]",
            "synapse flux host services --host HOST [--state STATE] [--service NAME]",
            "synapse flux host mounts --host HOST",
            "synapse flux host ports --host HOST [--protocol tcp|udp] [--limit N] [--offset N]",
            "synapse flux host doctor --host HOST [--checks c1,c2,...]",
        ],
    },
    UsageSection {
        title: "Compose",
        lines: &[
            "synapse flux compose list --host HOST",
            "synapse flux compose status --host HOST --project P [--service SVC]",
            "synapse flux compose up|down|restart|recreate|logs|build|pull|refresh --host HOST --project P [--service SVC]",
            "All flux actions accept [--response-format markdown|json].",
        ],
    },
];

pub(super) const SCOUT_USAGE_SECTIONS: &[UsageSection] = &[
    UsageSection {
        title: "Inventory & Files",
        lines: &[
            "synapse scout nodes",
            "synapse scout peek --host HOST --path PATH [--tree] [--depth N]",
            "synapse scout find --host HOST --path PATH --pattern GLOB [--depth N] [--limit N]",
            "synapse scout df --host HOST [--path PATH]",
        ],
    },
    UsageSection {
        title: "Processes & Exec",
        lines: &[
            "synapse scout ps --host HOST [--sort cpu|mem|pid|time] [--grep S] [--user U] [--limit N]",
            "synapse scout exec --host HOST --command CMD [--path PATH] [--args A1 A2...]",
        ],
    },
    UsageSection {
        title: "Transfer",
        lines: &[
            "synapse scout delta --source-host H --source-path P (--target-host H --target-path P | --content STR)",
            "synapse scout emit --command CMD --target HOST:PATH[,HOST:PATH...] [--timeout S]",
            "synapse scout beam --source-host H --source-path P --dest-host H --dest-path P",
        ],
    },
    UsageSection {
        title: "ZFS & Logs",
        lines: &[
            "synapse scout zfs pools|datasets|snapshots --host HOST [--pool POOL]",
            "synapse scout logs syslog|journal|dmesg|auth --host HOST [--lines N] [--grep STR]",
            "All scout actions accept [--response-format markdown|json].",
        ],
    },
];

pub(super) fn lookup(name: &str) -> Option<&'static CommandDoc> {
    CATALOG.iter().find(|doc| doc.name == name)
}

pub(super) fn nested_lookup(path: &str) -> Option<&'static NestedCommandDoc> {
    NESTED_CATALOG.iter().find(|doc| doc.path == path)
}

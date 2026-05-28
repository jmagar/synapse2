//! Tests for the formatters module.
//!
//! Validates:
//! 1. `ResponseFormat::parse` — unknown values are rejected, not silently fallen back.
//! 2. Container list markdown — column order and data shape match synapse-mcp output.
//! 3. Docker df markdown — column order and data shape match synapse-mcp output.
//! 4. Host resources markdown — column order and data shape match synapse-mcp output.
//! 5. JSON passthrough — valid JSON and round-trips through `serde_json::from_str`.
//! 6. Per-domain spot tests for the other formatters.

use serde_json::json;

use crate::formatters::{
    compose::{
        render_compose_down_markdown, render_compose_list_markdown, render_compose_status_markdown,
        render_compose_up_markdown,
    },
    container::{
        render_container_inspect_markdown, render_container_list_markdown,
        render_container_logs_markdown,
    },
    docker::{
        render_docker_df_markdown, render_docker_images_markdown, render_docker_info_markdown,
        render_docker_networks_markdown, render_docker_volumes_markdown,
    },
    host::{render_host_resources_markdown, render_host_status_markdown},
    scout::{
        render_scout_exec_markdown, render_scout_nodes_markdown, render_scout_peek_markdown,
        render_scout_syslog_markdown, render_scout_zfs_pools_markdown,
    },
    ResponseFormat,
};

// ──────────────────────────────────────────────
// ResponseFormat::parse
// ──────────────────────────────────────────────

#[test]
fn parse_none_defaults_to_markdown() {
    assert_eq!(
        ResponseFormat::parse(None).unwrap(),
        ResponseFormat::Markdown
    );
}

#[test]
fn parse_markdown_variants() {
    assert_eq!(
        ResponseFormat::parse(Some("markdown")).unwrap(),
        ResponseFormat::Markdown
    );
    assert_eq!(
        ResponseFormat::parse(Some("MARKDOWN")).unwrap(),
        ResponseFormat::Markdown
    );
    assert_eq!(
        ResponseFormat::parse(Some("md")).unwrap(),
        ResponseFormat::Markdown
    );
    assert_eq!(
        ResponseFormat::parse(Some("MD")).unwrap(),
        ResponseFormat::Markdown
    );
}

#[test]
fn parse_json_variant() {
    assert_eq!(
        ResponseFormat::parse(Some("json")).unwrap(),
        ResponseFormat::Json
    );
    assert_eq!(
        ResponseFormat::parse(Some("JSON")).unwrap(),
        ResponseFormat::Json
    );
    assert_eq!(
        ResponseFormat::parse(Some("Json")).unwrap(),
        ResponseFormat::Json
    );
}

#[test]
fn parse_unknown_is_error_not_fallback() {
    // This is the critical requirement from the bead Testing section.
    // Unknown values must be REJECTED, not silently fall back to markdown.
    let err = ResponseFormat::parse(Some("xml"));
    assert!(
        err.is_err(),
        "unknown format 'xml' should return Err, not Ok"
    );

    let err2 = ResponseFormat::parse(Some("html"));
    assert!(
        err2.is_err(),
        "unknown format 'html' should return Err, not Ok"
    );

    let err3 = ResponseFormat::parse(Some("yaml"));
    assert!(
        err3.is_err(),
        "unknown format 'yaml' should return Err, not Ok"
    );

    let err4 = ResponseFormat::parse(Some(""));
    assert!(err4.is_err(), "empty string should return Err, not Ok");
}

#[test]
fn parse_error_message_is_descriptive() {
    let err = ResponseFormat::parse(Some("xml")).unwrap_err();
    assert!(
        err.contains("xml"),
        "error should mention the unknown value"
    );
    assert!(
        err.contains("markdown") || err.contains("json"),
        "error should list valid options"
    );
}

// ──────────────────────────────────────────────
// ResponseFormat::render helper
// ──────────────────────────────────────────────

#[test]
fn render_markdown_calls_closure() {
    let fmt = ResponseFormat::Markdown;
    let value = json!({"foo": "bar"});
    let result = fmt.render(&value, |_| "custom markdown".to_owned());
    assert_eq!(result, "custom markdown");
}

#[test]
fn render_json_produces_pretty_json() {
    let fmt = ResponseFormat::Json;
    let value = json!({"foo": "bar"});
    let result = fmt.render(&value, |_| "should not be called".to_owned());
    // Must be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("JSON output must be valid JSON");
    assert_eq!(parsed["foo"], "bar");
    // Must be pretty-printed (contains newlines)
    assert!(
        result.contains('\n'),
        "JSON output should be pretty-printed"
    );
}

#[test]
fn json_output_roundtrips() {
    let fmt = ResponseFormat::Json;
    let original = json!({
        "containers": [
            {"name": "nginx", "state": "running"},
            {"name": "myapp", "state": "exited"}
        ],
        "count": 2
    });
    let rendered = fmt.render(&original, |_| String::new());
    let roundtripped: serde_json::Value = serde_json::from_str(&rendered).expect("must round-trip");
    assert_eq!(roundtripped["count"], 2);
    assert_eq!(roundtripped["containers"][0]["name"], "nginx");
}

// ──────────────────────────────────────────────
// Container list — validation gate (B4 spec)
// ──────────────────────────────────────────────

#[test]
fn container_list_basic_structure() {
    // Simulates the shape from docker_json wrapper
    let data = json!({
        "available": true,
        "command": "docker container ls -a --format {{json .}}",
        "stdout": concat!(
            r#"{"ID":"abc123","Names":"/nginx","Image":"nginx:latest","Status":"Up 2 hours","State":"running","Ports":""}"#,
            "\n",
            r#"{"ID":"def456","Names":"/myapp","Image":"myapp:v1","Status":"Exited (0) 1 hour ago","State":"exited","Ports":""}"#
        )
    });

    let output = render_container_list_markdown(&data);

    // §3.1: Title must be plain text "Docker Containers"
    assert!(
        output.starts_with("Docker Containers"),
        "must start with 'Docker Containers'"
    );

    // §3.2: Summary line with count
    assert!(
        output.contains("Showing 2 containers"),
        "must show container count"
    );

    // §3.3: Legend for mixed states
    assert!(
        output.contains("Legend:"),
        "must have legend for mixed states"
    );
    assert!(output.contains("running"), "must mention running in legend");
    assert!(output.contains("stopped"), "must mention stopped in legend");

    // Table header present
    assert!(
        output.contains("Container"),
        "must have Container column header"
    );
    assert!(output.contains("Status"), "must have Status column header");
    assert!(output.contains("Image"), "must have Image column header");

    // Data present
    assert!(output.contains("nginx"), "must contain nginx container");
    assert!(output.contains("myapp"), "must contain myapp container");

    // Running containers have ● symbol
    assert!(output.contains('●'), "running container must have ● symbol");
    // Stopped containers have ○ symbol
    assert!(output.contains('○'), "stopped container must have ○ symbol");
}

#[test]
fn container_list_unavailable_docker() {
    let data = json!({
        "available": false,
        "error": "docker command not found"
    });
    let output = render_container_list_markdown(&data);
    assert!(output.contains("Docker Containers"));
    assert!(output.contains("✗"));
    assert!(output.contains("docker command not found"));
}

#[test]
fn container_list_empty() {
    let data = json!({
        "available": true,
        "stdout": ""
    });
    let output = render_container_list_markdown(&data);
    assert!(output.contains("No containers found"));
}

// ──────────────────────────────────────────────
// Docker df — validation gate (B4 spec)
// ──────────────────────────────────────────────

#[test]
fn docker_df_basic_structure() {
    let data = json!({
        "available": true,
        "stdout": concat!(
            r#"{"Type":"Images","Total":5,"Active":3,"Size":"1.23GB","Reclaimable":"800MB"}"#,
            "\n",
            r#"{"Type":"Containers","Total":3,"Active":2,"Size":"10MB","Reclaimable":"5MB"}"#,
            "\n",
            r#"{"Type":"Volumes","Total":2,"Active":1,"Size":"50MB","Reclaimable":"0B"}"#
        )
    });

    let output = render_docker_df_markdown(&data);

    // §3.1: Title
    assert!(
        output.starts_with("Docker Disk Usage"),
        "must start with 'Docker Disk Usage'"
    );

    // Table structure — column order matches synapse-mcp
    assert!(
        output.contains("| Type | Count | Size | Reclaimable |"),
        "must have correct column headers"
    );

    // Data present in correct columns
    assert!(output.contains("Images"), "must contain Images type");
    assert!(
        output.contains("Containers"),
        "must contain Containers type"
    );
    assert!(output.contains("Volumes"), "must contain Volumes type");
}

#[test]
fn docker_df_unavailable() {
    let data = json!({
        "available": false,
        "stderr": "daemon not running"
    });
    let output = render_docker_df_markdown(&data);
    assert!(output.contains("Docker Disk Usage"));
    assert!(output.contains("✗"));
}

// ──────────────────────────────────────────────
// Host resources — validation gate (B4 spec)
// ──────────────────────────────────────────────

#[test]
fn host_resources_basic_structure() {
    let data = json!({
        "host": "squirts",
        "cpu_cores": 8,
        "cpu_percent": 45.5,
        "mem_used_mb": 8192,
        "mem_total_mb": 16384,
        "mem_percent": 50.0,
        "load_1m": 1.2,
        "load_5m": 1.5,
        "load_15m": 1.8,
        "disk": [
            {"mount": "/", "used_gb": 50.0, "total_gb": 100.0, "percent": 50.0},
            {"mount": "/data", "used_gb": 900.0, "total_gb": 1000.0, "percent": 90.0}
        ]
    });

    let output = render_host_resources_markdown(&data);

    // §3.1: Title
    assert!(
        output.starts_with("Host Resources"),
        "must start with 'Host Resources'"
    );

    // §3.6: Freshness timestamp
    assert!(
        output.contains("As of (UTC):"),
        "must have freshness timestamp"
    );

    // Host section header
    assert!(
        output.contains("### squirts"),
        "must have host section header"
    );

    // CPU data — column matches synapse-mcp
    assert!(output.contains("CPU"), "must have CPU field");
    assert!(output.contains("8 cores"), "must show core count");

    // Memory data
    assert!(output.contains("Memory"), "must have Memory field");
    assert!(output.contains("8192 MB"), "must show used memory");

    // Load average
    assert!(output.contains("Load"), "must have Load field");

    // Disk section
    assert!(output.contains("Disks:"), "must have Disks section");
    assert!(output.contains("/data"), "must show /data mount");

    // §10.2: Warning threshold for disk >85%
    assert!(output.contains("⚠"), "must warn for /data at 90%");
}

#[test]
fn host_resources_cpu_warning_at_threshold() {
    let data = json!({
        "host": "test",
        "cpu_percent": 91.0,
        "mem_percent": 50.0,
        "mem_used_mb": 4096,
        "mem_total_mb": 8192,
        "load_1m": 0.5,
        "load_5m": 0.5,
        "load_15m": 0.5
    });
    let output = render_host_resources_markdown(&data);
    // §10.2: CPU warning at >90%
    assert!(output.contains("⚠"), "must warn for CPU at 91%");
}

#[test]
fn host_resources_no_warning_below_threshold() {
    let data = json!({
        "host": "test",
        "cpu_percent": 50.0,
        "mem_percent": 50.0,
        "mem_used_mb": 4096,
        "mem_total_mb": 8192,
        "load_1m": 0.5,
        "load_5m": 0.5,
        "load_15m": 0.5
    });
    let output = render_host_resources_markdown(&data);
    assert!(!output.contains("⚠"), "must not warn below threshold");
}

// ──────────────────────────────────────────────
// Docker info
// ──────────────────────────────────────────────

#[test]
fn docker_info_basic() {
    let data = json!({
        "available": true,
        "stdout": r#"{"ServerVersion":"24.0.5","OSType":"linux","Architecture":"x86_64","KernelVersion":"6.1.0","NCPU":8,"MemTotal":17179869184,"Driver":"overlay2","ContainersRunning":5,"Containers":8,"Images":12}"#
    });
    let output = render_docker_info_markdown(&data);
    assert!(output.starts_with("Docker System Info"));
    assert!(output.contains("24.0.5"));
    assert!(output.contains("linux"));
    assert!(output.contains("overlay2"));
    assert!(output.contains("5 running / 8 total"));
    assert!(output.contains("Images: 12"));
}

// ──────────────────────────────────────────────
// Docker images
// ──────────────────────────────────────────────

#[test]
fn docker_images_basic() {
    let data = json!({
        "available": true,
        "stdout": concat!(
            r#"{"ID":"sha256:abc123def456","Repository":"nginx","Tag":"latest","Size":"142MB"}"#,
            "\n",
            r#"{"ID":"sha256:fedcba654321","Repository":"postgres","Tag":"15","Size":"370MB"}"#
        )
    });
    let output = render_docker_images_markdown(&data);
    assert!(output.starts_with("Docker Images"));
    assert!(output.contains("Showing 2 images"));
    assert!(output.contains("nginx:latest"));
    assert!(output.contains("postgres:15"));
    // Columns: ID | Repository:Tag | Size
    assert!(output.contains("| ID | Repository:Tag | Size |"));
}

// ──────────────────────────────────────────────
// Docker networks
// ──────────────────────────────────────────────

#[test]
fn docker_networks_basic() {
    let data = json!({
        "available": true,
        "stdout": concat!(
            r#"{"Name":"bridge","Driver":"bridge","Scope":"local"}"#,
            "\n",
            r#"{"Name":"host","Driver":"host","Scope":"local"}"#
        )
    });
    let output = render_docker_networks_markdown(&data);
    assert!(output.starts_with("Docker Networks"));
    assert!(output.contains("Showing 2 networks"));
    assert!(output.contains("bridge (bridge, local)"));
    assert!(output.contains("host (host, local)"));
}

// ──────────────────────────────────────────────
// Docker volumes
// ──────────────────────────────────────────────

#[test]
fn docker_volumes_named_and_anonymous() {
    let data = json!({
        "available": true,
        "stdout": concat!(
            r#"{"Name":"myapp_data","Driver":"local"}"#,
            "\n",
            // 64-char hex = anonymous
            r#"{"Name":"a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2","Driver":"local"}"#
        )
    });
    let output = render_docker_volumes_markdown(&data);
    assert!(output.starts_with("Docker Volumes"));
    assert!(output.contains("named: 1, anonymous: 1"));
    assert!(output.contains("myapp_data"));
    // Anonymous volumes are truncated
    assert!(
        output.contains("(anon)"),
        "anonymous volumes must be truncated"
    );
}

// ──────────────────────────────────────────────
// Container inspect
// ──────────────────────────────────────────────

#[test]
fn container_inspect_basic() {
    let data = json!({
        "available": true,
        "stdout": r#"[{"Name":"/nginx","State":{"Status":"running","Running":true,"StartedAt":"2024-01-01T00:00:00Z"},"RestartCount":0,"Config":{"Image":"nginx:latest","Cmd":["/usr/sbin/nginx"],"WorkingDir":"/","Env":["PATH=/usr/local/sbin:/usr/local/bin"]},"Mounts":[],"NetworkSettings":{"Ports":{},"Networks":{}}}]"#
    });
    let output = render_container_inspect_markdown(&data);
    assert!(output.contains("Container: nginx"));
    assert!(output.contains("**State**"));
    assert!(output.contains("Status: running"));
    assert!(output.contains("**Configuration**"));
    assert!(output.contains("Image: nginx:latest"));
}

#[test]
fn container_inspect_redacts_sensitive_env() {
    let data = json!({
        "available": true,
        "stdout": r#"[{"Name":"/myapp","State":{"Status":"running","Running":true,"StartedAt":"2024-01-01T00:00:00Z"},"RestartCount":0,"Config":{"Image":"myapp:v1","Cmd":[],"WorkingDir":"/","Env":["DB_PASSWORD=secret123","PUBLIC_VAR=hello"]},"Mounts":[],"NetworkSettings":{"Ports":{},"Networks":{}}}]"#
    });
    let output = render_container_inspect_markdown(&data);
    assert!(
        !output.contains("secret123"),
        "sensitive env values must be redacted"
    );
    assert!(
        output.contains("DB_PASSWORD=****"),
        "must show redacted key"
    );
    assert!(
        output.contains("PUBLIC_VAR=hello"),
        "non-sensitive env must be shown"
    );
}

// ──────────────────────────────────────────────
// Container logs
// ──────────────────────────────────────────────

#[test]
fn container_logs_short() {
    let data = json!({
        "available": true,
        "container": "nginx",
        "stdout": "line1\nline2\nline3"
    });
    let output = render_container_logs_markdown(&data);
    assert!(output.contains("Container Logs for nginx"));
    assert!(output.contains("line1"));
    assert!(output.contains("line2"));
    assert!(output.contains("line3"));
    assert!(
        !output.contains("Preview"),
        "short logs should not use preview format"
    );
}

#[test]
fn container_logs_preview_for_long_output() {
    let many_lines: Vec<String> = (1..=20).map(|i| format!("log line {i}")).collect();
    let data = json!({
        "available": true,
        "container": "nginx",
        "stdout": many_lines.join("\n")
    });
    let output = render_container_logs_markdown(&data);
    assert!(
        output.contains("Preview"),
        "long logs should use preview format"
    );
    assert!(output.contains("first 5"), "should show first 5 lines");
    assert!(output.contains("last 5"), "should show last 5 lines");
}

// ──────────────────────────────────────────────
// Host status
// ──────────────────────────────────────────────

#[test]
fn host_status_mixed_states() {
    let data = json!([
        {"name": "squirts", "connected": true, "container_count": 10, "running_count": 8},
        {"name": "boops", "connected": false, "container_count": 0, "running_count": 0, "error": "Timeout"}
    ]);
    let output = render_host_status_markdown(&data);
    assert!(output.starts_with("Homelab Host Status"));
    assert!(output.contains("Hosts: 2 | Online: 1 | Offline: 1"));
    assert!(output.contains("Legend:"));
    // Offline host appears first (severity-first)
    let boops_pos = output.find("boops").unwrap();
    let squirts_pos = output.find("squirts").unwrap();
    assert!(
        boops_pos < squirts_pos,
        "offline host must appear before online host (severity-first)"
    );
    assert!(
        output.contains("● Online"),
        "online host must have ● symbol"
    );
    assert!(
        output.contains("○ Offline"),
        "offline host must have ○ symbol"
    );
    assert!(output.contains("Timeout"), "error message must be shown");
}

// ──────────────────────────────────────────────
// Scout formatters
// ──────────────────────────────────────────────

#[test]
fn scout_nodes_basic() {
    let data = json!({
        "hosts": [
            {"name": "squirts", "host": "squirts.local", "protocol": "ssh"},
            {"name": "boops", "host": "boops.local", "protocol": "ssh"}
        ]
    });
    let output = render_scout_nodes_markdown(&data);
    assert!(output.starts_with("Scout Nodes"));
    assert!(output.contains("Hosts: 2"));
    assert!(output.contains("squirts"));
    assert!(output.contains("boops"));
    // Table format
    assert!(output.contains("| Host |"));
}

#[test]
fn scout_peek_file() {
    let data = json!({
        "host": "squirts",
        "path": "/etc/hostname",
        "kind": "file",
        "content": "squirts\n"
    });
    let output = render_scout_peek_markdown(&data);
    assert!(output.contains("File Read: squirts:/etc/hostname"));
    assert!(output.contains("Size:"));
    assert!(output.contains("squirts"));
    assert!(output.contains("```"), "must have code block");
}

#[test]
fn scout_peek_directory() {
    let data = json!({
        "host": "squirts",
        "path": "/etc",
        "kind": "directory",
        "entries": ["hostname", "hosts", "passwd"]
    });
    let output = render_scout_peek_markdown(&data);
    assert!(output.contains("Directory Listing: squirts:/etc"));
    assert!(output.contains("Items: 3"));
    assert!(output.contains("hostname"));
}

#[test]
fn scout_exec_success() {
    let data = json!({
        "host": "squirts",
        "path": "/tmp",
        "command": "uptime",
        "exit_code": 0,
        "stdout": " 15:23:45 up 3 days, 2:15, 1 user",
        "stderr": ""
    });
    let output = render_scout_exec_markdown(&data);
    assert!(output.starts_with('✓'), "success must use ✓ symbol");
    assert!(output.contains("Command Execution: squirts:/tmp"));
    assert!(output.contains("Exit: 0"));
    assert!(output.contains("uptime"));
    assert!(
        output.contains("As of (UTC):"),
        "must have freshness timestamp"
    );
}

#[test]
fn scout_exec_failure() {
    let data = json!({
        "host": "squirts",
        "path": "/tmp",
        "command": "cat /nonexistent",
        "exit_code": 1,
        "stdout": "",
        "stderr": "No such file or directory"
    });
    let output = render_scout_exec_markdown(&data);
    assert!(output.starts_with('✗'), "failure must use ✗ symbol");
    assert!(output.contains("Exit: 1"));
}

#[test]
fn scout_syslog_basic() {
    let data = json!({
        "host": "squirts",
        "lines_requested": 50,
        "logs": "Feb 13 11:00:00 squirts sshd: ok\nFeb 13 11:00:01 squirts kernel: info"
    });
    let output = render_scout_syslog_markdown(&data);
    assert!(output.starts_with("Syslog: squirts"));
    assert!(output.contains("Lines requested: 50 | Returned: 2"));
    assert!(output.contains("As of (UTC):"));
    assert!(output.contains("sshd"));
}

#[test]
fn scout_zfs_pools_annotates_health() {
    let data = json!({
        "host": "squirts",
        "pools": "NAME    SIZE   ALLOC  FREE    HEALTH  ALTROOT\ntank    10.9T  8.2T   2.7T    ONLINE  -\nbad_pool 1T 0.5T 0.5T DEGRADED -"
    });
    let output = render_scout_zfs_pools_markdown(&data);
    assert!(output.starts_with("ZFS Pools: squirts"));
    assert!(output.contains('●'), "ONLINE pool must have ● symbol");
    assert!(output.contains('⚠'), "DEGRADED pool must have ⚠ symbol");
}

// ──────────────────────────────────────────────
// Compose formatters
// ──────────────────────────────────────────────

#[test]
fn compose_list_basic() {
    let data = json!({
        "host": "local",
        "projects": [
            {"name": "myapp", "status": "running", "services": [{"name": "web"}, {"name": "db"}]},
            {"name": "oldapp", "status": "exited", "service_count": 1}
        ]
    });
    let output = render_compose_list_markdown(&data);
    assert!(output.starts_with("Docker Compose Stacks on local"));
    assert!(output.contains("running: 1"));
    assert!(output.contains("stopped: 1") || output.contains("exited: 1"));
    assert!(output.contains("myapp"));
    assert!(output.contains("oldapp"));
    assert!(output.contains('●'), "running project must have ● symbol");
    assert!(output.contains('○'), "stopped project must have ○ symbol");
}

#[test]
fn compose_status_services() {
    let data = json!({
        "name": "myapp",
        "status": "running",
        "services": [
            {"name": "web", "status": "running", "health": "healthy"},
            {"name": "worker", "status": "exited", "health": "—"}
        ]
    });
    let output = render_compose_status_markdown(&data);
    assert!(output.starts_with("Compose Stack: myapp"));
    assert!(output.contains("Services: 2"));
    assert!(output.contains("web"));
    assert!(output.contains("worker"));
}

#[test]
fn compose_up_markdown() {
    let data = json!({
        "project": "myapp",
        "host": "local",
        "services_started": 3
    });
    let output = render_compose_up_markdown(&data);
    assert!(output.starts_with("Compose Up for myapp on local"));
    assert!(output.contains("Services started: 3"));
    assert!(output.contains('●'));
}

#[test]
fn compose_down_markdown() {
    let data = json!({
        "project": "myapp",
        "host": "local",
        "services_stopped": 2
    });
    let output = render_compose_down_markdown(&data);
    assert!(output.starts_with("Compose Down for myapp on local"));
    assert!(output.contains("Services stopped: 2"));
    assert!(output.contains('○'));
}

// ──────────────────────────────────────────────
// format_bytes helper
// ──────────────────────────────────────────────

#[test]
fn format_bytes_ranges() {
    use crate::formatters::format_bytes;
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    assert_eq!(format_bytes(1024_u64.pow(4)), "1.0 TB");
}

// ──────────────────────────────────────────────
// truncate helper
// ──────────────────────────────────────────────

#[test]
fn truncate_short_string_unchanged() {
    use crate::formatters::truncate;
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn truncate_long_string_gets_ellipsis() {
    use crate::formatters::truncate;
    let result = truncate("hello world", 7);
    assert!(
        result.ends_with('…'),
        "truncated string must end with ellipsis"
    );
    assert!(
        result.chars().count() <= 7,
        "truncated string must be within limit"
    );
}

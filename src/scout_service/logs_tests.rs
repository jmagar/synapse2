//! Unit tests for scout log operations.

use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::HostConfig;
use async_trait::async_trait;

// ─── Mock executors ───────────────────────────────────────────────────────────

struct FixedExec {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

impl FixedExec {
    fn ok(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
        }
    }

    #[allow(dead_code)]
    fn err_no_such_file() -> Self {
        Self {
            stdout: String::new(),
            stderr: "tail: cannot open '/var/log/syslog' for reading: No such file or directory"
                .to_owned(),
            exit_code: Some(1),
        }
    }

    fn err_permission() -> Self {
        Self {
            stdout: String::new(),
            stderr: "dmesg: read kernel buffer failed: Operation not permitted".to_owned(),
            exit_code: Some(1),
        }
    }
}

#[async_trait]
impl SshExecutor for FixedExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        _program: &str,
        _args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: self.exit_code,
        })
    }
}

/// Mock that returns different outputs depending on which path is requested.
struct FallbackExec {
    primary_path: &'static str,
    primary_fails: bool,
    fallback_stdout: String,
}

impl FallbackExec {
    fn syslog_fallback_to_messages(content: &str) -> Self {
        Self {
            primary_path: "/var/log/syslog",
            primary_fails: true,
            fallback_stdout: content.to_owned(),
        }
    }
    fn auth_fallback_to_secure(content: &str) -> Self {
        Self {
            primary_path: "/var/log/auth.log",
            primary_fails: true,
            fallback_stdout: content.to_owned(),
        }
    }
}

#[async_trait]
impl SshExecutor for FallbackExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        _program: &str,
        args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        // Check if the last arg (the log path) matches primary
        let path_arg = args.last().copied().unwrap_or("");
        if self.primary_fails && path_arg == self.primary_path {
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "tail: cannot open '{}' for reading: No such file or directory",
                    self.primary_path
                ),
                exit_code: Some(1),
            });
        }
        Ok(CommandOutput {
            stdout: self.fallback_stdout.clone(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

fn ssh_host() -> HostConfig {
    HostConfig {
        name: "test-host".to_owned(),
        host: "192.168.1.1".to_owned(),
        port: None,
        protocol: crate::synapse::HostProtocol::Ssh,
        ssh_user: Some("root".to_owned()),
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: Vec::new(),
        scout_read_roots: Vec::new(),
        exec_allowlist: Vec::new(),
    }
}

const SYSLOG_SAMPLE: &str = "\
May 29 12:00:01 dookie kernel: [    0.000000] Linux version 7.0.0
May 29 12:00:02 dookie systemd[1]: Starting Network Time Synchronization
May 29 12:00:03 dookie sshd[1234]: Accepted publickey for jmagar
";

const JOURNAL_SAMPLE: &str = "\
May 29 12:00:00 dookie kernel: systemd[1]: Started
May 29 12:00:01 dookie sshd[1234]: Accepted publickey
May 29 12:00:02 dookie nginx[5678]: 127.0.0.1 - GET /health
";

const DMESG_SAMPLE: &str = "\
[    0.000000] Linux version 7.0.0
[    0.000001] Command line: BOOT_IMAGE=/vmlinuz
[    0.000002] BIOS-provided physical RAM map
[    0.000003] ACPI: RSDP 0x00000000000F05B0
[    1.234567] eth0: renamed from veth1234abc
";

const AUTH_SAMPLE: &str = "\
May 29 12:00:01 dookie sshd[1234]: Accepted publickey for jmagar from 192.168.1.10
May 29 12:00:02 dookie sudo[5678]: jmagar : TTY=pts/0 ; PWD=/home/jmagar ; USER=root
";

// ─── syslog tests ────────────────────────────────────────────────────────────

#[test]
fn syslog_returns_structured_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(SYSLOG_SAMPLE);

    let result = rt.block_on(super::syslog(&host, &exec, 100, None)).unwrap();
    assert_eq!(result["subaction"], "syslog");
    assert_eq!(result["host"], "test-host");
    assert_eq!(result["lines"], 100);
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("Linux version"), "output: {output}");
}

#[test]
fn syslog_applies_grep_filter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(SYSLOG_SAMPLE);

    let result = rt
        .block_on(super::syslog(&host, &exec, 100, Some("sshd")))
        .unwrap();
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("sshd"), "should contain sshd: {output}");
    assert!(
        !output.contains("kernel"),
        "should not contain non-matching kernel lines: {output}"
    );
}

#[test]
fn syslog_falls_back_to_messages() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FallbackExec::syslog_fallback_to_messages(SYSLOG_SAMPLE);

    let result = rt.block_on(super::syslog(&host, &exec, 100, None)).unwrap();
    assert_eq!(result["subaction"], "syslog");
    let output = result["output"].as_str().unwrap();
    assert!(
        output.contains("Linux version"),
        "should have fallback output: {output}"
    );
}

#[test]
fn syslog_clamps_lines_to_max() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(SYSLOG_SAMPLE);

    let result = rt
        .block_on(super::syslog(&host, &exec, 9999, None))
        .unwrap();
    assert_eq!(result["lines"], super::MAX_LINES);
}

// ─── journal tests ───────────────────────────────────────────────────────────

#[test]
fn journal_returns_structured_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(JOURNAL_SAMPLE);

    let result = rt
        .block_on(super::journal(
            &host, &exec, 100, None, None, None, None, None,
        ))
        .unwrap();
    assert_eq!(result["subaction"], "journal");
    assert_eq!(result["lines"], 100);
}

#[test]
fn journal_applies_grep_filter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(JOURNAL_SAMPLE);

    let result = rt
        .block_on(super::journal(
            &host,
            &exec,
            100,
            None,
            None,
            None,
            None,
            Some("nginx"),
        ))
        .unwrap();
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("nginx"), "should contain nginx: {output}");
    assert!(!output.contains("kernel"), "should filter out non-matches");
}

#[test]
fn journal_with_unit_filter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(JOURNAL_SAMPLE);

    let result = rt
        .block_on(super::journal(
            &host,
            &exec,
            50,
            Some("sshd"),
            None,
            None,
            None,
            None,
        ))
        .unwrap();
    assert_eq!(result["unit"], "sshd");
}

#[test]
fn journal_with_since_until_priority() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(JOURNAL_SAMPLE);

    let result = rt
        .block_on(super::journal(
            &host,
            &exec,
            50,
            None,
            Some("err"),
            Some("2026-05-29 00:00:00"),
            Some("2026-05-29 23:59:59"),
            None,
        ))
        .unwrap();
    assert_eq!(result["priority"], "err");
    assert_eq!(result["since"], "2026-05-29 00:00:00");
    assert_eq!(result["until"], "2026-05-29 23:59:59");
}

// ─── dmesg tests ─────────────────────────────────────────────────────────────

#[test]
fn dmesg_returns_structured_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(DMESG_SAMPLE);

    let result = rt.block_on(super::dmesg(&host, &exec, 100, None)).unwrap();
    assert_eq!(result["subaction"], "dmesg");
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("Linux version"), "output: {output}");
}

#[test]
fn dmesg_tails_to_line_limit() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(DMESG_SAMPLE);

    // DMESG_SAMPLE has 5 lines; request only 2
    let result = rt.block_on(super::dmesg(&host, &exec, 2, None)).unwrap();
    let output = result["output"].as_str().unwrap();
    let line_count = output.lines().count();
    assert_eq!(line_count, 2, "should tail to 2 lines, got {line_count}");
}

#[test]
fn dmesg_applies_grep_then_tail() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(DMESG_SAMPLE);

    let result = rt
        .block_on(super::dmesg(&host, &exec, 10, Some("ACPI")))
        .unwrap();
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("ACPI"), "should contain ACPI line");
    assert!(
        !output.contains("Linux version"),
        "should filter out others"
    );
}

#[test]
fn dmesg_returns_permission_error_gracefully() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::err_permission();

    let result = rt.block_on(super::dmesg(&host, &exec, 100, None)).unwrap();
    // Should NOT error — returns structured response
    assert_eq!(result["error"], "permission_required");
    assert!(result["help"].as_str().unwrap().contains("CAP_SYSLOG"));
}

// ─── auth tests ──────────────────────────────────────────────────────────────

#[test]
fn auth_returns_structured_output() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(AUTH_SAMPLE);

    let result = rt.block_on(super::auth(&host, &exec, 100, None)).unwrap();
    assert_eq!(result["subaction"], "auth");
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("publickey"), "output: {output}");
}

#[test]
fn auth_falls_back_to_secure() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FallbackExec::auth_fallback_to_secure(AUTH_SAMPLE);

    let result = rt.block_on(super::auth(&host, &exec, 100, None)).unwrap();
    assert_eq!(result["subaction"], "auth");
    let output = result["output"].as_str().unwrap();
    assert!(
        output.contains("publickey"),
        "should have fallback output: {output}"
    );
}

#[test]
fn auth_applies_grep_filter() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = ssh_host();
    let exec = FixedExec::ok(AUTH_SAMPLE);

    let result = rt
        .block_on(super::auth(&host, &exec, 100, Some("sudo")))
        .unwrap();
    let output = result["output"].as_str().unwrap();
    assert!(output.contains("sudo"), "should match sudo line");
    assert!(!output.contains("publickey"), "should filter sshd lines");
}

// ─── apply_grep helper ────────────────────────────────────────────────────────

#[test]
fn apply_grep_no_filter_returns_all() {
    let text = "line1\nline2\nline3".to_owned();
    let result = super::apply_grep(text.clone(), None);
    assert_eq!(result, text);
}

#[test]
fn apply_grep_filters_matching_lines() {
    let text = "line1 foo\nline2 bar\nline3 foo".to_owned();
    let result = super::apply_grep(text, Some("foo"));
    assert_eq!(result, "line1 foo\nline3 foo");
}

#[test]
fn apply_grep_empty_pattern_returns_all() {
    let text = "line1\nline2".to_owned();
    let result = super::apply_grep(text.clone(), Some(""));
    assert_eq!(result, text);
}

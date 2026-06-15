//! Unit tests for scout exec/emit/beam operations.
//!
//! Tests:
//! - `exec` rejects non-allowlisted commands (e.g. `rm`, `git`)
//! - `exec` rejects when confirmer declines
//! - `emit` fanout returns partial success on mixed outcomes
//! - path validation: relative path + `..` rejected

use std::sync::Arc;

use crate::elicitation_gate::{ConfirmationDenied, Confirmer};
use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::HostConfig;
use anyhow::Result;
use async_trait::async_trait;

// ─── Mock SSH executor ───────────────────────────────────────────────────────

/// Always succeeds with empty output.
struct AlwaysOkExec;

#[async_trait]
impl SshExecutor for AlwaysOkExec {
    async fn exec(&self, _: &HostConfig, _: &str, _: &[&str]) -> Result<CommandOutput> {
        Ok(CommandOutput {
            stdout: "ok".to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

/// Always fails with a canned error.
struct AlwaysFailExec;

#[async_trait]
impl SshExecutor for AlwaysFailExec {
    async fn exec(&self, _: &HostConfig, _: &str, _: &[&str]) -> Result<CommandOutput> {
        anyhow::bail!("ssh error")
    }
}

// ─── Mock confirmers ─────────────────────────────────────────────────────────

/// Always approves.
struct ApproveConfirmer;

#[async_trait]
impl Confirmer for ApproveConfirmer {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Ok(())
    }
}

/// Always declines.
struct DenyConfirmer;

#[async_trait]
impl Confirmer for DenyConfirmer {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Err(ConfirmationDenied::Declined)
    }
}

// ─── exec tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn exec_rejects_rm_command() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> =
        super::exec(&host, &AlwaysOkExec, &ApproveConfirmer, "rm", &[], None).await;
    assert!(result.is_err(), "rm must be rejected by allowlist");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("denied") || msg.contains("allowlist") || msg.contains("not allowlist"),
        "{msg}"
    );
}

#[tokio::test]
async fn exec_rejects_git_command() {
    // git was removed from ALLOWED_READ_COMMANDS by B0 security review.
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> =
        super::exec(&host, &AlwaysOkExec, &ApproveConfirmer, "git", &[], None).await;
    assert!(
        result.is_err(),
        "git must be rejected (removed from allowlist by B0)"
    );
}

#[tokio::test]
async fn exec_rejects_when_confirmer_declines() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> = super::exec(
        &host,
        &AlwaysOkExec,
        &DenyConfirmer,
        "cat", // cat IS allowlisted
        &[],
        None,
    )
    .await;
    assert!(result.is_err(), "declined confirmation must produce error");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("declined") || msg.contains("Declined"),
        "{msg}"
    );
}

#[tokio::test]
async fn exec_rejects_relative_path() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> = super::exec(
        &host,
        &AlwaysOkExec,
        &ApproveConfirmer,
        "cat",
        &[],
        Some("relative/path"), // non-absolute
    )
    .await;
    assert!(result.is_err(), "relative path must be rejected");
}

#[tokio::test]
async fn exec_rejects_dotdot_path() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> = super::exec(
        &host,
        &AlwaysOkExec,
        &ApproveConfirmer,
        "cat",
        &[],
        Some("/tmp/../etc"),
    )
    .await;
    assert!(result.is_err(), "path with .. must be rejected");
}

#[tokio::test]
async fn exec_rejects_non_allowlisted_command() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> = super::exec(
        &host,
        &AlwaysOkExec,
        &ApproveConfirmer,
        "myspecialcommand",
        &[],
        None,
    )
    .await;
    assert!(result.is_err(), "unlisted command must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("allowlist") || msg.contains("not allowlist") || msg.contains("denied"),
        "{msg}"
    );
}

// ─── emit tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn emit_empty_targets_is_error() {
    let result: anyhow::Result<serde_json::Value> = super::emit(
        &[],
        Arc::new(AlwaysOkExec),
        &ApproveConfirmer,
        "cat",
        &[],
        None,
    )
    .await;
    assert!(result.is_err(), "empty targets must be rejected");
}

#[tokio::test]
async fn emit_rejects_when_confirmer_declines() {
    let host = HostConfig::local();
    let target = super::EmitTarget {
        host: host.clone(),
        path: None,
    };
    let result: anyhow::Result<serde_json::Value> = super::emit(
        &[target],
        Arc::new(AlwaysOkExec),
        &DenyConfirmer,
        "cat",
        &[],
        None,
    )
    .await;
    assert!(result.is_err(), "declined emit must produce error");
}

#[tokio::test]
async fn emit_rejects_non_allowlisted_command() {
    let host = HostConfig::local();
    let target = super::EmitTarget {
        host: host.clone(),
        path: None,
    };
    let result: anyhow::Result<serde_json::Value> = super::emit(
        &[target],
        Arc::new(AlwaysOkExec),
        &ApproveConfirmer,
        "bash",
        &[],
        None,
    )
    .await;
    assert!(result.is_err(), "bash must be rejected by allowlist");
}

#[tokio::test]
async fn emit_returns_partial_success_on_mixed() {
    // One local host (runs cat locally — succeeds) + one SSH-protocol host
    // (uses AlwaysFailExec — fails). This should produce PartialSuccess.
    let mut host_ok = HostConfig::local();
    host_ok.name = "host-ok".into();

    let mut ssh_host = HostConfig::local();
    ssh_host.name = "ssh-remote".into();
    ssh_host.protocol = crate::synapse::HostProtocol::Ssh;
    ssh_host.host = "nonexistent.host".into();

    let targets = vec![
        super::EmitTarget {
            host: host_ok,
            path: None,
        },
        super::EmitTarget {
            host: ssh_host,
            path: None,
        },
    ];

    // AlwaysFailExec causes the SSH host to error; local host runs cat natively.
    let result: serde_json::Value = super::emit(
        &targets,
        Arc::new(AlwaysFailExec),
        &ApproveConfirmer,
        "cat",
        &[],
        Some(5),
    )
    .await
    .expect("emit should not error — partial_success is a valid return");

    // Exactly partial_success: one ok (local cat), one fail (SSH exec error).
    let status = result["status"].as_str().unwrap_or("");
    assert_eq!(
        status, "partial_success",
        "expected partial_success, got: {result}"
    );
    assert_eq!(result["succeeded"], 1u64, "one local host succeeded");
    assert_eq!(result["failed"], 1u64, "one SSH host failed");
    assert!(result["results"].is_array(), "results must be an array");
}

#[tokio::test]
async fn emit_local_target_path_sets_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = dir.path().to_string_lossy().into_owned();
    let target = super::EmitTarget {
        host: HostConfig::local(),
        path: Some(target_path.clone()),
    };

    let result: serde_json::Value = super::emit(
        &[target],
        Arc::new(AlwaysOkExec),
        &ApproveConfirmer,
        "pwd",
        &[],
        Some(5),
    )
    .await
    .expect("local emit with path should succeed");

    assert_eq!(result["status"], "all_ok");
    let result_entry = &result["results"][0]["result"];
    assert_eq!(result_entry["path"], target_path);
    assert_eq!(result_entry["stdout"].as_str().unwrap().trim(), target_path);
}

#[tokio::test]
async fn emit_remote_target_path_is_explicit_error() {
    let mut host = HostConfig::local();
    host.name = "ssh-remote".into();
    host.host = "remote.example".into();
    host.protocol = crate::synapse::HostProtocol::Ssh;
    let target = super::EmitTarget {
        host,
        path: Some("/tmp".into()),
    };

    let result: serde_json::Value = super::emit(
        &[target],
        Arc::new(AlwaysOkExec),
        &ApproveConfirmer,
        "pwd",
        &[],
        Some(5),
    )
    .await
    .expect("remote target path should be reported as a per-host emit failure");

    assert_eq!(result["status"], "all_failed");
    assert_eq!(result["failed"], 1u64);
    let error = result["results"][0]["error"].as_str().unwrap();
    assert!(
        error.contains("only supported for local emit targets"),
        "{error}"
    );
}

#[tokio::test]
async fn emit_rejects_unsafe_target_path_before_confirmation() {
    let target = super::EmitTarget {
        host: HostConfig::local(),
        path: Some("relative".into()),
    };

    let result: anyhow::Result<serde_json::Value> = super::emit(
        &[target],
        Arc::new(AlwaysOkExec),
        &DenyConfirmer,
        "pwd",
        &[],
        Some(5),
    )
    .await;

    assert!(result.is_err(), "unsafe target paths must be rejected");
    assert!(
        result.unwrap_err().to_string().contains("absolute"),
        "path validation should run before confirmation"
    );
}

// ─── SSH identity validation tests (S-M4) ────────────────────────────────────

#[test]
fn validate_ssh_user_accepts_normal_names() {
    assert!(super::validate_ssh_user("root").is_ok());
    assert!(super::validate_ssh_user("jmagar").is_ok());
    assert!(super::validate_ssh_user("deploy-bot").is_ok());
    assert!(super::validate_ssh_user("user.name").is_ok());
    assert!(super::validate_ssh_user("user_name").is_ok());
}

#[test]
fn validate_ssh_user_rejects_proxy_command_injection() {
    // This is the canonical ProxyCommand injection pattern.
    let result = super::validate_ssh_user("-oProxyCommand=evil");
    assert!(result.is_err(), "ProxyCommand injection must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("start with `-`") || msg.contains("invalid"),
        "{msg}"
    );
}

#[test]
fn validate_ssh_user_rejects_at_sign() {
    let result = super::validate_ssh_user("user@host");
    assert!(result.is_err(), "@ in ssh_user must be rejected");
}

#[test]
fn validate_ssh_user_rejects_whitespace() {
    let result = super::validate_ssh_user("user name");
    assert!(result.is_err(), "whitespace in ssh_user must be rejected");
}

#[test]
fn validate_ssh_user_rejects_empty() {
    let result = super::validate_ssh_user("");
    assert!(result.is_err(), "empty ssh_user must be rejected");
}

#[test]
fn validate_ssh_host_accepts_normal_hosts() {
    assert!(super::validate_ssh_host("192.168.1.1").is_ok());
    assert!(super::validate_ssh_host("example.com").is_ok());
    assert!(super::validate_ssh_host("my-server").is_ok());
}

#[test]
fn validate_ssh_host_rejects_colon() {
    // Colon could smuggle [host]:port syntax.
    let result = super::validate_ssh_host("host:22");
    assert!(result.is_err(), "colon in host must be rejected");
}

#[test]
fn validate_ssh_host_rejects_leading_dash() {
    let result = super::validate_ssh_host("-oProxyCommand=evil");
    assert!(result.is_err(), "leading dash host must be rejected");
}

#[tokio::test]
async fn beam_rejects_malicious_ssh_user() {
    // A host config with an ssh_user that contains a ProxyCommand injection
    // attempt must be rejected before scp is launched.
    let mut remote_host = HostConfig::local();
    remote_host.name = "evil-remote".into();
    remote_host.host = "remote.example".into();
    remote_host.protocol = crate::synapse::HostProtocol::Ssh;
    remote_host.ssh_user = Some("-oProxyCommand=id>/tmp/pwned".into());

    let local_host = HostConfig::local();

    let result: anyhow::Result<serde_json::Value> = super::beam(
        &local_host,
        "/tmp/source",
        &remote_host,
        "/tmp/dest",
        &ApproveConfirmer,
    )
    .await;

    assert!(
        result.is_err(),
        "beam must reject malicious ssh_user before launching scp"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("invalid") || msg.contains("start with") || msg.contains("`-`"),
        "error must mention validation failure: {msg}"
    );
}

// ─── beam tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn beam_rejects_when_confirmer_declines() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> =
        super::beam(&host, "/tmp/source", &host, "/tmp/dest", &DenyConfirmer).await;
    assert!(result.is_err(), "declined beam must produce error");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("declined") || msg.contains("Declined"),
        "error must mention declined: {msg}"
    );
}

#[tokio::test]
async fn beam_rejects_relative_source_path() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> = super::beam(
        &host,
        "relative/path",
        &host,
        "/tmp/dest",
        &ApproveConfirmer,
    )
    .await;
    assert!(
        result.is_err(),
        "relative source path must be rejected before confirmation"
    );
}

#[tokio::test]
async fn beam_rejects_dotdot_dest_path() {
    let host = HostConfig::local();
    let result: anyhow::Result<serde_json::Value> = super::beam(
        &host,
        "/tmp/source",
        &host,
        "/tmp/../etc/dest",
        &ApproveConfirmer,
    )
    .await;
    assert!(result.is_err(), "path with .. in dest must be rejected");
}

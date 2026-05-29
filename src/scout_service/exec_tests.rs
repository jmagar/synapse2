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

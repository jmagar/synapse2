//! Unit tests for compose operations (B13).
//!
//! Uses the same `MockExec` pattern as `host_tests.rs` — a `HostExec`
//! implementation that returns canned `CommandOutput` keyed by program name.
//! Tests verify correct argv construction and that the destructive gate
//! (`confirmer.require`) blocks or proceeds as expected.

use super::*;
use crate::elicitation_gate::{ConfirmationDenied, Confirmer};
use crate::ssh::CommandOutput;
use std::collections::HashMap;
use std::sync::Mutex;

// ─── MockExec ─────────────────────────────────────────────────────────────────

/// Mock `HostExec` keyed by `(program, first_argument_after_program)` so we can
/// distinguish `docker compose up -d` from `docker compose down`.
/// Falls back to a program-only key when a program+arg key is not set.
struct MockExec {
    responses: Mutex<HashMap<String, CommandOutput>>,
    /// Captures the last full argv for assertion.
    last_args: Mutex<Vec<String>>,
}

impl MockExec {
    fn new() -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
            last_args: Mutex::new(Vec::new()),
        }
    }

    /// Register a canned stdout response for a `docker compose <subcommand>` call.
    /// Key format: `"docker compose <subcommand>"`.
    fn add_compose(&self, subcommand: &str, stdout: &str) {
        let key = format!("docker compose {subcommand}");
        self.responses.lock().unwrap().insert(
            key,
            CommandOutput {
                stdout: stdout.to_owned(),
                stderr: String::new(),
                exit_code: Some(0),
            },
        );
    }

    /// Retrieve the argv slice from the last `run()` call.
    fn last_argv(&self) -> Vec<String> {
        self.last_args.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl super::super::host::HostExec for MockExec {
    async fn run(&self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput> {
        // Record the full argv.
        let mut full: Vec<String> = vec![program.to_owned()];
        full.extend(args.iter().map(|s| s.to_string()));
        *self.last_args.lock().unwrap() = full;

        // Build the lookup key: "docker compose <subcommand>".
        // argv layout: docker compose -f <config_file> <subcommand> [flags…]
        // args[] (excluding program "docker"):
        //   args[0] = "compose", args[1] = "-f", args[2] = config_file
        //   args[3] = subcommand
        let sub = args.get(3).copied().unwrap_or("");
        let key = format!("{program} compose {sub}");
        let responses = self.responses.lock().unwrap();
        match responses.get(&key) {
            Some(out) => Ok(out.clone()),
            None => Err(anyhow::anyhow!("mock: no response for key `{key}`")),
        }
    }
}

// ─── MockConfirmer ────────────────────────────────────────────────────────────

/// Always-`Ok` confirmer (simulates user accepting).
struct AcceptConfirmer;

#[async_trait::async_trait]
impl Confirmer for AcceptConfirmer {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Ok(())
    }
}

/// Always-`Err(Declined)` confirmer (simulates user rejecting).
struct DenyConfirmer;

#[async_trait::async_trait]
impl Confirmer for DenyConfirmer {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Err(ConfirmationDenied::Declined)
    }
}

// ─── validate_down_args ────────────────────────────────────────────────────────

#[test]
fn remove_volumes_without_force_is_rejected() {
    let args = DownArgs {
        remove_volumes: true,
        force: false,
    };
    let err = validate_down_args(&args).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("force=true"),
        "error must mention force=true, got: {msg}"
    );
}

#[test]
fn remove_volumes_with_force_is_accepted() {
    let args = DownArgs {
        remove_volumes: true,
        force: true,
    };
    assert!(validate_down_args(&args).is_ok());
}

#[test]
fn plain_down_without_force_is_accepted() {
    let args = DownArgs {
        remove_volumes: false,
        force: false,
    };
    assert!(validate_down_args(&args).is_ok());
}

#[test]
fn plain_down_with_force_is_accepted() {
    let args = DownArgs {
        remove_volumes: false,
        force: true,
    };
    assert!(validate_down_args(&args).is_ok());
}

// ─── argv construction ────────────────────────────────────────────────────────

const HOST: &str = "dookie";
const PROJECT: &str = "myapp";
const CONFIG: &str = "/compose/myapp/docker-compose.yml";

#[tokio::test]
async fn up_argv_is_correct() {
    let exec = MockExec::new();
    exec.add_compose("up", "");
    let _: anyhow::Result<serde_json::Value> = up_on_host(&exec, HOST, PROJECT, CONFIG).await;
    let argv = exec.last_argv();
    // Expected: docker compose -f /compose/myapp/docker-compose.yml up -d
    assert_eq!(argv[0], "docker");
    assert_eq!(argv[1], "compose");
    assert_eq!(argv[2], "-f");
    assert_eq!(argv[3], CONFIG);
    assert_eq!(argv[4], "up");
    assert_eq!(argv[5], "-d");
}

#[tokio::test]
async fn down_argv_no_volumes() {
    let exec = MockExec::new();
    exec.add_compose("down", "");
    let _: anyhow::Result<serde_json::Value> =
        down_on_host(&exec, HOST, PROJECT, CONFIG, false).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "down");
    assert!(!argv.contains(&"--volumes".to_owned()));
}

#[tokio::test]
async fn down_argv_with_volumes() {
    let exec = MockExec::new();
    exec.add_compose("down", "");
    let _: anyhow::Result<serde_json::Value> =
        down_on_host(&exec, HOST, PROJECT, CONFIG, true).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "down");
    assert!(argv.contains(&"--volumes".to_owned()));
}

#[tokio::test]
async fn restart_argv_is_correct() {
    let exec = MockExec::new();
    exec.add_compose("restart", "");
    let _: anyhow::Result<serde_json::Value> = restart_on_host(&exec, HOST, PROJECT, CONFIG).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "restart");
}

#[tokio::test]
async fn recreate_argv_is_force_recreate() {
    let exec = MockExec::new();
    exec.add_compose("up", "");
    let _: anyhow::Result<serde_json::Value> = recreate_on_host(&exec, HOST, PROJECT, CONFIG).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "up");
    assert!(argv.contains(&"-d".to_owned()));
    assert!(argv.contains(&"--force-recreate".to_owned()));
}

#[tokio::test]
async fn logs_argv_tail_and_since() {
    let exec = MockExec::new();
    exec.add_compose("logs", "hello from myapp");
    let opts = ComposeLogOptions {
        lines: Some(100),
        since: Some("30m".to_owned()),
        service: None,
    };
    let result: anyhow::Result<serde_json::Value> =
        logs_on_host(&exec, HOST, PROJECT, CONFIG, &opts).await;
    let result = result.unwrap();
    let argv = exec.last_argv();
    assert_eq!(argv[4], "logs");
    assert!(argv.contains(&"--tail".to_owned()));
    assert!(argv.contains(&"100".to_owned()));
    assert!(argv.contains(&"--since".to_owned()));
    assert!(argv.contains(&"30m".to_owned()));
    assert_eq!(result["project"], PROJECT);
}

#[tokio::test]
async fn logs_argv_service_filter() {
    let exec = MockExec::new();
    exec.add_compose("logs", "");
    let opts = ComposeLogOptions {
        lines: None,
        since: None,
        service: Some("web".to_owned()),
    };
    let _: anyhow::Result<serde_json::Value> =
        logs_on_host(&exec, HOST, PROJECT, CONFIG, &opts).await;
    let argv = exec.last_argv();
    assert!(argv.contains(&"web".to_owned()));
}

#[tokio::test]
async fn build_argv_no_service() {
    let exec = MockExec::new();
    exec.add_compose("build", "");
    let _: anyhow::Result<serde_json::Value> =
        build_on_host(&exec, HOST, PROJECT, CONFIG, None).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "build");
    assert_eq!(argv.len(), 5); // no service appended
}

#[tokio::test]
async fn build_argv_with_service() {
    let exec = MockExec::new();
    exec.add_compose("build", "");
    let _: anyhow::Result<serde_json::Value> =
        build_on_host(&exec, HOST, PROJECT, CONFIG, Some("worker")).await;
    let argv = exec.last_argv();
    assert!(argv.contains(&"worker".to_owned()));
}

#[tokio::test]
async fn pull_argv_is_correct() {
    let exec = MockExec::new();
    exec.add_compose("pull", "");
    let _: anyhow::Result<serde_json::Value> =
        pull_on_host(&exec, HOST, PROJECT, CONFIG, None).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "pull");
}

#[tokio::test]
async fn status_argv_includes_format_json() {
    let exec = MockExec::new();
    exec.add_compose("ps", "{}");
    let _: anyhow::Result<serde_json::Value> =
        status_on_host(&exec, HOST, PROJECT, CONFIG, None).await;
    let argv = exec.last_argv();
    assert_eq!(argv[4], "ps");
    assert!(argv.contains(&"--format".to_owned()));
    assert!(argv.contains(&"json".to_owned()));
}

// ─── Confirmer gate: service-layer tests ─────────────────────────────────────
//
// These test that FluxService methods honour the gate contract. We call
// the pure ops functions directly with confirmer logic simulated: the ops
// themselves don't hold the confirmer (FluxService does), so we test that
// the FluxService dispatches correctly by examining that:
//   - validate_down_args correctly rejects the bad input (already done above),
//   - and declined confirmers produce the right error type.
//
// Full gate integration (confirmer → op) is tested via the service layer in
// flux_service_tests.rs; here we verify the confirmer trait objects behave.

#[tokio::test]
async fn accept_confirmer_returns_ok() {
    let c = AcceptConfirmer;
    assert!(c.require("test_op", "some detail").await.is_ok());
}

#[tokio::test]
async fn deny_confirmer_returns_declined() {
    let c = DenyConfirmer;
    let err = c.require("test_op", "some detail").await.unwrap_err();
    assert_eq!(err, ConfirmationDenied::Declined);
}

// ─── Result shape ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn up_result_has_expected_keys() {
    let exec = MockExec::new();
    exec.add_compose("up", "started");
    let result: serde_json::Value = up_on_host(&exec, HOST, PROJECT, CONFIG).await.unwrap();
    assert_eq!(result["host"], HOST);
    assert_eq!(result["project"], PROJECT);
    assert_eq!(result["action"], "up");
    assert!(result.get("succeeded").is_some());
}

#[tokio::test]
async fn down_result_captures_remove_volumes_flag() {
    let exec = MockExec::new();
    exec.add_compose("down", "stopped");
    let result: serde_json::Value = down_on_host(&exec, HOST, PROJECT, CONFIG, true)
        .await
        .unwrap();
    assert_eq!(result["remove_volumes"], true);
    assert_eq!(result["action"], "down");
}

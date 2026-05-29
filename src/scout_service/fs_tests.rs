//! Unit tests for scout filesystem operations (peek/find/delta).

use super::*;
use crate::synapse::HostConfig;

// ─── validate_safe_path tests (security-critical) ────────────────────────────

#[test]
fn peek_rejects_relative_path() {
    // Must be an async test since peek is async, but path validation happens
    // synchronously — we can test validate_safe_path directly.
    let result = crate::synapse::validate_safe_path("relative/path");
    assert!(result.is_err(), "relative path must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("absolute"),
        "error must mention absolute: {msg}"
    );
}

#[test]
fn peek_rejects_dotdot() {
    let result = crate::synapse::validate_safe_path("/tmp/../etc/passwd");
    assert!(result.is_err(), "path with .. must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("traversal") || msg.contains(".."), "{msg}");
}

#[test]
fn peek_rejects_empty_path() {
    let result = crate::synapse::validate_safe_path("");
    assert!(result.is_err(), "empty path must be rejected");
}

// ─── compute_diff tests ───────────────────────────────────────────────────────

#[test]
fn diff_identical_files_is_empty() {
    let d = compute_diff("hello\nworld\n", "hello\nworld\n", "a", "b");
    assert!(d.is_empty(), "identical files should produce empty diff");
}

#[test]
fn diff_different_files_non_empty() {
    let d = compute_diff("hello\n", "world\n", "a", "b");
    assert!(!d.is_empty(), "different files should produce a diff");
    assert!(d.contains("--- a"), "diff should contain source label");
    assert!(d.contains("+++ b"), "diff should contain target label");
}

// ─── delta content limit ──────────────────────────────────────────────────────

#[tokio::test]
async fn delta_rejects_content_over_1mb() {
    // Content size check fires BEFORE any IO (validate_safe_path runs first,
    // then the content-size guard, then the source-file read).
    // The test verifies the size guard fires before the IO path.
    use crate::ssh::{CommandOutput, SshExecutor};
    use async_trait::async_trait;

    struct EchoExec;
    #[async_trait]
    impl SshExecutor for EchoExec {
        async fn exec(
            &self,
            _host: &HostConfig,
            _program: &str,
            _args: &[&str],
        ) -> anyhow::Result<CommandOutput> {
            // If this is called the test has failed: content check should fire first.
            panic!("IO should not be reached when content exceeds 1 MB");
        }
    }

    let host = HostConfig::local();
    let big_content = "x".repeat(DELTA_MAX_CONTENT_BYTES + 1);

    // Use a syntactically valid absolute path. The content-size check runs
    // before read_remote_file, so the source need not exist.
    let result = delta(
        &host,
        &EchoExec,
        "/tmp/synapse2_test_source",
        None,
        None,
        Some(&big_content),
    )
    .await;

    assert!(result.is_err(), "delta must reject content > 1 MB");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("1 MB") || msg.contains("content"),
        "error must mention content limit: {msg}"
    );
}

// ─── find pattern guard ───────────────────────────────────────────────────────

#[test]
fn find_rejects_leading_dash_pattern() {
    // Validate pattern rejection (not async — the check is synchronous inside).
    // We use a runtime to call the async function.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let host = HostConfig::local();

    use crate::ssh::{CommandOutput, SshExecutor};
    use async_trait::async_trait;
    struct NoopExec;
    #[async_trait]
    impl SshExecutor for NoopExec {
        async fn exec(&self, _: &HostConfig, _: &str, _: &[&str]) -> anyhow::Result<CommandOutput> {
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
    }

    let result = rt.block_on(super::find(
        &host,
        &NoopExec,
        "/tmp",
        "-exec rm -rf",
        None,
        None,
    ));
    assert!(result.is_err(), "leading dash pattern must be rejected");
}

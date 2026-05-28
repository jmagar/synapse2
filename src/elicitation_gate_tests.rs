//! Unit tests for the destructive-operation confirmation gate.
//!
//! The decision logic lives in the pure [`map_elicit_outcome`] function, which
//! is testable without a live `Peer<RoleServer>` (which cannot be constructed in
//! a unit test). The `Confirmer` impls that *can* be exercised standalone
//! (`CliStderrWarn`, `NoConfirm`, and a test-only `TestAlwaysDeny`) are tested
//! directly.

use std::sync::atomic::{AtomicBool, Ordering};

use rmcp::service::ElicitationError;
use tokio::time::error::Elapsed;

use super::*;

// ── helpers ─────────────────────────────────────────────────────────────────

/// A timeout `Elapsed` value is not publicly constructible, so synthesize one by
/// awaiting an immediately-expiring timeout over a future that never resolves.
async fn elapsed_marker() -> Elapsed {
    tokio::time::timeout(
        std::time::Duration::from_nanos(1),
        std::future::pending::<()>(),
    )
    .await
    .expect_err("an immediately-expiring timeout over pending() must elapse")
}

fn accepted(confirm: bool, understood: bool) -> ConfirmDestructive {
    ConfirmDestructive {
        confirm,
        understood,
    }
}

/// Test-only `Confirmer` that always hard-blocks. Never wired into production.
#[derive(Debug, Default, Clone, Copy)]
struct TestAlwaysDeny;

#[async_trait::async_trait]
impl Confirmer for TestAlwaysDeny {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Err(ConfirmationDenied::Declined)
    }
}

// ── map_elicit_outcome: the full decision matrix ──────────────────────────────

#[test]
fn accept_with_both_true_is_ok() {
    let outcome = Ok(Ok(Some(accepted(true, true))));
    assert_eq!(map_elicit_outcome(outcome), Ok(()));
}

#[test]
fn accept_with_confirm_only_is_incomplete() {
    let outcome = Ok(Ok(Some(accepted(true, false))));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Incomplete)
    );
}

#[test]
fn accept_with_understood_only_is_incomplete() {
    let outcome = Ok(Ok(Some(accepted(false, true))));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Incomplete)
    );
}

#[test]
fn accept_with_neither_is_incomplete() {
    let outcome = Ok(Ok(Some(accepted(false, false))));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Incomplete)
    );
}

#[test]
fn no_content_is_incomplete() {
    let outcome = Ok(Ok(None));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Incomplete)
    );
}

#[test]
fn declined_is_hard_error() {
    let outcome = Ok(Err(ElicitationError::UserDeclined));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Declined)
    );
}

#[test]
fn cancelled_is_hard_error() {
    let outcome = Ok(Err(ElicitationError::UserCancelled));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Cancelled)
    );
}

#[test]
fn unsupported_is_hard_error() {
    let outcome = Ok(Err(ElicitationError::CapabilityNotSupported));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Unsupported)
    );
}

#[test]
fn elicitation_no_content_variant_is_incomplete() {
    let outcome = Ok(Err(ElicitationError::NoContent));
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Incomplete)
    );
}

#[test]
fn parse_error_is_failed() {
    let error = serde_json::from_str::<ConfirmDestructive>("not json").unwrap_err();
    let outcome = Ok(Err(ElicitationError::ParseError {
        error,
        data: serde_json::json!({"confirm": "yes"}),
    }));
    assert!(matches!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Failed(_))
    ));
}

#[tokio::test]
async fn timeout_is_hard_error() {
    let outcome: Result<Result<Option<ConfirmDestructive>, ElicitationError>, Elapsed> =
        Err(elapsed_marker().await);
    assert_eq!(
        map_elicit_outcome(outcome),
        Err(ConfirmationDenied::Timeout)
    );
}

// ── CliStderrWarn: warns and proceeds ─────────────────────────────────────────

#[tokio::test]
async fn cli_stderr_warn_proceeds() {
    let confirmer = CliStderrWarn;
    assert_eq!(
        confirmer
            .require("stop container abc", "irreversible")
            .await,
        Ok(())
    );
}

// ── NoConfirm: env-override path always proceeds ──────────────────────────────

#[tokio::test]
async fn no_confirm_always_ok() {
    let confirmer = NoConfirm;
    assert_eq!(confirmer.require("docker prune", "").await, Ok(()));
}

// ── Hard-block demonstration via &dyn Confirmer + IO sentinel ──────────────────

/// Stand-in for a destructive service method: it requires confirmation through
/// the `&dyn Confirmer` gate before touching its "external IO" (the sentinel).
async fn destructive_op(
    confirmer: &dyn Confirmer,
    io_ran: &AtomicBool,
) -> Result<(), ConfirmationDenied> {
    confirmer.require("delete everything", "no undo").await?;
    io_ran.store(true, Ordering::SeqCst);
    Ok(())
}

#[tokio::test]
async fn decline_hard_blocks_before_any_io() {
    let io_ran = AtomicBool::new(false);
    let result = destructive_op(&TestAlwaysDeny, &io_ran).await;
    assert_eq!(result, Err(ConfirmationDenied::Declined));
    assert!(
        !io_ran.load(Ordering::SeqCst),
        "IO must NOT run when confirmation is denied — gate must hard-block"
    );
}

#[tokio::test]
async fn granted_confirmation_allows_io() {
    let io_ran = AtomicBool::new(false);
    let result = destructive_op(&NoConfirm, &io_ran).await;
    assert_eq!(result, Ok(()));
    assert!(io_ran.load(Ordering::SeqCst), "IO must run when granted");
}

// ── logging discipline ────────────────────────────────────────────────────────
//
// The "no confirmation outcome leaks at info level or below" property is
// satisfied *by construction*, not asserted at runtime: the decision lives in
// the pure `map_elicit_outcome` (which emits no logs at all), and
// `McpPeerElicit::require` logs only a fixed `debug!` line that carries no
// accept/decline/cancel value. A runtime capture test would require a live
// `Peer<RoleServer>`, which is not constructible in a unit test, so the
// structural guarantee above is the enforcement mechanism.

// ── ConfirmationDenied is a real std::error::Error ────────────────────────────

#[test]
fn confirmation_denied_implements_error() {
    fn assert_error<E: std::error::Error>() {}
    assert_error::<ConfirmationDenied>();
    // Display strings carry no accept/decline outcome that would leak as a value.
    assert!(!ConfirmationDenied::Timeout.to_string().is_empty());
}

//! Destructive-operation confirmation gate.
//!
//! Destructive service methods (container stop/rm, docker prune, compose down,
//! …) must obtain explicit confirmation **before** performing any external IO.
//! This module defines that gate as a single service-layer abstraction so both
//! the MCP and CLI shims enforce it through one code path — no dual enforcement,
//! no drift.
//!
//! ## Design
//!
//! - [`Confirmer`] is an object-safe (`&dyn`) trait with one method, `require`.
//! - Two production impls:
//!   - [`McpPeerElicit`] — wraps `peer.elicit::<ConfirmDestructive>()` in a 10s
//!     timeout and asks the connected MCP client for confirmation.
//!   - [`CliStderrWarn`] — the CLI is human-driven; it prints a single warning
//!     line to stderr and proceeds (the human at the keyboard *is* the gate).
//! - [`NoConfirm`] is a zero-sized always-`Ok` impl used **only** by the shims
//!   when `SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true` (operational override). The shim
//!   logs a `warn!` and substitutes `NoConfirm`; the service signature is
//!   unchanged.
//!
//! ## Hard-block contract
//!
//! A declined, cancelled, unsupported, or timed-out confirmation returns
//! `Err(ConfirmationDenied)`. Callers MUST treat this as a hard error and never
//! soft-pass. At the MCP boundary this maps to `ErrorData::invalid_request`.
//!
//! ## Logging discipline
//!
//! The confirmation *outcome* (accept/decline/cancel) must never leak through
//! tracing at `info` level or below. The decision logic lives in the pure
//! [`map_elicit_outcome`] function (which logs nothing); `McpPeerElicit::require`
//! logs at most `debug!`. The `Err` return path may surface at `warn!` in the
//! caller — that is expected and non-secret.

use std::time::Duration;

use rmcp::{
    service::{ElicitationError, Peer},
    RoleServer,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::error::Elapsed;

/// Timeout applied to `peer.elicit()` to prevent a client that opens a
/// connection and never answers from holding the request open (DoS mitigation).
pub const ELICIT_TIMEOUT: Duration = Duration::from_secs(10);

// ── confirmation request schema ────────────────────────────────────────────────

/// Structured confirmation requested from the MCP client via elicitation.
///
/// Both fields must be `true` for the operation to proceed — a single checkbox
/// is intentionally weaker than two independent affirmations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfirmDestructive {
    /// Confirm you want to perform this destructive operation.
    pub confirm: bool,
    /// Confirm you understand this operation cannot be undone.
    pub understood: bool,
}

// `elicit_safe!` requires `ConfirmDestructive` to derive `JsonSchema`, be an
// object (struct), and contain only elicit-safe field types (bools qualify).
rmcp::elicit_safe!(ConfirmDestructive);

// ── denial error ────────────────────────────────────────────────────────────────

/// Returned when a destructive operation was not confirmed.
///
/// This is a hard error: the destructive operation MUST NOT proceed. Callers at
/// the MCP boundary map this to `ErrorData::invalid_request`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationDenied {
    /// The user explicitly declined.
    Declined,
    /// The user cancelled/dismissed the request.
    Cancelled,
    /// The MCP client does not support elicitation, so confirmation could not
    /// be obtained. (Set `SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true` to override, only
    /// on a trusted/loopback bind.)
    Unsupported,
    /// The confirmation request timed out (no response within [`ELICIT_TIMEOUT`]).
    Timeout,
    /// The user accepted but did not affirm both `confirm` and `understood`.
    Incomplete,
    /// The elicitation call failed at the protocol/transport level, or the
    /// response could not be parsed.
    Failed(String),
}

impl std::fmt::Display for ConfirmationDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Declined => f.write_str("destructive operation declined by user"),
            Self::Cancelled => f.write_str("destructive operation confirmation cancelled"),
            Self::Unsupported => f.write_str(
                "destructive operation requires confirmation, but the client does not support \
                 elicitation",
            ),
            Self::Timeout => f.write_str("destructive operation confirmation timed out"),
            Self::Incomplete => f.write_str(
                "destructive operation not confirmed (both confirm and understood \
                 must be true)",
            ),
            Self::Failed(reason) => {
                write!(f, "destructive operation confirmation failed: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfirmationDenied {}

// ── the gate trait ────────────────────────────────────────────────────────────

/// Service-layer confirmation gate.
///
/// Destructive service methods take `&dyn Confirmer` and `.await` on
/// [`require`](Confirmer::require) **before** any external IO. Both shims
/// construct a concrete impl and pass it in — the service never decides which
/// impl to use.
#[async_trait::async_trait]
pub trait Confirmer: Send + Sync {
    /// Require confirmation for operation `op` with human-readable `details`.
    ///
    /// Returns `Ok(())` only when confirmation is affirmatively granted.
    /// Any decline / cancel / unsupported / timeout is a hard `Err`.
    async fn require(&self, op: &str, details: &str) -> Result<(), ConfirmationDenied>;
}

// ── pure outcome mapping (the testable decision logic) ──────────────────────────

/// Map the raw result of a timeout-wrapped `peer.elicit::<ConfirmDestructive>()`
/// into the gate decision.
///
/// This is a pure function — it performs no IO and emits no logs — so the full
/// decision matrix (accept-both / accept-incomplete / decline / cancel /
/// unsupported / timeout / parse-failure) is unit-testable without a live
/// `Peer`. Keeping the outcome decision here (and out of `require`) is also what
/// guarantees no confirmation outcome leaks into tracing.
pub fn map_elicit_outcome(
    outcome: Result<Result<Option<ConfirmDestructive>, ElicitationError>, Elapsed>,
) -> Result<(), ConfirmationDenied> {
    match outcome {
        Err(_elapsed) => Err(ConfirmationDenied::Timeout),
        Ok(Err(ElicitationError::UserDeclined)) => Err(ConfirmationDenied::Declined),
        Ok(Err(ElicitationError::UserCancelled)) => Err(ConfirmationDenied::Cancelled),
        Ok(Err(ElicitationError::CapabilityNotSupported)) => Err(ConfirmationDenied::Unsupported),
        Ok(Err(ElicitationError::NoContent)) => Err(ConfirmationDenied::Incomplete),
        Ok(Err(other)) => Err(ConfirmationDenied::Failed(other.to_string())),
        Ok(Ok(None)) => Err(ConfirmationDenied::Incomplete),
        Ok(Ok(Some(answer))) => {
            if answer.confirm && answer.understood {
                Ok(())
            } else {
                Err(ConfirmationDenied::Incomplete)
            }
        }
    }
}

/// Build the prompt message shown to the MCP client for a confirmation request.
fn elicit_message(op: &str, details: &str) -> String {
    if details.is_empty() {
        format!("Confirm destructive operation: {op}")
    } else {
        format!("Confirm destructive operation: {op} ({details})")
    }
}

// ── McpPeerElicit ───────────────────────────────────────────────────────────────

/// [`Confirmer`] that asks the connected MCP client for confirmation via
/// elicitation, with a 10-second timeout.
pub struct McpPeerElicit {
    peer: Peer<RoleServer>,
}

impl McpPeerElicit {
    pub fn new(peer: Peer<RoleServer>) -> Self {
        Self { peer }
    }
}

#[async_trait::async_trait]
impl Confirmer for McpPeerElicit {
    async fn require(&self, op: &str, details: &str) -> Result<(), ConfirmationDenied> {
        let message = elicit_message(op, details);
        let outcome = tokio::time::timeout(
            ELICIT_TIMEOUT,
            self.peer.elicit::<ConfirmDestructive>(message),
        )
        .await;
        // Never log the outcome (accept/decline/cancel) at info or below.
        tracing::debug!("peer.elicit() completed for destructive-op confirmation");
        map_elicit_outcome(outcome)
    }
}

// ── CliStderrWarn ───────────────────────────────────────────────────────────────

/// [`Confirmer`] for the CLI. The CLI is invoked interactively by a human, so
/// there is no elicitation channel — print a single warning line to stderr and
/// proceed. The human running the command is the gate.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliStderrWarn;

#[async_trait::async_trait]
impl Confirmer for CliStderrWarn {
    async fn require(&self, op: &str, details: &str) -> Result<(), ConfirmationDenied> {
        if details.is_empty() {
            eprintln!("WARNING: about to {op}");
        } else {
            eprintln!("WARNING: about to {op} ({details})");
        }
        Ok(())
    }
}

// ── NoConfirm ───────────────────────────────────────────────────────────────────

/// Zero-sized always-`Ok` [`Confirmer`].
///
/// Used **only** by the shims when `SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true`. The
/// shim logs a `warn!` and substitutes this so the service runs the destructive
/// operation without prompting. It is never wired into a default code path.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoConfirm;

#[async_trait::async_trait]
impl Confirmer for NoConfirm {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Ok(())
    }
}

// ── DenyConfirm ───────────────────────────────────────────────────────────────

/// Zero-sized always-`Err(Unsupported)` [`Confirmer`].
///
/// Used by the REST surface, which has no elicitation channel — there is no way
/// to obtain interactive confirmation over a one-shot HTTP request, so a
/// destructive op is hard-denied as `Unsupported` (unless the
/// `SYNAPSE_MCP_ALLOW_DESTRUCTIVE` override substitutes [`NoConfirm`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyConfirm;

#[async_trait::async_trait]
impl Confirmer for DenyConfirm {
    async fn require(&self, _op: &str, _details: &str) -> Result<(), ConfirmationDenied> {
        Err(ConfirmationDenied::Unsupported)
    }
}

#[cfg(test)]
#[path = "elicitation_gate_tests.rs"]
mod tests;

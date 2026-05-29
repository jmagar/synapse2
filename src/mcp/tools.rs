//! MCP tool dispatch — thin shims only.
//!
//! **Rule**: no business logic here. Parse args → call service → return Value.
//! All logic belongs in `app.rs` (or `synapse2.rs` for transport concerns).
//!
//! The `peer` parameter is threaded through so that elicitation actions can
//! ask the MCP client for user input mid-call. For non-elicitation actions
//! it is unused.

use rmcp::{service::Peer, RoleServer};
use serde_json::Value;

use crate::actions::{execute_service_action, SynapseAction};
use crate::app::SynapseService;
use crate::elicitation_gate::{Confirmer, McpPeerElicit, NoConfirm};
use crate::server::AppState;

/// Dispatch an incoming MCP tool call to the appropriate handler.
///
/// `name`   — tool name (matches schema, currently only "synapse2")
/// `args`   — parsed JSON arguments from the MCP client
/// `peer`   — connection to the MCP client; used for elicitation
pub(super) async fn execute_tool(
    state: &AppState,
    name: &str,
    args: Value,
    peer: &Peer<RoleServer>,
) -> anyhow::Result<Value> {
    // Select the destructive-op confirmer (B5): the operational override
    // substitutes a no-op, otherwise the connected MCP client is asked via
    // elicitation. Service methods enforce the gate; the shim only chooses impl.
    let confirmer = build_confirmer(state, peer);
    match name {
        "flux" => dispatch_flux(state, args, confirmer.as_ref()).await,
        "scout" => dispatch_scout(state, args, confirmer.as_ref()).await,
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    }
}

/// Choose the [`Confirmer`] impl for this request. `NoConfirm` only when the
/// `SYNAPSE_MCP_ALLOW_DESTRUCTIVE` override is set (logged at `warn!`); otherwise
/// elicit from the connected MCP client.
fn build_confirmer(state: &AppState, peer: &Peer<RoleServer>) -> Box<dyn Confirmer> {
    if state.config.allow_destructive {
        tracing::warn!(
            "SYNAPSE_MCP_ALLOW_DESTRUCTIVE is set — destructive operations run without \
             confirmation"
        );
        Box::new(NoConfirm)
    } else {
        Box::new(McpPeerElicit::new(peer.clone()))
    }
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub async fn execute_tool_without_peer_for_test(
    state: &AppState,
    name: &str,
    args: Value,
) -> anyhow::Result<Value> {
    // Tests without a live peer use the override-driven confirmer: `NoConfirm`
    // when allow_destructive, else `DenyConfirm` (hard-blocks destructive ops).
    let confirmer: Box<dyn Confirmer> = if state.config.allow_destructive {
        Box::new(NoConfirm)
    } else {
        Box::new(crate::elicitation_gate::DenyConfirm)
    };
    match name {
        "flux" => dispatch_flux(state, args, confirmer.as_ref()).await,
        "scout" => dispatch_scout(state, args, confirmer.as_ref()).await,
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    }
}

async fn dispatch_flux(
    state: &AppState,
    args: Value,
    confirmer: &dyn Confirmer,
) -> anyhow::Result<Value> {
    let action = SynapseAction::from_flux_args(&args)?;
    dispatch_action(&state.service, &action, confirmer).await
}

async fn dispatch_scout(
    state: &AppState,
    args: Value,
    confirmer: &dyn Confirmer,
) -> anyhow::Result<Value> {
    let action = SynapseAction::from_scout_args(&args)?;
    dispatch_action(&state.service, &action, confirmer).await
}

async fn dispatch_action(
    service: &SynapseService,
    action: &SynapseAction,
    confirmer: &dyn Confirmer,
) -> anyhow::Result<Value> {
    execute_service_action(service, action, confirmer).await
}

// ── arg helpers ───────────────────────────────────────────────────────────────

// ── help text ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;

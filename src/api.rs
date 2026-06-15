//! REST API handlers — `POST /v1/synapse2`, `GET /health`, `GET /status`, `GET /openapi.json`.
//!
//! All handlers are thin: parse the request, call the service, return JSON.
//! Business logic lives in `app.rs`.

use anyhow::Result;
use axum::{
    extract::{Extension, State},
    http::{StatusCode, header},
    response::{IntoResponse, Json},
};
use lab_auth::AuthContext;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::actions::{SynapseAction, execute_service_action, required_scope_for_parsed_action};
use crate::server::{AppState, AuthPolicy};
use crate::token_limit::MAX_RESPONSE_BYTES;

/// Request body for `POST /v1/synapse2`.
///
/// REST uses an explicit `{ action, params }` envelope. MCP uses a flat
/// argument object such as `{ action, message }`. Both convert into the same
/// typed `SynapseAction` before calling `SynapseService`.
#[derive(Deserialize)]
pub struct ActionRequest {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub params: Value,
}

/// `POST /v1/synapse2` — dispatches an action by name.
///
/// Request:  `{"action": "flux.docker.info", "params": {}}`
pub async fn api_dispatch(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<ActionRequest>,
) -> impl IntoResponse {
    let result = match rest_action_from_request(&body.action, &body.params) {
        Ok(action) => {
            if let Some(response) =
                enforce_rest_scope(&state, auth.as_ref().map(|Extension(auth)| auth), &action)
            {
                return response;
            }
            // REST has no elicitation channel: destructive ops are hard-denied
            // (DenyConfirm) unless the SYNAPSE_MCP_ALLOW_DESTRUCTIVE override is
            // set, in which case NoConfirm runs them. Read-only ops are
            // unaffected (their service methods never call the confirmer).
            let confirmer: Box<dyn crate::elicitation_gate::Confirmer> =
                if state.config.allow_destructive {
                    Box::new(crate::elicitation_gate::NoConfirm)
                } else {
                    Box::new(crate::elicitation_gate::DenyConfirm)
                };
            execute_service_action(&state.service, &action, confirmer.as_ref()).await
        }
        Err(error) => Err(error),
    };

    match result {
        Ok(value) => match cap_rest_response(value) {
            Ok(value) => Json(value).into_response(),
            Err(e) => {
                tracing::error!(error = %e, action = %body.action, "REST response serialization failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        },
        Err(e) if crate::actions::is_validation_error(&e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
        // Destructive-op confirmation denied (no elicitation channel over REST).
        // Return 403 Forbidden — not 500 — and do not log at error level.
        Err(e) if crate::actions::is_confirmation_denied(&e) => {
            tracing::debug!(action = %body.action, "REST destructive action denied: no confirmation channel");
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden: destructive action requires confirmation; set SYNAPSE_MCP_ALLOW_DESTRUCTIVE=true or use MCP"})),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, action = %body.action, "REST action execution failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

/// REST compatibility subset of synapse2 actions.
///
/// This surface is intentionally limited to the dotted action names listed
/// below — it is NOT a full mirror of the MCP/CLI surface. New actions must be
/// explicitly added here; unknown dotted names fall through to the
/// `UnknownAction` error rather than accidentally routing to wrong subactions.
///
/// Parsing uses `split_once('.')` so that malformed names such as
/// `flux.docker.foo.bar` (three dots) are handled correctly: the first split
/// gives `("flux", "docker.foo.bar")` which is then matched against the exact
/// known second-segment set, and any non-matching value falls through to the
/// catch-all error.
fn rest_action_from_request(action: &str, params: &Value) -> Result<SynapseAction> {
    match action {
        "help" => Ok(SynapseAction::FluxHelp {
            topic: None,
            format: None,
        }),
        "scout.nodes" => Ok(SynapseAction::ScoutNodes),
        other => {
            // Split exactly once on '.' to get (domain, rest).
            // A name with zero dots or three-or-more dots will either not match
            // in the inner match below or fall through to UnknownAction.
            let Some((domain, rest)) = other.split_once('.') else {
                return Err(crate::actions::ValidationError::UnknownAction {
                    action: other.to_owned(),
                }
                .into());
            };

            match (domain, rest) {
                // Docker subactions over REST. Merge caller params (host,
                // dangling_only, image, etc.) into the flux arg shape so REST
                // honors the same options as MCP/CLI. Destructive subactions
                // (build/rmi/prune) are reachable but hard-denied without the
                // allow-destructive override (no elicitation channel over REST
                // — see api_dispatch).
                (
                    "flux",
                    sub @ ("docker.info" | "docker.df" | "docker.images" | "docker.networks"
                    | "docker.volumes" | "docker.pull" | "docker.build" | "docker.rmi"
                    | "docker.prune"),
                ) => {
                    // sub is "docker.<subaction>"; split once more to isolate
                    // the subaction name without trimming a variable prefix.
                    let subaction = sub.split_once('.').map(|(_, s)| s).unwrap_or(sub);
                    let mut obj = params.as_object().cloned().unwrap_or_default();
                    obj.insert("action".into(), json!("docker"));
                    obj.insert("subaction".into(), json!(subaction));
                    SynapseAction::from_flux_args(&Value::Object(obj))
                }
                ("flux", "container.list") => {
                    // Merge caller params (may be null/absent) into the flux arg
                    // shape so REST honors the same list filters as MCP/CLI.
                    let mut obj = params.as_object().cloned().unwrap_or_default();
                    obj.insert("action".into(), json!("container"));
                    obj.insert("subaction".into(), json!("list"));
                    SynapseAction::from_flux_args(&Value::Object(obj))
                }
                // Scout subactions: pass the raw params object through to
                // from_scout_args so the parser owns defaults and required-field
                // errors, matching the MCP path exactly.
                ("scout", "peek") => {
                    let mut obj = params.as_object().cloned().unwrap_or_default();
                    obj.insert("action".into(), json!("peek"));
                    SynapseAction::from_scout_args(&Value::Object(obj))
                }
                ("scout", "exec") => {
                    let mut obj = params.as_object().cloned().unwrap_or_default();
                    obj.insert("action".into(), json!("exec"));
                    SynapseAction::from_scout_args(&Value::Object(obj))
                }
                _ => Err(crate::actions::ValidationError::UnknownAction {
                    action: other.to_owned(),
                }
                .into()),
            }
        }
    }
}

fn cap_rest_response(value: Value) -> Result<Value> {
    let serialized = serde_json::to_vec(&value)?;
    if serialized.len() <= MAX_RESPONSE_BYTES {
        return Ok(value);
    }
    Ok(json!({
        "truncated": true,
        "error": "response exceeded REST response size limit",
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "hint": "Use limit/offset parameters or more specific filters to get a smaller result.",
    }))
}

fn enforce_rest_scope(
    state: &AppState,
    auth: Option<&AuthContext>,
    action: &SynapseAction,
) -> Option<axum::response::Response> {
    if !matches!(&state.auth_policy, AuthPolicy::Mounted { .. }) {
        return None;
    }
    let required_scope = required_scope_for_parsed_action(action)?;
    let Some(auth) = auth else {
        tracing::warn!(action = %action.name(), "REST action denied: missing auth context");
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden: missing auth context"})),
            )
                .into_response(),
        );
    };
    let satisfied = crate::actions::scopes_satisfy(&auth.scopes, required_scope);
    if satisfied {
        return None;
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action.name(),
        required_scope = %required_scope,
        "REST action denied: insufficient scope"
    );
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(json!({"error": format!("forbidden: requires scope: {required_scope}")})),
        )
            .into_response(),
    )
}

/// `GET /health` — liveness probe (unauthenticated).
pub async fn health() -> impl IntoResponse {
    tracing::debug!("health probe");
    Json(json!({ "status": "ok" }))
}

/// `GET /openapi.json` — generated OpenAPI schema for the REST surface.
pub async fn openapi_json() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        include_str!("../docs/generated/openapi.json"),
    )
}

/// `GET /status` — local runtime status (unauthenticated, redacts secrets).
pub async fn status(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "server": state.config.server_name,
        "version": env!("CARGO_PKG_VERSION"),
        "transport": "http",
    }))
    .into_response()
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;

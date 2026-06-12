//! `SynapseRmcpServer` — the `ServerHandler` implementation.
//!
//! This is the adapter between the rmcp crate and your application. It:
//!   - Advertises tools, resources, and prompts to MCP clients
//!   - Enforces auth scopes on every call
//!   - Delegates business logic to `tools.rs` → `app.rs` → `synapse2.rs`
//!
//! **Template**: rename `SynapseRmcpServer`. Update action metadata in
//! `src/actions.rs` to keep schemas, scope rules, and dispatch in sync.

use std::{borrow::Cow, sync::Arc, time::Instant};

use lab_auth::AuthContext;
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
        Implementation, ListPromptsResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ServerCapabilities,
        ServerInfo, Tool,
    },
    service::{Peer, RequestContext},
    ErrorData, RoleServer, ServerHandler,
};
use serde_json::{Map, Value};

use crate::{
    actions::{required_scope_for_parsed_action, SynapseAction},
    token_limit,
};

use crate::server::{AppState, AuthPolicy};

use super::{prompts, resources, schemas::tool_definitions, tools::execute_tool};

// ── server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SynapseRmcpServer {
    state: AppState,
}

pub fn rmcp_server(state: AppState) -> SynapseRmcpServer {
    SynapseRmcpServer { state }
}

impl ServerHandler for SynapseRmcpServer {
    // ── tools ─────────────────────────────────────────────────────────────────

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        let tools = rmcp_tool_definitions()?;
        tracing::debug!(tool_count = tools.len(), "MCP tools listed");
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = request.name.to_string();

        let auth = require_auth_context(&self.state, &context)?;
        if tool_name != "flux" && tool_name != "scout" {
            return Err(ErrorData::invalid_params(
                format!("unknown tool: {tool_name}"),
                None,
            ));
        }

        let arguments = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| Value::Object(Map::new()));

        // Extract action before scope check so a missing action returns the
        // more useful "action is required" validation error, not DENY_SCOPE.
        let action_opt: Option<String> = arguments
            .get("action")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        let parsed_action = if action_opt.is_some() {
            match parse_mcp_action(&tool_name, &arguments) {
                Ok(action) => Some(action),
                Err(error) if auth.is_some() => return Err(error),
                Err(_) => return Err(ErrorData::invalid_request("invalid request", None)),
            }
        } else {
            None
        };

        // SECURITY FIX: Before auth succeeds, return generic error for both unknown
        // actions and missing scopes. This prevents unauthenticated probes from
        // enumerating valid action names.
        if let Some(auth_ctx) = auth {
            // Authenticated: safe to return specific errors
            if let Some(parsed_action) = parsed_action.as_ref() {
                if let Some(required_scope) = required_scope_for_parsed_action(parsed_action) {
                    check_scope(auth_ctx, required_scope, parsed_action.name())?;
                }
            }
        } else {
            // Unauthenticated (loopback or trusted gateway): still validate action
            // but don't leak information before scope check
            if let Some(parsed_action) = parsed_action.as_ref() {
                if let Some(required_scope) = required_scope_for_parsed_action(parsed_action) {
                    if required_scope != crate::actions::READ_SCOPE
                        && required_scope != crate::actions::WRITE_SCOPE
                    {
                        return Err(ErrorData::invalid_request("invalid request", None));
                    }
                }
            }
        }

        let action: String = action_opt.unwrap_or_default();

        validate_response_format_arg(&arguments)?;

        // Clone the peer so we can pass it to the tool dispatcher.
        // The peer is needed for elicitation (asking the client for user input).
        let peer: Peer<RoleServer> = context.peer.clone();

        let started = Instant::now();
        tracing::info!(tool = %tool_name, action = %action, "MCP tool execution started");

        let render_args = arguments.clone();
        match execute_tool(&self.state, &tool_name, arguments, &peer).await {
            Ok(result) => {
                tracing::info!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    "MCP tool execution completed"
                );
                let text = render_mcp_tool_output(&tool_name, &render_args, &result)
                    .map_err(|e| ErrorData::internal_error(format!("render error: {e}"), None))?;
                tool_result_from_text(text)
            }
            Err(error) if crate::actions::is_confirmation_denied(&error) => {
                tracing::warn!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    "MCP tool destructive op not confirmed"
                );
                Err(ErrorData::invalid_request(error.to_string(), None))
            }
            Err(error) if crate::actions::is_validation_error(&error) => {
                tracing::warn!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    "MCP tool rejected invalid params"
                );
                Err(ErrorData::invalid_params(error.to_string(), None))
            }
            Err(error) => {
                tracing::error!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    error = %error,
                    "MCP tool execution failed"
                );
                Err(ErrorData::internal_error(
                    internal_tool_error_message(&action),
                    None,
                ))
            }
        }
    }

    // ── resources ─────────────────────────────────────────────────────────────

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        Ok(ListResourcesResult {
            resources: resources::all_resources(),
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        let contents = resources::read_resource(&request.uri, &self.state)
            .await
            .map_err(|e| {
                if e.to_string().contains("unknown resource") {
                    ErrorData::invalid_params(e.to_string(), None)
                } else {
                    ErrorData::internal_error(format!("resource read failed: {e}"), None)
                }
            })?;
        Ok(ReadResourceResult::new(vec![contents]))
    }

    // ── prompts ───────────────────────────────────────────────────────────────

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        Ok(prompts::list_prompts())
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        prompts::get_prompt(request).map_err(|e| ErrorData::invalid_params(e.to_string(), None))
    }

    // ── server info ───────────────────────────────────────────────────────────

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            self.state.config.server_name.clone(),
            env!("CARGO_PKG_VERSION"),
        ))
    }
}

// ── tool definition conversion ────────────────────────────────────────────────

fn rmcp_tool_definitions() -> Result<Vec<Tool>, ErrorData> {
    tool_definitions()
        .iter()
        .cloned()
        .map(rmcp_tool_from_json)
        .collect()
}

fn rmcp_tool_from_json(value: Value) -> Result<Tool, ErrorData> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ErrorData::internal_error("tool definition missing name", None))?;
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(|d| Cow::Owned(d.to_string()));
    let input_schema = value
        .get("inputSchema")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("tool definition missing inputSchema", None))?;
    Ok(Tool::new_with_raw(
        Cow::Owned(name.to_string()),
        description,
        Arc::new(input_schema),
    ))
}

#[cfg(test)]
fn tool_result_from_json(value: Value) -> Result<CallToolResult, ErrorData> {
    // Compact JSON (not pretty) recovers ~30-40% of the 40 KB token budget.
    let text = serde_json::to_string(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let text = token_limit::truncate_if_needed(&text);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

fn tool_result_from_text(text: String) -> Result<CallToolResult, ErrorData> {
    let text = token_limit::truncate_if_needed(&text);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

fn render_mcp_tool_output(tool_name: &str, args: &Value, value: &Value) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let subaction = args.get("subaction").and_then(Value::as_str);
    let response_format = if action == "help" {
        args.get("format").and_then(Value::as_str)
    } else {
        args.get("response_format").and_then(Value::as_str)
    };
    crate::formatters::render_action_output(tool_name, action, subaction, response_format, value)
}

fn validate_response_format_arg(args: &Value) -> Result<(), ErrorData> {
    let Some(value) = args.get("response_format").and_then(Value::as_str) else {
        return Ok(());
    };
    crate::formatters::ResponseFormat::parse(Some(value))
        .map(|_| ())
        .map_err(|e| ErrorData::invalid_params(e, None))
}

#[cfg(test)]
fn reject_unknown_action_before_scope(action: &str) -> Result<(), ErrorData> {
    if crate::actions::is_known_action(action) {
        return Ok(());
    }
    Err(ErrorData::invalid_params(
        crate::actions::ValidationError::UnknownAction {
            action: action.to_owned(),
        }
        .to_string(),
        None,
    ))
}

fn parse_mcp_action(tool_name: &str, arguments: &Value) -> Result<SynapseAction, ErrorData> {
    match tool_name {
        "flux" => SynapseAction::from_flux_args(arguments),
        "scout" => SynapseAction::from_scout_args(arguments),
        _ => unreachable!("tool name validated before parse_mcp_action"),
    }
    .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

fn internal_tool_error_message(action: &str) -> String {
    format!("tool execution failed: kind=execution_error action='{action}'")
}

// ── auth helpers ──────────────────────────────────────────────────────────────

fn require_auth_context<'a>(
    state: &AppState,
    ctx: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a AuthContext>, ErrorData> {
    match &state.auth_policy {
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => Ok(None),
        AuthPolicy::Mounted { .. } => {
            let parts = ctx
                .extensions
                .get::<axum::http::request::Parts>()
                .ok_or_else(|| {
                    tracing::error!(
                        "rmcp HTTP Parts extension absent — middleware ordering may be broken"
                    );
                    ErrorData::invalid_request("forbidden: missing http context", None)
                })?;
            let auth = parts.extensions.get::<AuthContext>().ok_or_else(|| {
                tracing::warn!("AuthContext absent — AuthLayer may not be mounted");
                ErrorData::invalid_request("forbidden: missing auth context", None)
            })?;
            Ok(Some(auth))
        }
    }
}

fn check_scope(auth: &AuthContext, required_scope: &str, action: &str) -> Result<(), ErrorData> {
    if scope_satisfied(&auth.scopes, required_scope) {
        return Ok(());
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action,
        required_scope = %required_scope,
        "MCP tool denied: insufficient scope"
    );
    Err(ErrorData::invalid_request(
        format!("forbidden: requires scope: {required_scope}"),
        None,
    ))
}

fn scope_satisfied(token_scopes: &[String], required: &str) -> bool {
    crate::actions::scopes_satisfy(token_scopes, required)
}

#[cfg(test)]
#[path = "rmcp_server_tests.rs"]
mod tests;

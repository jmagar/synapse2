//! MCP tool response rendering and argument validation helpers.

use rmcp::{
    ErrorData,
    model::{CallToolResult, Content},
};
use serde_json::Value;

use crate::token_limit;

#[cfg(test)]
pub(super) fn tool_result_from_json(value: Value) -> Result<CallToolResult, ErrorData> {
    // Compact JSON (not pretty) recovers ~30-40% of the 40 KB token budget.
    let text = serde_json::to_string(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let text = token_limit::truncate_if_needed(&text);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

pub(super) fn tool_result_from_text(text: String) -> Result<CallToolResult, ErrorData> {
    let text = token_limit::truncate_if_needed(&text);
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

pub(super) fn render_mcp_tool_output(
    tool_name: &str,
    args: &Value,
    value: &Value,
) -> Result<String, String> {
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

pub(super) fn validate_response_format_arg(args: &Value) -> Result<(), ErrorData> {
    let Some(value) = args.get("response_format").and_then(Value::as_str) else {
        return Ok(());
    };
    crate::formatters::ResponseFormat::parse(Some(value))
        .map(|_| ())
        .map_err(|e| ErrorData::invalid_params(e, None))
}

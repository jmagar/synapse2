//! Shared runtime budgets for service actions.
//!
//! Final MCP/REST response truncation protects clients, but it does not protect
//! the service while it is collecting subprocess, SSH, Docker, or log output.
//! This module provides the earlier guardrails used by the service layer.

use std::{future::Future, path::Path, time::Duration};

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::ssh::CommandOutput;

#[cfg(test)]
#[path = "runtime_budget_tests.rs"]
mod tests;

/// Default ceiling for one high-level service action.
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Maximum bytes retained for large textual output fields before rendering.
pub const SERVICE_TEXT_FIELD_BYTE_CAP: usize = 16_384;

/// Maximum number of Docker progress frames retained in a service payload.
pub const SERVICE_PROGRESS_ITEM_CAP: usize = 200;

/// Run a future with a caller-provided deadline.
pub async fn with_deadline<T, E, Fut>(label: &str, timeout: Duration, fut: Fut) -> Result<T>
where
    Fut: Future<Output = std::result::Result<T, E>>,
    E: Into<anyhow::Error>,
{
    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(value)) => Ok(value),
        // Propagate the original error unchanged (type + message preserved) so
        // downstream `downcast_ref` checks — e.g. `is_confirmation_denied` mapping
        // a destructive denial to HTTP 403 — still see the typed error instead of a
        // an opaque anyhow wrapper that loses the concrete type. (The deadline
        // only rewrites the message on timeout.)
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(anyhow!("{label} timed out after {}s", timeout.as_secs())),
    }
}

/// Run a future with the default service-operation deadline.
pub async fn with_operation_deadline<T, E, Fut>(label: &str, fut: Fut) -> Result<T>
where
    Fut: Future<Output = std::result::Result<T, E>>,
    E: Into<anyhow::Error>,
{
    with_deadline(label, DEFAULT_OPERATION_TIMEOUT, fut).await
}

/// Run a local subprocess with the shared operation deadline.
pub async fn run_local_command(
    program: &str,
    args: &[&str],
    current_dir: Option<&Path>,
) -> Result<CommandOutput> {
    let mut command = tokio::process::Command::new(program);
    command.args(args).kill_on_drop(true);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }

    let output =
        with_operation_deadline(&format!("local command `{program}`"), command.output()).await?;
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
    })
}

/// Cap large output fields in a service value before MCP/REST rendering.
#[must_use]
pub fn cap_service_value(mut value: Value) -> Value {
    cap_value_inner(&mut value);
    value
}

fn cap_value_inner(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Collect only the keys that need special capping/truncation,
            // avoiding a full clone of every key in the object.
            // Note: "progress" appears in should_cap_text_field (for string
            // capping) and is also handled for array truncation below —
            // both paths run in the same capped_keys loop.
            let mut capped_keys: Vec<String> = Vec::new();
            for key in map.keys() {
                if should_cap_text_field(key) {
                    capped_keys.push(key.clone());
                }
            }

            let mut truncations = Vec::new();

            // Second pass: apply text capping and array truncation for known
            // fields, then recurse into any nested objects/arrays.
            for key in &capped_keys {
                let Some(child) = map.get_mut(key) else {
                    continue;
                };

                // Text-string capping: truncate oversized string values.
                if let Some(text) = child.as_str()
                    && text.len() > SERVICE_TEXT_FIELD_BYTE_CAP
                {
                    let original_bytes = text.len();
                    *child = Value::String(truncate_utf8(text, SERVICE_TEXT_FIELD_BYTE_CAP));
                    truncations.push(json!({
                        "field": key,
                        "original_bytes": original_bytes,
                        "retained_bytes": SERVICE_TEXT_FIELD_BYTE_CAP,
                    }));
                    // Scalar; nothing to recurse into.
                    continue;
                }

                // Array-item truncation for "progress" arrays.
                if key == "progress"
                    && let Value::Array(items) = child
                    && items.len() > SERVICE_PROGRESS_ITEM_CAP
                {
                    let original_items = items.len();
                    items.truncate(SERVICE_PROGRESS_ITEM_CAP);
                    truncations.push(json!({
                        "field": "progress",
                        "original_items": original_items,
                        "retained_items": SERVICE_PROGRESS_ITEM_CAP,
                    }));
                }

                cap_value_inner(child);
            }

            // Third pass: recurse into all remaining keys not already handled
            // above (i.e. keys that are not in the capped-fields set).
            for (key, child) in map.iter_mut() {
                if should_cap_text_field(key) {
                    continue;
                }
                cap_value_inner(child);
            }

            if !truncations.is_empty() {
                map.insert("truncated".into(), Value::Bool(true));
                map.insert("truncation".into(), Value::Array(truncations));
            }
        }
        Value::Array(items) => {
            for item in items {
                cap_value_inner(item);
            }
        }
        _ => {}
    }
}

/// Return `true` when a JSON object key names a large textual field that should
/// be byte-capped before MCP/REST rendering.
///
/// Uses a `match` expression over string literals, which the compiler can
/// optimize (e.g. length/prefix dispatch) better than the bounds-checked
/// `[&str]::contains` slice scan it replaced.
fn should_cap_text_field(key: &str) -> bool {
    matches!(
        key,
        "stdout"
            | "stderr"
            | "logs"
            | "output"
            | "progress"
            | "services"
            | "network"
            | "mounts"
            | "processes"
            | "disk_usage"
            | "diff"
            | "content"
    )
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut boundary = max_bytes;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text[..boundary].to_owned()
}

/// Append lossy UTF-8 bytes to a bounded string.
///
/// Returns `true` when this append had to drop bytes because the cap was hit.
pub fn append_lossy_bounded(target: &mut String, bytes: &[u8], max_bytes: usize) -> bool {
    if target.len() >= max_bytes {
        return !bytes.is_empty();
    }

    let chunk = String::from_utf8_lossy(bytes);
    let remaining = max_bytes - target.len();
    if chunk.len() <= remaining {
        target.push_str(&chunk);
        false
    } else {
        target.push_str(&truncate_utf8(&chunk, remaining));
        true
    }
}

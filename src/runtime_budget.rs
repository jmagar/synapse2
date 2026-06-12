//! Shared runtime budgets for service actions.
//!
//! Final MCP/REST response truncation protects clients, but it does not protect
//! the service while it is collecting subprocess, SSH, Docker, or log output.
//! This module provides the earlier guardrails used by the service layer.

use std::{future::Future, path::Path, time::Duration};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

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

const CAPPED_TEXT_FIELDS: &[&str] = &[
    "stdout",
    "stderr",
    "logs",
    "output",
    "progress",
    "services",
    "network",
    "mounts",
    "processes",
    "disk_usage",
    "diff",
    "content",
];

/// Run a future with a caller-provided deadline.
pub async fn with_deadline<T, E, Fut>(label: &str, timeout: Duration, fut: Fut) -> Result<T>
where
    Fut: Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Display,
{
    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(anyhow!("{label} failed: {error}")),
        Err(_) => Err(anyhow!("{label} timed out after {}s", timeout.as_secs())),
    }
}

/// Run a future with the default service-operation deadline.
pub async fn with_operation_deadline<T, E, Fut>(label: &str, fut: Fut) -> Result<T>
where
    Fut: Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Display,
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
            let keys = map.keys().cloned().collect::<Vec<_>>();
            let mut truncations = Vec::new();

            for key in keys {
                let Some(child) = map.get_mut(&key) else {
                    continue;
                };

                if should_cap_text_field(&key) {
                    if let Some(text) = child.as_str() {
                        if text.len() > SERVICE_TEXT_FIELD_BYTE_CAP {
                            let original_bytes = text.len();
                            *child =
                                Value::String(truncate_utf8(text, SERVICE_TEXT_FIELD_BYTE_CAP));
                            truncations.push(json!({
                                "field": key,
                                "original_bytes": original_bytes,
                                "retained_bytes": SERVICE_TEXT_FIELD_BYTE_CAP,
                            }));
                            continue;
                        }
                    }
                }

                if key == "progress" {
                    if let Value::Array(items) = child {
                        if items.len() > SERVICE_PROGRESS_ITEM_CAP {
                            let original_items = items.len();
                            items.truncate(SERVICE_PROGRESS_ITEM_CAP);
                            truncations.push(json!({
                                "field": key,
                                "original_items": original_items,
                                "retained_items": SERVICE_PROGRESS_ITEM_CAP,
                            }));
                        }
                    }
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

fn should_cap_text_field(key: &str) -> bool {
    CAPPED_TEXT_FIELDS.contains(&key)
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

use anyhow::Result;
use serde_json::{json, Value};

use crate::app::SynapseService;

// ── Validation error type ─────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ValidationError {
    MissingAction,
    MissingField { field: String },
    WrongType { field: String },
    NotAvailableOverRest { action: String },
    UnknownAction { action: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAction => write!(f, "action is required"),
            Self::MissingField { field } => {
                write!(f, "`{field}` is required and must not be empty")
            }
            Self::WrongType { field } => write!(f, "`{field}` must be a string"),
            Self::NotAvailableOverRest { action } => write!(
                f,
                "action={action} is not available over REST; use MCP or action=help for documentation"
            ),
            Self::UnknownAction { action } => write!(
                f,
                "unknown synapse2 action: {action}; use action=help for documentation"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

pub const READ_SCOPE: &str = "synapse:read";
pub const WRITE_SCOPE: &str = "synapse:write";
pub const DENY_SCOPE: &str = "synapse2:__deny__";

/// Returns true if `token_scopes` satisfy `required`.
/// Write scope satisfies read (write ⊇ read).
/// Single source of truth — called from both REST and MCP enforcement paths.
pub fn scopes_satisfy(token_scopes: &[String], required: &str) -> bool {
    token_scopes
        .iter()
        .any(|s| s == required || (required == READ_SCOPE && s == WRITE_SCOPE))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTransport {
    Any,
    McpOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    pub name: &'static str,
    pub required_scope: Option<&'static str>,
    pub transport: ActionTransport,
    /// True if this action mutates or destroys state irreversibly (container
    /// rm/stop, docker prune, compose down, …). Destructive actions must pass
    /// through the `elicitation_gate::Confirmer` before performing IO.
    ///
    /// This is the single source of truth — `read_only` is derived, not stored
    /// (see [`is_read_only`]).
    pub destructive: bool,
}

pub const ACTION_SPECS: &[ActionSpec] = &[
    ActionSpec {
        name: "help",
        required_scope: None,
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "docker",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "container",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "host",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "nodes",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "peek",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "exec",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
];

/// Derive whether an action is read-only.
///
/// `read_only` is NOT stored on [`ActionSpec`]; it is derived from `destructive`
/// plus the scope: an action is read-only when it is not destructive and does
/// not require the write scope. This is the source for the MCP `readOnlyHint`
/// tool annotation, while `destructiveHint` comes straight from
/// [`ActionSpec::destructive`].
pub fn is_read_only(spec: &ActionSpec) -> bool {
    !spec.destructive && spec.required_scope != Some(WRITE_SCOPE)
}

pub fn action_names() -> Vec<&'static str> {
    ACTION_SPECS.iter().map(|spec| spec.name).collect()
}

pub fn is_known_action(action: &str) -> bool {
    ACTION_SPECS.iter().any(|spec| spec.name == action)
}

pub fn rest_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.transport == ActionTransport::Any)
        .map(|spec| spec.name)
        .collect()
}

pub fn is_rest_action(action: &str) -> bool {
    action_spec(action)
        .map(|spec| spec.transport == ActionTransport::Any)
        .unwrap_or(false)
}

pub fn mcp_only_action_names() -> Vec<&'static str> {
    ACTION_SPECS
        .iter()
        .filter(|spec| spec.transport == ActionTransport::McpOnly)
        .map(|spec| spec.name)
        .collect()
}

pub fn required_scope_for_action(action: &str) -> Option<&'static str> {
    action_spec(action)
        .map(|spec| spec.required_scope)
        .unwrap_or(Some(DENY_SCOPE))
}

fn action_spec(action: &str) -> Option<&'static ActionSpec> {
    ACTION_SPECS.iter().find(|spec| spec.name == action)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynapseAction {
    FluxHelp,
    FluxDocker {
        subaction: String,
    },
    FluxContainer {
        subaction: String,
        container_id: Option<String>,
        lines: Option<u32>,
    },
    FluxHost {
        subaction: String,
        host: Option<String>,
    },
    ScoutHelp,
    ScoutNodes,
    ScoutPeek {
        host: String,
        path: String,
    },
    ScoutExec {
        host: String,
        path: String,
        command: String,
    },
}

impl SynapseAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::FluxHelp | Self::ScoutHelp => "help",
            Self::FluxDocker { .. } => "docker",
            Self::FluxContainer { .. } => "container",
            Self::FluxHost { .. } => "host",
            Self::ScoutNodes => "nodes",
            Self::ScoutPeek { .. } => "peek",
            Self::ScoutExec { .. } => "exec",
        }
    }

    pub fn from_flux_args(args: &Value) -> Result<Self> {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .ok_or(ValidationError::MissingAction)?;
        match action {
            "help" => Ok(Self::FluxHelp),
            "docker" => Ok(Self::FluxDocker {
                subaction: required_string_param(args, "subaction")?,
            }),
            "container" => Ok(Self::FluxContainer {
                subaction: required_string_param(args, "subaction")?,
                container_id: optional_string_param(args, "container_id")?,
                lines: optional_u32_param(args, "lines")?,
            }),
            "host" => Ok(Self::FluxHost {
                subaction: required_string_param(args, "subaction")?,
                host: optional_string_param(args, "host")?,
            }),
            other => Err(ValidationError::UnknownAction {
                action: other.to_owned(),
            }
            .into()),
        }
    }

    pub fn from_scout_args(args: &Value) -> Result<Self> {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .ok_or(ValidationError::MissingAction)?;
        match action {
            "help" => Ok(Self::ScoutHelp),
            "nodes" => Ok(Self::ScoutNodes),
            "peek" => Ok(Self::ScoutPeek {
                host: required_string_param(args, "host")?,
                path: required_string_param(args, "path")?,
            }),
            "exec" => Ok(Self::ScoutExec {
                host: required_string_param(args, "host")?,
                path: required_string_param(args, "path")?,
                command: required_string_param(args, "command")?,
            }),
            other => Err(ValidationError::UnknownAction {
                action: other.to_owned(),
            }
            .into()),
        }
    }
}

pub async fn execute_service_action(
    service: &SynapseService,
    action: &SynapseAction,
) -> Result<Value> {
    match action {
        SynapseAction::FluxHelp => service.flux_help().await,
        SynapseAction::FluxDocker { subaction } => match subaction.as_str() {
            "info" => service.flux_docker_info().await,
            "images" => service.flux_docker_images().await,
            "networks" => service.flux_docker_networks().await,
            "volumes" => service.flux_docker_volumes().await,
            other => Err(ValidationError::UnknownAction {
                action: format!("docker:{other}"),
            }
            .into()),
        },
        SynapseAction::FluxContainer {
            subaction,
            container_id,
            lines,
        } => match subaction.as_str() {
            "list" => service.flux_container_list().await,
            "inspect" => {
                service
                    .flux_container_inspect(container_id.as_deref().ok_or_else(|| {
                        ValidationError::MissingField {
                            field: "container_id".into(),
                        }
                    })?)
                    .await
            }
            "logs" => {
                service
                    .flux_container_logs(
                        container_id
                            .as_deref()
                            .ok_or_else(|| ValidationError::MissingField {
                                field: "container_id".into(),
                            })?,
                        lines.unwrap_or(50),
                    )
                    .await
            }
            other => Err(ValidationError::UnknownAction {
                action: format!("container:{other}"),
            }
            .into()),
        },
        SynapseAction::FluxHost { subaction, host } => match subaction.as_str() {
            "status" => service.flux_host_status(host.as_deref()).await,
            other => Err(ValidationError::UnknownAction {
                action: format!("host:{other}"),
            }
            .into()),
        },
        SynapseAction::ScoutHelp => service.scout_help().await,
        SynapseAction::ScoutNodes => service.scout_nodes().await,
        SynapseAction::ScoutPeek { host, path } => service.scout_peek(host, path).await,
        SynapseAction::ScoutExec {
            host,
            path,
            command,
        } => service.scout_exec(host, path, command).await,
    }
}

pub fn rest_help() -> Value {
    json!({
        "actions": rest_action_names(),
        "mcp_only_actions": mcp_only_action_names(),
        "usage": "Use MCP tools `flux` and `scout`, or CLI commands `synapse2 flux ...` and `synapse2 scout ...`.",
        "examples": {
            "flux":  {"action": "docker", "subaction": "info"},
            "scout": {"action": "nodes"},
        }
    })
}

fn required_string_param(params: &Value, name: &str) -> Result<String> {
    optional_string_param(params, name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ValidationError::MissingField { field: name.into() }.into())
}

fn optional_string_param(params: &Value, name: &str) -> Result<Option<String>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }.into()),
    }
}

fn optional_u32_param(params: &Value, name: &str) -> Result<Option<u32>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }.into()),
    }
}

pub fn is_validation_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ValidationError>().is_some()
        || error
            .downcast_ref::<crate::app::ScaffoldIntentValidationError>()
            .is_some()
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

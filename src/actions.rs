use anyhow::Result;
use serde_json::{json, Value};

// ── Submodules ────────────────────────────────────────────────────────────────

mod dispatch;
mod flux;
pub(crate) mod scout;

// ── Re-exports (keep crate::actions::X resolving for all callers) ─────────────

pub use dispatch::{execute_service_action, is_confirmation_denied, is_validation_error};
pub use flux::{ComposeArgs, ContainerArgs, DockerArgs, HostArgs};
pub use scout::{
    ScoutBeamArgs, ScoutDeltaArgs, ScoutEmitArgs, ScoutEmitTarget, ScoutExecArgs, ScoutFindArgs,
    ScoutLogsArgs, ScoutPsArgs, ScoutZfsArgs,
};

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

// ── Scope constants & helpers ─────────────────────────────────────────────────

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
        name: "compose",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        // The action spec marks the top-level action; destructive subactions
        // (down/restart/recreate) are gated at the service layer via the
        // Confirmer — not through the action spec flag (which is used for
        // the schema-level readOnlyHint annotation only).
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
        name: "find",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "ps",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "df",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    ActionSpec {
        name: "delta",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    // exec/emit/beam are classified destructive (per synapse-mcp convention)
    // even though the exec allowlist limits them to read-only commands.
    // The Confirmer gate enforces this at the service layer (B5).
    ActionSpec {
        name: "exec",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        destructive: true,
    },
    ActionSpec {
        name: "emit",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        destructive: true,
    },
    ActionSpec {
        name: "beam",
        required_scope: Some(WRITE_SCOPE),
        transport: ActionTransport::Any,
        destructive: true,
    },
    // B15: ZFS read-only introspection (pools/datasets/snapshots).
    ActionSpec {
        name: "zfs",
        required_scope: Some(READ_SCOPE),
        transport: ActionTransport::Any,
        destructive: false,
    },
    // B15: Log retrieval read-only (syslog/journal/dmesg/auth).
    ActionSpec {
        name: "logs",
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

// ── SynapseAction enum ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynapseAction {
    FluxHelp,
    FluxDocker(Box<DockerArgs>),
    FluxContainer(Box<ContainerArgs>),
    FluxHost(Box<HostArgs>),
    FluxCompose(Box<ComposeArgs>),
    ScoutHelp,
    ScoutNodes,
    ScoutPeek {
        host: String,
        path: String,
        tree: bool,
        depth: u8,
    },
    ScoutFind(Box<ScoutFindArgs>),
    ScoutPs(Box<ScoutPsArgs>),
    ScoutDf {
        host: String,
        path: Option<String>,
    },
    ScoutDelta(Box<ScoutDeltaArgs>),
    ScoutExec(Box<ScoutExecArgs>),
    ScoutEmit(Box<ScoutEmitArgs>),
    ScoutBeam(Box<ScoutBeamArgs>),
    /// B15: ZFS subactions (pools/datasets/snapshots).
    ScoutZfs(Box<ScoutZfsArgs>),
    /// B15: Log subactions (syslog/journal/dmesg/auth).
    ScoutLogs(Box<ScoutLogsArgs>),
}

impl SynapseAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::FluxHelp | Self::ScoutHelp => "help",
            Self::FluxDocker(_) => "docker",
            Self::FluxContainer(_) => "container",
            Self::FluxHost(_) => "host",
            Self::FluxCompose(_) => "compose",
            Self::ScoutNodes => "nodes",
            Self::ScoutPeek { .. } => "peek",
            Self::ScoutFind(_) => "find",
            Self::ScoutPs(_) => "ps",
            Self::ScoutDf { .. } => "df",
            Self::ScoutDelta(_) => "delta",
            Self::ScoutExec(_) => "exec",
            Self::ScoutEmit(_) => "emit",
            Self::ScoutBeam(_) => "beam",
            Self::ScoutZfs(_) => "zfs",
            Self::ScoutLogs(_) => "logs",
        }
    }
}

// ── REST help ─────────────────────────────────────────────────────────────────

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

// ── Shared param helpers (used by flux.rs and scout.rs via super::) ───────────

pub(crate) fn required_string_param(params: &Value, name: &str) -> Result<String> {
    optional_string_param(params, name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ValidationError::MissingField { field: name.into() }.into())
}

pub(crate) fn optional_string_param(params: &Value, name: &str) -> Result<Option<String>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }.into()),
    }
}

/// Require a non-empty optional string field, returning a `MissingField`
/// validation error when absent or empty.
pub(crate) fn require_field<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str> {
    value
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ValidationError::MissingField { field: name.into() }.into())
}

/// Require a `container_id` for single-container subactions.
pub(crate) fn require_container_id(container_id: &Option<String>) -> Result<&str> {
    container_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ValidationError::MissingField {
                field: "container_id".into(),
            }
            .into()
        })
}

pub(crate) fn optional_bool_param(params: &Value, name: &str) -> Result<Option<bool>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }.into()),
    }
}

pub(crate) fn optional_u32_param(params: &Value, name: &str) -> Result<Option<u32>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }.into()),
    }
}

pub(crate) fn optional_u64_param(params: &Value, name: &str) -> Result<Option<u64>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| ValidationError::WrongType { field: name.into() }.into()),
    }
}

/// Extract an optional array of strings from `params[name]`.
/// Returns an empty `Vec` when the key is absent; errors on type mismatch.
pub(crate) fn optional_string_array_param(params: &Value, name: &str) -> Result<Vec<String>> {
    match params.get(name) {
        None => Ok(Vec::new()),
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|v| {
                v.as_str().map(|s| s.to_owned()).ok_or_else(|| {
                    ValidationError::WrongType {
                        field: format!("{name}[]"),
                    }
                    .into()
                })
            })
            .collect(),
        Some(_) => Err(ValidationError::WrongType { field: name.into() }.into()),
    }
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

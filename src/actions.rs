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

/// Parsed parameters for `flux container` subactions.
///
/// Boxed inside [`SynapseAction::FluxContainer`] (and mirrored by the CLI
/// `Command`) so the enum stays small — every read-only container subaction's
/// params live here. Extraction stays in the shim; logic lives in `FluxService`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerArgs {
    pub subaction: String,
    pub container_id: Option<String>,
    pub host: Option<String>,
    pub lines: Option<u32>,
    // list filters
    pub state: Option<String>,
    pub name_filter: Option<String>,
    pub image_filter: Option<String>,
    pub label_filter: Option<String>,
    // logs params
    pub since: Option<String>,
    pub until: Option<String>,
    pub grep: Option<String>,
    pub stream: Option<String>,
    // inspect param
    pub summary: Option<bool>,
    // search param
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynapseAction {
    FluxHelp,
    FluxDocker {
        subaction: String,
    },
    FluxContainer(Box<ContainerArgs>),
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
            Self::FluxContainer(_) => "container",
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
            "container" => {
                // Validate `response_format` at the shim per B4 contract (no-op
                // on output shape today; full rendering wiring is a separate
                // codebase-wide concern). Invalid value → hard error.
                if let Some(rf) = optional_string_param(args, "response_format")? {
                    crate::formatters::ResponseFormat::parse(Some(&rf))
                        .map_err(|e| anyhow::anyhow!(e))?;
                }
                Ok(Self::FluxContainer(Box::new(ContainerArgs {
                    subaction: required_string_param(args, "subaction")?,
                    container_id: optional_string_param(args, "container_id")?,
                    host: optional_string_param(args, "host")?,
                    lines: optional_u32_param(args, "lines")?,
                    state: optional_string_param(args, "state")?,
                    name_filter: optional_string_param(args, "name_filter")?,
                    image_filter: optional_string_param(args, "image_filter")?,
                    label_filter: optional_string_param(args, "label_filter")?,
                    since: optional_string_param(args, "since")?,
                    until: optional_string_param(args, "until")?,
                    grep: optional_string_param(args, "grep")?,
                    stream: optional_string_param(args, "stream")?,
                    summary: optional_bool_param(args, "summary")?,
                    query: optional_string_param(args, "query")?,
                })))
            }
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
        SynapseAction::FluxHelp => service.flux().help().await,
        SynapseAction::FluxDocker { subaction } => match subaction.as_str() {
            "info" => service.flux().docker_info().await,
            "images" => service.flux().docker_images().await,
            "networks" => service.flux().docker_networks().await,
            "volumes" => service.flux().docker_volumes().await,
            other => Err(ValidationError::UnknownAction {
                action: format!("docker:{other}"),
            }
            .into()),
        },
        SynapseAction::FluxContainer(args) => dispatch_flux_container(service, args).await,
        SynapseAction::FluxHost { subaction, host } => match subaction.as_str() {
            "status" => service.flux().host_status(host.as_deref()).await,
            other => Err(ValidationError::UnknownAction {
                action: format!("host:{other}"),
            }
            .into()),
        },
        SynapseAction::ScoutHelp => service.scout().help().await,
        SynapseAction::ScoutNodes => service.scout().nodes().await,
        SynapseAction::ScoutPeek { host, path } => service.scout().peek(host, path).await,
        SynapseAction::ScoutExec {
            host,
            path,
            command,
        } => service.scout().exec(host, path, command).await,
    }
}

/// Dispatch a `flux container` read-only subaction to the [`FluxService`].
///
/// Thin: extracts the parsed [`ContainerArgs`] and calls the matching service
/// method. All filtering/fanout logic lives in `FluxService` / `container_read`.
async fn dispatch_flux_container(service: &SynapseService, args: &ContainerArgs) -> Result<Value> {
    use crate::flux_service::container_read::{ListFilters, LogOptions, DEFAULT_LOG_LINES};
    let flux = service.flux();
    let host = args.host.as_deref();
    match args.subaction.as_str() {
        "list" => {
            let filters = ListFilters {
                state: args.state.clone(),
                name_filter: args.name_filter.clone(),
                image_filter: args.image_filter.clone(),
                label_filter: args.label_filter.clone(),
            };
            flux.container_list(host, filters).await
        }
        "search" => {
            let q = args.query.as_deref().ok_or(ValidationError::MissingField {
                field: "query".into(),
            })?;
            flux.container_search(host, q).await
        }
        "stats" => {
            flux.container_stats(host, args.container_id.as_deref())
                .await
        }
        "inspect" => {
            flux.container_inspect(
                host,
                require_container_id(&args.container_id)?,
                args.summary.unwrap_or(false),
            )
            .await
        }
        "top" => {
            flux.container_top(host, require_container_id(&args.container_id)?)
                .await
        }
        "logs" => {
            let opts = LogOptions {
                lines: args.lines.unwrap_or(DEFAULT_LOG_LINES),
                since: args.since.clone(),
                until: args.until.clone(),
                grep: args.grep.clone(),
                stream: args.stream.clone().unwrap_or_else(|| "both".to_owned()),
            };
            flux.container_logs(host, require_container_id(&args.container_id)?, opts)
                .await
        }
        other => Err(ValidationError::UnknownAction {
            action: format!("container:{other}"),
        }
        .into()),
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

/// Require a `container_id` for single-container subactions.
fn require_container_id(container_id: &Option<String>) -> Result<&str> {
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

fn optional_bool_param(params: &Value, name: &str) -> Result<Option<bool>> {
    match params.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
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

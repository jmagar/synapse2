//! Business service layer.
//!
//! **All business logic lives here.** CLI and MCP are thin shims that call into this.
//!
//! `SynapseService` owns an `SynapseClient` and exposes typed methods.
//! If you need caching, retries, data transformation, or validation, do it here —
//! never in `cli.rs` or `mcp/tools.rs`.

use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::compose::{ComposeDiscovery, ComposeProject};
use crate::host_config::{FileHostRepository, HostRepository};
use crate::ssh::SshPool;
use crate::synapse2::SynapseClient;
use crate::{docker, scout};

// Unit tests live in a sidecar file — see src/app_tests.rs for the pattern.
#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

/// The service layer — wraps the transport client and adds business logic.
///
/// **Template**: rename this to `MyServiceService` (or whatever fits).
/// Add any fields you need: caches, config, metrics, etc.
#[derive(Clone)]
pub struct SynapseService {
    client: SynapseClient,
    /// Host configuration repository — injected for testability.
    /// Defaults to `FileHostRepository` (reads env / disk / `~/.ssh/config`).
    pub host_repo: Arc<dyn HostRepository>,
    /// Compose project discovery engine + per-host TTL cache (B12).
    ///
    /// Held behind `Arc` so the shared cache survives `SynapseService::clone()`
    /// — a fresh per-request engine would never hit the cache. Defaults to an
    /// engine over a fresh `SshPool`. B13 (compose operations) consumes this.
    pub compose: Arc<ComposeDiscovery>,
}

#[derive(Debug, Clone)]
pub struct ScaffoldIntent {
    pub display_name: String,
    pub crate_name: String,
    pub binary_name: String,
    pub server_category: String,
    pub env_prefix: String,
    pub auth_kind: String,
    pub host: String,
    pub port: u16,
    pub mcp_transport: String,
    pub mcp_primitives: String,
    pub deployment: String,
    pub plugins: String,
    pub publish_mcp: bool,
    pub crawl_urls: String,
    pub crawl_repos: String,
    pub crawl_search_topics: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElicitedNameOutcome<'a> {
    Accepted(&'a str),
    NoInput,
    Declined,
    Cancelled,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldIntentValidationError {
    message: String,
}

impl ScaffoldIntentValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ScaffoldIntentValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ScaffoldIntentValidationError {}

impl SynapseService {
    /// Create a new `SynapseService` with the production host repository.
    ///
    /// The `client` parameter signature is unchanged — existing callers compile as-is.
    pub fn new(client: SynapseClient) -> Self {
        Self {
            client,
            host_repo: Arc::new(FileHostRepository::default()),
            compose: Arc::new(ComposeDiscovery::new(Arc::new(SshPool::new()))),
        }
    }

    /// Inject a custom `HostRepository` (for testing or future DI).
    pub fn with_host_repo(mut self, repo: Arc<dyn HostRepository>) -> Self {
        self.host_repo = repo;
        self
    }

    /// Inject a custom compose discovery engine (for testing or future DI).
    pub fn with_compose_discovery(mut self, compose: Arc<ComposeDiscovery>) -> Self {
        self.compose = compose;
        self
    }

    /// Discover compose projects on `host_name`, merging `docker compose ls`
    /// with a filesystem scan (cache-aware). Thin delegation to the discovery
    /// engine; resolves the host via the injected repository.
    pub async fn compose_list(&self, host_name: &str) -> Result<Vec<ComposeProject>> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        self.compose.list(&host).await
    }

    /// Invalidate the compose discovery cache for `host_name` (or all hosts when
    /// `None`), forcing the next `compose_list` to re-scan.
    pub fn compose_refresh(&self, host_name: Option<&str>) {
        self.compose.refresh(host_name);
    }

    /// Return a greeting for `name`, defaulting to "World".
    pub async fn greet(&self, name: Option<&str>) -> Result<Value> {
        self.client.greet(name).await
    }

    /// Echo `message` back unchanged.
    pub async fn echo(&self, message: &str) -> Result<Value> {
        self.client.echo(message).await
    }

    /// Return the server status.
    pub async fn status(&self) -> Result<Value> {
        self.client.status().await
    }

    pub async fn flux_help(&self) -> Result<Value> {
        Ok(json!({
            "tool": "flux",
            "actions": {
                "docker": ["info", "images", "networks", "volumes"],
                "container": ["list", "inspect", "logs"],
                "host": ["status"],
                "help": []
            },
            "deferred": ["compose", "destructive container lifecycle", "docker prune/rmi"],
        }))
    }

    pub async fn flux_docker_info(&self) -> Result<Value> {
        docker::docker_json(&["info", "--format", "{{json .}}"]).await
    }

    pub async fn flux_docker_images(&self) -> Result<Value> {
        docker::docker_json(&["images", "--format", "{{json .}}"]).await
    }

    pub async fn flux_docker_networks(&self) -> Result<Value> {
        docker::docker_json(&["network", "ls", "--format", "{{json .}}"]).await
    }

    pub async fn flux_docker_volumes(&self) -> Result<Value> {
        docker::docker_json(&["volume", "ls", "--format", "{{json .}}"]).await
    }

    pub async fn flux_container_list(&self) -> Result<Value> {
        docker::docker_json(&["container", "ls", "-a", "--format", "{{json .}}"]).await
    }

    pub async fn flux_container_inspect(&self, container_id: &str) -> Result<Value> {
        docker::docker_json(&["container", "inspect", container_id]).await
    }

    pub async fn flux_container_logs(&self, container_id: &str, lines: u32) -> Result<Value> {
        let lines = lines.clamp(1, 500).to_string();
        docker::docker_json(&["container", "logs", "--tail", &lines, container_id]).await
    }

    pub async fn flux_host_status(&self, host: Option<&str>) -> Result<Value> {
        Ok(json!({
            "host": host.unwrap_or("local"),
            "docker": self.flux_docker_info().await?,
        }))
    }

    pub async fn scout_help(&self) -> Result<Value> {
        Ok(json!({
            "tool": "scout",
            "actions": ["nodes", "peek", "exec", "help"],
            "deferred": ["find", "delta", "emit", "beam", "ps", "df", "zfs", "logs"],
        }))
    }

    pub async fn scout_nodes(&self) -> Result<Value> {
        scout::nodes(self.host_repo.as_ref())
    }

    pub async fn scout_peek(&self, host: &str, path: &str) -> Result<Value> {
        scout::peek(self.host_repo.as_ref(), host, path)
    }

    pub async fn scout_exec(&self, host: &str, path: &str, command: &str) -> Result<Value> {
        scout::exec(self.host_repo.as_ref(), host, path, command)
    }

    /// Build the response for the elicited-name demo after the MCP shim collects input.
    pub fn elicited_name_greeting(&self, outcome: ElicitedNameOutcome<'_>) -> Value {
        match outcome {
            ElicitedNameOutcome::Accepted(name) => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    json!({
                        "greeting": "Hello, mysterious stranger!",
                        "note": "You submitted an empty name - that's perfectly fine!",
                    })
                } else {
                    json!({
                        "greeting": format!("Hello, {name}! Welcome to the synapse2 MCP server."),
                        "name": name,
                    })
                }
            }
            ElicitedNameOutcome::NoInput => json!({
                "greeting": "Hello! (you provided no name - that's okay)",
            }),
            ElicitedNameOutcome::Declined => json!({
                "message": "No problem - you chose not to share your name.",
                "greeting": "Hello, anonymous user!",
            }),
            ElicitedNameOutcome::Cancelled => json!({
                "message": "Elicitation was cancelled.",
                "greeting": "Hello there!",
            }),
            ElicitedNameOutcome::Unsupported => json!({
                "message": "Elicitation is not supported by this MCP client.",
                "hint": "Try a client like Claude.app that supports MCP elicitation (spec 2025-06-18).",
                "fallback_greeting": "Hello, World! (elicitation unavailable)",
            }),
        }
    }

    /// Convert elicited scaffold requirements into the handoff contract consumed by the skill.
    pub fn scaffold_intent(&self, input: ScaffoldIntent) -> Result<Value> {
        validate_scaffold_intent(&input)?;
        let category = normalize_category(&input.server_category);
        let required_surfaces = if category == "application-platform" {
            vec!["api", "cli", "mcp", "web"]
        } else {
            vec!["mcp", "cli"]
        };
        let service_name = input.binary_name.trim().replace('-', "_");
        let env_prefix = input.env_prefix.trim().to_ascii_uppercase();

        Ok(json!({
            "kind": "synapse2_scaffold_intent",
            "schema_version": 1,
            "server_category": category,
            "required_surfaces": required_surfaces,
            "project": {
                "display_name": input.display_name.trim(),
                "crate_name": input.crate_name.trim(),
                "binary_name": input.binary_name.trim(),
                "service_name": service_name,
                "env_prefix": env_prefix,
            },
            "upstream": {
                "base_url_env": format!("{env_prefix}_API_URL"),
                "auth_kind": normalize_auth_kind(&input.auth_kind),
            },
            "runtime": {
                "host": normalize_host(&input.host),
                "port": input.port,
                "mcp_transport": normalize_transport(&input.mcp_transport),
            },
            "mcp_primitives": normalize_primitives(&input.mcp_primitives),
            "deployment": normalize_deployment(&input.deployment),
            "plugins": normalize_plugins(&input.plugins),
            "publish_mcp": input.publish_mcp,
            "crawl_docs": {
                "urls": split_csv(&input.crawl_urls),
                "repos": split_csv(&input.crawl_repos),
                "search_topics": split_csv(&input.crawl_search_topics),
            },
            "handoff": {
                "recommended_skill": "scaffold-project",
                "instructions": "Create an approval-first scaffold plan from this JSON. Do not mutate files until the user approves the plan.",
            },
            "policy": {
                "business_action_minimum_surfaces": ["mcp", "cli"],
                "upstream_client_surfaces": ["mcp", "cli"],
                "application_platform_surfaces": ["api", "cli", "mcp", "web"],
            }
        }))
    }
}

fn validate_scaffold_intent(input: &ScaffoldIntent) -> Result<()> {
    validate_non_empty("display_name", &input.display_name)?;
    validate_kebab_identifier("crate_name", &input.crate_name)?;
    validate_kebab_identifier("binary_name", &input.binary_name)?;
    validate_env_prefix(&input.env_prefix)?;
    if input.port == 0 {
        return Err(ScaffoldIntentValidationError::new("port must be between 1 and 65535").into());
    }
    validate_urls("crawl_urls", &input.crawl_urls)?;
    validate_urls("crawl_repos", &input.crawl_repos)?;
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ScaffoldIntentValidationError::new(format!(
            "`{field}` is required and must not be empty"
        ))
        .into());
    }
    Ok(())
}

fn validate_kebab_identifier(field: &str, value: &str) -> Result<()> {
    let value = value.trim();
    validate_non_empty(field, value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(ScaffoldIntentValidationError::new(format!(
            "`{field}` is required and must not be empty"
        ))
        .into());
    };
    if !first.is_ascii_lowercase()
        || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err(ScaffoldIntentValidationError::new(format!(
            "`{field}` must match ^[a-z][a-z0-9-]*$"
        ))
        .into());
    }
    Ok(())
}

fn validate_env_prefix(value: &str) -> Result<()> {
    let value = value.trim().to_ascii_uppercase();
    validate_non_empty("env_prefix", &value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(ScaffoldIntentValidationError::new(
            "`env_prefix` is required and must not be empty",
        )
        .into());
    };
    if !first.is_ascii_uppercase()
        || !chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(ScaffoldIntentValidationError::new(
            "`env_prefix` must match ^[A-Z][A-Z0-9_]*$",
        )
        .into());
    }
    Ok(())
}

fn validate_urls(field: &str, value: &str) -> Result<()> {
    for item in split_csv(value) {
        url::Url::parse(&item).map_err(|_| {
            ScaffoldIntentValidationError::new(format!("`{field}` contains invalid URL: {item}"))
        })?;
    }
    Ok(())
}

fn normalize_category(category: &str) -> &'static str {
    let normalized = category.trim().to_ascii_lowercase();
    if normalized.contains("application") || normalized.contains("platform") {
        "application-platform"
    } else {
        "upstream-client"
    }
}

fn normalize_auth_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => "none",
        "api-key" | "apikey" | "api_key" | "api key" | "key" => "api-key",
        "bearer" | "token" => "bearer",
        "oauth" => "oauth",
        "both" => "both",
        _ => "other",
    }
}

fn normalize_host(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "127.0.0.1".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_transport(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "stdio" => "stdio",
        "http" | "streamable-http" | "streamable_http" => "http",
        _ => "dual",
    }
}

fn normalize_deployment(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "systemd" => "systemd",
        "docker" | "container" | "containers" => "docker",
        _ => "none",
    }
}

fn normalize_primitives(value: &str) -> Vec<String> {
    let requested = split_csv(value);
    let mut primitives = Vec::new();
    for item in requested {
        let primitive = match item.to_ascii_lowercase().as_str() {
            "tools" | "tool" => Some("tools"),
            "resources" | "resource" => Some("resources"),
            "prompts" | "prompt" => Some("prompts"),
            "elicitation" | "elicit" => Some("elicitation"),
            _ => None,
        };
        if let Some(primitive) = primitive {
            let primitive = primitive.to_owned();
            if !primitives.contains(&primitive) {
                primitives.push(primitive);
            }
        }
    }
    if primitives.is_empty() {
        primitives.push("tools".to_owned());
    }
    primitives
}

fn normalize_plugins(value: &str) -> Vec<String> {
    let requested = split_csv(value);
    let mut plugins = Vec::new();
    for item in requested {
        let plugin = match item.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" | "claude_code" => Some("claude"),
            "codex" => Some("codex"),
            "gemini" => Some("gemini"),
            "none" => None,
            _ => None,
        };
        if let Some(plugin) = plugin {
            let plugin = plugin.to_owned();
            if !plugins.contains(&plugin) {
                plugins.push(plugin);
            }
        }
    }
    plugins
}

fn split_csv(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    for item in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let item = item.to_owned();
        if !items.contains(&item) {
            items.push(item);
        }
    }
    items
}

//! MCP resource enumeration and read handlers.
//!
//! Exposed resource URIs (scheme `synapse://`):
//!
//! | URI | Description | Source |
//! |-----|-------------|--------|
//! | `synapse://schema/flux` | Full flux JSON tool schema | static |
//! | `synapse://schema/scout` | Full scout JSON tool schema | static |
//! | `synapse://hosts` | Configured host list as JSON | live from host repo |
//! | `synapse://compose/projects` | Compose project list | live from ComposeDiscovery cache |
//! | `synapse://help/flux` | Flux full help text (markdown) | static |
//! | `synapse://help/scout` | Scout full help text (markdown) | static |
//!
//! Wire [`list_resources`] and [`read_resource`] from `rmcp_server.rs`.

use anyhow::{Result, bail};
use rmcp::model::{RawResource, Resource, ResourceContents};
use serde_json::Value;

use crate::server::AppState;

use super::{help::full_domain_markdown, schemas::tool_definitions};

// ── URI constants ─────────────────────────────────────────────────────────────

pub const URI_SCHEMA_FLUX: &str = "synapse://schema/flux";
pub const URI_SCHEMA_SCOUT: &str = "synapse://schema/scout";
pub const URI_HOSTS: &str = "synapse://hosts";
pub const URI_COMPOSE_PROJECTS: &str = "synapse://compose/projects";
pub const URI_HELP_FLUX: &str = "synapse://help/flux";
pub const URI_HELP_SCOUT: &str = "synapse://help/scout";

/// All resource URIs exposed by this server (ordered, stable).
#[cfg(test)]
pub const ALL_URIS: &[&str] = &[
    URI_SCHEMA_FLUX,
    URI_SCHEMA_SCOUT,
    URI_HOSTS,
    URI_COMPOSE_PROJECTS,
    URI_HELP_FLUX,
    URI_HELP_SCOUT,
];

// ── Resource metadata ─────────────────────────────────────────────────────────

/// Build the full list of resource descriptors for `list_resources`.
pub fn all_resources() -> Vec<Resource> {
    vec![
        make_resource(
            URI_SCHEMA_FLUX,
            "flux tool schema",
            "JSON schema for the flux MCP tool",
            "application/json",
        ),
        make_resource(
            URI_SCHEMA_SCOUT,
            "scout tool schema",
            "JSON schema for the scout MCP tool",
            "application/json",
        ),
        make_resource(
            URI_HOSTS,
            "configured hosts",
            "Current host list from the host repository",
            "application/json",
        ),
        make_resource(
            URI_COMPOSE_PROJECTS,
            "compose projects",
            "Current compose project list from the discovery cache",
            "application/json",
        ),
        make_resource(
            URI_HELP_FLUX,
            "flux help",
            "Full flux tool help text in markdown",
            "text/markdown",
        ),
        make_resource(
            URI_HELP_SCOUT,
            "scout help",
            "Full scout tool help text in markdown",
            "text/markdown",
        ),
    ]
}

fn make_resource(uri: &str, name: &str, description: &str, mime: &str) -> Resource {
    Resource::new(
        RawResource::new(uri, name.to_string())
            .with_description(description.to_string())
            .with_mime_type(mime),
        None,
    )
}

// ── Resource content handlers ─────────────────────────────────────────────────

/// Read a resource by URI. Returns `(content_text, mime_type)` or an error if
/// the URI is unknown.
pub async fn read_resource(uri: &str, state: &AppState) -> Result<ResourceContents> {
    match uri {
        URI_SCHEMA_FLUX | URI_SCHEMA_SCOUT => read_schema_resource(uri),
        URI_HOSTS => read_hosts_resource(state).await,
        URI_COMPOSE_PROJECTS => read_compose_projects_resource(state).await,
        URI_HELP_FLUX => Ok(read_help_resource("flux", uri)),
        URI_HELP_SCOUT => Ok(read_help_resource("scout", uri)),
        other => bail!("unknown resource: {other}"),
    }
}

fn read_schema_resource(uri: &str) -> Result<ResourceContents> {
    let schemas = tool_definitions();
    let text = serde_json::to_string_pretty(schemas)?;
    Ok(ResourceContents::text(text, uri).with_mime_type("application/json"))
}

async fn read_hosts_resource(state: &AppState) -> Result<ResourceContents> {
    let hosts = state.service.flux().host_repo.load_hosts()?;
    let json: Value = serde_json::to_value(&hosts)?;
    let text = serde_json::to_string_pretty(&json)?;
    Ok(ResourceContents::text(text, URI_HOSTS).with_mime_type("application/json"))
}

async fn read_compose_projects_resource(state: &AppState) -> Result<ResourceContents> {
    let hosts = state.service.flux().host_repo.load_hosts()?;
    let compose = &state.service.flux().compose;

    let mut host_projects: Vec<Value> = Vec::new();
    for host in &hosts {
        match compose.list(host).await {
            Ok(projects) => {
                let projects_json: Value = serde_json::to_value(&projects)?;
                host_projects.push(serde_json::json!({
                    "host": host.name,
                    "projects": projects_json,
                }));
            }
            Err(e) => {
                tracing::warn!(host = %host.name, "compose discovery failed: {e}");
                host_projects.push(serde_json::json!({
                    "host": host.name,
                    "error": e.to_string(),
                }));
            }
        }
    }

    let text = serde_json::to_string_pretty(&host_projects)?;
    Ok(ResourceContents::text(text, URI_COMPOSE_PROJECTS).with_mime_type("application/json"))
}

fn read_help_resource(domain: &str, uri: &str) -> ResourceContents {
    let text = full_domain_markdown(domain);
    ResourceContents::text(text, uri).with_mime_type("text/markdown")
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;

//! `synapse2` library crate.
// The `json!` macro in schemas.rs requires a higher recursion limit due to the
// large size of the tool schema definitions. 256 is sufficient; the default is 128.
#![recursion_limit = "256"]
//!
//! Exposes the service layer, config, and transport client so that integration
//! tests can import them without duplicating state construction.
//!
//! Public modules:
//!   [`app`]         — `SynapseService` (business logic)
//!   [`cache`]       — `Cache` trait and `MemoryCache` implementation (TTL, LRU eviction)
//!   [`fanout`]      — multi-host fanout helper with `PartialSuccess` aggregation
//!   [`formatters`]  — `ResponseFormat` enum + per-domain markdown renderers
//!   [`config`]      — `Config`, `McpConfig`
//!   [`host_config`] — `HostRepository` trait + `FileHostRepository` (precedence chain + SSH auto-discovery)
//!   [`mcp`]         — MCP protocol layer (tools, schemas, prompts, server handler)
//!   [`server`]      — `AppState`, `AuthPolicy`, HTTP router
//!   [`api`]         — REST API handlers (`POST /v1/synapse2`, health, status)

pub mod actions;
pub mod api;
pub mod app;
pub mod cache;
pub mod cli;
pub mod color_policy;
pub mod compose;
pub mod config;
pub mod docker;
pub mod docker_client;
pub mod elicitation_gate;
pub mod fanout;
pub mod flux_service;
pub mod formatters;
pub mod host_config;
pub mod logging;
pub mod mcp;
pub mod runtime_budget;
pub mod scaffold;
pub mod scout;
pub mod scout_service;
pub mod server;
pub mod ssh;
pub mod synapse;
pub mod token_limit;
pub mod web;

/// Test helpers — available when `features = ["test-support"]` or in `cfg(test)`.
///
/// Use these in integration tests to construct `AppState` without real creds.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod testing {
    use std::sync::Arc;

    use crate::{
        app::SynapseService,
        config::McpConfig,
        server::{AppState, AuthPolicy},
    };

    /// Re-export of the Docker client test double so action-bead integration
    /// tests (B8/B9/B10/B13, in the separate `tests/` crate) can construct a
    /// `&dyn DockerClient` without a live daemon.
    pub use crate::docker_client::MockDockerClient;

    fn stub_service() -> SynapseService {
        SynapseService::new()
    }

    /// `AppState` with no auth (loopback trust boundary).
    /// Use this for unit tests that don't need auth.
    pub fn loopback_state() -> AppState {
        AppState {
            config: McpConfig::default(),
            auth_policy: AuthPolicy::LoopbackDev,
            service: stub_service(),
        }
    }

    /// `AppState` requiring a static bearer token.
    pub fn bearer_state(token: &str) -> AppState {
        AppState {
            config: McpConfig {
                api_token: Some(token.to_string()),
                ..McpConfig::default()
            },
            auth_policy: AuthPolicy::Mounted { auth_state: None },
            service: stub_service(),
        }
    }

    /// `AppState` with full OAuth (requires data directory for SQLite + key file).
    pub async fn oauth_state(data_dir: &std::path::Path) -> AppState {
        let auth_state = build_auth_state(data_dir).await;
        AppState {
            config: McpConfig {
                auth: crate::config::AuthConfig {
                    public_url: Some("https://synapse2.synapse2.com".to_string()),
                    ..Default::default()
                },
                ..McpConfig::default()
            },
            auth_policy: AuthPolicy::Mounted {
                auth_state: Some(Arc::new(auth_state)),
            },
            service: stub_service(),
        }
    }

    pub async fn build_auth_state(data_dir: &std::path::Path) -> lab_auth::state::AuthState {
        let vars: Vec<(String, String)> = vec![
            ("SYNAPSE_MCP_AUTH_MODE".into(), "oauth".into()),
            (
                "SYNAPSE_MCP_PUBLIC_URL".into(),
                "https://synapse2.synapse2.com".into(),
            ),
            (
                "SYNAPSE_MCP_GOOGLE_CLIENT_ID".into(),
                "test-client-id".into(),
            ),
            (
                "SYNAPSE_MCP_GOOGLE_CLIENT_SECRET".into(),
                "test-client-secret".into(),
            ),
            (
                "SYNAPSE_MCP_AUTH_ADMIN_EMAIL".into(),
                "admin@synapse2.com".into(),
            ),
            (
                "SYNAPSE_MCP_AUTH_SQLITE_PATH".into(),
                data_dir.join("auth.db").display().to_string(),
            ),
            (
                "SYNAPSE_MCP_AUTH_KEY_PATH".into(),
                data_dir.join("auth-jwt.pem").display().to_string(),
            ),
        ];

        let auth_config = lab_auth::config::AuthConfigBuilder::new()
            .env_prefix("SYNAPSE_MCP")
            .session_cookie_name("synapse_mcp_session")
            .scopes_supported(vec![
                crate::actions::READ_SCOPE.into(),
                crate::actions::WRITE_SCOPE.into(),
            ])
            .default_scope("synapse:read")
            .resource_path("/mcp")
            .build_from_sources(vars)
            .expect("test auth config should build");

        lab_auth::state::AuthState::new(auth_config)
            .await
            .expect("test auth state should init")
    }
}

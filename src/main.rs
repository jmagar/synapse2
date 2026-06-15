//! Binary entry point — mode dispatch only.
//!
//! Modes:
//!   `synapse2 [serve]`        Start MCP HTTP server (default if no args)
//!   `synapse2 mcp`            Start MCP stdio transport
//!   `synapse2 flux ...`       CLI flux (Docker / container / host / compose) commands
//!   `synapse2 scout ...`      CLI scout (SSH / filesystem / exec) commands
//!   `synapse2 --help`         Print usage
//!   `synapse2 --version`      Print version

use anyhow::Result;
use std::sync::Arc;

use rmcp::{ServiceExt, transport::stdio};
use synapse2::{
    app::SynapseService,
    cli,
    config::{Config, load_dotenv_environment},
    mcp,
    server::{self, AppState, AuthPolicy, AuthPolicyKind, resolve_auth_policy_kind},
};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

fn main() -> Result<()> {
    // Load `.env` into the process environment BEFORE starting the Tokio
    // runtime. `std::env::set_var` is only sound while single-threaded; doing it
    // here (no runtime, no worker threads, nothing reading the environment
    // concurrently) keeps the `unsafe` in `load_dotenv_environment` actually safe.
    load_dotenv_environment()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    cli::install_color_from_args(&mut args)?;

    // Handle meta-flags before initialising logging (they print and exit)
    if cli::maybe_handle_help(&args) {
        return Ok(());
    }
    match args.as_slice() {
        [f] if matches!(f.as_str(), "--version" | "-V" | "version") => {
            println!("synapse2 {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    // Suppress logs in stdio/CLI mode — MCP clients communicate over stdio
    // and cannot tolerate log lines mixed into the JSON stream.
    let stdio_mode = matches!(args.as_slice(), [c] if c == "mcp");
    let serve_mode = args.is_empty()
        || matches!(args.as_slice(), [c] if c == "serve")
        || matches!(args.as_slice(), [a, b] if a == "serve" && b == "mcp");

    let log_level = if stdio_mode || !serve_mode {
        "warn"
    } else {
        "info"
    };

    // In stdio and CLI modes use a lightweight inline subscriber.
    // Stdio mode MUST stay at warn-level so log lines don't corrupt the
    // JSON-RPC stream on stdout. In serve_mode the full logging::init()
    // path (with file sink) is used instead — see below.
    //
    // When LOG_FORMAT=json or RUST_LOG_FORMAT=json, emit JSON lines so that
    // container log aggregators (Loki, Datadog, etc.) receive structured data.
    let json_format = synapse2::logging::json_format_requested();
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));
    if json_format {
        fmt()
            .json()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .with_target(true)
            .init();
    } else {
        fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .with_target(true)
            .init();
    }

    if serve_mode || stdio_mode {
        // Startup sweep: remove stale forwarded sockets from prior runs whose
        // owning pid is dead (sockets persist on SIGKILL/panic). Must run before
        // any SSH pool / port-forward initialisation so a leftover
        // `/tmp/synapse2-*-*.sock` cannot shadow a fresh forward. Server modes
        // only — a one-shot CLI invocation should not sweep shared `/tmp`.
        synapse2::ssh::sweep_stale_sockets();
        // Warn if known_hosts has wildcard patterns that defeat strict host-key
        // checking (suppressed in stdio mode since logs are warn-level only and
        // must not pollute the JSON-RPC stream — tracing writes to stderr, OK).
        synapse2::ssh::warn_on_known_hosts_wildcards();
    }

    if serve_mode {
        serve_mcp().await
    } else if stdio_mode {
        serve_stdio_mcp().await
    } else {
        run_cli(args).await
    }
}

// ── modes ─────────────────────────────────────────────────────────────────────

/// Start the MCP HTTP server (Streamable HTTP transport).
async fn serve_mcp() -> Result<()> {
    let config = Config::load()?;
    enforce_destructive_policy(&config)?;
    let state = build_state(config).await?;

    info!(
        bind = %state.config.bind_addr(),
        server_name = %state.config.server_name,
        auth = ?state.auth_policy,
        "synapse2 starting"
    );

    let bind = state.config.bind_addr();
    let app = server::router(state).layer(tower_http::trace::TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!(bind = %bind, "MCP HTTP server listening");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Start the MCP stdio transport (for local/subprocess MCP clients).
///
/// Stdio is always LoopbackDev — it's a local trusted pipe between parent and
/// child process. HTTP auth middleware doesn't apply; forcing Mounted here
/// breaks all stdio clients with "forbidden: missing http context".
async fn serve_stdio_mcp() -> Result<()> {
    let config = Config::load()?;
    let service = SynapseService::new();
    let state = AppState {
        config: config.mcp,
        auth_policy: AuthPolicy::LoopbackDev, // stdio = trusted local transport
        service,
    };
    let svc = mcp::rmcp_server(state).serve(stdio()).await?;
    svc.waiting().await?;
    Ok(())
}

/// Dispatch CLI subcommands.
async fn run_cli(args: Vec<String>) -> Result<()> {
    let config = Config::load()?;
    match cli::parse_args_from(args)? {
        Some(cli::Command::Doctor { json }) => {
            // Doctor needs the full Config (not just SynapseConfig) to check
            // MCP port, auth mode, etc. — intercept here before service construction.
            cli::doctor::run_doctor(&config, json).await
        }
        Some(cli::Command::Watch { url, interval }) => {
            // Watch needs the MCP port to build the default URL but no service layer.
            let base = url.unwrap_or_else(|| format!("http://localhost:{}", config.mcp.port));
            cli::watch::run_watch(&base, interval).await
        }
        Some(cli::Command::Setup(command)) => cli::run_setup(&config, command).await,
        Some(cmd) => cli::run(cmd).await,
        None => {
            eprintln!("Unknown command. Run `synapse2 --help` for usage.");
            cli::print_top_level_help_stderr();
            std::process::exit(1);
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Surface the `SYNAPSE_MCP_ALLOW_DESTRUCTIVE` override at startup and refuse to
/// bind when it is enabled on a non-loopback host.
///
/// - Override set: WARN so an accidental prod leak (e.g. from a dev `.env`) is
///   visible in tracing output.
/// - Override set AND bound to a non-loopback address: ERROR + refuse to start —
///   an internet-reachable server that skips destructive confirmation is a
///   critical misconfiguration.
fn enforce_destructive_policy(config: &Config) -> Result<()> {
    if !config.mcp.allow_destructive {
        return Ok(());
    }
    if config.mcp.is_loopback() {
        tracing::warn!(
            "SYNAPSE_MCP_ALLOW_DESTRUCTIVE is enabled — destructive operations will run \
             without confirmation. Verify this is intentional."
        );
        Ok(())
    } else {
        tracing::error!(
            bind = %config.mcp.bind_addr(),
            "CRITICAL: SYNAPSE_MCP_ALLOW_DESTRUCTIVE enabled on a non-loopback bind — refusing \
             to start. Destructive confirmation must not be disabled on a reachable host."
        );
        anyhow::bail!(
            "SYNAPSE_MCP_ALLOW_DESTRUCTIVE may not be enabled on a non-loopback bind ({})",
            config.mcp.bind_addr()
        )
    }
}

async fn build_state(config: Config) -> Result<AppState> {
    let auth_policy = build_auth_policy(&config).await?;
    let service = SynapseService::new();
    Ok(AppState {
        config: config.mcp,
        auth_policy,
        service,
    })
}

async fn build_auth_policy(config: &Config) -> Result<AuthPolicy> {
    match resolve_auth_policy_kind(config, config.mcp.trusted_gateway)? {
        AuthPolicyKind::LoopbackDev => Ok(AuthPolicy::LoopbackDev),
        AuthPolicyKind::TrustedGatewayUnscoped => {
            // SECURITY (S-H1): TrustedGatewayUnscoped grants fully-unauthenticated
            // access to all endpoints — including container exec, scout exec, and
            // lifecycle operations — with no bearer token, no OAuth, and no proof
            // that a gateway is actually present. ANY peer that can reach
            // {}:{} at the network level has complete fleet control.
            //
            // Safe only when this port is reachable exclusively by a trusted reverse
            // proxy (e.g., Labby gateway, SWAG) that enforces its own authentication
            // and authorization BEFORE forwarding to synapse2. Isolation MUST be
            // enforced at the network/Docker-network layer (e.g., a Docker internal
            // network where only the gateway container has access to this port).
            //
            // To further restrict which source IPs may reach this server without auth,
            // set SYNAPSE_TRUSTED_GATEWAY_IP to a comma-separated list of allowed
            // peer CIDRs/IPs and enforce it at the firewall or reverse-proxy layer.
            // synapse2 itself does not enforce this IP allowlist at present — it is
            // a documentation and ops-contract signal only.
            //
            // Mitigation checklist:
            //   1. Run synapse2 on an isolated Docker network; the gateway is the
            //      only container with access.
            //   2. Firewall :40080 from all sources except the gateway.
            //   3. Set SYNAPSE_TRUSTED_GATEWAY_IP to the gateway's IP for ops
            //      documentation (not yet enforced in-process; see TODO below).
            //   4. Rotate credentials if this port was ever inadvertently exposed.
            tracing::warn!(
                bind = %config.mcp.bind_addr(),
                "SECURITY: TrustedGatewayUnscoped mode active (SYNAPSE_NOAUTH=true). \
                 All requests on {} are accepted without authentication. \
                 This is ONLY safe when a trusted reverse proxy (e.g., Labby) is the \
                 sole network peer that can reach this port. \
                 Ensure this port is on an isolated Docker network or firewall-restricted. \
                 Set SYNAPSE_TRUSTED_GATEWAY_IP to document the expected gateway IP.",
                config.mcp.bind_addr(),
            );
            // TODO(S-H1): enforce SYNAPSE_TRUSTED_GATEWAY_IP as a peer-addr allowlist
            // once axum ConnectInfo is threaded through to the handler layer, so that
            // connections from unexpected source IPs are refused at the TCP level.
            Ok(AuthPolicy::TrustedGatewayUnscoped)
        }
        AuthPolicyKind::MountedBearer => Ok(AuthPolicy::Mounted { auth_state: None }),
        AuthPolicyKind::MountedOAuth => {
            let auth_cfg = lab_auth::config::AuthConfigBuilder::new()
                .env_prefix("SYNAPSE_MCP")
                .session_cookie_name("synapse_mcp_session")
                .scopes_supported(vec![
                    synapse2::actions::READ_SCOPE.into(),
                    synapse2::actions::WRITE_SCOPE.into(),
                ])
                .default_scope("synapse:read")
                .resource_path("/mcp")
                .enable_dynamic_registration(true)
                .disable_static_token_with_oauth(config.mcp.auth.disable_static_token_with_oauth)
                .build_from_sources(auth_config_sources(config))
                .map_err(|e| anyhow::anyhow!("OAuth config error: {e}"))?;
            let auth_state = lab_auth::state::AuthState::new(auth_cfg)
                .await
                .map_err(|e| anyhow::anyhow!("OAuth state init error: {e}"))?;
            Ok(AuthPolicy::Mounted {
                auth_state: Some(Arc::new(auth_state)),
            })
        }
    }
}

fn auth_config_sources(config: &Config) -> Vec<(String, String)> {
    let auth = &config.mcp.auth;
    let mut vars = vec![
        ("SYNAPSE_MCP_AUTH_MODE".into(), "oauth".into()),
        (
            "SYNAPSE_MCP_AUTH_SQLITE_PATH".into(),
            auth.sqlite_path.clone(),
        ),
        ("SYNAPSE_MCP_AUTH_KEY_PATH".into(), auth.key_path.clone()),
        (
            "SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS".into(),
            auth.access_token_ttl_secs.to_string(),
        ),
        (
            "SYNAPSE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS".into(),
            auth.refresh_token_ttl_secs.to_string(),
        ),
        (
            "SYNAPSE_MCP_AUTH_CODE_TTL_SECS".into(),
            auth.auth_code_ttl_secs.to_string(),
        ),
        (
            "SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE".into(),
            auth.register_rpm.to_string(),
        ),
        (
            "SYNAPSE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE".into(),
            auth.authorize_rpm.to_string(),
        ),
    ];
    push_optional(&mut vars, "SYNAPSE_MCP_PUBLIC_URL", &auth.public_url);
    push_optional(
        &mut vars,
        "SYNAPSE_MCP_GOOGLE_CLIENT_ID",
        &auth.google_client_id,
    );
    push_optional(
        &mut vars,
        "SYNAPSE_MCP_GOOGLE_CLIENT_SECRET",
        &auth.google_client_secret,
    );
    if !auth.admin_email.is_empty() {
        vars.push((
            "SYNAPSE_MCP_AUTH_ADMIN_EMAIL".into(),
            auth.admin_email.clone(),
        ));
    }
    if !auth.allowed_client_redirect_uris.is_empty() {
        vars.push((
            "SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS".into(),
            auth.allowed_client_redirect_uris.join(","),
        ));
    }
    vars
}

fn push_optional(vars: &mut Vec<(String, String)>, key: &str, value: &Option<String>) {
    if let Some(value) = value.as_ref().filter(|value| !value.is_empty()) {
        vars.push((key.into(), value.clone()));
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "CTRL+C handler failed");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "SIGTERM handler failed");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("Shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(allow_destructive: bool, host: &str) -> Config {
        let mut config = Config::default();
        config.mcp.allow_destructive = allow_destructive;
        config.mcp.host = host.to_string();
        config
    }

    #[test]
    fn override_off_is_ok_even_on_non_loopback() {
        assert!(enforce_destructive_policy(&cfg(false, "0.0.0.0")).is_ok());
    }

    #[test]
    fn override_on_loopback_warns_but_proceeds() {
        assert!(enforce_destructive_policy(&cfg(true, "127.0.0.1")).is_ok());
    }

    #[test]
    fn override_on_non_loopback_refuses_to_bind() {
        let err = enforce_destructive_policy(&cfg(true, "0.0.0.0"))
            .expect_err("non-loopback + override must refuse to start");
        assert!(err.to_string().contains("non-loopback"));
    }

    #[test]
    fn auth_config_sources_include_typed_oauth_settings() {
        let mut config = Config::default();
        config.mcp.auth.sqlite_path = "/tmp/auth.db".into();
        config.mcp.auth.key_path = "/tmp/key.pem".into();
        config.mcp.auth.access_token_ttl_secs = 120;
        config.mcp.auth.refresh_token_ttl_secs = 240;
        config.mcp.auth.auth_code_ttl_secs = 60;
        config.mcp.auth.register_rpm = 3;
        config.mcp.auth.authorize_rpm = 4;
        config.mcp.auth.public_url = Some("https://synapse.example".into());
        config.mcp.auth.google_client_id = Some("client-id".into());
        config.mcp.auth.google_client_secret = Some("client-secret".into());
        config.mcp.auth.admin_email = "admin@example.com".into();
        config.mcp.auth.allowed_client_redirect_uris =
            vec!["https://claude.ai/api/mcp/auth_callback".into()];

        let vars = auth_config_sources(&config);
        assert!(vars.contains(&("SYNAPSE_MCP_AUTH_SQLITE_PATH".into(), "/tmp/auth.db".into())));
        assert!(vars.contains(&("SYNAPSE_MCP_AUTH_KEY_PATH".into(), "/tmp/key.pem".into())));
        assert!(vars.contains(&(
            "SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS".into(),
            "120".into()
        )));
        assert!(vars.contains(&(
            "SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE".into(),
            "3".into()
        )));
        assert!(vars.contains(&(
            "SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS".into(),
            "https://claude.ai/api/mcp/auth_callback".into()
        )));
        assert!(vars.contains(&(
            "SYNAPSE_MCP_GOOGLE_CLIENT_SECRET".into(),
            "client-secret".into()
        )));
    }
}

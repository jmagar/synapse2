//! Configuration structs for the Synapse2 MCP server.
//!
//! Values are loaded in priority order:
//!   1. `config.toml` — non-secret defaults. Searched in the service data dir
//!      (`/data` in Docker, `~/.synapse2` bare-metal) then the current directory.
//!      First match wins.
//!   2. `.env` — secrets, URLs, and runtime vars. Lower-priority `./.env` is
//!      applied before appdata/SYNAPSE_HOME so explicit appdata values win.
//!   3. Existing process environment variables — the final, highest-priority
//!      override.
//!
//! Host topology is loaded separately in `host_config.rs` via
//! `SYNAPSE_HOSTS_CONFIG` / `SYNAPSE_CONFIG_FILE` / `~/.ssh/config`.

use serde::{Deserialize, Serialize};

const SERVICE_HOME_DIRNAME: &str = ".synapse2";

mod env;

pub use env::load_dotenv_environment;
use env::{
    apply_dotenv_file, config_search_dirs, dotenv_precedence_dirs, env_bool, env_list, env_opt_str,
    env_parse, env_str, parse_auth_mode_value,
};

/// Top-level config (maps to `config.toml` sections).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub mcp: McpConfig,
}

/// MCP HTTP server configuration.
// `Debug` is hand-written (below) to redact `api_token`; do not re-derive it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Bind host (SYNAPSE_MCP_HOST). Default: `127.0.0.1` (loopback).
    /// Set to `0.0.0.0` to listen on all interfaces — requires auth configured.
    #[serde(default = "default_mcp_host")]
    pub host: String,
    /// Bind port (SYNAPSE_MCP_PORT). Default: `40080`.
    #[serde(default = "default_mcp_port")]
    pub port: u16,
    /// MCP server name advertised to clients (SYNAPSE_MCP_SERVER_NAME).
    #[serde(default = "default_server_name")]
    pub server_name: String,
    /// Disable auth entirely — only safe when bound to loopback (SYNAPSE_MCP_NO_AUTH).
    pub no_auth: bool,
    /// Allow unauthenticated access on non-loopback when behind a trusted reverse proxy
    /// that enforces its own auth (SYNAPSE_NOAUTH). Loaded here so it participates in
    /// typed config rather than being a raw env read at call sites.
    pub trusted_gateway: bool,
    /// Skip destructive-operation confirmation prompts (SYNAPSE_MCP_ALLOW_DESTRUCTIVE).
    /// Operational override only — the shims substitute a no-op `Confirmer` when set.
    /// Loaded here (rather than a raw env read at call sites) so it is typed config.
    /// Strict `true`/`false` parsing (see `env_bool`).
    /// SECURITY: only safe on loopback; binding to a non-loopback address with this
    /// set causes startup failure (enforced in `main.rs`).
    pub allow_destructive: bool,
    /// Static bearer token for simple auth (SYNAPSE_MCP_TOKEN).
    pub api_token: Option<String>,
    /// Additional allowed Host header values (comma-separated in env).
    pub allowed_hosts: Vec<String>,
    /// Additional allowed CORS origins (comma-separated in env).
    pub allowed_origins: Vec<String>,
    /// Maximum number of concurrent in-flight requests on `/mcp` and
    /// `/v1/synapse2` (SYNAPSE_MCP_MAX_CONCURRENCY). Additional requests are
    /// queued until a permit is available. Default: 50. Set to 0 to disable
    /// the limit.
    ///
    /// This is a global concurrency cap across all connected clients — not a
    /// per-client rate limit. It protects the SSH pool, Docker socket, and CPU
    /// from request storms (e.g., a misbehaving MCP client sending large
    /// scout emit fanouts in parallel).
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    /// OAuth sub-config (nested under `[mcp.auth]` in config.toml).
    pub auth: AuthConfig,
}

impl std::fmt::Debug for McpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("server_name", &self.server_name)
            .field("no_auth", &self.no_auth)
            .field("trusted_gateway", &self.trusted_gateway)
            .field("allow_destructive", &self.allow_destructive)
            // Redact the static bearer token — never log secrets.
            .field("api_token", &self.api_token.as_ref().map(|_| "[REDACTED]"))
            .field("allowed_hosts", &self.allowed_hosts)
            .field("allowed_origins", &self.allowed_origins)
            .field("max_concurrency", &self.max_concurrency)
            .field("auth", &self.auth)
            .finish()
    }
}

impl McpConfig {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Return true if the configured bind host resolves to a loopback address.
    ///
    /// Uses `IpAddr::is_loopback()` for numeric addresses. Accepts "localhost"
    /// as a canonical loopback hostname. Any other hostname or parse failure is
    /// treated as non-loopback — callers must not assume safety in that case.
    pub fn is_loopback(&self) -> bool {
        let host = &self.host;
        // Match "localhost" literal and numeric loopback addresses.
        // Strip bracket notation ([::1]) before parsing so IPv6 loopback works.
        host == "localhost"
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false)
    }
}

/// OAuth / JWT auth sub-config.
///
/// # Security: manual `Debug` impl
///
/// `google_client_secret` (and any other credential fields) must never appear
/// in log output. The derived `Debug` would print them verbatim, so this type
/// opts out of `#[derive(Debug)]` and implements it manually, redacting secret
/// fields to `"[REDACTED]"`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub public_url: Option<String>,
    pub google_client_id: Option<String>,
    /// SECURITY: never print this field — see manual `Debug` impl below.
    pub google_client_secret: Option<String>,
    pub admin_email: String,
    pub sqlite_path: String,
    pub key_path: String,
    pub access_token_ttl_secs: u64,
    pub refresh_token_ttl_secs: u64,
    pub auth_code_ttl_secs: u64,
    pub register_rpm: u32,
    pub authorize_rpm: u32,
    pub disable_static_token_with_oauth: bool,
    pub allowed_client_redirect_uris: Vec<String>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("mode", &self.mode)
            .field("public_url", &self.public_url)
            .field("google_client_id", &self.google_client_id)
            // Redact credential fields — never log secrets.
            .field(
                "google_client_secret",
                &self.google_client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("admin_email", &self.admin_email)
            .field("sqlite_path", &self.sqlite_path)
            .field("key_path", &self.key_path)
            .field("access_token_ttl_secs", &self.access_token_ttl_secs)
            .field("refresh_token_ttl_secs", &self.refresh_token_ttl_secs)
            .field("auth_code_ttl_secs", &self.auth_code_ttl_secs)
            .field("register_rpm", &self.register_rpm)
            .field("authorize_rpm", &self.authorize_rpm)
            .field(
                "disable_static_token_with_oauth",
                &self.disable_static_token_with_oauth,
            )
            .field(
                "allowed_client_redirect_uris",
                &self.allowed_client_redirect_uris,
            )
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    Bearer,
    OAuth,
}

// ── defaults ──────────────────────────────────────────────────────────────────

fn default_mcp_host() -> String {
    // Default to loopback for safety. Operators who need external access must
    // explicitly set SYNAPSE_MCP_HOST=0.0.0.0 (and configure auth).
    "127.0.0.1".into()
}
fn default_mcp_port() -> u16 {
    40080
}
fn default_server_name() -> String {
    "synapse2".into()
}
fn default_max_concurrency() -> usize {
    50
}
fn default_auth_sqlite_path() -> String {
    "/data/auth.db".into()
}
fn default_auth_key_path() -> String {
    "/data/auth-jwt.pem".into()
}
fn default_access_token_ttl_secs() -> u64 {
    3600
}
fn default_refresh_token_ttl_secs() -> u64 {
    86400 * 30
}
fn default_auth_code_ttl_secs() -> u64 {
    300
}
fn default_register_rpm() -> u32 {
    10
}
fn default_authorize_rpm() -> u32 {
    60
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            host: default_mcp_host(),
            port: default_mcp_port(),
            server_name: default_server_name(),
            no_auth: false,
            trusted_gateway: false,
            allow_destructive: false,
            api_token: None,
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            max_concurrency: default_max_concurrency(),
            auth: AuthConfig::default(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::default(),
            public_url: None,
            google_client_id: None,
            google_client_secret: None,
            admin_email: String::new(),
            sqlite_path: default_auth_sqlite_path(),
            key_path: default_auth_key_path(),
            access_token_ttl_secs: default_access_token_ttl_secs(),
            refresh_token_ttl_secs: default_refresh_token_ttl_secs(),
            auth_code_ttl_secs: default_auth_code_ttl_secs(),
            register_rpm: default_register_rpm(),
            authorize_rpm: default_authorize_rpm(),
            disable_static_token_with_oauth: true,
            allowed_client_redirect_uris: Vec::new(),
        }
    }
}

// ── Appdata directory ─────────────────────────────────────────────────────────

/// Return the default local data directory for this service.
///
/// Pattern §25 + §28: The same `.env` and `config.toml` in `~/.<service>/`
/// work for both Docker and bare-metal deployment without modification.
///
/// | Environment   | Path                                |
/// |---------------|-------------------------------------|
/// | Container     | `/data` (bind-mounted from host)     |
/// | Bare-metal    | `~/.synapse2` (user home dir)        |
///
/// TEMPLATE: Replace `.synapse2` with your service name (e.g. `.unraid`, `.gotify`).
///           The name should match the docker-compose.yml volume mount source.
pub fn default_data_dir() -> anyhow::Result<std::path::PathBuf> {
    // Running inside a Docker container — /data is always the mount point.
    // Detection uses /.dockerenv (created by the Docker runtime) or an explicit
    // RUNNING_IN_CONTAINER env var (useful for testing or systemd-nspawn).
    if std::path::Path::new("/.dockerenv").exists()
        || std::env::var("RUNNING_IN_CONTAINER").is_ok()
        || std::env::var("container").is_ok()
    {
        return Ok(std::path::PathBuf::from("/data"));
    }

    // Bare-metal or local dev — use ~/.<service>/
    let home = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("cannot determine home directory — set HOME or RUNNING_IN_CONTAINER=1")
    })?;
    Ok(home.join(SERVICE_HOME_DIRNAME))
}

// ── Config loading ────────────────────────────────────────────────────────────

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Config::default();

        // Search for config.toml in the service config dirs, first match wins.
        // See `config_search_dirs` for the resolved precedence — critically this
        // includes `/data` in Docker, which is where the `~/.synapse2` bind mount
        // lands, so config dropped in the appdata dir is honored in-container.
        for dir in config_search_dirs() {
            let path = dir.join("config.toml");
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    config = toml::from_str(&contents)
                        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))?;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(anyhow::anyhow!("Failed to read {}: {e}", path.display())),
            }
        }

        for dir in dotenv_precedence_dirs() {
            apply_dotenv_file(&mut config, &dir.join(".env"))?;
        }

        // Env overrides — SYNAPSE_MCP_* for server config, SYNAPSE_API_* for upstream
        env_str("SYNAPSE_MCP_HOST", &mut config.mcp.host);
        env_parse("SYNAPSE_MCP_PORT", &mut config.mcp.port)?;
        env_str("SYNAPSE_MCP_SERVER_NAME", &mut config.mcp.server_name);
        env_bool("SYNAPSE_MCP_NO_AUTH", &mut config.mcp.no_auth)?;
        env_bool("SYNAPSE_NOAUTH", &mut config.mcp.trusted_gateway)?;
        env_bool(
            "SYNAPSE_MCP_ALLOW_DESTRUCTIVE",
            &mut config.mcp.allow_destructive,
        )?;
        env_opt_str("SYNAPSE_MCP_TOKEN", &mut config.mcp.api_token);
        env_list("SYNAPSE_MCP_ALLOWED_HOSTS", &mut config.mcp.allowed_hosts);
        env_list(
            "SYNAPSE_MCP_ALLOWED_ORIGINS",
            &mut config.mcp.allowed_origins,
        );
        env_parse(
            "SYNAPSE_MCP_MAX_CONCURRENCY",
            &mut config.mcp.max_concurrency,
        )?;
        env_opt_str("SYNAPSE_MCP_PUBLIC_URL", &mut config.mcp.auth.public_url);
        env_str(
            "SYNAPSE_MCP_AUTH_ADMIN_EMAIL",
            &mut config.mcp.auth.admin_email,
        );
        env_opt_str(
            "SYNAPSE_MCP_GOOGLE_CLIENT_ID",
            &mut config.mcp.auth.google_client_id,
        );
        env_opt_str(
            "SYNAPSE_MCP_GOOGLE_CLIENT_SECRET",
            &mut config.mcp.auth.google_client_secret,
        );
        env_str(
            "SYNAPSE_MCP_AUTH_SQLITE_PATH",
            &mut config.mcp.auth.sqlite_path,
        );
        env_str("SYNAPSE_MCP_AUTH_KEY_PATH", &mut config.mcp.auth.key_path);
        env_parse(
            "SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS",
            &mut config.mcp.auth.access_token_ttl_secs,
        )?;
        env_parse(
            "SYNAPSE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS",
            &mut config.mcp.auth.refresh_token_ttl_secs,
        )?;
        env_parse(
            "SYNAPSE_MCP_AUTH_CODE_TTL_SECS",
            &mut config.mcp.auth.auth_code_ttl_secs,
        )?;
        env_parse(
            "SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE",
            &mut config.mcp.auth.register_rpm,
        )?;
        env_parse(
            "SYNAPSE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE",
            &mut config.mcp.auth.authorize_rpm,
        )?;
        env_bool(
            "SYNAPSE_MCP_AUTH_DISABLE_STATIC_TOKEN_WITH_OAUTH",
            &mut config.mcp.auth.disable_static_token_with_oauth,
        )?;
        env_list(
            "SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS",
            &mut config.mcp.auth.allowed_client_redirect_uris,
        );
        if let Ok(v) = std::env::var("SYNAPSE_MCP_AUTH_MODE")
            && !v.is_empty()
        {
            config.mcp.auth.mode = parse_auth_mode_value(&v)?;
        }

        Ok(config)
    }
}

#[cfg(test)]
use env::{apply_config_env_value, parse_dotenv};

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

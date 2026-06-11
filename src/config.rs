//! Configuration structs for the Example MCP server.
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
use std::collections::BTreeMap;

const SERVICE_HOME_DIRNAME: &str = ".synapse2";

/// Top-level config (maps to `config.toml` sections).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub mcp: McpConfig,
}

/// MCP HTTP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// OAuth sub-config (nested under `[mcp.auth]` in config.toml).
    pub auth: AuthConfig,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub public_url: Option<String>,
    pub google_client_id: Option<String>,
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
        if let Ok(v) = std::env::var("SYNAPSE_MCP_AUTH_MODE") {
            if !v.is_empty() {
                config.mcp.auth.mode = match v.to_lowercase().as_str() {
                    "oauth" => AuthMode::OAuth,
                    "bearer" => AuthMode::Bearer,
                    other => {
                        return Err(anyhow::anyhow!(
                            "invalid SYNAPSE_MCP_AUTH_MODE {:?}: must be \"bearer\" or \"oauth\"",
                            other
                        ));
                    }
                };
            }
        }

        Ok(config)
    }
}

/// Seed process environment variables from configured `.env` files.
///
/// This supports settings read directly by libraries or early runtime setup
/// (`RUST_LOG`, `NO_COLOR`, upstream API credentials, Docker Compose variables).
/// Existing process environment variables always win. Among files, `./.env` is
/// lower priority than appdata/SYNAPSE_HOME.
pub fn load_dotenv_environment() -> anyhow::Result<()> {
    let mut entries = BTreeMap::new();
    for dir in dotenv_precedence_dirs() {
        let path = dir.join(".env");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(anyhow::anyhow!("Failed to read {}: {e}", path.display())),
        };
        for (key, value) in parse_dotenv(&contents, &path)? {
            entries.insert(key, value);
        }
    }
    for (key, value) in entries {
        if std::env::var_os(&key).is_none() {
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

// ── env helpers ───────────────────────────────────────────────────────────────

/// Directories searched for `config.toml` and `.env`, in priority order:
///   1. `SYNAPSE_HOME` (explicit override), if set.
///   2. The service data dir — `/data` inside Docker (where the `~/.synapse2`
///      bind mount lands) or `~/.synapse2` on bare-metal (`default_data_dir`).
///   3. The current working directory — local dev / repo-root fallback.
///
/// First match wins for `config.toml`. Use `dotenv_precedence_dirs` for `.env`
/// loading so lower-priority files are applied first.
fn config_search_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("SYNAPSE_HOME") {
        dirs.push(std::path::PathBuf::from(home));
    } else if let Ok(data_dir) = default_data_dir() {
        dirs.push(data_dir);
    }
    dirs.push(std::path::PathBuf::from("."));
    dirs
}

fn dotenv_precedence_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = config_search_dirs();
    dirs.reverse();
    dirs
}

fn apply_dotenv_file(config: &mut Config, path: &std::path::Path) -> anyhow::Result<()> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(anyhow::anyhow!("Failed to read {}: {e}", path.display())),
    };

    for (key, value) in parse_dotenv(&contents, path)? {
        apply_config_env_value(config, &key, &value)?;
    }
    Ok(())
}

fn parse_dotenv(contents: &str, path: &std::path::Path) -> anyhow::Result<Vec<(String, String)>> {
    let mut entries = Vec::new();
    for (idx, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            anyhow::bail!(
                "Failed to parse {}:{}: expected KEY=VALUE",
                path.display(),
                idx + 1
            );
        };
        let key = raw_key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            anyhow::bail!(
                "Failed to parse {}:{}: invalid env key {:?}",
                path.display(),
                idx + 1,
                key
            );
        }
        entries.push((key.to_string(), parse_dotenv_value(raw_value.trim())?));
    }
    Ok(entries)
}

fn parse_dotenv_value(raw: &str) -> anyhow::Result<String> {
    if !(raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2) {
        return Ok(raw.to_string());
    }

    let mut value = String::new();
    let mut escaped = false;
    for ch in raw[1..raw.len() - 1].chars() {
        if escaped {
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            value.push(ch);
        }
    }
    if escaped {
        anyhow::bail!("dotenv quoted value cannot end with a trailing backslash");
    }
    Ok(value)
}

fn apply_config_env_value(config: &mut Config, key: &str, value: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        return Ok(());
    }

    match key {
        "SYNAPSE_MCP_HOST" => config.mcp.host = value.to_string(),
        "SYNAPSE_MCP_PORT" => {
            config.mcp.port = value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
        }
        "SYNAPSE_MCP_SERVER_NAME" => config.mcp.server_name = value.to_string(),
        "SYNAPSE_MCP_NO_AUTH" => config.mcp.no_auth = parse_bool_value(key, value)?,
        "SYNAPSE_NOAUTH" => config.mcp.trusted_gateway = parse_bool_value(key, value)?,
        "SYNAPSE_MCP_ALLOW_DESTRUCTIVE" => {
            config.mcp.allow_destructive = parse_bool_value(key, value)?;
        }
        "SYNAPSE_MCP_TOKEN" => config.mcp.api_token = Some(value.to_string()),
        "SYNAPSE_MCP_ALLOWED_HOSTS" => config.mcp.allowed_hosts = parse_list_value(value),
        "SYNAPSE_MCP_ALLOWED_ORIGINS" => config.mcp.allowed_origins = parse_list_value(value),
        "SYNAPSE_MCP_PUBLIC_URL" => config.mcp.auth.public_url = Some(value.to_string()),
        "SYNAPSE_MCP_AUTH_ADMIN_EMAIL" => config.mcp.auth.admin_email = value.to_string(),
        "SYNAPSE_MCP_GOOGLE_CLIENT_ID" => {
            config.mcp.auth.google_client_id = Some(value.to_string());
        }
        "SYNAPSE_MCP_GOOGLE_CLIENT_SECRET" => {
            config.mcp.auth.google_client_secret = Some(value.to_string());
        }
        "SYNAPSE_MCP_AUTH_SQLITE_PATH" => config.mcp.auth.sqlite_path = value.to_string(),
        "SYNAPSE_MCP_AUTH_KEY_PATH" => config.mcp.auth.key_path = value.to_string(),
        "SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS" => {
            config.mcp.auth.access_token_ttl_secs = value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
        }
        "SYNAPSE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS" => {
            config.mcp.auth.refresh_token_ttl_secs = value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
        }
        "SYNAPSE_MCP_AUTH_CODE_TTL_SECS" => {
            config.mcp.auth.auth_code_ttl_secs = value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
        }
        "SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE" => {
            config.mcp.auth.register_rpm = value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
        }
        "SYNAPSE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE" => {
            config.mcp.auth.authorize_rpm = value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
        }
        "SYNAPSE_MCP_AUTH_DISABLE_STATIC_TOKEN_WITH_OAUTH" => {
            config.mcp.auth.disable_static_token_with_oauth = parse_bool_value(key, value)?;
        }
        "SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS" => {
            config.mcp.auth.allowed_client_redirect_uris = parse_list_value(value);
        }
        "SYNAPSE_MCP_AUTH_MODE" => {
            config.mcp.auth.mode = parse_auth_mode_value(value)?;
        }
        _ => {}
    }
    Ok(())
}

fn parse_bool_value(key: &str, value: &str) -> anyhow::Result<bool> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        other => anyhow::bail!("{key}: expected bool, got {other:?}"),
    }
}

fn parse_auth_mode_value(value: &str) -> anyhow::Result<AuthMode> {
    match value.to_lowercase().as_str() {
        "oauth" => Ok(AuthMode::OAuth),
        "bearer" => Ok(AuthMode::Bearer),
        other => anyhow::bail!(
            "invalid SYNAPSE_MCP_AUTH_MODE {other:?}: must be \"bearer\" or \"oauth\""
        ),
    }
}

fn parse_list_value(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn env_str(key: &str, target: &mut String) {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = v;
        }
    }
}

fn env_opt_str(key: &str, target: &mut Option<String>) {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = Some(v);
        }
    }
}

fn env_parse<T: std::str::FromStr>(key: &str, target: &mut T) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = v
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {v:?}"))?;
        }
    }
    Ok(())
}

fn env_bool(key: &str, target: &mut bool) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key) {
        *target = parse_bool_value(key, &v)?;
    }
    Ok(())
}

fn env_list(key: &str, target: &mut Vec<String>) {
    if let Ok(v) = std::env::var(key) {
        let items = parse_list_value(&v);
        if !items.is_empty() {
            *target = items;
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

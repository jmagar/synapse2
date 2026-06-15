use super::{AuthMode, Config, default_data_dir};
use std::collections::BTreeMap;

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
            // SAFETY: `load_dotenv_environment` is called from the synchronous
            // `main()` BEFORE the Tokio runtime is built (see `src/main.rs`), so
            // no other threads exist and nothing reads the environment
            // concurrently. Edition 2024 marks `set_var` unsafe to guard against
            // the multi-threaded case, which cannot occur at this point.
            // Invariant: this must not be called after the runtime starts.
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }
    Ok(())
}

/// Directories searched for `config.toml` and `.env`, in priority order:
///   1. `SYNAPSE_HOME` (explicit override), if set.
///   2. The service data dir — `/data` inside Docker (where the `~/.synapse2`
///      bind mount lands) or `~/.synapse2` on bare-metal (`default_data_dir`).
///   3. The current working directory — local dev / repo-root fallback.
///
/// First match wins for `config.toml`. Use `dotenv_precedence_dirs` for `.env`
/// loading so lower-priority files are applied first.
pub(super) fn config_search_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("SYNAPSE_HOME") {
        dirs.push(std::path::PathBuf::from(home));
    } else if let Ok(data_dir) = default_data_dir() {
        dirs.push(data_dir);
    }
    dirs.push(std::path::PathBuf::from("."));
    dirs
}

pub(super) fn dotenv_precedence_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = config_search_dirs();
    dirs.reverse();
    dirs
}

pub(super) fn apply_dotenv_file(config: &mut Config, path: &std::path::Path) -> anyhow::Result<()> {
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

pub(super) fn parse_dotenv(
    contents: &str,
    path: &std::path::Path,
) -> anyhow::Result<Vec<(String, String)>> {
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

pub(super) fn apply_config_env_value(
    config: &mut Config,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
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

pub(super) fn parse_auth_mode_value(value: &str) -> anyhow::Result<AuthMode> {
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

pub(super) fn env_str(key: &str, target: &mut String) {
    if let Ok(v) = std::env::var(key)
        && !v.is_empty()
    {
        *target = v;
    }
}

pub(super) fn env_opt_str(key: &str, target: &mut Option<String>) {
    if let Ok(v) = std::env::var(key)
        && !v.is_empty()
    {
        *target = Some(v);
    }
}

pub(super) fn env_parse<T: std::str::FromStr>(key: &str, target: &mut T) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key)
        && !v.is_empty()
    {
        *target = v
            .parse()
            .map_err(|_| anyhow::anyhow!("{key}: invalid value {v:?}"))?;
    }
    Ok(())
}

pub(super) fn env_bool(key: &str, target: &mut bool) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key) {
        *target = parse_bool_value(key, &v)?;
    }
    Ok(())
}

pub(super) fn env_list(key: &str, target: &mut Vec<String>) {
    if let Ok(v) = std::env::var(key) {
        let items = parse_list_value(&v);
        if !items.is_empty() {
            *target = items;
        }
    }
}

//! Unit tests for src/config.rs

use super::*;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const CONFIG_ENV_KEYS: &[&str] = &[
    "SYNAPSE_HOME",
    "RUNNING_IN_CONTAINER",
    "container",
    "SYNAPSE_MCP_HOST",
    "SYNAPSE_MCP_PORT",
    "SYNAPSE_MCP_SERVER_NAME",
    "SYNAPSE_MCP_NO_AUTH",
    "SYNAPSE_NOAUTH",
    "SYNAPSE_MCP_ALLOW_DESTRUCTIVE",
    "SYNAPSE_MCP_TOKEN",
    "SYNAPSE_MCP_ALLOWED_HOSTS",
    "SYNAPSE_MCP_ALLOWED_ORIGINS",
    "SYNAPSE_MCP_PUBLIC_URL",
    "SYNAPSE_MCP_AUTH_ADMIN_EMAIL",
    "SYNAPSE_MCP_GOOGLE_CLIENT_ID",
    "SYNAPSE_MCP_GOOGLE_CLIENT_SECRET",
    "SYNAPSE_MCP_AUTH_SQLITE_PATH",
    "SYNAPSE_MCP_AUTH_KEY_PATH",
    "SYNAPSE_MCP_AUTH_ACCESS_TOKEN_TTL_SECS",
    "SYNAPSE_MCP_AUTH_REFRESH_TOKEN_TTL_SECS",
    "SYNAPSE_MCP_AUTH_CODE_TTL_SECS",
    "SYNAPSE_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE",
    "SYNAPSE_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE",
    "SYNAPSE_MCP_AUTH_DISABLE_STATIC_TOKEN_WITH_OAUTH",
    "SYNAPSE_MCP_AUTH_ALLOWED_REDIRECT_URIS",
    "SYNAPSE_MCP_AUTH_MODE",
    "SYNAPSE_API_URL",
    "RUST_LOG",
];

struct EnvSnapshot {
    values: Vec<(&'static str, Option<String>)>,
}

impl EnvSnapshot {
    fn capture(keys: &'static [&'static str]) -> Self {
        let values = keys
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect();
        for key in keys {
            unsafe {
                std::env::remove_var(key);
            }
        }
        Self { values }
    }
}

impl Drop for EnvSnapshot {
    fn drop(&mut self) {
        for (key, value) in &self.values {
            if let Some(value) = value {
                unsafe {
                    std::env::set_var(key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

fn locked_env() -> (MutexGuard<'static, ()>, EnvSnapshot) {
    let guard = ENV_LOCK.lock().unwrap();
    let snapshot = EnvSnapshot::capture(CONFIG_ENV_KEYS);
    (guard, snapshot)
}

// ── McpConfig::is_loopback edge cases ─────────────────────────────────────────

fn mcp_with_host(host: &str) -> McpConfig {
    McpConfig {
        host: host.to_string(),
        ..McpConfig::default()
    }
}

#[test]
fn is_loopback_ipv6_bare() {
    // "::1" without brackets — parsed as IpAddr, is_loopback() returns true
    assert!(mcp_with_host("::1").is_loopback(), "::1 should be loopback");
}

#[test]
fn is_loopback_ipv6_bracketed() {
    // "[::1]" bracket-quoted IPv6 — brackets are stripped before parse
    assert!(
        mcp_with_host("[::1]").is_loopback(),
        "[::1] should be loopback"
    );
}

#[test]
fn is_loopback_127_0_0_2() {
    // Any 127.x.x.x address is in the loopback range
    assert!(
        mcp_with_host("127.0.0.2").is_loopback(),
        "127.0.0.2 should be loopback"
    );
}

#[test]
fn is_loopback_0_0_0_0_is_false() {
    // 0.0.0.0 is unspecified, not loopback
    assert!(
        !mcp_with_host("0.0.0.0").is_loopback(),
        "0.0.0.0 should not be loopback"
    );
}

#[test]
fn is_loopback_uppercase_localhost_is_false() {
    // is_loopback only matches the literal "localhost" (case-sensitive)
    assert!(
        !mcp_with_host("LOCALHOST").is_loopback(),
        "LOCALHOST (uppercase) should not be loopback — check is case-sensitive"
    );
}

#[test]
fn is_loopback_subdomain_is_false() {
    // "localhost.synapse2.com" must not be treated as loopback
    assert!(
        !mcp_with_host("localhost.synapse2.com").is_loopback(),
        "localhost.synapse2.com should not be loopback"
    );
}

// ── env_bool helper ───────────────────────────────────────────────────────────
//
// env_bool is private, so we exercise it via a thin wrapper that sets a
// uniquely-named env var, calls the function, and unsets it again.
// Each test uses a distinct key to avoid collisions with parallel test threads.

fn call_env_bool(key: &str, raw: &str) -> anyhow::Result<bool> {
    unsafe {
        std::env::set_var(key, raw);
    }
    let mut target = false;
    let result = env_bool(key, &mut target);
    unsafe {
        std::env::remove_var(key);
    }
    result.map(|_| target)
}

#[test]
fn env_bool_accepts_1() {
    assert!(call_env_bool("TEST_ENV_BOOL_1", "1").unwrap());
}

#[test]
fn env_bool_accepts_true() {
    assert!(call_env_bool("TEST_ENV_BOOL_TRUE", "true").unwrap());
}

#[test]
fn env_bool_accepts_yes() {
    assert!(call_env_bool("TEST_ENV_BOOL_YES", "yes").unwrap());
}

#[test]
fn env_bool_accepts_0() {
    assert!(!call_env_bool("TEST_ENV_BOOL_0", "0").unwrap());
}

#[test]
fn env_bool_accepts_false() {
    assert!(!call_env_bool("TEST_ENV_BOOL_FALSE", "false").unwrap());
}

#[test]
fn env_bool_accepts_no() {
    assert!(!call_env_bool("TEST_ENV_BOOL_NO", "no").unwrap());
}

#[test]
fn env_bool_rejects_invalid() {
    let result = call_env_bool("TEST_ENV_BOOL_INVALID", "maybe");
    assert!(result.is_err(), "invalid bool string should return Err");
}

// ── env_list helper ───────────────────────────────────────────────────────────

fn call_env_list(key: &str, raw: &str) -> Vec<String> {
    unsafe {
        std::env::set_var(key, raw);
    }
    let mut target: Vec<String> = Vec::new();
    env_list(key, &mut target);
    unsafe {
        std::env::remove_var(key);
    }
    target
}

#[test]
fn env_list_splits_comma_separated() {
    let result = call_env_list("TEST_ENV_LIST_CSV", "a,b,c");
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn env_list_trims_spaces_around_commas() {
    let result = call_env_list("TEST_ENV_LIST_SPACES", "foo , bar , baz");
    assert_eq!(result, vec!["foo", "bar", "baz"]);
}

#[test]
fn env_list_empty_string_leaves_target_unchanged() {
    // An empty env var should not overwrite an existing target
    unsafe {
        std::env::set_var("TEST_ENV_LIST_EMPTY", "");
    }
    let mut target = vec!["existing".to_string()];
    env_list("TEST_ENV_LIST_EMPTY", &mut target);
    unsafe {
        std::env::remove_var("TEST_ENV_LIST_EMPTY");
    }
    assert_eq!(
        target,
        vec!["existing"],
        "empty env var should not clear target"
    );
}

// ── dotenv loading helpers ───────────────────────────────────────────────────

#[test]
fn dotenv_entries_override_config_values() {
    let mut config = Config::default();
    config.mcp.host = "0.0.0.0".to_string();
    config.mcp.port = 40060;

    let entries = parse_dotenv(
        r#"
        SYNAPSE_MCP_HOST=127.0.0.1
        SYNAPSE_MCP_PORT=40080
        SYNAPSE_MCP_NO_AUTH=true
        "#,
        std::path::Path::new("test.env"),
    )
    .unwrap();
    for (key, value) in entries {
        apply_config_env_value(&mut config, &key, &value).unwrap();
    }

    assert_eq!(config.mcp.host, "127.0.0.1");
    assert_eq!(config.mcp.port, 40080);
    assert!(config.mcp.no_auth);
}

#[test]
fn dotenv_quoted_values_unescape_quotes_and_backslashes() {
    let entries = parse_dotenv(
        r#"SYNAPSE_MCP_TOKEN="secret # \"quoted\" \\ token""#,
        std::path::Path::new("test.env"),
    )
    .unwrap();

    assert_eq!(
        entries,
        vec![(
            "SYNAPSE_MCP_TOKEN".to_string(),
            "secret # \"quoted\" \\ token".to_string()
        )]
    );
}

#[test]
fn config_load_reads_synapse_home_config() {
    let (_lock, _env) = locked_env();
    let appdata = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("SYNAPSE_HOME", appdata.path());
    }

    std::fs::write(
        appdata.path().join("config.toml"),
        r#"[mcp]
host = "127.0.0.1"
port = 40111
"#,
    )
    .unwrap();
    let config = Config::load().unwrap();
    assert_eq!(config.mcp.host, "127.0.0.1");
    assert_eq!(config.mcp.port, 40111);
}

#[test]
fn config_example_toml_matches_typed_schema() {
    let contents =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml"))
            .unwrap();
    assert!(
        !contents.contains("[synapse2]"),
        "upstream service settings belong in .env, not an ignored TOML table"
    );
    assert!(
        !contents.contains("allowed_emails"),
        "lab-auth allowed emails are managed by the OAuth store, not static TOML"
    );
    toml::from_str::<Config>(&contents).expect("config.example.toml should match Config schema");
}

#[test]
fn dotenv_precedence_dirs_apply_cwd_before_synapse_home() {
    let (_lock, _env) = locked_env();
    let appdata = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("SYNAPSE_HOME", appdata.path());
    }

    let dirs = dotenv_precedence_dirs();
    assert_eq!(
        dirs,
        vec![std::path::PathBuf::from("."), appdata.path().to_path_buf()]
    );
}

#[test]
fn load_dotenv_environment_seeds_runtime_vars_without_overriding_process_env() {
    let (_lock, _env) = locked_env();
    let appdata = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("SYNAPSE_HOME", appdata.path());
    }
    unsafe {
        std::env::set_var("RUST_LOG", "warn");
    }

    std::fs::write(
        appdata.path().join(".env"),
        "SYNAPSE_API_URL=https://appdata.example\nRUST_LOG=info\n",
    )
    .unwrap();

    load_dotenv_environment().unwrap();
    assert_eq!(
        std::env::var("SYNAPSE_API_URL").unwrap(),
        "https://appdata.example"
    );
    assert_eq!(std::env::var("RUST_LOG").unwrap(), "warn");
}

// ── AuthMode serde parsing ────────────────────────────────────────────────────
//
// AuthMode parsing in Config::load() is an inline match on the env var string,
// not a standalone function. We test the serde Deserialize path instead, which
// exercises the #[serde(rename_all = "lowercase")] attribute.

#[test]
fn auth_mode_deserializes_oauth() {
    let mode: AuthMode = serde_json::from_str("\"oauth\"").expect("oauth should deserialize");
    assert_eq!(mode, AuthMode::OAuth);
}

#[test]
fn auth_mode_deserializes_bearer() {
    let mode: AuthMode = serde_json::from_str("\"bearer\"").expect("bearer should deserialize");
    assert_eq!(mode, AuthMode::Bearer);
}

#[test]
fn auth_mode_rejects_bad_value() {
    let result = serde_json::from_str::<AuthMode>("\"bad\"");
    assert!(
        result.is_err(),
        "unknown auth mode should fail to deserialize"
    );
}

// SECURITY FIX: allow_destructive field and env parsing tests

#[test]
fn allow_destructive_defaults_false() {
    let config = McpConfig::default();
    assert!(
        !config.allow_destructive,
        "allow_destructive should default to false"
    );
}

#[test]
fn allow_destructive_env_parse_strict_true() {
    // Rust's str::parse::<bool> accepts "true" (case-insensitive)
    unsafe {
        std::env::set_var("TEST_ALLOW_DESTRUCTIVE_TRUE", "true");
    }
    let mut target = false;
    let result = env_bool("TEST_ALLOW_DESTRUCTIVE_TRUE", &mut target);
    unsafe {
        std::env::remove_var("TEST_ALLOW_DESTRUCTIVE_TRUE");
    }
    assert!(result.is_ok());
    assert!(target, "\"true\" should parse to true");
}

#[test]
fn allow_destructive_env_parse_strict_false() {
    unsafe {
        std::env::set_var("TEST_ALLOW_DESTRUCTIVE_FALSE", "false");
    }
    let mut target = true;
    let result = env_bool("TEST_ALLOW_DESTRUCTIVE_FALSE", &mut target);
    unsafe {
        std::env::remove_var("TEST_ALLOW_DESTRUCTIVE_FALSE");
    }
    assert!(result.is_ok());
    assert!(!target, "\"false\" should parse to false");
}

#[test]
fn allow_destructive_env_parse_rejects_invalid() {
    // Invalid values like "1", "yes", "TRUE" should error or fail gracefully
    // env_bool accepts "1" as true for compatibility, but strict parsing would reject it
    // For now we test that the function handles it
    unsafe {
        std::env::set_var("TEST_ALLOW_DESTRUCTIVE_MAYBE", "maybe");
    }
    let mut target = false;
    let result = env_bool("TEST_ALLOW_DESTRUCTIVE_MAYBE", &mut target);
    unsafe {
        std::env::remove_var("TEST_ALLOW_DESTRUCTIVE_MAYBE");
    }
    assert!(result.is_err(), "\"maybe\" should not be a valid bool");
}

//! `HostRepository` trait and `FileHostRepository` implementation.
//!
//! Loads and merges host configurations from multiple sources in precedence order:
//!
//! 1. `SYNAPSE_HOSTS_CONFIG` env (JSON array) — highest priority
//! 2. `SYNAPSE_CONFIG_FILE` env (path override)
//! 3. `./synapse.config.json`
//! 4. `$XDG_CONFIG_HOME/synapse-mcp/config.json`
//! 5. `~/.config/synapse-mcp/config.json`
//! 6. `~/.synapse-mcp.json`
//! 7. `~/.ssh/config` (auto-discovered, additive below the explicit sources)
//! 8. Built-in `local` fallback (lowest priority)
//!
//! **Precedence semantics:**
//! - Explicit sources (1–6): first non-empty source wins entirely (no merging between them).
//! - SSH config (7): additive; any host name already present in the explicit set is skipped.
//! - Ensure-local (8): `HostConfig::local()` is appended if no host named `"local"` exists.
//!
//! **Error policy:**
//! - Malformed JSON in explicit config → propagated as a hard error.
//! - SSH config parse failure → logged and silently returns empty (non-fatal; server must start).
//!
//! **Include directives:** `ssh2-config` 0.7.1 DOES expand `Include` directives natively
//! (verified empirically by `ssh_config_include_directives_are_expanded` in the test file).
//! Hosts defined in `Include`d sub-files are parsed and available for auto-discovery.
//! Note: The bead spec comment (2026-05-25) stated Include was NOT handled — that was true
//! for older versions; 0.7.1 supports it via the `glob` dependency in the parser.
//!
//! **Wildcard Host blocks** (e.g. `Host *`, `Host *.example.com`): skipped — they don't
//! represent connectable hosts.

use anyhow::Result;
use std::collections::HashSet;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use ssh2_config::{ParseRule, SshConfig};

use crate::synapse::{HostConfig, HostProtocol, HostsFile};

#[cfg(test)]
#[path = "host_config_tests.rs"]
mod tests;

// ---------------------------------------------------------------------------
// Known non-infrastructure hosts to skip during SSH auto-discovery
// ---------------------------------------------------------------------------

/// Well-known git-hosting and backup services that appear in `~/.ssh/config`
/// but are not Synapse-connectable infrastructure hosts.
const SKIP_SSH_HOSTS: &[&str] = &[
    "github.com",
    "gitlab.com",
    "bitbucket.org",
    "ssh.github.com",
    "backup.unraid.net",
];

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstraction over host config loading, so tests can inject fixtures.
pub trait HostRepository: Send + Sync {
    /// Return the full, merged list of configured hosts.
    fn load_hosts(&self) -> Result<Vec<HostConfig>>;
}

// ---------------------------------------------------------------------------
// FileHostRepository — production implementation
// ---------------------------------------------------------------------------

/// Loads hosts from disk / env following the precedence chain documented
/// in [`crate::host_config`].
///
/// Call [`FileHostRepository::default()`] in production.  In tests, construct
/// with explicit tempfile paths using [`FileHostRepository::for_test`] to avoid
/// reading process env or the real `~/.ssh/config`.
pub struct FileHostRepository {
    /// Pre-captured value of `SYNAPSE_HOSTS_CONFIG` env var (if any).
    env_hosts_json: Option<String>,
    /// Ordered list of JSON config file paths to check (first non-empty wins).
    config_file_paths: Vec<PathBuf>,
    /// Path to the SSH config file, or `None` to skip SSH auto-discovery.
    ssh_config_path: Option<PathBuf>,
}

impl Default for FileHostRepository {
    /// Production constructor — reads env vars and resolves default paths.
    fn default() -> Self {
        Self {
            env_hosts_json: std::env::var("SYNAPSE_HOSTS_CONFIG").ok(),
            config_file_paths: default_config_paths(),
            ssh_config_path: default_ssh_config_path(),
        }
    }
}

impl FileHostRepository {
    /// Test constructor: all sources explicit, bypasses process env.
    pub fn for_test(
        env_hosts_json: Option<String>,
        config_file_paths: Vec<PathBuf>,
        ssh_config_path: Option<PathBuf>,
    ) -> Self {
        Self {
            env_hosts_json,
            config_file_paths,
            ssh_config_path,
        }
    }

    // ------------------------------------------------------------------
    // Explicit sources
    // ------------------------------------------------------------------

    /// Load from `SYNAPSE_HOSTS_CONFIG` env (pre-captured at construction).
    ///
    /// Returns `Err` on malformed JSON.
    fn load_from_env_json(&self) -> Result<Vec<HostConfig>> {
        let raw = match &self.env_hosts_json {
            Some(s) if !s.trim().is_empty() => s,
            _ => return Ok(Vec::new()),
        };
        // Hard error on malformed JSON — do not silently fall back.
        let hosts: Vec<HostConfig> = serde_json::from_str(raw)?;
        Ok(hosts)
    }

    /// Scan `config_file_paths` and return hosts from the first non-empty file.
    ///
    /// Returns `Err` on malformed JSON in any file that actually exists.
    fn load_from_files(&self) -> Result<Vec<HostConfig>> {
        for path in &self.config_file_paths {
            if !path.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(path)?;
            // Hard error on malformed JSON.
            let parsed: HostsFile = serde_json::from_str(&raw)?;
            if !parsed.hosts.is_empty() {
                tracing::info!(
                    path = %path.display(),
                    count = parsed.hosts.len(),
                    "loaded hosts from config file"
                );
                return Ok(parsed.hosts);
            }
        }
        Ok(Vec::new())
    }

    // ------------------------------------------------------------------
    // SSH auto-discovery
    // ------------------------------------------------------------------

    /// Load hosts from the SSH config file at `self.ssh_config_path`.
    ///
    /// Failures are soft (logged + returns empty) so the server still starts
    /// even with a malformed / missing `~/.ssh/config`.
    fn load_from_ssh_config(&self) -> Vec<HostConfig> {
        let path = match &self.ssh_config_path {
            Some(p) => p,
            None => return Vec::new(),
        };

        if !path.exists() {
            return Vec::new();
        }

        match load_ssh_config_file(path) {
            Ok(hosts) => {
                if !hosts.is_empty() {
                    tracing::info!(
                        path = %path.display(),
                        count = hosts.len(),
                        "auto-discovered hosts from SSH config"
                    );
                }
                hosts
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "SSH config parse failed — continuing without auto-discovered hosts"
                );
                Vec::new()
            }
        }
    }
}

impl HostRepository for FileHostRepository {
    fn load_hosts(&self) -> Result<Vec<HostConfig>> {
        // Step 1: Find the single winning explicit source.
        let mut explicit: Vec<HostConfig> = self.load_from_env_json()?;
        if explicit.is_empty() {
            explicit = self.load_from_files()?;
        }

        // Reject unsupported protocols early — Http/Https would silently route
        // as SSH otherwise (A-H3 / S-M6).
        for host in &explicit {
            reject_unsupported_protocol(host)?;
        }

        // Step 2: SSH auto-discovery (additive, explicit wins on name conflict).
        let ssh_hosts = self.load_from_ssh_config();
        let hosts = merge_hosts(explicit, ssh_hosts);

        // Step 3: Ensure the built-in `local` host is always present.
        let hosts = ensure_local(hosts);

        Ok(hosts)
    }
}

// ---------------------------------------------------------------------------
// SSH config parsing helper
// ---------------------------------------------------------------------------

/// Parse an SSH config file and return `HostConfig` entries.
///
/// Uses `ALLOW_UNKNOWN_FIELDS | ALLOW_UNSUPPORTED_FIELDS` so real-world
/// `~/.ssh/config` files with `Include`, `Match`, or any directive the crate
/// doesn't know about don't abort the parse.
pub fn load_ssh_config_file(path: &Path) -> Result<Vec<HostConfig>> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let config = SshConfig::default().parse(
        &mut reader,
        ParseRule::ALLOW_UNKNOWN_FIELDS | ParseRule::ALLOW_UNSUPPORTED_FIELDS,
    )?;

    let mut hosts: Vec<HostConfig> = Vec::new();

    for host in config.get_hosts() {
        // A Host block can have multiple patterns (e.g. `Host a b *.x`).
        // We only emit entries for non-wildcard, non-skipped concrete aliases.
        for clause in &host.pattern {
            // Skip negated clauses (they exclude, not include).
            if clause.negated {
                continue;
            }

            let alias = &clause.pattern;

            // Skip wildcard patterns — they match many hosts and don't represent
            // a single connectable endpoint.
            if alias.contains('*') || alias.contains('?') {
                continue;
            }

            // Skip well-known non-infrastructure services.
            if SKIP_SSH_HOSTS.contains(&alias.as_str()) {
                continue;
            }

            // Resolve per-host params (inherits Host * globals automatically).
            let params = config.query(alias.as_str());

            // `HostName` is required for a connectable host.  Skip alias-only
            // stanzas that lack a HostName directive.
            let hostname = match params.host_name {
                Some(ref h) => h.clone(),
                None => continue,
            };

            let port = params.port;
            let ssh_user = params.user.clone();
            let ssh_key_path = params
                .identity_file
                .as_ref()
                .and_then(|files| files.first())
                .map(|p| p.to_string_lossy().into_owned());

            hosts.push(HostConfig {
                name: alias.clone(),
                host: hostname,
                port,
                protocol: HostProtocol::Ssh,
                ssh_user,
                ssh_key_path,
                ssh_port: port,
                ssh_config_path: Some(path.to_string_lossy().into_owned()),
                docker_socket_path: None,
                tags: Vec::new(),
                compose_search_paths: Vec::new(),
                scout_read_roots: Vec::new(),
                exec_allowlist: Vec::new(),
            });
        }
    }

    // Deduplicate by name, first-seen wins (SSH first-match-wins semantics).
    let mut seen: HashSet<String> = HashSet::new();
    let deduped: Vec<HostConfig> = hosts
        .into_iter()
        .filter(|h| seen.insert(h.name.clone()))
        .collect();

    Ok(deduped)
}

// ---------------------------------------------------------------------------
// Merge helpers
// ---------------------------------------------------------------------------

/// Merge explicit and SSH-discovered hosts.
/// Explicit hosts take full precedence: SSH hosts with the same name are dropped.
pub fn merge_hosts(explicit: Vec<HostConfig>, ssh: Vec<HostConfig>) -> Vec<HostConfig> {
    let explicit_names: HashSet<String> = explicit.iter().map(|h| h.name.clone()).collect();

    let mut merged = explicit;
    for ssh_host in ssh {
        if !explicit_names.contains(&ssh_host.name) {
            merged.push(ssh_host);
        }
    }
    merged
}

/// Append the built-in `local` host if no host named `"local"` exists.
pub fn ensure_local(mut hosts: Vec<HostConfig>) -> Vec<HostConfig> {
    if !hosts.iter().any(|h| h.name == "local") {
        hosts.push(HostConfig::local());
    }
    hosts
}

// ---------------------------------------------------------------------------
// Protocol validation
// ---------------------------------------------------------------------------

/// Reject hosts whose protocol is `http` or `https`.
///
/// These variants exist in the `HostProtocol` enum but have never been
/// implemented. Accepting them silently causes them to be routed as SSH
/// (the else-branch in dispatch), which is a silent misconfiguration.
/// Fail loudly at load time instead (A-H3 / S-M6).
pub fn reject_unsupported_protocol(host: &HostConfig) -> Result<()> {
    match host.protocol {
        HostProtocol::Http | HostProtocol::Https => {
            anyhow::bail!(
                "host '{}': protocol '{}' is not supported; use 'local' or 'ssh'",
                host.name,
                match host.protocol {
                    HostProtocol::Http => "http",
                    HostProtocol::Https => "https",
                    _ => unreachable!(),
                }
            )
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Default path resolution (separated for testability)
// ---------------------------------------------------------------------------

/// Ordered list of JSON config file paths for the production precedence chain.
pub fn default_config_paths() -> Vec<PathBuf> {
    if let Ok(path) = std::env::var("SYNAPSE_CONFIG_FILE") {
        return vec![PathBuf::from(path)];
    }
    let mut paths = vec![PathBuf::from("synapse.config.json")];
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        paths.push(Path::new(&xdg).join("synapse-mcp").join("config.json"));
    }
    if let Ok(home) = std::env::var("HOME") {
        paths.push(
            Path::new(&home)
                .join(".config")
                .join("synapse-mcp")
                .join("config.json"),
        );
        paths.push(Path::new(&home).join(".synapse-mcp.json"));
    }
    paths
}

/// Return `~/.ssh/config` path, or `None` if `HOME` is unset.
pub fn default_ssh_config_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| Path::new(&home).join(".ssh").join("config"))
}

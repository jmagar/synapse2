use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "synapse_tests.rs"]
mod tests;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HostProtocol {
    Local,
    Ssh,
    Http,
    Https,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HostConfig {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default = "default_protocol")]
    pub protocol: HostProtocol,
    #[serde(rename = "sshUser", default)]
    pub ssh_user: Option<String>,
    #[serde(rename = "sshKeyPath", default)]
    pub ssh_key_path: Option<String>,
    #[serde(rename = "sshPort", default)]
    pub ssh_port: Option<u16>,
    #[serde(rename = "sshConfigPath", default)]
    pub ssh_config_path: Option<String>,
    #[serde(rename = "dockerSocketPath", default)]
    pub docker_socket_path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(rename = "composeSearchPaths", default)]
    pub compose_search_paths: Vec<String>,
    #[serde(rename = "execAllowlist", default)]
    pub exec_allowlist: Vec<String>,
}

impl HostConfig {
    pub fn local() -> Self {
        Self {
            name: "local".into(),
            host: "localhost".into(),
            port: None,
            protocol: HostProtocol::Local,
            ssh_user: None,
            ssh_key_path: None,
            ssh_port: None,
            ssh_config_path: None,
            docker_socket_path: Some("/var/run/docker.sock".into()),
            tags: vec!["local".into()],
            compose_search_paths: Vec::new(),
            exec_allowlist: Vec::new(),
        }
    }
}

fn default_protocol() -> HostProtocol {
    HostProtocol::Local
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct HostsFile {
    pub hosts: Vec<HostConfig>,
}

pub const ALLOWED_READ_COMMANDS: &[&str] = &[
    "cat", "head", "tail", "grep", "rg", "ls", "tree", "wc", "uniq", "diff", "stat", "file", "du",
    "df", "pwd", "hostname", "uptime", "whoami",
];

pub const EXEC_DENYLIST: &[&str] = &[
    "sh", "bash", "zsh", "dash", "sudo", "su", "doas", "python", "python3", "perl", "ruby", "node",
    "lua", "php", "curl", "wget", "nc", "ncat", "socat", "rm", "dd", "mkfs", "cp", "mv", "chmod",
    "chown", "docker", "podman", "kubectl", "kill", "pkill", "env", "xargs", "awk", "sed", "vi",
    "vim", "nano", "cargo", "rustc", "apt", "apk", "dnf",
];

pub fn validate_safe_path(path: &str) -> Result<()> {
    if path.is_empty() {
        bail!("path must not be empty");
    }

    // SECURITY FIX: Require absolute path (starts with /)
    if !path.starts_with('/') {
        bail!("absolute path required");
    }

    if path.split('/').any(|part| part == "..") {
        bail!("path traversal is not allowed");
    }
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
    {
        bail!("path contains unsafe characters");
    }

    // SECURITY FIX: Reject symlinks via symlink_metadata before any read.
    // std::fs::read_to_string follows symlinks — this protects against
    // symlink-based arbitrary file reads in world-writable directories.
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                bail!("symlinks not permitted");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Path doesn't exist yet — this is OK (e.g., during file creation).
            // The actual operation (read/write) will check existence.
        }
        Err(e) => bail!("cannot validate path: {e}"),
    }

    Ok(())
}

pub fn validate_command(command: &str, host_allowlist: &[String]) -> Result<()> {
    if command.is_empty()
        || !command
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("command name is invalid");
    }
    let deny: BTreeSet<&str> = EXEC_DENYLIST.iter().copied().collect();
    if deny.contains(command) {
        bail!("command is denied");
    }
    let allowed: BTreeSet<&str> = ALLOWED_READ_COMMANDS.iter().copied().collect();
    if allowed.contains(command) || host_allowlist.iter().any(|c| c == command) {
        return Ok(());
    }
    bail!("command is not allowlisted");
}

pub fn load_hosts() -> Result<Vec<HostConfig>> {
    if let Ok(raw) = std::env::var("SYNAPSE_HOSTS_CONFIG") {
        let hosts: Vec<HostConfig> = serde_json::from_str(&raw)?;
        if !hosts.is_empty() {
            return Ok(hosts);
        }
    }

    for path in host_config_paths() {
        if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            let parsed: HostsFile = serde_json::from_str(&raw)?;
            if !parsed.hosts.is_empty() {
                return Ok(parsed.hosts);
            }
        }
    }

    Ok(vec![HostConfig::local()])
}

fn host_config_paths() -> Vec<PathBuf> {
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

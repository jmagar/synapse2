//! Known-hosts wildcard warning and stale-socket sweep.
//!
//! - [`warn_on_known_hosts_wildcards`] scans `~/.ssh/known_hosts` at startup and
//!   logs a warning if any wildcard host pattern is found (MITM risk).
//! - [`sweep_stale_sockets`] / [`sweep_stale_sockets_in`] remove leftover
//!   `/tmp/synapse2-*-*.sock` files whose owning pid is no longer alive. Called
//!   once from `main.rs` before pool init to prevent socket accumulation across
//!   crashes.

use std::path::{Path, PathBuf};

// ── known_hosts wildcard warning ────────────────────────────────────────────

/// Scan `~/.ssh/known_hosts` and WARN if any host pattern contains a wildcard
/// (`*` or `?`). A wildcard entry trusts any host key, defeating the MITM
/// protection of `KnownHosts::Strict`. Called once at startup.
///
/// SECURITY (security-sentinel, MEDIUM): documents the assumption that the
/// user's known_hosts is wildcard-free; see docs/SECURITY.md.
pub fn warn_on_known_hosts_wildcards() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let path = Path::new(&home).join(".ssh").join("known_hosts");
    if let Some(patterns) = scan_known_hosts_wildcards(&path)
        && !patterns.is_empty()
    {
        tracing::warn!(
            count = patterns.len(),
            "~/.ssh/known_hosts contains wildcard host patterns ({}); \
                 these trust ANY host key and undermine StrictHostKeyChecking — \
                 see docs/SECURITY.md",
            patterns.join(", ")
        );
    }
}

/// Return the wildcard host patterns found in a known_hosts file, or `None` if
/// the file can't be read. Extracted for unit testing.
pub fn scan_known_hosts_wildcards(path: &Path) -> Option<Vec<String>> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut found = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // First whitespace-delimited field is the comma-separated host list.
        let Some(hosts) = line.split_whitespace().next() else {
            continue;
        };
        for host in hosts.split(',') {
            if host.contains('*') || host.contains('?') {
                found.push(host.to_string());
            }
        }
    }
    Some(found)
}

// ── Startup sweep ───────────────────────────────────────────────────────────

/// Remove stale `/tmp/synapse2-*-*.sock` files whose owning pid is no longer
/// running. Called once from `main.rs` before pool init to stop accumulation
/// across crashes (the socket persists on SIGKILL/panic).
pub fn sweep_stale_sockets() {
    sweep_stale_sockets_in(Path::new("/tmp"));
}

/// Sweep a specific directory (extracted so the unit test can point at a tmp
/// dir without touching the real `/tmp`). Returns the paths removed.
pub fn sweep_stale_sockets_in(dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return removed,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(pid) = parse_socket_pid(name) else {
            continue;
        };
        if !pid_is_alive(pid) && std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// Parse the pid out of `synapse2-{host}-{pid}.sock`. Host names contain
/// hyphens, so strip the fixed prefix/suffix and split from the RIGHT.
pub(crate) fn parse_socket_pid(name: &str) -> Option<u32> {
    let inner = name.strip_prefix("synapse2-")?.strip_suffix(".sock")?;
    let (_host, pid) = inner.rsplit_once('-')?;
    pid.parse::<u32>().ok()
}

/// Linux-only liveness check via `/proc/{pid}` — avoids pulling in `libc`/`nix`.
pub(crate) fn pid_is_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

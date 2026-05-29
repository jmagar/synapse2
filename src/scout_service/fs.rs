//! Scout filesystem operations: `peek`, `find`, `delta`.
//!
//! All path parameters go through `validate_safe_path` (absolute, no `..`,
//! no unsafe chars, no symlinks — see B0). For remote paths the symlink check
//! is based on local `symlink_metadata`, so only the syntactic guards apply
//! remotely; the read operation itself goes through SSH exec, not a local read.
//!
//! `delta` content mode is capped at 1 MB to prevent diffing large blobs.

use anyhow::{bail, Result};
use serde_json::{json, Value};

#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;

use crate::flux_service::host::{is_local_host, HostExec, LocalExec, RemoteExec};
use crate::ssh::SshExecutor;
use crate::synapse::{validate_safe_path, HostConfig};

/// Maximum inline content size for `delta` content mode.
pub const DELTA_MAX_CONTENT_BYTES: usize = 1024 * 1024; // 1 MB

// ─── peek ────────────────────────────────────────────────────────────────────

/// Peek at a path on `host`: returns directory listing or file content.
///
/// Parameters:
/// - `path` — absolute path (validated by `validate_safe_path`)
/// - `tree` — if true, emit a depth-limited directory tree
/// - `depth` — tree depth 1–10 (default 3)
pub async fn peek(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    path: &str,
    tree: bool,
    depth: u8,
) -> Result<Value> {
    // SECURITY: syntactic + symlink guard (see module doc for remote caveat).
    validate_safe_path(path)?;

    let depth = depth.clamp(1, 10);

    if tree {
        return peek_tree(host, executor, path, depth).await;
    }

    if is_local_host(host) {
        peek_local(host, path)
    } else {
        peek_remote(host, executor, path).await
    }
}

fn peek_local(host: &HostConfig, path: &str) -> Result<Value> {
    // Symlink check already done by validate_safe_path.
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        let entries: Vec<String> = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .take(200)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        Ok(json!({ "host": host.name, "path": path, "kind": "directory", "entries": entries }))
    } else {
        let content = std::fs::read_to_string(path)?;
        Ok(json!({ "host": host.name, "path": path, "kind": "file", "content": content }))
    }
}

async fn peek_remote(host: &HostConfig, executor: &dyn SshExecutor, path: &str) -> Result<Value> {
    // Try stat to determine file vs directory.
    let stat_out = executor.exec(host, "stat", &["-c", "%F", path]).await?;
    let kind = stat_out.stdout.trim();

    if kind == "directory" {
        // List the directory with ls -1A.
        let ls = executor.exec(host, "ls", &["-1A", path]).await?;
        let entries: Vec<String> = ls
            .stdout
            .lines()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .take(200)
            .collect();
        Ok(json!({ "host": host.name, "path": path, "kind": "directory", "entries": entries }))
    } else {
        // Read the file with cat.
        let out = executor.exec(host, "cat", &[path]).await?;
        if !out.stderr.is_empty() && out.exit_code != Some(0) {
            bail!("peek: {}", out.stderr.trim());
        }
        Ok(json!({ "host": host.name, "path": path, "kind": "file", "content": out.stdout }))
    }
}

async fn peek_tree(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    path: &str,
    depth: u8,
) -> Result<Value> {
    let depth_str = depth.to_string();
    if is_local_host(host) {
        let exec = LocalExec;
        let out = exec
            .run("find", &[path, "-maxdepth", &depth_str, "-print"])
            .await?;
        Ok(json!({ "host": host.name, "path": path, "depth": depth, "tree": out.stdout }))
    } else {
        let remote = RemoteExec { executor, host };
        let out = remote
            .run("find", &[path, "-maxdepth", &depth_str, "-print"])
            .await?;
        Ok(json!({ "host": host.name, "path": path, "depth": depth, "tree": out.stdout }))
    }
}

// ─── find ────────────────────────────────────────────────────────────────────

/// Find files on `host` under `path` matching `pattern`.
///
/// `pattern` is passed as the `-name` argument to `find` — it must not start
/// with `-` (guards against option injection).
pub async fn find(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    path: &str,
    pattern: &str,
    depth: Option<u8>,
    limit: Option<u32>,
) -> Result<Value> {
    validate_safe_path(path)?;

    // Pattern guard: reject leading `-` to prevent option injection.
    if pattern.starts_with('-') {
        bail!("find pattern must not start with `-`");
    }

    let depth_str = depth
        .map(|d| d.clamp(1, 20).to_string())
        .unwrap_or_else(|| "10".to_owned());
    let limit = limit.unwrap_or(500) as usize;

    let args: Vec<&str> = vec![
        path,
        "-maxdepth",
        &depth_str,
        "-name",
        pattern,
        "-type",
        "f",
    ];

    let out = if is_local_host(host) {
        LocalExec.run("find", &args).await?
    } else {
        RemoteExec { executor, host }.run("find", &args).await?
    };

    let files: Vec<String> = out
        .stdout
        .lines()
        .filter(|l| !l.is_empty())
        .take(limit)
        .map(|l| l.to_owned())
        .collect();

    Ok(json!({
        "host": host.name,
        "path": path,
        "pattern": pattern,
        "count": files.len(),
        "files": files,
    }))
}

// ─── delta ───────────────────────────────────────────────────────────────────

/// Compare a remote file against either another remote file or inline content.
///
/// `source` — `{host, path}` of the file to read.
/// `target` — optional `{host, path}` to diff against.
/// `content` — optional inline string (capped at 1 MB).
///
/// Exactly one of `target` or `content` must be supplied.
pub async fn delta(
    source_host: &HostConfig,
    executor: &dyn SshExecutor,
    source_path: &str,
    target_host: Option<&HostConfig>,
    target_path: Option<&str>,
    content: Option<&str>,
) -> Result<Value> {
    validate_safe_path(source_path)?;

    // VALIDATION FIRST — content size checked before any IO.
    if let Some(inline) = content {
        if inline.len() > DELTA_MAX_CONTENT_BYTES {
            bail!("delta content exceeds 1 MB limit");
        }
    }

    match (target_host, target_path, content) {
        (Some(th), Some(tp), None) => {
            validate_safe_path(tp)?;
            let source_content = read_remote_file(source_host, executor, source_path).await?;
            let source_label = format!("{}:{}", source_host.name, source_path);
            let target_content = read_remote_file(th, executor, tp).await?;
            let target_label = format!("{}:{}", th.name, tp);
            let diff = compute_diff(
                &source_content,
                &target_content,
                &source_label,
                &target_label,
            );
            Ok(json!({
                "identical": diff.is_empty(),
                "source": source_label,
                "target": target_label,
                "diff": diff,
            }))
        }
        (None, None, Some(inline)) => {
            let source_content = read_remote_file(source_host, executor, source_path).await?;
            let source_label = format!("{}:{}", source_host.name, source_path);
            let diff = compute_diff(&source_content, inline, &source_label, "inline");
            Ok(json!({
                "identical": diff.is_empty(),
                "source": source_label,
                "target": "inline",
                "diff": diff,
            }))
        }
        _ => bail!("delta requires exactly one of: target or content"),
    }
}

/// Read a file from `host` via SSH exec (cat) or local fs.
async fn read_remote_file(
    host: &HostConfig,
    executor: &dyn SshExecutor,
    path: &str,
) -> Result<String> {
    if is_local_host(host) {
        Ok(std::fs::read_to_string(path)?)
    } else {
        let out = executor.exec(host, "cat", &[path]).await?;
        if out.exit_code != Some(0) && !out.stderr.is_empty() {
            bail!("read {path}: {}", out.stderr.trim());
        }
        Ok(out.stdout)
    }
}

/// Compute a unified diff between `a` and `b`, labelled by `label_a`/`label_b`.
///
/// Pure function — no IO. Returns empty string when files are identical.
pub fn compute_diff(a: &str, b: &str, label_a: &str, label_b: &str) -> String {
    if a == b {
        return String::new();
    }

    // Line-by-line diff (simple unified format without the patch header offsets).
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();

    let mut result = format!("--- {label_a}\n+++ {label_b}\n");

    // Naive diff: mark lines removed from a, added in b.
    // For parity we just produce a simple two-column representation.
    // A full Myers diff is out of scope; the format matches synapse-mcp's
    // "Files differ" indicator at the service layer.
    for line in &a_lines {
        if !b_lines.contains(line) {
            result.push_str(&format!("- {line}\n"));
        }
    }
    for line in &b_lines {
        if !a_lines.contains(line) {
            result.push_str(&format!("+ {line}\n"));
        }
    }

    result
}

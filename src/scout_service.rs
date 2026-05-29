//! Scout domain service — node discovery, filesystem peek, remote exec.
//!
//! Extracted from the `SynapseService` god-object so scout concerns live in one
//! focused module. Resolves hosts through the injected `HostRepository`.
//!
//! All scout business logic lives in the submodules below. CLI (`cli.rs`) and
//! MCP (via `actions.rs`) are thin shims that call into these methods.
//!
//! # Submodule layout
//!
//! | Submodule | Responsibilities |
//! |-----------|-----------------|
//! | `fs`      | `peek`, `find`, `delta` — filesystem read operations |
//! | `proc`    | `ps`, `df` — process / disk inspection |
//! | `exec`    | `exec`, `emit`, `beam` — destructive execution / transfer |

use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::elicitation_gate::Confirmer;
use crate::host_config::HostRepository;
use crate::scout;
use crate::ssh::{SshExecutor, SshPool};

pub mod exec;
pub mod fs;
pub mod proc;

#[cfg(test)]
#[path = "scout_service_tests.rs"]
mod tests;

/// Scout domain service. Cheap to clone — all fields are `Arc`-shared.
#[derive(Clone)]
pub struct ScoutService {
    /// Host configuration repository — shared with the facade and flux so an
    /// injected repo (tests / DI) resolves the same hosts everywhere.
    pub(crate) host_repo: Arc<dyn HostRepository>,
    /// SSH session pool — shared so ControlMaster connections are reused.
    pub(crate) ssh_pool: Arc<dyn SshExecutor>,
}

impl ScoutService {
    /// Construct with the supplied host repository and a default SSH pool.
    pub fn new(host_repo: Arc<dyn HostRepository>) -> Self {
        Self {
            host_repo,
            ssh_pool: Arc::new(SshPool::new()),
        }
    }

    /// Inject a custom SSH executor (for testing — pass a mock).
    pub fn with_ssh_executor(mut self, executor: Arc<dyn SshExecutor>) -> Self {
        self.ssh_pool = executor;
        self
    }

    // ── help ─────────────────────────────────────────────────────────────────

    pub async fn help(&self) -> Result<Value> {
        Ok(json!({
            "tool": "scout",
            "actions": ["nodes", "peek", "find", "ps", "df", "delta", "exec", "emit", "beam", "help"],
            "destructive": ["exec", "emit", "beam"],
            "deferred": ["zfs", "logs"],
        }))
    }

    // ── nodes ────────────────────────────────────────────────────────────────

    pub async fn nodes(&self) -> Result<Value> {
        scout::nodes(self.host_repo.as_ref())
    }

    // ── peek ─────────────────────────────────────────────────────────────────

    /// Peek at a path on `host_name`. `tree` triggers a depth-limited listing.
    pub async fn peek(&self, host_name: &str, path: &str, tree: bool, depth: u8) -> Result<Value> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        fs::peek(&host, self.ssh_pool.as_ref(), path, tree, depth).await
    }

    // ── find ─────────────────────────────────────────────────────────────────

    pub async fn find(
        &self,
        host_name: &str,
        path: &str,
        pattern: &str,
        depth: Option<u8>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        fs::find(&host, self.ssh_pool.as_ref(), path, pattern, depth, limit).await
    }

    // ── ps ───────────────────────────────────────────────────────────────────

    pub async fn ps(
        &self,
        host_name: &str,
        sort: Option<&str>,
        grep: Option<&str>,
        user: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        proc::ps(&host, self.ssh_pool.as_ref(), sort, grep, user, limit).await
    }

    // ── df ───────────────────────────────────────────────────────────────────

    pub async fn df(&self, host_name: &str, path: Option<&str>) -> Result<Value> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        proc::df(&host, self.ssh_pool.as_ref(), path).await
    }

    // ── delta ────────────────────────────────────────────────────────────────

    pub async fn delta(
        &self,
        source_host_name: &str,
        source_path: &str,
        target_host_name: Option<&str>,
        target_path: Option<&str>,
        content: Option<&str>,
    ) -> Result<Value> {
        let source_host = scout::resolve_host(self.host_repo.as_ref(), source_host_name)?;
        let target_host = match target_host_name {
            Some(name) => Some(scout::resolve_host(self.host_repo.as_ref(), name)?),
            None => None,
        };
        fs::delta(
            &source_host,
            self.ssh_pool.as_ref(),
            source_path,
            target_host.as_ref(),
            target_path,
            content,
        )
        .await
    }

    // ── exec ─────────────────────────────────────────────────────────────────

    /// Run `command` on `host_name`, gated by `confirmer`.
    ///
    /// `path` is the optional working directory (local hosts only — SSH exec
    /// cannot change directory without a shell; the no-shell invariant is locked).
    pub async fn exec(
        &self,
        host_name: &str,
        path: Option<&str>,
        command: &str,
        args: &[String],
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
        exec::exec(
            &host,
            self.ssh_pool.as_ref(),
            confirmer,
            command,
            args,
            path,
        )
        .await
    }

    // ── emit ─────────────────────────────────────────────────────────────────

    /// Run `command` across multiple `targets`, gated by `confirmer`.
    pub async fn emit(
        &self,
        targets: &[exec::EmitTarget],
        command: &str,
        args: &[String],
        timeout_secs: Option<u64>,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        exec::emit(
            targets,
            Arc::clone(&self.ssh_pool),
            confirmer,
            command,
            args,
            timeout_secs,
        )
        .await
    }

    /// Resolve a list of `{host_name, path}` into `EmitTarget`s.
    pub fn resolve_emit_targets(
        &self,
        raw: &[(String, Option<String>)],
    ) -> Result<Vec<exec::EmitTarget>> {
        raw.iter()
            .map(|(host_name, path)| {
                let host = scout::resolve_host(self.host_repo.as_ref(), host_name)?;
                Ok(exec::EmitTarget {
                    host,
                    path: path.clone(),
                })
            })
            .collect()
    }

    // ── beam ─────────────────────────────────────────────────────────────────

    pub async fn beam(
        &self,
        source_host_name: &str,
        source_path: &str,
        dest_host_name: &str,
        dest_path: &str,
        confirmer: &dyn Confirmer,
    ) -> Result<Value> {
        let source_host = scout::resolve_host(self.host_repo.as_ref(), source_host_name)?;
        let dest_host = scout::resolve_host(self.host_repo.as_ref(), dest_host_name)?;
        exec::beam(&source_host, source_path, &dest_host, dest_path, confirmer).await
    }
}

//! Multi-host fanout helper with `PartialSuccess` aggregation.
//!
//! Provides [`fanout`] — a generic, bounded-concurrency combinator that runs
//! an async per-host closure across a slice of hosts and aggregates results
//! into a [`FanoutOutcome`], preserving host identity in both success and
//! failure arms.
//!
//! # Design decisions (locked in bead `rmcp-template-3tt.6`)
//!
//! - **Concurrency cap:** `min(N_hosts, 8)`, matching the synapse-mcp default.
//! - **`FanoutOutcome<T, E>` variants:** `AllOk`, `PartialSuccess`, `AllFailed`.
//!   Callers pattern-match; convenience methods (`ok_results`, `err_results`,
//!   `is_partial`, `is_total_failure`) make the common "iterate all oks" case
//!   ergonomic without collapsing the enum.
//! - **`FuturesUnordered` + `Arc<Semaphore>`:** `acquire()` is called INSIDE the
//!   spawned future, not before, to avoid deadlocks under load (lab learning).
//! - **Stable result ordering:** futures complete in arbitrary order;
//!   `fanout` re-sorts by the original host index before building the outcome.
//! - **No outer timeout:** per-host timeouts are the responsibility of the
//!   caller's op closure (e.g. `tokio::time::timeout` around the SSH call).
//! - **Generic error type `E`:** the spec says `ErrorData` but fanout runs at
//!   the service layer where errors are `anyhow::Error` or domain-specific types;
//!   generic `E` avoids an rmcp-boundary type leaking into the service layer.
//!
//! # Note on formatters
//!
//! `FanoutOutcome<T, E>` is generic — domain-specific markdown rendering belongs
//! in the concrete consumers (B8/B10/B11/B14). This module provides
//! [`FanoutOutcome::error_summary`] for a human-readable per-host error list
//! that any consumer can embed.

use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Semaphore;

#[cfg(test)]
#[path = "fanout_tests.rs"]
mod tests;

// ---------------------------------------------------------------------------
// Per-host result
// ---------------------------------------------------------------------------

/// The outcome of a single-host operation within a fanout.
///
/// Carries the host name (and original index for stable re-ordering) in both
/// arms so callers never lose the association between result and host.
#[derive(Debug)]
pub struct HostResult<T, E> {
    /// Position in the original input slice — used internally for stable ordering.
    pub(crate) index: usize,
    /// Host name this result belongs to.
    pub host: String,
    /// The per-host outcome.
    pub result: Result<T, E>,
}

impl<T, E> HostResult<T, E> {
    /// Returns `true` if the per-host operation succeeded.
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }
}

// ---------------------------------------------------------------------------
// Aggregated fanout outcome
// ---------------------------------------------------------------------------

/// Aggregated result of running an op across N hosts with bounded concurrency.
///
/// The three variants model all possible combinations of per-host success/failure:
///
/// | Variant | Meaning |
/// |---------|---------|
/// | `AllOk` | Every host succeeded — `Vec` is in original host order. |
/// | `PartialSuccess` | At least one succeeded AND at least one failed. |
/// | `AllFailed` | Every host failed — no usable data was returned. |
///
/// Use [`FanoutOutcome::ok_results`] and [`FanoutOutcome::err_results`] to iterate
/// without matching all three variants.
#[derive(Debug)]
pub enum FanoutOutcome<T, E> {
    /// Every host operation succeeded.
    AllOk(Vec<(String, T)>),
    /// Some hosts succeeded, some failed.
    PartialSuccess {
        ok: Vec<(String, T)>,
        errors: Vec<(String, E)>,
    },
    /// Every host operation failed.
    AllFailed(Vec<(String, E)>),
}

impl<T, E: std::fmt::Display> FanoutOutcome<T, E> {
    /// Returns `true` if this is `PartialSuccess`.
    pub fn is_partial(&self) -> bool {
        matches!(self, Self::PartialSuccess { .. })
    }

    /// Returns `true` if every host failed (`AllFailed`).
    pub fn is_total_failure(&self) -> bool {
        matches!(self, Self::AllFailed(_))
    }

    /// Returns `true` if every host succeeded (`AllOk`).
    pub fn is_all_ok(&self) -> bool {
        matches!(self, Self::AllOk(_))
    }

    /// Borrow the successful (host, value) pairs from any variant.
    ///
    /// Returns an empty slice for `AllFailed`.
    pub fn ok_results(&self) -> &[(String, T)] {
        match self {
            Self::AllOk(ok) => ok.as_slice(),
            Self::PartialSuccess { ok, .. } => ok.as_slice(),
            Self::AllFailed(_) => &[],
        }
    }

    /// Borrow the failed (host, error) pairs from any variant.
    ///
    /// Returns an empty slice for `AllOk`.
    pub fn err_results(&self) -> &[(String, E)] {
        match self {
            Self::AllOk(_) => &[],
            Self::PartialSuccess { errors, .. } => errors.as_slice(),
            Self::AllFailed(errors) => errors.as_slice(),
        }
    }

    /// Render a compact multi-line summary of per-host errors.
    ///
    /// Returns an empty string when there are no errors.
    /// Each line has the form `  - <host>: <error>`.
    ///
    /// This is intended for embedding in markdown renderers inside action
    /// handlers (B8/B10/B11/B14) without pulling formatter logic into this
    /// generic module.
    pub fn error_summary(&self) -> String {
        let errors = self.err_results();
        if errors.is_empty() {
            return String::new();
        }
        errors
            .iter()
            .map(|(host, err)| format!("  - {host}: {err}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------------------------------------------------------------------------
// Core fanout function
// ---------------------------------------------------------------------------

/// Run `op` against every host in `hosts` with a concurrency cap of `min(N, 8)`.
///
/// Results are aggregated into a [`FanoutOutcome`] that preserves:
/// - host identity on both success and failure arms
/// - original host ordering (despite `FuturesUnordered` completion order)
///
/// # Concurrency model
///
/// Uses [`FuturesUnordered`] + [`Arc<Semaphore>`]. The semaphore permit is
/// acquired **inside** each future (not before pushing to the set) to prevent
/// deadlocks under load.
///
/// # Timeouts
///
/// This function imposes **no outer timeout**. Callers must wrap `op` in
/// `tokio::time::timeout` if they need per-host deadline enforcement.
///
/// # Example
///
/// ```rust,ignore
/// let outcome = fanout(&hosts, |host| async move {
///     tokio::time::timeout(
///         Duration::from_secs(10),
///         some_service.call(&host),
///     ).await.map_err(|_| "timed out".to_string())?
/// }).await;
/// ```
pub async fn fanout<T, E, F, Fut>(
    hosts: &[crate::synapse::HostConfig],
    op: F,
) -> FanoutOutcome<T, E>
where
    F: Fn(crate::synapse::HostConfig) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    T: Send + 'static,
    E: Send + 'static,
{
    let n = hosts.len();
    if n == 0 {
        return FanoutOutcome::AllOk(Vec::new());
    }

    let cap = n.min(8);
    let sem = Arc::new(Semaphore::new(cap));

    let mut futures: FuturesUnordered<_> = FuturesUnordered::new();

    for (index, host) in hosts.iter().enumerate() {
        let host_name = host.name.clone();
        let host_clone = host.clone();
        let sem_clone = Arc::clone(&sem);
        let fut = op(host_clone);

        futures.push(async move {
            // Acquire the permit INSIDE the future (not before push) to prevent
            // deadlock under load — the semaphore is not held across poll boundaries.
            let _permit = sem_clone.acquire().await.expect("semaphore never closed");
            let result = fut.await;
            HostResult {
                index,
                host: host_name,
                result,
            }
        });
    }

    // Collect all results (arbitrary completion order).
    let mut raw: Vec<HostResult<T, E>> = Vec::with_capacity(n);
    while let Some(hr) = futures.next().await {
        raw.push(hr);
    }

    // Re-sort by original input index for stable ordering.
    raw.sort_unstable_by_key(|hr| hr.index);

    // Partition into ok and err vecs.
    let mut ok: Vec<(String, T)> = Vec::new();
    let mut errors: Vec<(String, E)> = Vec::new();

    for hr in raw {
        match hr.result {
            Ok(v) => ok.push((hr.host, v)),
            Err(e) => errors.push((hr.host, e)),
        }
    }

    match (ok.is_empty(), errors.is_empty()) {
        (false, true) => FanoutOutcome::AllOk(ok),
        (false, false) => FanoutOutcome::PartialSuccess { ok, errors },
        (true, false) => FanoutOutcome::AllFailed(errors),
        (true, true) => {
            // Unreachable: n > 0 guarantees at least one future ran.
            FanoutOutcome::AllOk(Vec::new())
        }
    }
}

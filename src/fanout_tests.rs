//! Tests for `src/fanout.rs`.
//!
//! Covers:
//! - All-success path returns `AllOk` in stable original host order.
//! - Partial-failure returns `PartialSuccess` with both arms populated.
//! - All-failure returns `AllFailed`.
//! - Empty host set returns `AllOk([])`.
//! - Single host.
//! - Concurrency cap: N=20 hosts, cap=8 — never more than 8 in-flight (measured).
//! - Per-host timeout in op does not affect other hosts.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::fanout::{FanoutOutcome, fanout};
use crate::synapse::{HostConfig, HostProtocol};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a minimal `HostConfig` for use in tests.
fn fake_host(name: &str) -> HostConfig {
    HostConfig {
        name: name.to_string(),
        host: format!("{name}.example.com"),
        port: None,
        protocol: HostProtocol::Ssh,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: Vec::new(),
        scout_read_roots: Vec::new(),
        exec_allowlist: Vec::new(),
    }
}

fn fake_hosts(names: &[&str]) -> Vec<HostConfig> {
    names.iter().map(|n| fake_host(n)).collect()
}

// ---------------------------------------------------------------------------
// Basic outcome variants
// ---------------------------------------------------------------------------

#[tokio::test]
async fn all_ok_returns_allok_in_stable_order() {
    let hosts = fake_hosts(&["alpha", "beta", "gamma"]);
    let outcome = fanout(&hosts, |host| async move {
        // Small async yield to let futures interleave.
        tokio::task::yield_now().await;
        Ok::<String, String>(format!("ok:{}", host.name))
    })
    .await;

    assert!(
        matches!(outcome, FanoutOutcome::AllOk(_)),
        "expected AllOk, got {outcome:?}"
    );
    let FanoutOutcome::AllOk(results) = outcome else {
        unreachable!()
    };
    // Stable original order.
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], ("alpha".to_string(), "ok:alpha".to_string()));
    assert_eq!(results[1], ("beta".to_string(), "ok:beta".to_string()));
    assert_eq!(results[2], ("gamma".to_string(), "ok:gamma".to_string()));
}

#[tokio::test]
async fn partial_failure_returns_partial_success_with_both_arms() {
    // alpha succeeds, beta fails, gamma succeeds.
    let hosts = fake_hosts(&["alpha", "beta", "gamma"]);
    let outcome = fanout(&hosts, |host| async move {
        tokio::task::yield_now().await;
        if host.name == "beta" {
            Err::<String, String>("beta-error".to_string())
        } else {
            Ok(format!("ok:{}", host.name))
        }
    })
    .await;

    assert!(outcome.is_partial(), "expected PartialSuccess");

    let ok = outcome.ok_results();
    let err = outcome.err_results();

    assert_eq!(ok.len(), 2);
    assert_eq!(err.len(), 1);

    // Ok arm preserves stable order (alpha before gamma).
    assert_eq!(ok[0].0, "alpha");
    assert_eq!(ok[1].0, "gamma");

    // Error arm has the failing host.
    assert_eq!(err[0].0, "beta");
    assert_eq!(err[0].1, "beta-error");
}

#[tokio::test]
async fn all_fail_returns_allfailed() {
    let hosts = fake_hosts(&["a", "b", "c"]);
    let outcome = fanout(&hosts, |host| async move {
        tokio::task::yield_now().await;
        Err::<String, String>(format!("err:{}", host.name))
    })
    .await;

    assert!(outcome.is_total_failure(), "expected AllFailed");
    let errors = outcome.err_results();
    assert_eq!(errors.len(), 3);
    // Stable original order.
    assert_eq!(errors[0].0, "a");
    assert_eq!(errors[1].0, "b");
    assert_eq!(errors[2].0, "c");
}

#[tokio::test]
async fn empty_host_set_returns_allok_empty() {
    let hosts: Vec<HostConfig> = vec![];
    let outcome = fanout(&hosts, |_host| async move { Ok::<(), String>(()) }).await;

    assert!(outcome.is_all_ok(), "expected AllOk for empty input");
    assert!(outcome.ok_results().is_empty(), "ok results must be empty");
}

#[tokio::test]
async fn single_host_ok() {
    let hosts = fake_hosts(&["solo"]);
    let outcome = fanout(&hosts, |host| async move {
        Ok::<String, String>(format!("result:{}", host.name))
    })
    .await;

    assert!(outcome.is_all_ok());
    let ok = outcome.ok_results();
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0], ("solo".to_string(), "result:solo".to_string()));
}

#[tokio::test]
async fn single_host_err() {
    let hosts = fake_hosts(&["solo"]);
    let outcome = fanout(&hosts, |_host| async move {
        Err::<String, String>("nope".to_string())
    })
    .await;

    assert!(outcome.is_total_failure());
    let err = outcome.err_results();
    assert_eq!(err.len(), 1);
    assert_eq!(err[0].0, "solo");
}

// ---------------------------------------------------------------------------
// Concurrency cap
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrency_cap_is_at_most_8_with_n_20_hosts() {
    // N=20 hosts, each op sleeps briefly so concurrency builds up.
    // We measure the maximum concurrent in-flight count and assert it never
    // exceeds the cap of 8.
    let names: Vec<String> = (0..20).map(|i| format!("host{i:02}")).collect();
    let name_strs: Vec<&str> = names.iter().map(String::as_str).collect();
    let hosts = fake_hosts(&name_strs);

    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));

    let in_flight_clone = Arc::clone(&in_flight);
    let max_seen_clone = Arc::clone(&max_seen);

    let outcome = fanout(&hosts, move |_host| {
        let in_flight = Arc::clone(&in_flight_clone);
        let max_seen = Arc::clone(&max_seen_clone);
        async move {
            let current = in_flight.fetch_add(1, Ordering::Relaxed) + 1;
            // Track the high-water mark.
            let mut prev = max_seen.load(Ordering::Relaxed);
            while current > prev {
                match max_seen.compare_exchange_weak(
                    prev,
                    current,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(observed) => prev = observed,
                }
            }
            // Sleep long enough for multiple futures to be in-flight simultaneously.
            tokio::time::sleep(Duration::from_millis(20)).await;
            in_flight.fetch_sub(1, Ordering::Relaxed);
            Ok::<(), String>(())
        }
    })
    .await;

    assert!(outcome.is_all_ok(), "expected all hosts to succeed");
    let observed_max = max_seen.load(Ordering::Relaxed);
    assert!(
        observed_max <= 8,
        "concurrency exceeded cap=8: max_in_flight={observed_max}"
    );
    // Also assert that the cap was actually exercised (at least some concurrency).
    assert!(
        observed_max > 1,
        "expected some concurrent in-flight ops, got max={observed_max}"
    );
}

// ---------------------------------------------------------------------------
// Per-host timeout in op does not affect other hosts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn per_host_timeout_in_op_does_not_block_others() {
    // slow_host will be wrapped with a short timeout and will always "fail"
    // due to timeout. fast_host completes immediately.
    let hosts = fake_hosts(&["fast", "slow"]);

    let outcome = fanout(&hosts, |host| async move {
        if host.name == "slow" {
            // Op itself simulates a long operation.
            let res = tokio::time::timeout(Duration::from_millis(10), async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok::<String, String>("unreachable".to_string())
            })
            .await;
            // Timeout returns Err(Elapsed), map to our error type.
            res.unwrap_or_else(|_| Err("timed out".to_string()))
        } else {
            Ok(format!("ok:{}", host.name))
        }
    })
    .await;

    assert!(
        outcome.is_partial(),
        "expected PartialSuccess: fast ok, slow timed out"
    );

    let ok = outcome.ok_results();
    let err = outcome.err_results();

    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].0, "fast");

    assert_eq!(err.len(), 1);
    assert_eq!(err[0].0, "slow");
    assert_eq!(err[0].1, "timed out");
}

// ---------------------------------------------------------------------------
// Accessors and helpers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn error_summary_is_empty_for_all_ok() {
    let hosts = fake_hosts(&["x"]);
    let outcome = fanout(&hosts, |_| async { Ok::<(), String>(()) }).await;
    assert_eq!(outcome.error_summary(), "");
}

#[tokio::test]
async fn error_summary_lists_failed_hosts() {
    let hosts = fake_hosts(&["a", "b"]);
    let outcome = fanout(&hosts, |host| async move {
        Err::<(), String>(format!("err:{}", host.name))
    })
    .await;

    let summary = outcome.error_summary();
    assert!(summary.contains("  - a: err:a"), "summary: {summary}");
    assert!(summary.contains("  - b: err:b"), "summary: {summary}");
}

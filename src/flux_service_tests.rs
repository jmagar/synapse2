//! Unit tests for FluxService — sidecar for src/flux_service.rs.
//!
//! Verifies the help contract and host_status shape without requiring a live
//! Docker daemon (help and the static fields of host_status don't shell out).

use super::*;
use crate::host_config::FileHostRepository;

fn stub_flux() -> FluxService {
    FluxService::new(Arc::new(FileHostRepository::default()))
}

#[tokio::test]
async fn test_flux_help_shape() {
    let flux = stub_flux();
    let result = flux.help().await.expect("help should succeed");

    assert_eq!(result["tool"], "flux");
    assert_eq!(
        result["actions"]["docker"],
        serde_json::json!([
            "info", "df", "images", "networks", "volumes", "pull", "build", "rmi", "prune"
        ])
    );
    assert_eq!(
        result["actions"]["container"],
        serde_json::json!(["list", "inspect", "logs", "stats", "top", "search"])
    );
    assert_eq!(result["actions"]["host"], serde_json::json!(["status"]));
}

#[test]
fn flatten_list_outcome_partial_success() {
    use crate::fanout::FanoutOutcome;
    let outcome: FanoutOutcome<Vec<serde_json::Value>, String> = FanoutOutcome::PartialSuccess {
        ok: vec![(
            "dookie".to_owned(),
            vec![serde_json::json!({"name": "nginx", "host": "dookie"})],
        )],
        errors: vec![("tootie".to_owned(), "connection refused".to_owned())],
    };
    let out = flatten_list_outcome(outcome, "containers");
    assert_eq!(out["count"], 1);
    assert_eq!(out["partial"], true);
    assert_eq!(out["containers"][0]["name"], "nginx");
    assert_eq!(out["errors"]["tootie"], "connection refused");
}

#[test]
fn flatten_list_outcome_all_ok_has_no_errors() {
    use crate::fanout::FanoutOutcome;
    let outcome: FanoutOutcome<Vec<serde_json::Value>, String> = FanoutOutcome::AllOk(vec![(
        "dookie".to_owned(),
        vec![serde_json::json!({"name": "nginx"})],
    )]);
    let out = flatten_list_outcome(outcome, "containers");
    assert_eq!(out["partial"], false);
    assert!(out.get("errors").is_none());
}

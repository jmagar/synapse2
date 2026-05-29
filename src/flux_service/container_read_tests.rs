//! Unit tests for the pure read-only container operations (B8).
//!
//! These exercise the per-host pure functions against a [`MockDockerClient`]
//! and the standalone parsing helpers — no live docker daemon required.

use super::*;
use crate::docker_client::MockDockerClient;
use bollard::container::LogOutput;
use bollard::models::{
    ContainerStatsResponse, ContainerSummary, ContainerSummaryStateEnum, ContainerTopResponse,
};
use bytes::Bytes;

fn summary(name: &str, image: &str, state: ContainerSummaryStateEnum) -> ContainerSummary {
    ContainerSummary {
        id: Some(format!("id-{name}")),
        names: Some(vec![format!("/{name}")]),
        image: Some(image.to_owned()),
        state: Some(state),
        status: Some("Up 2 hours".to_owned()),
        labels: Some(
            [("com.docker.compose.project".to_owned(), "web".to_owned())]
                .into_iter()
                .collect(),
        ),
        ..Default::default()
    }
}

fn mock_with(containers: Vec<ContainerSummary>) -> MockDockerClient {
    MockDockerClient {
        containers,
        ..Default::default()
    }
}

// ───────────────────────────── list filters ─────────────────────────────

#[tokio::test]
async fn list_no_filter_returns_all_host_tagged() {
    let client = mock_with(vec![
        summary("nginx", "nginx:latest", ContainerSummaryStateEnum::RUNNING),
        summary("db", "postgres:16", ContainerSummaryStateEnum::EXITED),
    ]);
    let out = list_on_host(&client, "dookie", &ListFilters::default())
        .await
        .unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["host"], "dookie");
    assert_eq!(out[0]["name"], "nginx");
}

#[tokio::test]
async fn list_state_filter_prunes_non_matching() {
    let client = mock_with(vec![
        summary("nginx", "nginx:latest", ContainerSummaryStateEnum::RUNNING),
        summary("db", "postgres:16", ContainerSummaryStateEnum::EXITED),
    ]);
    let filters = ListFilters {
        state: Some("running".to_owned()),
        ..Default::default()
    };
    let out = list_on_host(&client, "h", &filters).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["name"], "nginx");
    assert_eq!(out[0]["state"], "running");
}

#[tokio::test]
async fn list_name_filter_is_case_insensitive_substring() {
    let client = mock_with(vec![
        summary(
            "nginx-proxy",
            "nginx:latest",
            ContainerSummaryStateEnum::RUNNING,
        ),
        summary("db", "postgres:16", ContainerSummaryStateEnum::RUNNING),
    ]);
    let filters = ListFilters {
        name_filter: Some("NGINX".to_owned()),
        ..Default::default()
    };
    let out = list_on_host(&client, "h", &filters).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["name"], "nginx-proxy");
}

#[tokio::test]
async fn list_image_filter_prunes() {
    let client = mock_with(vec![
        summary("a", "nginx:latest", ContainerSummaryStateEnum::RUNNING),
        summary("b", "postgres:16", ContainerSummaryStateEnum::RUNNING),
    ]);
    let filters = ListFilters {
        image_filter: Some("postgres".to_owned()),
        ..Default::default()
    };
    let out = list_on_host(&client, "h", &filters).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["image"], "postgres:16");
}

// ───────────────────────────── search ─────────────────────────────

#[test]
fn search_matches_name_image_and_labels() {
    let c = serde_json::json!({
        "name": "web-frontend",
        "image": "node:20",
        "labels": { "com.docker.compose.project": "shop", "tier": "edge" },
    });
    assert!(search_matches(&c, "frontend")); // name
    assert!(search_matches(&c, "node")); // image
    assert!(search_matches(&c, "shop")); // label value
    assert!(search_matches(&c, "tier")); // label key
    assert!(search_matches(&c, "WEB")); // case-insensitive
    assert!(!search_matches(&c, "database")); // no match
}

// ───────────────────────────── inspect ─────────────────────────────

#[tokio::test]
async fn inspect_full_vs_summary() {
    let mut client = MockDockerClient::new();
    let resp = bollard::models::ContainerInspectResponse {
        id: Some("abc123".to_owned()),
        name: Some("/nginx".to_owned()),
        ..Default::default()
    };
    client.inspect.insert("nginx".to_owned(), resp);

    let full = inspect_on_host(&client, "h", "nginx", false).await.unwrap();
    assert_eq!(full["host"], "h");
    assert_eq!(full["summary"], false);
    assert_eq!(full["container"]["Id"], "abc123");

    let brief = inspect_on_host(&client, "h", "nginx", true).await.unwrap();
    assert_eq!(brief["summary"], true);
    assert_eq!(brief["container"]["id"], "abc123");
    assert_eq!(brief["container"]["name"], "/nginx");
    // summary form must NOT carry the full PascalCase inspect body
    assert!(brief["container"].get("Id").is_none());
}

// ───────────────────────────── top ─────────────────────────────

#[tokio::test]
async fn top_returns_titles_and_processes() {
    let mut client = MockDockerClient::new();
    client.top.insert(
        "nginx".to_owned(),
        ContainerTopResponse {
            titles: Some(vec!["PID".to_owned(), "CMD".to_owned()]),
            processes: Some(vec![vec!["1".to_owned(), "nginx".to_owned()]]),
        },
    );
    let out = top_on_host(&client, "h", "nginx").await.unwrap();
    assert_eq!(out["titles"][0], "PID");
    assert_eq!(out["processes"][0][1], "nginx");
    assert_eq!(out["container"], "nginx");
}

// ───────────────────────────── stats ─────────────────────────────

#[tokio::test]
async fn stats_reads_one_shot_frame() {
    let stat = ContainerStatsResponse {
        name: Some("/nginx".to_owned()),
        ..Default::default()
    };
    let client = MockDockerClient {
        stats_frames: vec![stat],
        ..Default::default()
    };
    let out = stats_on_host(&client, "h", "nginx").await.unwrap();
    assert_eq!(out["host"], "h");
    assert_eq!(out["stats"]["name"], "/nginx");
}

#[tokio::test]
async fn stats_empty_stream_errors_so_find_host_advances() {
    // Default mock yields no stats frame → must error (not Ok(Null)) so the
    // find-host caller moves to the next host instead of stopping here.
    let client = MockDockerClient::new();
    assert!(stats_on_host(&client, "h", "ghost").await.is_err());
}

#[tokio::test]
async fn logs_missing_container_errors_so_find_host_advances() {
    // A frame-level error must propagate (not be swallowed) so an absent
    // container on one host doesn't masquerade as empty logs for that host.
    let client = MockDockerClient {
        logs_error: true,
        ..Default::default()
    };
    let opts = build_logs_options(&LogOptions::default()).unwrap();
    assert!(collect_log_lines(&client, "ghost", opts).await.is_err());
}

// ───────────────────────────── logs ─────────────────────────────

#[tokio::test]
async fn logs_collect_and_grep_happy_path() {
    let frames = vec![
        LogOutput::StdOut {
            message: Bytes::from("starting up\nlisten on 80\n"),
        },
        LogOutput::StdErr {
            message: Bytes::from("WARN slow query\n"),
        },
    ];
    let client = MockDockerClient {
        log_frames: frames,
        ..Default::default()
    };
    let opts = build_logs_options(&LogOptions::default()).unwrap();
    let lines = collect_log_lines(&client, "nginx", opts).await.unwrap();
    assert_eq!(lines.len(), 3);

    let grepped = grep_lines(lines, Some("WARN"));
    assert_eq!(grepped, vec!["WARN slow query".to_owned()]);

    let value = logs_value("h", "nginx", grepped);
    assert_eq!(value["count"], 1);
    assert_eq!(value["lines"][0], "WARN slow query");
}

#[test]
fn log_output_lines_splits_and_trims() {
    let frame = LogOutput::StdOut {
        message: Bytes::from("line one\r\nline two\n\n"),
    };
    let lines = log_output_lines(&frame);
    assert_eq!(lines, vec!["line one".to_owned(), "line two".to_owned()]);
}

#[test]
fn grep_empty_pattern_is_noop() {
    let lines = vec!["a".to_owned(), "b".to_owned()];
    assert_eq!(grep_lines(lines.clone(), Some("")), lines);
    assert_eq!(grep_lines(lines.clone(), None), lines);
}

// ───────────────────────────── build_logs_options ─────────────────────────────

#[test]
fn logs_options_stream_selection() {
    let both = build_logs_options(&LogOptions::default()).unwrap();
    assert!(both.stdout && both.stderr);
    assert!(!both.follow, "logs must be one-shot (follow=false)");

    let out = build_logs_options(&LogOptions {
        stream: "stdout".to_owned(),
        ..Default::default()
    })
    .unwrap();
    assert!(out.stdout && !out.stderr);

    let err = build_logs_options(&LogOptions {
        stream: "stderr".to_owned(),
        ..Default::default()
    })
    .unwrap();
    assert!(!err.stdout && err.stderr);
}

#[test]
fn logs_lines_clamped_to_max() {
    let opts = build_logs_options(&LogOptions {
        lines: 99_999,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(opts.tail, MAX_LOG_LINES.to_string());

    let zero = build_logs_options(&LogOptions {
        lines: 0,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(zero.tail, "1");
}

// ───────────────────────────── parse_time_spec ─────────────────────────────

#[test]
fn parse_time_spec_relative_forms() {
    let now = chrono::Utc::now().timestamp() as i32;
    // 1h ago — allow a small slack for test execution time.
    let one_h = parse_time_spec("1h").unwrap();
    assert!((now - 3600 - one_h).abs() <= 2, "1h ≈ now-3600");
    let thirty_m = parse_time_spec("30m").unwrap();
    assert!((now - 1800 - thirty_m).abs() <= 2);
    let two_d = parse_time_spec("2d").unwrap();
    assert!((now - 172_800 - two_d).abs() <= 2);
    let ten_s = parse_time_spec("10s").unwrap();
    assert!((now - 10 - ten_s).abs() <= 2);
}

#[test]
fn parse_time_spec_unix_timestamp() {
    assert_eq!(parse_time_spec("1700000000").unwrap(), 1_700_000_000);
}

#[test]
fn parse_time_spec_iso8601() {
    // 2021-01-01T00:00:00Z == 1609459200
    assert_eq!(
        parse_time_spec("2021-01-01T00:00:00Z").unwrap(),
        1_609_459_200
    );
}

#[test]
fn parse_time_spec_invalid_errors() {
    assert!(parse_time_spec("not-a-time").is_err());
    assert!(parse_time_spec("5x").is_err());
}

//! Unit tests for the compose discovery layer.
//!
//! All discovery runs through a programmable [`SshExecutor`] mock that returns
//! canned `find` / `docker compose ls` / `cat` output and counts invocations,
//! so no live SSH server is needed.

use super::*;
use crate::ssh::{CommandOutput, SshExecutor};
use crate::synapse::{HostConfig, HostProtocol};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ── programmable mock executor ──────────────────────────────────────────────

/// Mock that dispatches on the program name and counts every exec call.
struct MockExec {
    /// stdout for `find`.
    find_stdout: String,
    /// stdout for `docker compose ls --format json`.
    ls_stdout: String,
    /// map from file path (last cat arg) → file contents.
    cat: std::collections::HashMap<String, String>,
    calls: AtomicUsize,
    find_calls: AtomicUsize,
    ls_calls: AtomicUsize,
}

impl MockExec {
    fn new() -> Self {
        Self {
            find_stdout: String::new(),
            ls_stdout: String::new(),
            cat: std::collections::HashMap::new(),
            calls: AtomicUsize::new(0),
            find_calls: AtomicUsize::new(0),
            ls_calls: AtomicUsize::new(0),
        }
    }

    fn with_find(mut self, stdout: impl Into<String>) -> Self {
        self.find_stdout = stdout.into();
        self
    }

    fn with_ls(mut self, stdout: impl Into<String>) -> Self {
        self.ls_stdout = stdout.into();
        self
    }

    fn with_cat(mut self, path: impl Into<String>, contents: impl Into<String>) -> Self {
        self.cat.insert(path.into(), contents.into());
        self
    }

    fn total_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SshExecutor for MockExec {
    async fn exec(
        &self,
        _host: &HostConfig,
        program: &str,
        args: &[&str],
    ) -> anyhow::Result<CommandOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (stdout, exit_code) = match program {
            "find" => {
                self.find_calls.fetch_add(1, Ordering::SeqCst);
                (self.find_stdout.clone(), Some(0))
            }
            "docker" => {
                self.ls_calls.fetch_add(1, Ordering::SeqCst);
                (self.ls_stdout.clone(), Some(0))
            }
            "cat" => {
                let path = args.last().copied().unwrap_or("");
                match self.cat.get(path) {
                    Some(contents) => (contents.clone(), Some(0)),
                    None => (String::new(), Some(1)),
                }
            }
            other => panic!("unexpected program in mock: {other}"),
        };
        Ok(CommandOutput {
            stdout,
            stderr: String::new(),
            exit_code,
        })
    }
}

fn host_with_paths(paths: &[&str]) -> HostConfig {
    HostConfig {
        name: "tootie".into(),
        host: "tootie".into(),
        port: None,
        protocol: HostProtocol::Ssh,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: paths.iter().map(|s| s.to_string()).collect(),
        scout_read_roots: Vec::new(),
        exec_allowlist: Vec::new(),
    }
}

// ── pure-function tests ─────────────────────────────────────────────────────

#[test]
fn parse_service_count_sums_all_groups() {
    assert_eq!(parse_service_count("running(5)"), 5);
    assert_eq!(parse_service_count("running(2), exited(1)"), 3);
    assert_eq!(parse_service_count("running"), 0);
    assert_eq!(parse_service_count(""), 0);
}

#[test]
fn project_name_from_path_uses_parent_dir() {
    assert_eq!(
        project_name_from_path("/compose/myapp/docker-compose.yml"),
        "myapp"
    );
    assert_eq!(project_name_from_path("compose.yaml"), "");
}

#[test]
fn parse_compose_name_only_matches_top_level() {
    // Top-level name wins.
    assert_eq!(
        parse_compose_name("name: explicit\nservices:\n  web:\n    container_name: nested"),
        Some("explicit".to_string())
    );
    // Nested container_name must NOT match.
    assert_eq!(
        parse_compose_name("services:\n  web:\n    container_name: nope"),
        None
    );
    // Quoted value stripped; trailing comment dropped.
    assert_eq!(
        parse_compose_name("name: \"quoted\"  # comment"),
        Some("quoted".to_string())
    );
}

#[test]
fn validate_search_path_rejects_relative_and_traversal() {
    assert!(validate_search_path("/compose").is_ok());
    assert!(validate_search_path("relative/path").is_err());
    assert!(validate_search_path("/compose/../etc").is_err());
    assert!(validate_search_path("").is_err());
    assert!(validate_search_path("/compose/$(evil)").is_err());
}

#[test]
fn effective_search_paths_defaults_when_unset() {
    let host = host_with_paths(&[]);
    assert_eq!(
        effective_search_paths(&host),
        DEFAULT_COMPOSE_SEARCH_PATHS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn effective_search_paths_drops_invalid_overrides() {
    let host = host_with_paths(&["/valid", "../bad"]);
    assert_eq!(effective_search_paths(&host), vec!["/valid".to_string()]);
}

#[test]
fn parse_find_output_dedups() {
    let out = parse_find_output("/a/docker-compose.yml\n/a/docker-compose.yml\n/b/compose.yaml\n");
    assert_eq!(out, vec!["/a/docker-compose.yml", "/b/compose.yaml"]);
}

#[test]
fn parse_compose_ls_handles_empty_and_entries() {
    assert!(parse_compose_ls("").unwrap().is_empty());
    assert!(parse_compose_ls("   ").unwrap().is_empty());
    let json =
        r#"[{"Name":"app","Status":"running(2)","ConfigFiles":"/compose/app/docker-compose.yml"}]"#;
    let projects = parse_compose_ls(json).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "app");
    assert_eq!(projects[0].service_count, 2);
    assert_eq!(projects[0].discovered_from, DiscoveredFrom::DockerLs);
}

// ── discovery: filesystem scan finds nested compose files ───────────────────

#[tokio::test]
async fn discovery_parses_compose_files_in_nested_dirs() {
    // find returns two compose files of different recognized names in nested dirs.
    let mock = MockExec::new()
        .with_ls("") // no running projects
        .with_find("/compose/alpha/docker-compose.yml\n/mnt/cache/code/beta/compose.yaml\n")
        // alpha has an explicit top-level name; beta falls back to its dir name.
        .with_cat(
            "/compose/alpha/docker-compose.yml",
            "name: alpha-explicit\n",
        )
        .with_cat(
            "/mnt/cache/code/beta/compose.yaml",
            "services:\n  web: {}\n",
        );

    let disco = ComposeDiscovery::new(Arc::new(mock));
    let host = host_with_paths(&["/compose", "/mnt/cache/code"]);
    let projects = disco.list(&host).await.unwrap();

    let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"alpha-explicit"),
        "explicit name: {names:?}"
    );
    assert!(names.contains(&"beta"), "dir-name fallback: {names:?}");
    assert!(projects
        .iter()
        .all(|p| p.discovered_from == DiscoveredFrom::Scan));
}

// ── cache: second list() does not re-run discovery ──────────────────────────

#[tokio::test]
async fn cache_hit_avoids_rescanning() {
    let mock = Arc::new(
        MockExec::new()
            .with_ls("")
            .with_find("/compose/alpha/docker-compose.yml\n")
            .with_cat("/compose/alpha/docker-compose.yml", "name: alpha\n"),
    );
    let disco = ComposeDiscovery::new(mock.clone());
    let host = host_with_paths(&["/compose"]);

    let first = disco.list(&host).await.unwrap();
    assert_eq!(first.len(), 1);
    let after_first = mock.total_calls();
    assert!(after_first > 0, "first list should exec");

    // Second list must hit the cache: zero additional exec calls.
    let second = disco.list(&host).await.unwrap();
    assert_eq!(second, first);
    assert_eq!(
        mock.total_calls(),
        after_first,
        "cache hit must not re-run discovery"
    );
}

// ── refresh: invalidates and forces a re-walk ───────────────────────────────

#[tokio::test]
async fn refresh_forces_rescan() {
    let mock = Arc::new(
        MockExec::new()
            .with_ls("")
            .with_find("/compose/alpha/docker-compose.yml\n")
            .with_cat("/compose/alpha/docker-compose.yml", "name: alpha\n"),
    );
    let disco = ComposeDiscovery::new(mock.clone());
    let host = host_with_paths(&["/compose"]);

    disco.list(&host).await.unwrap();
    let after_first = mock.total_calls();

    // Refresh the host, then list again → exec count must increase.
    disco.refresh(Some(&host.name));
    disco.list(&host).await.unwrap();
    assert!(
        mock.total_calls() > after_first,
        "refresh must force a re-scan"
    );

    // refresh(None) also invalidates everything.
    let before_all = mock.total_calls();
    disco.refresh(None);
    disco.list(&host).await.unwrap();
    assert!(mock.total_calls() > before_all);
}

// ── merge: docker-ls + filesystem results combine, deduped by name ──────────

#[tokio::test]
async fn docker_ls_and_filesystem_results_merge() {
    // "shared" appears in BOTH sources (docker-ls wins, status preserved).
    // "fsonly" appears only in the filesystem scan (a stopped project).
    // "running-only" appears only in docker-ls.
    let ls_json = r#"[
        {"Name":"shared","Status":"running(3)","ConfigFiles":""},
        {"Name":"running-only","Status":"running(1)","ConfigFiles":"/compose/ro/docker-compose.yml"}
    ]"#;
    let mock = MockExec::new()
        .with_ls(ls_json)
        .with_find("/compose/shared/docker-compose.yml\n/compose/fsonly/compose.yaml\n")
        .with_cat("/compose/shared/docker-compose.yml", "services: {}\n")
        .with_cat("/compose/fsonly/compose.yaml", "services: {}\n");

    let mock = Arc::new(mock);
    let disco = ComposeDiscovery::new(mock.clone());
    let host = host_with_paths(&["/compose"]);
    let projects = disco.list(&host).await.unwrap();

    // Both discovery sources must have been queried exactly once — proves the
    // bead validation requirement that `list` returns projects from both the
    // filesystem scan AND active `docker compose ls`.
    assert_eq!(
        mock.find_calls.load(Ordering::SeqCst),
        1,
        "filesystem scanned"
    );
    assert_eq!(
        mock.ls_calls.load(Ordering::SeqCst),
        1,
        "docker compose ls run"
    );

    let by_name: std::collections::HashMap<&str, &ComposeProject> =
        projects.iter().map(|p| (p.name.as_str(), p)).collect();
    assert_eq!(projects.len(), 3, "merged: {:?}", projects);

    // docker-ls wins for "shared": status + service count from ls.
    let shared = by_name.get("shared").unwrap();
    assert_eq!(shared.discovered_from, DiscoveredFrom::DockerLs);
    assert_eq!(shared.service_count, 3);
    // ...and its empty ConfigFiles is backfilled from the filesystem scan.
    assert_eq!(
        shared.config_files,
        vec!["/compose/shared/docker-compose.yml".to_string()]
    );

    // filesystem-only stopped project survives.
    let fsonly = by_name.get("fsonly").unwrap();
    assert_eq!(fsonly.discovered_from, DiscoveredFrom::Scan);

    // docker-ls-only running project survives.
    assert!(by_name.contains_key("running-only"));
}

// ── empty/missing paths handled ─────────────────────────────────────────────

#[tokio::test]
async fn empty_find_and_ls_yield_empty_list() {
    // find prints nothing (all paths missing), ls prints nothing.
    let mock = MockExec::new().with_find("").with_ls("");
    let disco = ComposeDiscovery::new(Arc::new(mock));
    let host = host_with_paths(&["/nonexistent"]);
    let projects = disco.list(&host).await.unwrap();
    assert!(projects.is_empty());
}

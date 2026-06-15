//! Unit tests for `HostRepository` / `FileHostRepository`.
//!
//! All tests use explicit tempfile fixtures and `FileHostRepository::for_test` to
//! avoid reading process env or the real `~/.ssh/config`.

use super::*;

use std::io::Write as _;

use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write content to a fresh temp file and return the open file.
fn temp_json(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("create temp file");
    f.write_all(content.as_bytes()).expect("write temp file");
    f
}

/// Write content to a fresh temp file and return the open file.
fn temp_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("create temp file");
    f.write_all(content.as_bytes()).expect("write temp file");
    f
}

fn single_ssh_host_json(name: &str, host: &str) -> String {
    serde_json::to_string(&vec![HostConfig {
        name: name.into(),
        host: host.into(),
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
    }])
    .unwrap()
}

fn hosts_file_json(hosts: &[(&str, &str)]) -> String {
    let v: Vec<serde_json::Value> = hosts
        .iter()
        .map(|(name, host)| {
            serde_json::json!({
                "name": name,
                "host": host,
                "protocol": "ssh"
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({ "hosts": v })).unwrap()
}

// ---------------------------------------------------------------------------
// Env JSON source
// ---------------------------------------------------------------------------

#[test]
fn env_json_source_loads_correctly() {
    let json = single_ssh_host_json("myserver", "192.168.1.100");
    let repo = FileHostRepository::for_test(Some(json), Vec::new(), None);
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    // myserver from env + local fallback
    assert!(
        hosts.iter().any(|h| h.name == "myserver"),
        "myserver should be present"
    );
    assert!(
        hosts.iter().any(|h| h.name == "local"),
        "local fallback should be appended"
    );
}

#[test]
fn env_json_malformed_returns_error() {
    let repo = FileHostRepository::for_test(Some("not-valid-json".into()), Vec::new(), None);
    let result = repo.load_hosts();
    assert!(result.is_err(), "malformed JSON must return an error");
}

#[test]
fn env_json_empty_string_falls_through_to_files() {
    // Empty env → no hosts from env → load_from_files returns empty → local fallback
    let repo = FileHostRepository::for_test(Some(String::new()), Vec::new(), None);
    let hosts = repo.load_hosts().expect("should succeed");
    assert_eq!(hosts.len(), 1, "only local fallback expected");
    assert_eq!(hosts[0].name, "local");
}

// ---------------------------------------------------------------------------
// File source
// ---------------------------------------------------------------------------

#[test]
fn file_source_loads_correctly() {
    let f = temp_json(&hosts_file_json(&[("filehost", "10.0.0.1")]));
    let repo = FileHostRepository::for_test(None, vec![f.path().to_path_buf()], None);
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    assert!(hosts.iter().any(|h| h.name == "filehost"));
    assert!(hosts.iter().any(|h| h.name == "local"));
}

#[test]
fn file_source_malformed_json_returns_error() {
    let f = temp_json("{bad json}");
    let repo = FileHostRepository::for_test(None, vec![f.path().to_path_buf()], None);
    let result = repo.load_hosts();
    assert!(
        result.is_err(),
        "malformed JSON in file must return an error"
    );
}

#[test]
fn first_non_empty_file_wins() {
    let empty = temp_json(r#"{"hosts":[]}"#);
    let winner = temp_json(&hosts_file_json(&[("winner", "10.1.2.3")]));
    let repo = FileHostRepository::for_test(
        None,
        vec![empty.path().to_path_buf(), winner.path().to_path_buf()],
        None,
    );
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    // Empty file is skipped; winner is used
    assert!(hosts.iter().any(|h| h.name == "winner"));
    // Empty file's absence means no "empty" entry
    assert!(!hosts.iter().any(|h| h.name == "empty"));
}

#[test]
fn nonexistent_files_are_skipped_gracefully() {
    let repo = FileHostRepository::for_test(
        None,
        vec![PathBuf::from("/nonexistent/path/config.json")],
        None,
    );
    let hosts = repo
        .load_hosts()
        .expect("should succeed with missing files");
    assert_eq!(hosts.len(), 1, "only local fallback");
    assert_eq!(hosts[0].name, "local");
}

// ---------------------------------------------------------------------------
// SSH config auto-discovery
// ---------------------------------------------------------------------------

const BASIC_SSH_CONFIG: &str = r#"
Host server1
    HostName 192.168.1.10
    User alice
    IdentityFile ~/.ssh/id_ed25519

Host server2
    HostName 192.168.1.20
    Port 2222

Host local
    HostName localhost
"#;

#[test]
fn ssh_config_source_loads_correctly() {
    let f = temp_file(BASIC_SSH_CONFIG);
    let repo = FileHostRepository::for_test(None, Vec::new(), Some(f.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");

    // All 3 SSH hosts should be present
    assert!(
        hosts.iter().any(|h| h.name == "server1"),
        "server1 missing from hosts: {hosts:?}"
    );
    assert!(hosts.iter().any(|h| h.name == "server2"), "server2 missing");
    assert!(hosts.iter().any(|h| h.name == "local"), "local missing");
}

#[test]
fn ssh_config_user_and_key_are_mapped() {
    let f = temp_file(BASIC_SSH_CONFIG);
    let repo = FileHostRepository::for_test(None, Vec::new(), Some(f.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    let server1 = hosts.iter().find(|h| h.name == "server1").unwrap();
    assert_eq!(server1.ssh_user.as_deref(), Some("alice"));
    assert!(server1.ssh_key_path.is_some(), "ssh_key_path should be set");
    assert!(
        server1.ssh_config_path.is_some(),
        "ssh_config_path should preserve alias semantics"
    );
    assert_eq!(server1.protocol, HostProtocol::Ssh);
    assert_eq!(server1.host, "192.168.1.10");
}

#[test]
fn ssh_config_port_is_mapped() {
    let f = temp_file(BASIC_SSH_CONFIG);
    let repo = FileHostRepository::for_test(None, Vec::new(), Some(f.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    let server2 = hosts.iter().find(|h| h.name == "server2").unwrap();
    assert_eq!(server2.port, Some(2222));
}

#[test]
fn ssh_config_missing_file_returns_empty_not_error() {
    let repo = FileHostRepository::for_test(
        None,
        Vec::new(),
        Some(PathBuf::from("/nonexistent/.ssh/config")),
    );
    let hosts = repo
        .load_hosts()
        .expect("missing SSH config should not error");
    assert_eq!(hosts.len(), 1, "only local fallback");
    assert_eq!(hosts[0].name, "local");
}

#[test]
fn ssh_config_wildcard_host_star_is_skipped() {
    let config = r#"
Host *
    ServerAliveInterval 60
    User default_user

Host realhost
    HostName 10.0.0.1
"#;
    let f = temp_file(config);
    let repo = FileHostRepository::for_test(None, Vec::new(), Some(f.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    // `Host *` should not produce a host entry
    assert!(
        !hosts.iter().any(|h| h.name == "*"),
        "wildcard * should be skipped"
    );
    assert!(hosts.iter().any(|h| h.name == "realhost"));
}

#[test]
fn ssh_config_pattern_hosts_are_skipped() {
    let config = r#"
Host *.example.com
    User admin

Host concrete
    HostName 10.1.2.3
"#;
    let f = temp_file(config);
    let repo = FileHostRepository::for_test(None, Vec::new(), Some(f.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    assert!(!hosts.iter().any(|h| h.name.contains('*')));
    assert!(hosts.iter().any(|h| h.name == "concrete"));
}

#[test]
fn ssh_config_known_service_hosts_are_skipped() {
    let config = r#"
Host github.com
    User git
    IdentityFile ~/.ssh/id_rsa

Host myserver
    HostName 10.0.0.5
"#;
    let f = temp_file(config);
    let repo = FileHostRepository::for_test(None, Vec::new(), Some(f.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    assert!(!hosts.iter().any(|h| h.name == "github.com"));
    assert!(hosts.iter().any(|h| h.name == "myserver"));
}

// ---------------------------------------------------------------------------
// Precedence merging
// ---------------------------------------------------------------------------

#[test]
fn explicit_config_overrides_ssh_config_for_same_host() {
    // SSH config has "server1" at 192.168.1.10; explicit config has "server1" at 10.99.99.99
    let ssh_config = r#"
Host server1
    HostName 192.168.1.10
    User ssh_user
"#;
    let explicit_json = hosts_file_json(&[("server1", "10.99.99.99")]);

    let ssh_file = temp_file(ssh_config);
    let explicit_file = temp_json(&explicit_json);

    let repo = FileHostRepository::for_test(
        None,
        vec![explicit_file.path().to_path_buf()],
        Some(ssh_file.path().to_path_buf()),
    );
    let hosts = repo.load_hosts().expect("load_hosts should succeed");

    // Exactly one "server1" entry
    let server1_entries: Vec<_> = hosts.iter().filter(|h| h.name == "server1").collect();
    assert_eq!(server1_entries.len(), 1, "only one server1 should exist");
    // Explicit takes priority
    assert_eq!(server1_entries[0].host, "10.99.99.99");
}

#[test]
fn fixture_three_ssh_hosts_one_explicit_override_yields_three_hosts() {
    // SSH config: local (localhost), alpha (10.0.0.1), beta (10.0.0.2)
    // Explicit: local with port 8080 (overrides SSH)
    // Result: 3 hosts — local (from explicit), alpha, beta
    let ssh_config = r#"
Host local
    HostName localhost

Host alpha
    HostName 10.0.0.1

Host beta
    HostName 10.0.0.2
"#;
    let explicit_json =
        r#"{"hosts":[{"name":"local","host":"localhost","port":8080,"protocol":"ssh"}]}"#;

    let ssh_file = temp_file(ssh_config);
    let explicit_file = temp_json(explicit_json);

    let repo = FileHostRepository::for_test(
        None,
        vec![explicit_file.path().to_path_buf()],
        Some(ssh_file.path().to_path_buf()),
    );
    let hosts = repo.load_hosts().expect("load_hosts should succeed");

    // Exactly 3 hosts
    assert_eq!(hosts.len(), 3, "should have exactly 3 hosts: {hosts:?}");
    // local comes from explicit (port 8080)
    let local = hosts.iter().find(|h| h.name == "local").unwrap();
    assert_eq!(local.port, Some(8080), "local should have explicit port");
    // alpha and beta from SSH
    assert!(hosts.iter().any(|h| h.name == "alpha"));
    assert!(hosts.iter().any(|h| h.name == "beta"));
}

// ---------------------------------------------------------------------------
// Fallback / ensure-local
// ---------------------------------------------------------------------------

#[test]
fn fallback_local_is_appended_when_no_sources() {
    let repo = FileHostRepository::for_test(None, Vec::new(), None);
    let hosts = repo.load_hosts().expect("should succeed");
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].name, "local");
}

#[test]
fn local_is_not_duplicated_when_explicit_config_has_local() {
    let json = r#"{"hosts":[{"name":"local","host":"localhost","protocol":"local"}]}"#;
    let f = temp_json(json);
    let repo = FileHostRepository::for_test(None, vec![f.path().to_path_buf()], None);
    let hosts = repo.load_hosts().expect("load_hosts should succeed");
    let local_count = hosts.iter().filter(|h| h.name == "local").count();
    assert_eq!(local_count, 1, "local should not be duplicated");
}

#[test]
fn env_source_takes_priority_over_file_source() {
    let env_json = single_ssh_host_json("fromenv", "1.2.3.4");
    let file_json = hosts_file_json(&[("fromfile", "5.6.7.8")]);
    let f = temp_json(&file_json);

    let repo = FileHostRepository::for_test(Some(env_json), vec![f.path().to_path_buf()], None);
    let hosts = repo.load_hosts().expect("load_hosts should succeed");

    assert!(
        hosts.iter().any(|h| h.name == "fromenv"),
        "env source should win"
    );
    assert!(
        !hosts.iter().any(|h| h.name == "fromfile"),
        "file source should be suppressed"
    );
}

// ---------------------------------------------------------------------------
// merge_hosts / ensure_local unit tests
// ---------------------------------------------------------------------------

#[test]
fn merge_hosts_explicit_wins_on_conflict() {
    let explicit = vec![HostConfig {
        name: "server".into(),
        host: "explicit.host".into(),
        port: None,
        protocol: HostProtocol::Http,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: None,
        ssh_config_path: None,
        docker_socket_path: None,
        tags: Vec::new(),
        compose_search_paths: Vec::new(),
        scout_read_roots: Vec::new(),
        exec_allowlist: Vec::new(),
    }];
    let ssh = vec![HostConfig {
        name: "server".into(),
        host: "ssh.host".into(),
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
    }];
    let merged = merge_hosts(explicit, ssh);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].host, "explicit.host");
}

#[test]
fn merge_hosts_ssh_only_hosts_are_added() {
    let merged = merge_hosts(
        Vec::new(),
        vec![HostConfig {
            name: "sshonly".into(),
            host: "10.0.0.1".into(),
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
        }],
    );
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].name, "sshonly");
}

#[test]
fn ensure_local_appends_when_absent() {
    let hosts = vec![HostConfig {
        name: "other".into(),
        host: "10.0.0.1".into(),
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
    }];
    let result = ensure_local(hosts);
    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|h| h.name == "local"));
}

#[test]
fn ensure_local_does_not_append_when_present() {
    let hosts = vec![HostConfig::local()];
    let result = ensure_local(hosts);
    assert_eq!(result.len(), 1);
}

// ---------------------------------------------------------------------------
// Include directive behavior (verified: natively supported in ssh2-config 0.7.1)
// ---------------------------------------------------------------------------

/// Empirically verify that ssh2-config 0.7.1 DOES expand Include directives natively.
///
/// The bead spec FACT (2026-05-25) stated Include was NOT handled — that was true for
/// older crate versions. Empirical testing against 0.7.1 confirms it IS expanded
/// (via the `glob` dependency in parser.rs). The module doc in `host_config.rs`
/// reflects the verified behaviour.
///
/// If this test starts failing on a future downgrade, Include support was removed.
/// Update the module doc accordingly.
#[test]
fn ssh_config_include_directives_are_expanded() {
    use std::io::Write as IoWrite;
    use tempfile::NamedTempFile;

    // Write the file that would be Included
    let mut included = NamedTempFile::new().expect("create temp file");
    included
        .write_all(b"Host included_host\n    HostName 10.5.5.5\n")
        .expect("write included file");

    // Write main config referencing the included file
    let mut main_config = NamedTempFile::new().expect("create temp file");
    let content = format!(
        "Include {}\n\nHost mainhost\n    HostName 10.0.0.1\n",
        included.path().display()
    );
    main_config
        .write_all(content.as_bytes())
        .expect("write main config");

    let repo =
        FileHostRepository::for_test(None, Vec::new(), Some(main_config.path().to_path_buf()));
    let hosts = repo.load_hosts().expect("load_hosts should succeed");

    // mainhost should be present (directly in the main config)
    assert!(
        hosts.iter().any(|h| h.name == "mainhost"),
        "mainhost should be present: {hosts:?}"
    );

    // included_host SHOULD be present: ssh2-config 0.7.1 natively expands Include.
    // If this assertion fails, a crate version change removed Include support.
    assert!(
        hosts.iter().any(|h| h.name == "included_host"),
        "included_host missing — ssh2-config may have lost native Include support: {hosts:?}"
    );
}

// ---------------------------------------------------------------------------
// Protocol validation — Http/Https rejected at load time (A-H3 / S-M6)
// ---------------------------------------------------------------------------

#[test]
fn http_protocol_host_is_rejected_at_load() {
    let json = serde_json::to_string(&serde_json::json!([{
        "name": "badhost",
        "host": "10.0.0.1",
        "protocol": "http"
    }]))
    .unwrap();
    let repo = FileHostRepository::for_test(Some(json), Vec::new(), None);
    let result = repo.load_hosts();
    assert!(
        result.is_err(),
        "http protocol must be rejected at load time"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("http") && msg.contains("not supported"),
        "error should mention 'http' and 'not supported', got: {msg}"
    );
}

#[test]
fn https_protocol_host_is_rejected_at_load() {
    let json = serde_json::to_string(&serde_json::json!([{
        "name": "badhost",
        "host": "10.0.0.1",
        "protocol": "https"
    }]))
    .unwrap();
    let repo = FileHostRepository::for_test(Some(json), Vec::new(), None);
    let result = repo.load_hosts();
    assert!(
        result.is_err(),
        "https protocol must be rejected at load time"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("https") && msg.contains("not supported"),
        "error should mention 'https' and 'not supported', got: {msg}"
    );
}

#[test]
fn reject_unsupported_protocol_allows_local_and_ssh() {
    let local = HostConfig::local();
    assert!(
        reject_unsupported_protocol(&local).is_ok(),
        "local protocol should be accepted"
    );
    let ssh_host = HostConfig {
        name: "sshbox".into(),
        host: "10.0.0.2".into(),
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
    };
    assert!(
        reject_unsupported_protocol(&ssh_host).is_ok(),
        "ssh protocol should be accepted"
    );
}

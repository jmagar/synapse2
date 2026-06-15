use super::DoctorCheck;
use super::checks::check_port_available;

#[test]
fn check_port_available_passes_for_unused_high_port() {
    let result = check_port_available("127.0.0.1", 59999);
    assert!(result.ok, "unused high port should be available");
}

#[test]
fn doctor_check_pass_sets_value_without_hint() {
    let check = DoctorCheck::pass("config", "Config file", "/tmp/config.toml");

    assert_eq!(check.category, "config");
    assert_eq!(check.name, "Config file");
    assert!(check.ok);
    assert_eq!(check.value.as_deref(), Some("/tmp/config.toml"));
    assert_eq!(check.hint, None);
    assert_eq!(check.latency_ms, None);
}

#[test]
fn doctor_check_fail_sets_hint_without_value() {
    let check = DoctorCheck::fail("auth", "Token", "set SYNAPSE_MCP_TOKEN");

    assert_eq!(check.category, "auth");
    assert_eq!(check.name, "Token");
    assert!(!check.ok);
    assert_eq!(check.value, None);
    assert_eq!(check.hint.as_deref(), Some("set SYNAPSE_MCP_TOKEN"));
    assert_eq!(check.latency_ms, None);
}

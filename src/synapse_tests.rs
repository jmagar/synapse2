use super::{validate_command, validate_safe_path};
use std::fs;

#[test]
fn guardrails_accept_safe_paths_and_commands() {
    assert!(validate_safe_path("/tmp/logs/app.log").is_ok());
    assert!(validate_safe_path("/tmp/a_b-c/01.log").is_ok());
    assert!(validate_command("ls", &[]).is_ok());
    assert!(validate_command("custom-read", &["custom-read".into()]).is_ok());
}

#[test]
fn guardrails_reject_traversal_metacharacters_and_denied_commands() {
    assert!(validate_safe_path("../secret").is_err());
    assert!(validate_safe_path("/tmp/a;rm-rf").is_err());
    assert!(validate_command("rm", &[]).is_err());
    assert!(validate_command("python", &["python".into()]).is_err());
}

// SECURITY FIX: Absolute path requirement
#[test]
fn validate_safe_path_requires_absolute_path() {
    // Relative paths must be rejected
    let result = validate_safe_path("./foo");
    assert!(
        result.is_err(),
        "relative path starting with ./ should be rejected"
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("absolute path required"));

    let result = validate_safe_path("foo/bar");
    assert!(
        result.is_err(),
        "relative path without leading / should be rejected"
    );

    // Absolute path should pass basic checks (until other validations run)
    assert!(validate_safe_path("/absolute/path").is_ok());
}

// SECURITY FIX: Symlink rejection
#[test]
fn validate_safe_path_rejects_symlinks() {
    // Create a temporary directory and symlink for testing
    let tmpdir = tempfile::tempdir().expect("create tempdir");
    let tmpdir_path = tmpdir.path();

    // Create a real target file
    let target_file = tmpdir_path.join("target.txt");
    fs::write(&target_file, "test content").expect("write target file");

    // Create a symlink pointing to the target
    let symlink_path = tmpdir_path.join("symlink.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &symlink_path).expect("create symlink");

    #[cfg(not(unix))]
    std::os::windows::fs::symlink_file(&target_file, &symlink_path).expect("create symlink");

    // Validate the symlink — should be rejected
    let result = validate_safe_path(&symlink_path.to_string_lossy());
    assert!(
        result.is_err(),
        "symlinked path should be rejected — got {result:?}"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("symlinks not permitted"),
        "error message should mention symlinks"
    );

    // Validate the real file — should pass symlink check
    let result = validate_safe_path(&target_file.to_string_lossy());
    assert!(
        result.is_ok(),
        "real file should pass symlink check — got {result:?}"
    );
}

// SECURITY FIX: git removed from ALLOWED_READ_COMMANDS
#[test]
fn validate_command_rejects_git() {
    let result = validate_command("git", &[]);
    assert!(
        result.is_err(),
        "git should not be in ALLOWED_READ_COMMANDS — got {result:?}"
    );
    assert!(result.unwrap_err().to_string().contains("not allowlisted"));
}

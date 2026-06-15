use anyhow::Result;
use serde_json::{Value, json};
use std::process::Command;

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;

pub async fn docker_json(args: &[&str]) -> Result<Value> {
    let output = Command::new("docker").args(args).output();
    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            Ok(json!({
                "available": true,
                "command": format!("docker {}", args.join(" ")),
                "stdout": stdout,
            }))
        }
        Ok(output) => Ok(json!({
            "available": false,
            "command": format!("docker {}", args.join(" ")),
            "exit_code": output.status.code(),
            "stderr": String::from_utf8_lossy(&output.stderr).trim(),
        })),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(json!({
            "available": false,
            "command": format!("docker {}", args.join(" ")),
            "error": "docker command not found",
        })),
        Err(err) => Err(err.into()),
    }
}

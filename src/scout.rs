use anyhow::Result;
use serde_json::{Value, json};

use crate::host_config::HostRepository;
use crate::synapse::HostConfig;

#[cfg(test)]
#[path = "scout_tests.rs"]
mod tests;

pub fn nodes(repo: &dyn HostRepository) -> Result<Value> {
    let hosts = repo.load_hosts()?;
    Ok(json!({ "hosts": hosts }))
}

pub fn resolve_host(repo: &dyn HostRepository, name: &str) -> Result<HostConfig> {
    repo.load_hosts()?
        .into_iter()
        .find(|host| host.name == name)
        .ok_or_else(|| anyhow::anyhow!("unknown host: {name}"))
}

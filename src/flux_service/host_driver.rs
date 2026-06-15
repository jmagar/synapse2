//! `FluxService` driver methods for host inspection ops (B11).
//!
//! This module holds `impl FluxService` blocks that drive host resolution, exec
//! seam routing (local vs. SSH), and fanout for host operations. The per-host
//! pure functions live in the `host` sibling module.

use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;

use super::{
    FluxService,
    container_read::{self, ListFilters},
    docker, flatten_scalar_outcome,
    host::{
        self, CheckResult, CheckStatus, LocalExec, RemoteExec, is_local_host, mounts_on_host,
        network_on_host, resources_on_host, services_on_host, uptime_on_host,
    },
};
use crate::fanout::fanout;
use crate::scout;

#[cfg(test)]
#[path = "host_driver_tests.rs"]
mod tests;

impl FluxService {
    /// Quick connectivity probe: docker info + container count (+ failed services
    /// when systemd is available) per target host(s). Fans out when `host` is None.
    pub async fn host_status(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let clients = Arc::clone(&self.docker_clients);
        let ssh = Arc::clone(&self.ssh_pool);
        let outcome = fanout(&hosts, move |h| {
            let clients = Arc::clone(&clients);
            let ssh = Arc::clone(&ssh);
            let is_local = is_local_host(&h);
            async move {
                let client = clients.client_for(&h).await.map_err(|e| e.to_string())?;
                let info_val = docker::info_on_host(client.as_ref(), &h.name)
                    .await
                    .map_err(|e| e.to_string())?;
                let containers =
                    container_read::list_on_host(client.as_ref(), &h.name, &ListFilters::default())
                        .await
                        .map_err(|e| e.to_string())?;
                let running = containers
                    .iter()
                    .filter(|c| c.get("state").and_then(Value::as_str) == Some("running"))
                    .count();
                // docker_info returns { "host": "...", "info": { "ServerVersion": "...", ... } }
                let docker_version = info_val
                    .get("info")
                    .and_then(|i| i.get("ServerVersion"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);

                // Best-effort: failed service count via systemctl
                let failed_service_count: usize = {
                    let exec: Box<dyn host::HostExec> = if is_local {
                        Box::new(LocalExec)
                    } else {
                        Box::new(RemoteExec {
                            executor: ssh.as_ref(),
                            host: &h,
                        })
                    };
                    match exec.run("systemctl", &["--failed", "--no-legend"]).await {
                        Ok(out) => out
                            .stdout
                            .lines()
                            .filter(|l| {
                                let t = l.trim();
                                !t.is_empty()
                                    && t.split_whitespace().any(|tok| tok.ends_with(".service"))
                            })
                            .count(),
                        Err(_) => 0,
                    }
                };

                Ok(json!({
                    "name": h.name,
                    "connected": true,
                    "containerCount": containers.len(),
                    "runningCount": running,
                    "failedServiceCount": failed_service_count,
                    "dockerVersion": docker_version,
                }))
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "status"))
    }

    /// `uname -a` output for target host(s). Fans out when `host` is None.
    pub async fn host_info(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let ssh = Arc::clone(&self.ssh_pool);
        let outcome = fanout(&hosts, move |h| {
            let ssh = Arc::clone(&ssh);
            async move {
                let exec: Box<dyn host::HostExec> = if is_local_host(&h) {
                    Box::new(LocalExec)
                } else {
                    Box::new(RemoteExec {
                        executor: ssh.as_ref(),
                        host: &h,
                    })
                };
                host::info_on_host(exec.as_ref(), &h.name)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "info"))
    }

    /// `uptime` output for target host(s). Fans out when `host` is None.
    pub async fn host_uptime(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let ssh = Arc::clone(&self.ssh_pool);
        let outcome = fanout(&hosts, move |h| {
            let ssh = Arc::clone(&ssh);
            async move {
                let exec: Box<dyn host::HostExec> = if is_local_host(&h) {
                    Box::new(LocalExec)
                } else {
                    Box::new(RemoteExec {
                        executor: ssh.as_ref(),
                        host: &h,
                    })
                };
                uptime_on_host(exec.as_ref(), &h.name)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "uptime"))
    }

    /// CPU/memory/disk metrics for target host(s). Fans out when `host` is None.
    pub async fn host_resources(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let ssh = Arc::clone(&self.ssh_pool);
        let outcome = fanout(&hosts, move |h| {
            let ssh = Arc::clone(&ssh);
            async move {
                let exec: Box<dyn host::HostExec> = if is_local_host(&h) {
                    Box::new(LocalExec)
                } else {
                    Box::new(RemoteExec {
                        executor: ssh.as_ref(),
                        host: &h,
                    })
                };
                resources_on_host(exec.as_ref(), &h.name)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "resources"))
    }

    /// Systemd service list for a single host (single-host only — service filter
    /// context doesn't fan out meaningfully).
    pub async fn host_services(
        &self,
        host: &str,
        state: Option<&str>,
        service: Option<&str>,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        let exec = self.exec_for_host(&h);
        services_on_host(exec.as_ref(), &h.name, state, service).await
    }

    /// Network interface info for target host(s). Fans out when `host` is None.
    pub async fn host_network(&self, host: Option<&str>) -> Result<Value> {
        let hosts = self.target_hosts(host)?;
        let ssh = Arc::clone(&self.ssh_pool);
        let outcome = fanout(&hosts, move |h| {
            let ssh = Arc::clone(&ssh);
            async move {
                let exec: Box<dyn host::HostExec> = if is_local_host(&h) {
                    Box::new(LocalExec)
                } else {
                    Box::new(RemoteExec {
                        executor: ssh.as_ref(),
                        host: &h,
                    })
                };
                network_on_host(exec.as_ref(), &h.name)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await;
        Ok(flatten_scalar_outcome(outcome, "network"))
    }

    /// Mounted filesystem info via `df -h` for a single host.
    pub async fn host_mounts(&self, host: &str) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        let exec = self.exec_for_host(&h);
        mounts_on_host(exec.as_ref(), &h.name).await
    }

    /// Container port mappings for a single host, with optional protocol filter.
    pub async fn host_ports(
        &self,
        host: &str,
        protocol: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        let client = self.docker_clients.client_for(&h).await?;
        let containers =
            container_read::list_on_host(client.as_ref(), &h.name, &ListFilters::default()).await?;

        let mut ports: Vec<Value> = containers
            .iter()
            .flat_map(|c| {
                let container_name = c.get("name").and_then(Value::as_str).unwrap_or("");
                let image = c.get("image").and_then(Value::as_str).unwrap_or("");
                let state = c.get("state").and_then(Value::as_str).unwrap_or("");
                c.get("ports")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .map(|p| {
                                json!({
                                    "container": container_name,
                                    "image": image,
                                    "state": state,
                                    "hostPort": p.get("host_port"),
                                    "containerPort": p.get("container_port"),
                                    "protocol": p.get("protocol"),
                                    "hostIp": p.get("host_ip"),
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect();

        if let Some(proto) = protocol {
            ports.retain(|p| {
                p.get("protocol")
                    .and_then(Value::as_str)
                    .map(|pr| pr.eq_ignore_ascii_case(proto))
                    .unwrap_or(false)
            });
        }

        let total = ports.len();
        let off = offset.unwrap_or(0);
        let lim = limit.unwrap_or(usize::MAX);
        let paginated: Vec<Value> = ports.into_iter().skip(off).take(lim).collect();
        let has_more = off + lim < total;

        Ok(json!({
            "host": h.name,
            "ports": paginated,
            "total": total,
            "offset": off,
            "hasMore": has_more,
        }))
    }

    /// Run health checks for a single host, aggregating results from all
    /// specified `checks`. `docker` and `containers` use bollard; the rest use
    /// the exec seam. Fans out only for `checks` that need SSH.
    pub async fn host_doctor(&self, host: &str, checks: Vec<String>) -> Result<Value> {
        let h = scout::resolve_host(self.host_repo.as_ref(), host)?;
        let client = self.docker_clients.client_for(&h).await;
        let exec = self.exec_for_host(&h);

        // Partition: bollard checks first, exec-based checks deferred.
        let mut pre_results: Vec<CheckResult> = Vec::new();
        let mut exec_checks: Vec<String> = Vec::new();

        for check in &checks {
            match check.as_str() {
                "docker" => {
                    let result = match &client {
                        Ok(c) => match docker::info_on_host(c.as_ref(), &h.name).await {
                            Ok(info_val) => {
                                let ver = info_val
                                    .get("info")
                                    .and_then(|i| i.get("ServerVersion"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("?");
                                let api = info_val
                                    .get("info")
                                    .and_then(|i| i.get("ApiVersion"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("?");
                                CheckResult {
                                    check: "docker".into(),
                                    status: CheckStatus::Pass,
                                    detail: format!("Docker {ver} — API {api}"),
                                }
                            }
                            Err(e) => CheckResult {
                                check: "docker".into(),
                                status: CheckStatus::Fail,
                                detail: e.to_string(),
                            },
                        },
                        Err(e) => CheckResult {
                            check: "docker".into(),
                            status: CheckStatus::Fail,
                            detail: e.to_string(),
                        },
                    };
                    pre_results.push(result);
                }
                "containers" => {
                    let result = match &client {
                        Ok(c) => {
                            match container_read::list_on_host(
                                c.as_ref(),
                                &h.name,
                                &ListFilters::default(),
                            )
                            .await
                            {
                                Ok(ctrs) => {
                                    let running = ctrs
                                        .iter()
                                        .filter(|c| {
                                            c.get("state").and_then(Value::as_str)
                                                == Some("running")
                                        })
                                        .count();
                                    CheckResult {
                                        check: "containers".into(),
                                        status: CheckStatus::Pass,
                                        detail: format!("{running} running / {} total", ctrs.len()),
                                    }
                                }
                                Err(e) => CheckResult {
                                    check: "containers".into(),
                                    status: CheckStatus::Fail,
                                    detail: e.to_string(),
                                },
                            }
                        }
                        Err(e) => CheckResult {
                            check: "containers".into(),
                            status: CheckStatus::Fail,
                            detail: e.to_string(),
                        },
                    };
                    pre_results.push(result);
                }
                _ => exec_checks.push(check.clone()),
            }
        }

        Ok(host::doctor_on_host(exec.as_ref(), &h.name, &exec_checks, pre_results).await)
    }
}

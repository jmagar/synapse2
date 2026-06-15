use crate::{actions::HostArgs, app::SynapseService};
use anyhow::{Result, anyhow};
use serde_json::Value;

pub(in crate::cli) async fn run_host(args: &HostArgs, service: &SynapseService) -> Result<Value> {
    use crate::flux_service::host::DEFAULT_DOCTOR_CHECKS;
    let flux = service.flux();
    let host = args.host.as_deref();
    let result = match args.subaction.as_str() {
        "status" => flux.host_status(host).await?,
        "info" => flux.host_info(host).await?,
        "uptime" => flux.host_uptime(host).await?,
        "resources" => flux.host_resources(host).await?,
        "services" => {
            let h = host.ok_or_else(|| anyhow!("host services requires --host"))?;
            flux.host_services(h, args.state.as_deref(), args.service.as_deref())
                .await?
        }
        "network" => flux.host_network(host).await?,
        "mounts" => {
            let h = host.ok_or_else(|| anyhow!("host mounts requires --host"))?;
            flux.host_mounts(h).await?
        }
        "ports" => {
            let h = host.ok_or_else(|| anyhow!("host ports requires --host"))?;
            flux.host_ports(
                h,
                args.protocol.as_deref(),
                args.limit.map(|v| v as usize),
                args.offset.map(|v| v as usize),
            )
            .await?
        }
        "doctor" => {
            let h = host.ok_or_else(|| anyhow!("host doctor requires --host"))?;
            let checks: Vec<String> = match &args.checks {
                Some(s) if !s.is_empty() => s.split(',').map(|c| c.trim().to_owned()).collect(),
                _ => DEFAULT_DOCTOR_CHECKS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            };
            flux.host_doctor(h, checks).await?
        }
        other => return Err(anyhow!("unknown flux host subaction `{other}`")),
    };
    Ok(result)
}

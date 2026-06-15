//! Shared MCP + CLI dispatch hub (`execute_service_action`) and error-type
//! helpers (`is_validation_error`, `is_confirmation_denied`).
//!
//! All items here are re-exported from the parent [`crate::actions`] module so
//! call sites need no changes.

use std::borrow::Cow;

use anyhow::Result;
use serde_json::Value;

use crate::app::SynapseService;

use super::SynapseAction;
use super::flux::{
    dispatch_flux_compose, dispatch_flux_container, dispatch_flux_docker, dispatch_flux_host,
};
use crate::actions::scout::{dispatch_scout_logs, dispatch_scout_zfs};

/// Single dispatch hub used by both the MCP tool shim and the REST/CLI layer.
///
/// Each arm is thin: it delegates to the appropriate service method or a
/// `dispatch_flux_*` helper. Scout arms stay inline because they map 1-to-1
/// to service calls without needing separate helpers.
pub async fn execute_service_action(
    service: &SynapseService,
    action: &SynapseAction,
    confirmer: &dyn crate::elicitation_gate::Confirmer,
) -> Result<Value> {
    let label = action_timeout_label(action);
    let value = crate::runtime_budget::with_operation_deadline(
        label.as_ref(),
        execute_service_action_inner(service, action, confirmer),
    )
    .await?;
    Ok(crate::runtime_budget::cap_service_value(value))
}

async fn execute_service_action_inner(
    service: &SynapseService,
    action: &SynapseAction,
    confirmer: &dyn crate::elicitation_gate::Confirmer,
) -> Result<Value> {
    match action {
        SynapseAction::FluxHelp { topic, format } => {
            service
                .flux()
                .help(topic.as_deref(), format.as_deref())
                .await
        }
        SynapseAction::FluxDocker(args) => dispatch_flux_docker(service, args, confirmer).await,
        SynapseAction::FluxContainer(args) => {
            dispatch_flux_container(service, args, confirmer).await
        }
        SynapseAction::FluxHost(args) => dispatch_flux_host(service, args).await,
        SynapseAction::FluxCompose(args) => dispatch_flux_compose(service, args, confirmer).await,
        SynapseAction::ScoutHelp { topic, format } => {
            service
                .scout()
                .help(topic.as_deref(), format.as_deref())
                .await
        }
        SynapseAction::ScoutNodes => service.scout().nodes().await,
        SynapseAction::ScoutPeek {
            host,
            path,
            tree,
            depth,
        } => service.scout().peek(host, path, *tree, *depth).await,
        SynapseAction::ScoutFind(a) => {
            service
                .scout()
                .find(&a.host, &a.path, &a.pattern, a.depth, a.limit)
                .await
        }
        SynapseAction::ScoutPs(a) => {
            service
                .scout()
                .ps(
                    &a.host,
                    a.sort.as_deref(),
                    a.grep.as_deref(),
                    a.user.as_deref(),
                    a.limit,
                )
                .await
        }
        SynapseAction::ScoutDf { host, path } => service.scout().df(host, path.as_deref()).await,
        SynapseAction::ScoutDelta(a) => {
            service
                .scout()
                .delta(
                    &a.source_host,
                    &a.source_path,
                    a.target_host.as_deref(),
                    a.target_path.as_deref(),
                    a.content.as_deref(),
                )
                .await
        }
        SynapseAction::ScoutExec(a) => {
            service
                .scout()
                .exec(&a.host, a.path.as_deref(), &a.command, &a.args, confirmer)
                .await
        }
        SynapseAction::ScoutEmit(a) => {
            let targets = service.scout().resolve_emit_targets(
                &a.targets
                    .iter()
                    .map(|t| (t.host.clone(), t.path.clone()))
                    .collect::<Vec<_>>(),
            )?;
            service
                .scout()
                .emit(&targets, &a.command, &a.args, a.timeout_secs, confirmer)
                .await
        }
        SynapseAction::ScoutBeam(a) => {
            service
                .scout()
                .beam(
                    &a.source_host,
                    &a.source_path,
                    &a.dest_host,
                    &a.dest_path,
                    confirmer,
                )
                .await
        }
        SynapseAction::ScoutZfs(a) => dispatch_scout_zfs(service, a).await,
        SynapseAction::ScoutLogs(a) => dispatch_scout_logs(service, a).await,
    }
}

fn action_timeout_label(action: &SynapseAction) -> Cow<'static, str> {
    match action {
        SynapseAction::FluxHelp { .. } => Cow::Borrowed("flux help"),
        SynapseAction::FluxDocker(args) => Cow::Owned(format!("flux docker {}", args.subaction)),
        SynapseAction::FluxContainer(args) => {
            Cow::Owned(format!("flux container {}", args.subaction))
        }
        SynapseAction::FluxHost(args) => Cow::Owned(format!("flux host {}", args.subaction)),
        SynapseAction::FluxCompose(args) => Cow::Owned(format!("flux compose {}", args.subaction)),
        SynapseAction::ScoutHelp { .. } => Cow::Borrowed("scout help"),
        SynapseAction::ScoutNodes => Cow::Borrowed("scout nodes"),
        SynapseAction::ScoutPeek { .. } => Cow::Borrowed("scout peek"),
        SynapseAction::ScoutFind(_) => Cow::Borrowed("scout find"),
        SynapseAction::ScoutPs(_) => Cow::Borrowed("scout ps"),
        SynapseAction::ScoutDf { .. } => Cow::Borrowed("scout df"),
        SynapseAction::ScoutDelta(_) => Cow::Borrowed("scout delta"),
        SynapseAction::ScoutExec(_) => Cow::Borrowed("scout exec"),
        SynapseAction::ScoutEmit(_) => Cow::Borrowed("scout emit"),
        SynapseAction::ScoutBeam(_) => Cow::Borrowed("scout beam"),
        SynapseAction::ScoutZfs(args) => Cow::Owned(format!("scout zfs {}", args.subaction)),
        SynapseAction::ScoutLogs(args) => Cow::Owned(format!("scout logs {}", args.subaction)),
    }
}

/// Returns `true` when `error` is a validation error (missing/wrong-type
/// parameter, unknown action, etc.).
///
/// Called by the MCP boundary to map validation errors to `invalid_params`
/// rather than `internal_error`.
pub fn is_validation_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<super::ValidationError>().is_some()
        || error
            .downcast_ref::<crate::app::ScaffoldIntentValidationError>()
            .is_some()
}

/// Returns `true` when `error` is a destructive-op confirmation denial (B5
/// gate). The MCP boundary maps these to `ErrorData::invalid_request` (per
/// the bead's hard-block contract), distinct from `invalid_params` validation
/// errors.
pub fn is_confirmation_denied(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<crate::elicitation_gate::ConfirmationDenied>()
        .is_some()
}

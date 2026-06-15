//! Axum router — wires HTTP endpoints to the MCP service, REST API, and auth middleware.
//!
//! Endpoints:
//!   `POST /mcp`         — MCP Streamable HTTP transport (tools, resources, prompts)
//!   `GET  /health`      — Health check (unauthenticated)
//!   `GET  /status`      — Runtime status (unauthenticated, redacts secrets)
//!   `GET  /openapi.json` — OpenAPI schema (auth-gated on non-loopback, see below)
//!   `POST /v1/synapse2`  — REST API action dispatch (see `crate::api`)
//!   `/*`                — SPA fallback for embedded web UI (when web feature enabled)

use std::sync::Arc;

use axum::{
    Router,
    http::{HeaderValue, Method, StatusCode},
    response::Json,
    routing::{get, post},
};
use serde_json::json;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};

// ── Global concurrency limit (S-H5 / A-M5) ───────────────────────────────────
//
// A lightweight tower Layer backed by a `tokio::sync::Semaphore` that caps the
// number of concurrently in-flight requests on the API+MCP router.  Requests
// arriving when all permits are taken wait for a permit before being forwarded
// to the inner service — they are *queued*, not rejected, so clients experience
// back-pressure rather than errors under normal load spikes.
//
// We implement the layer inline rather than enabling the tower "limit" feature
// (which is not currently in the feature set) to avoid a Cargo.toml edit.
// `tokio::sync::Semaphore` is already used extensively in this codebase
// (fanout.rs, ssh/pool.rs) so there is no new dependency.

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::Semaphore;
use tower::{Layer, Service};

/// Boxed inner-service future produced by [`ConcurrencyLimitService::call`].
type BoxedServiceFuture<Res, Err> =
    Pin<Box<dyn std::future::Future<Output = Result<Res, Err>> + Send + 'static>>;

/// A tower [`Layer`] that wraps a service with a global concurrency cap.
///
/// Create via [`ConcurrencyLimitLayer::new`] with the maximum number of
/// simultaneous in-flight requests. Set `max_concurrent` to `0` to disable
/// the limit entirely (all requests pass through immediately).
#[derive(Clone)]
struct ConcurrencyLimitLayer {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyLimitLayer {
    /// Create a new layer allowing at most `max_concurrent` simultaneous
    /// requests. Pass `0` to disable limiting.
    fn new(max_concurrent: usize) -> Self {
        // Semaphore::MAX_PERMITS is the effective "unlimited" sentinel.
        let permits = if max_concurrent == 0 {
            Semaphore::MAX_PERMITS
        } else {
            max_concurrent
        };
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
        }
    }
}

impl<S> Layer<S> for ConcurrencyLimitLayer {
    type Service = ConcurrencyLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        ConcurrencyLimitService {
            inner,
            semaphore: Arc::clone(&self.semaphore),
        }
    }
}

/// The service wrapper produced by [`ConcurrencyLimitLayer`].
#[derive(Clone)]
struct ConcurrencyLimitService<S> {
    inner: S,
    semaphore: Arc<Semaphore>,
}

impl<S, ReqBody> Service<axum::http::Request<ReqBody>> for ConcurrencyLimitService<S>
where
    S: Service<axum::http::Request<ReqBody>, Error = std::convert::Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxedServiceFuture<S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<ReqBody>) -> Self::Future {
        let sem = Arc::clone(&self.semaphore);
        // Standard tower clone-for-call: `poll_ready` readied `self.inner`, so
        // move that readied instance into the future and leave a fresh clone
        // behind for the next call (calling `poll_ready` and `call` on the same
        // instance is required by the Service contract).
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            // Acquire a permit before forwarding. Requests are queued (awaited),
            // not rejected, under back-pressure. The permit is released when this
            // future completes or is dropped. The semaphore is never closed, so
            // `acquire_owned` cannot fail.
            let _permit = sem
                .acquire_owned()
                .await
                .expect("concurrency semaphore is never closed");
            inner.call(req).await
        })
    }
}

use crate::api::{api_dispatch, health, openapi_json, status};
use crate::mcp::{allowed_origins, streamable_http_config, streamable_http_service};
use crate::server::{AppState, AuthPolicy, build_auth_layer};

const MCP_BODY_LIMIT_BYTES: usize = 65_536;

pub fn router(state: AppState) -> Router {
    let rmcp_config = streamable_http_config(&state.config);

    let resource_url = match &state.auth_policy {
        AuthPolicy::Mounted { .. } => state
            .config
            .auth
            .public_url
            .as_deref()
            .map(|u| Arc::<str>::from(format!("{}/mcp", u.trim_end_matches('/')))),
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => None,
    };

    // Auth layer applied to both /mcp and /v1/synapse2.
    let auth_layer = build_auth_layer(
        &state.auth_policy,
        state.config.api_token.as_deref().map(Arc::<str>::from),
        resource_url,
    );

    // Global concurrency limit (S-H5 / A-M5): cap simultaneous in-flight
    // requests on the API+MCP router to prevent SSH/CPU/FD exhaustion under
    // request storms.  Controlled by SYNAPSE_MCP_MAX_CONCURRENCY (default 50).
    // Set to 0 to disable.  /health and /status are NOT covered by this limit
    // so monitoring probes always get a response.
    let concurrency_layer = ConcurrencyLimitLayer::new(state.config.max_concurrency);

    let api_and_mcp: Router<AppState> = Router::new()
        .nest_service("/mcp", streamable_http_service(state.clone(), rmcp_config))
        .route("/v1/synapse2", post(api_dispatch));

    let api_and_mcp_resolved: Router<()> = api_and_mcp.with_state(state.clone());

    // Layer order matters: in axum the LAST `.layer(...)` is the OUTERMOST and
    // runs first. Apply the concurrency cap first (inner) and auth last (outer)
    // so unauthenticated requests are rejected before consuming a concurrency
    // permit; the cap then wraps only authenticated work.
    let authenticated = if let Some(layer) = auth_layer {
        api_and_mcp_resolved.layer(concurrency_layer).layer(layer)
    } else {
        api_and_mcp_resolved.layer(concurrency_layer)
    };

    let oauth_router: Option<Router> = if let AuthPolicy::Mounted {
        auth_state: Some(ref state_arc),
    } = state.auth_policy
    {
        let auth_state = state_arc.as_ref().clone();
        let path_based_discovery = Router::new()
            .route(
                "/mcp/.well-known/oauth-authorization-server",
                get(lab_auth::metadata::authorization_server_metadata),
            )
            .route(
                "/mcp/.well-known/openid-configuration",
                get(lab_auth::metadata::authorization_server_metadata),
            )
            .route(
                "/mcp/.well-known/oauth-protected-resource",
                get(lab_auth::metadata::protected_resource_metadata),
            )
            .with_state(auth_state.clone());
        Some(lab_auth::routes::router(auth_state).merge(path_based_discovery))
    } else {
        None
    };

    // /health and /status are always public (monitoring probes).
    // /openapi.json exposes the full action schema and is gated behind auth on
    // non-loopback/Mounted policies to prevent unauthenticated schema enumeration
    // (S-M7 / CWE-200). On LoopbackDev and TrustedGatewayUnscoped it remains
    // open — auth is enforced at the transport/gateway layer in those modes.
    let always_public: Router<()> = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .with_state(state.clone());

    // Build the openapi route: authenticated on Mounted policies, public otherwise.
    let openapi_route: Router<()> = {
        let openapi_only: Router<AppState> =
            Router::new().route("/openapi.json", get(openapi_json));
        let openapi_resolved: Router<()> = openapi_only.with_state(state.clone());

        match &state.auth_policy {
            AuthPolicy::Mounted { .. } => {
                // Re-use the same auth layer (built above from the same config) for
                // the openapi route.  We cannot re-use the already-consumed layer
                // value, so we build a fresh one here.
                let openapi_auth = build_auth_layer(
                    &state.auth_policy,
                    state.config.api_token.as_deref().map(Arc::<str>::from),
                    None, // no resource URL needed for schema endpoint
                );
                if let Some(layer) = openapi_auth {
                    openapi_resolved.layer(layer)
                } else {
                    openapi_resolved
                }
            }
            // LoopbackDev / TrustedGatewayUnscoped: openapi remains public.
            // The trust boundary is the bind address / upstream gateway.
            AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => openapi_resolved,
        }
    };

    let public: Router<()> = always_public.merge(openapi_route);

    let mut base: Router<()> = Router::new().merge(authenticated).merge(public);

    if let Some(oauth) = oauth_router {
        base = base.merge(oauth);
    }

    let base = if crate::web::web_assets_available() {
        base.fallback(crate::web::serve_web_assets)
    } else {
        base.fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))) })
    };

    base.layer(RequestBodyLimitLayer::new(MCP_BODY_LIMIT_BYTES))
        .layer(cors_layer(&state.config))
}

fn cors_layer(config: &crate::config::McpConfig) -> CorsLayer {
    // SECURITY FIX: Document the CORS allowlist policy.
    //
    // By default, the following origins are always allowed (permissive-by-design for API use):
    //   - http://localhost:{port}
    //   - http://127.0.0.1:{port}
    //
    // Additionally, CORS origins can be expanded via:
    //   - SYNAPSE_MCP_ALLOWED_ORIGINS env var (comma-separated)
    //   - [mcp] allowed_origins in config.toml
    //   - SYNAPSE_MCP_PUBLIC_URL (for OAuth deployments)
    //
    // This default policy is intentionally broad for local development and API gatewaying.
    // In production, restrict CORS to specific client origins (e.g., https://claude.ai)
    // to prevent browser-based CSRF attacks. Auth middleware (bearer token or OAuth)
    // is the primary security control; CORS is defense-in-depth for browser clients.
    let origins: Vec<HeaderValue> = allowed_origins(config)
        .into_iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(hv) => Some(hv),
            Err(e) => {
                tracing::warn!(origin = %o, error = %e, "invalid CORS origin — skipping");
                None
            }
        })
        .collect();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::POST, Method::GET])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ])
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;

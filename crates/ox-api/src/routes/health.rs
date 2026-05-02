//! # Health probes
//!
//! Two endpoints expose the same component check, intentionally:
//!
//! - `GET /api/health`  — wrapped in [`ApiResponse`], consumed by the
//!   FE admin page (`getHealth()` → unwraps `data` like every other
//!   endpoint). Preserves the universal envelope invariant.
//! - `GET /api/healthz` — flat shape, no envelope, no auth. The
//!   industry-standard probe surface (k8s liveness/readiness,
//!   Datadog, Prometheus blackbox exporter, ops scripts). Probes
//!   want a stable, minimal contract — they should not have to
//!   know about API envelopes.
//!
//! Body construction is shared so the two endpoints cannot drift.

use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use crate::response::ApiResponse;
use crate::state::AppState;

/// Wire shape of both `/api/health` (wrapped in `ApiResponse`) and
/// `/api/healthz` (returned flat).
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthBody {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub components: HealthComponents,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthComponents {
    pub postgres: &'static str,
    /// Kept under the `neo4j` key for backward compatibility with
    /// existing FE/monitors. The actual backend name lives in
    /// `graph_backend`.
    pub neo4j: &'static str,
    pub graph_backend: String,
    pub llm: HealthLlm,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthLlm {
    pub provider: String,
    pub model: String,
}

async fn collect_health(state: &AppState) -> HealthBody {
    let health_timeout = state.timeouts.health_check;

    let db_ok = match tokio::time::timeout(health_timeout, state.store.health_check()).await {
        Ok(true) => true,
        Ok(false) => {
            tracing::warn!("PostgreSQL health check returned unhealthy");
            false
        }
        Err(_) => {
            tracing::warn!("PostgreSQL health check timed out");
            false
        }
    };

    let graph_ok = match &state.runtime {
        Some(runtime) => match tokio::time::timeout(health_timeout, runtime.health_check()).await {
            Ok(true) => true,
            Ok(false) => {
                tracing::warn!("Graph DB health check returned unhealthy");
                false
            }
            Err(_) => {
                tracing::warn!("Graph DB health check timed out");
                false
            }
        },
        None => false,
    };

    let graph_runtime_name = state
        .runtime
        .as_ref()
        .map(|r| r.name().to_string())
        .unwrap_or_else(|| "none".to_string());

    // PostgreSQL is critical — without it the service cannot function.
    // Graph DB is optional — chat still works but graph queries fail.
    let status = if !db_ok {
        "unavailable"
    } else if !graph_ok {
        "degraded"
    } else {
        "ok"
    };

    let provider = state.brain.default_model_info();

    HealthBody {
        status,
        service: "ontosyx",
        version: env!("CARGO_PKG_VERSION"),
        components: HealthComponents {
            postgres: if db_ok { "ok" } else { "unavailable" },
            neo4j: if graph_ok { "ok" } else { "unavailable" },
            graph_backend: graph_runtime_name,
            llm: HealthLlm {
                provider: provider.name.to_string(),
                model: provider.model.to_string(),
            },
        },
    }
}

#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 200, description = "Service health status (wrapped)", body = HealthBody),
    ),
    tag = "Health",
)]
pub(crate) async fn health_check(State(state): State<AppState>) -> Json<ApiResponse<HealthBody>> {
    let body = collect_health(&state).await;
    ApiResponse::of(body)
}

#[utoipa::path(
    get,
    path = "/api/healthz",
    responses(
        (status = 200, description = "Liveness/readiness probe — flat shape (no envelope, no auth)", body = HealthBody),
    ),
    tag = "Health",
)]
pub(crate) async fn healthz(State(state): State<AppState>) -> Json<HealthBody> {
    Json(collect_health(&state).await)
}

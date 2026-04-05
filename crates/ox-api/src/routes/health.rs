use axum::{Json, extract::State};
use serde_json::{Value, json};

use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 200, description = "Service health status", body = Object),
    ),
    tag = "Health",
)]
pub async fn health_check(State(state): State<AppState>) -> Json<Value> {
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
        .map(|r| r.runtime_name().to_string())
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

    Json(json!({
        "status": status,
        "service": "ontosyx",
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "postgres": if db_ok { "ok" } else { "unavailable" },
            // Keep "neo4j" key for backward compatibility with existing frontend/monitors
            "neo4j": if graph_ok { "ok" } else { "unavailable" },
            "graph_backend": graph_runtime_name,
            "llm": {
                "provider": provider.name,
                "model": provider.model,
            },
        },
    }))
}

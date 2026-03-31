use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/config — all config rows grouped by category (protected)
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub data_type: String,
    pub description: String,
}

#[utoipa::path(
    get,
    path = "/api/config",
    responses(
        (status = 200, description = "Configuration grouped by category", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Config",
)]
pub async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<BTreeMap<String, Vec<ConfigEntry>>>, AppError> {
    let rows = state.store.get_all_config().await.map_err(AppError::from)?;

    let mut grouped: BTreeMap<String, Vec<ConfigEntry>> = BTreeMap::new();
    for row in rows {
        grouped.entry(row.category).or_default().push(ConfigEntry {
            key: row.key,
            value: row.value,
            data_type: row.data_type,
            description: row.description,
        });
    }

    Ok(Json(grouped))
}

// ---------------------------------------------------------------------------
// GET /api/config/ui — frontend-relevant config subset (public)
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct UiConfig {
    pub elk_direction: String,
    pub elk_node_spacing: i64,
    pub elk_layer_spacing: i64,
    pub elk_edge_routing: String,
    pub worker_timeout_ms: i64,
}

#[utoipa::path(
    get,
    path = "/api/config/ui",
    responses(
        (status = 200, description = "Frontend-relevant configuration", body = UiConfig),
    ),
    tag = "Config",
)]
pub async fn get_ui_config(State(state): State<AppState>) -> Result<Json<UiConfig>, AppError> {
    let config = state.system_config.read().await;
    Ok(Json(UiConfig {
        elk_direction: config.elk_direction(),
        elk_node_spacing: config.elk_node_spacing(),
        elk_layer_spacing: config.elk_layer_spacing(),
        elk_edge_routing: config.elk_edge_routing(),
        worker_timeout_ms: config.worker_timeout_ms(),
    }))
}

// ---------------------------------------------------------------------------
// PATCH /api/config — update config values (protected)
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ConfigUpdate {
    pub category: String,
    pub key: String,
    pub value: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ConfigUpdateRequest {
    pub updates: Vec<ConfigUpdate>,
}

#[utoipa::path(
    patch,
    path = "/api/config",
    request_body = ConfigUpdateRequest,
    responses(
        (status = 200, description = "Config updated", body = Object),
        (status = 400, description = "No updates provided", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Config",
)]
pub async fn update_config(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_admin()?;
    if req.updates.is_empty() {
        return Err(AppError::bad_request("No updates provided"));
    }

    let batch: Vec<(String, String, String)> = req
        .updates
        .iter()
        .map(|u| (u.category.clone(), u.key.clone(), u.value.clone()))
        .collect();
    state
        .store
        .update_config_batch(&batch)
        .await
        .map_err(AppError::from)?;

    // Refresh in-memory cache immediately after updates
    let new_config = crate::system_config::load_system_config(state.store.as_ref()).await;
    *state.system_config.write().await = new_config;

    Ok(Json(serde_json::json!({ "updated": req.updates.len() })))
}

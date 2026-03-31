use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::{
    ModelConfig, ModelConfigUpdate, ModelRoutingRule, NewModelConfig, NewRoutingRule,
    RoutingRuleUpdate,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Query types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListConfigsParams {
    pub workspace_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct ListRulesParams {
    pub workspace_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct TestModelRequest {
    pub provider: String,
    pub model_id: String,
    pub api_key_env: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Serialize)]
pub struct TestModelResponse {
    pub ok: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// GET /api/models/configs — list model configs
// ---------------------------------------------------------------------------

pub(crate) async fn list_model_configs(
    State(state): State<AppState>,
    _principal: Principal,
    Query(params): Query<ListConfigsParams>,
) -> Result<Json<Vec<ModelConfig>>, AppError> {
    let configs = state
        .store
        .list_model_configs(params.workspace_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(configs))
}

// ---------------------------------------------------------------------------
// POST /api/models/configs — create a model config
// ---------------------------------------------------------------------------

pub(crate) async fn create_model_config(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<NewModelConfig>,
) -> Result<(StatusCode, Json<ModelConfig>), AppError> {
    principal.require_admin()?;

    let config = state
        .store
        .create_model_config(&req)
        .await
        .map_err(AppError::from)?;

    // Invalidate caches so new config takes effect immediately
    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(config_id = %config.id, name = %config.name, "Model config created");

    Ok((StatusCode::CREATED, Json(config)))
}

// ---------------------------------------------------------------------------
// PATCH /api/models/configs/{id} — update a model config
// ---------------------------------------------------------------------------

pub(crate) async fn update_model_config(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ModelConfigUpdate>,
) -> Result<Json<ModelConfig>, AppError> {
    principal.require_admin()?;

    let config = state
        .store
        .update_model_config(id, &req)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(config_id = %id, "Model config updated");

    Ok(Json(config))
}

// ---------------------------------------------------------------------------
// DELETE /api/models/configs/{id} — delete a model config
// ---------------------------------------------------------------------------

pub(crate) async fn delete_model_config(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;

    state
        .store
        .delete_model_config(id)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(config_id = %id, "Model config deleted");

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/models/routing-rules — list routing rules
// ---------------------------------------------------------------------------

pub(crate) async fn list_routing_rules(
    State(state): State<AppState>,
    _principal: Principal,
    Query(params): Query<ListRulesParams>,
) -> Result<Json<Vec<ModelRoutingRule>>, AppError> {
    let rules = state
        .store
        .list_routing_rules(params.workspace_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(rules))
}

// ---------------------------------------------------------------------------
// POST /api/models/routing-rules — create a routing rule
// ---------------------------------------------------------------------------

pub(crate) async fn create_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<NewRoutingRule>,
) -> Result<(StatusCode, Json<ModelRoutingRule>), AppError> {
    principal.require_admin()?;

    let rule = state
        .store
        .create_routing_rule(&req)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(rule_id = %rule.id, operation = %rule.operation, "Routing rule created");

    Ok((StatusCode::CREATED, Json(rule)))
}

// ---------------------------------------------------------------------------
// PATCH /api/models/routing-rules/{id} — update a routing rule
// ---------------------------------------------------------------------------

pub(crate) async fn update_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<RoutingRuleUpdate>,
) -> Result<Json<ModelRoutingRule>, AppError> {
    principal.require_admin()?;

    let rule = state
        .store
        .update_routing_rule(id, &req)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(rule_id = %id, "Routing rule updated");

    Ok(Json(rule))
}

// ---------------------------------------------------------------------------
// DELETE /api/models/routing-rules/{id} — delete a routing rule
// ---------------------------------------------------------------------------

pub(crate) async fn delete_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;

    state
        .store
        .delete_routing_rule(id)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(rule_id = %id, "Routing rule deleted");

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/models/test — test model connection
// ---------------------------------------------------------------------------

pub(crate) async fn test_model_connection(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<TestModelRequest>,
) -> Result<Json<TestModelResponse>, AppError> {
    principal.require_admin()?;

    // Resolve the API key from the environment variable
    let api_key = req
        .api_key_env
        .as_deref()
        .and_then(|env_var| std::env::var(env_var).ok());

    let provider_config = ox_brain::auth::LlmProviderConfig {
        provider: req.provider.clone(),
        model: req.model_id.clone(),
        api_key,
        region: req.region.clone(),
        base_url: req.base_url.clone(),
        timeout_secs: Some(15),
    };

    // Try to create a client and send a minimal request
    match state.client_pool.get_or_create(&provider_config).await {
        Ok(client) => {
            use branchforge::client::CreateMessageRequest;
            use branchforge::types::Message;

            let request = CreateMessageRequest::new(
                &provider_config.model,
                vec![Message::user("Say OK")],
            )
            .max_tokens(16_u32);

            match client.send(request).await {
                Ok(_) => Ok(Json(TestModelResponse {
                    ok: true,
                    message: format!(
                        "Successfully connected to {} / {}",
                        req.provider, req.model_id
                    ),
                })),
                Err(e) => Ok(Json(TestModelResponse {
                    ok: false,
                    message: format!("Client created but request failed: {e}"),
                })),
            }
        }
        Err(e) => Ok(Json(TestModelResponse {
            ok: false,
            message: format!("Failed to create client: {e}"),
        })),
    }
}

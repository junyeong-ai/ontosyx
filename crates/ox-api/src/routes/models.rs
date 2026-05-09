use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::{
    ModelConfig, ModelConfigUpdate, ModelRoutingRule, NewModelConfig, NewRoutingRule,
    RoutingRuleUpdate,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::validation;

// ---------------------------------------------------------------------------
// Request / Query types
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListConfigsParams {
    pub workspace_id: Option<Uuid>,
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListRulesParams {
    pub workspace_id: Option<Uuid>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TestModelRequest {
    pub provider: String,
    pub model_id: String,
    pub api_key_env: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TestModelResponse {
    pub ok: bool,
    pub message: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ModelOperation {
    pub key: String,
    pub tier: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// GET /api/models/operations — known routing operation registry
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/models/operations",
    operation_id = "list_model_operations",
    responses((status = 200, description = "Known model routing operations", body = Vec<ModelOperation>)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn list_model_operations(
    _principal: Principal,
) -> Result<Json<ApiResponse<Vec<ModelOperation>>>, AppError> {
    let operations = ox_brain::model_resolver::KNOWN_OPERATIONS
        .iter()
        .map(|op| ModelOperation {
            key: op.key.to_string(),
            tier: op.tier.to_string(),
            description: op.description.to_string(),
        })
        .collect();
    Ok(ApiResponse::of(operations))
}

fn validate_provider(value: &str) -> Result<(), AppError> {
    validation::validate_name("provider", value)?;
    if !value.chars().enumerate().all(|(idx, ch)| {
        if idx == 0 {
            ch.is_ascii_lowercase()
        } else {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-'
        }
    }) {
        return Err(AppError::validation(
            "provider",
            "must use lowercase provider identifiers",
        ));
    }
    Ok(())
}

fn validate_model_id(value: &str) -> Result<(), AppError> {
    validation::validate_name("model_id", value)
}

fn validate_optional_env(field: &str, value: Option<&str>) -> Result<(), AppError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Ok(());
    }
    if !value.chars().enumerate().all(|(idx, ch)| {
        if idx == 0 {
            ch.is_ascii_uppercase() || ch == '_'
        } else {
            ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'
        }
    }) {
        return Err(AppError::validation(
            field,
            "must be an uppercase environment variable name",
        ));
    }
    Ok(())
}

fn validate_optional_base_url(value: Option<&str>) -> Result<(), AppError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Ok(());
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| AppError::validation("base_url", "must be a valid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::validation("base_url", "must use http or https"));
    }
    Ok(())
}

fn validate_model_limits(
    max_tokens: Option<i32>,
    temperature: Option<f32>,
    timeout_secs: Option<i32>,
    cost_per_1m_input: Option<f64>,
    cost_per_1m_output: Option<f64>,
    daily_budget_usd: Option<f64>,
) -> Result<(), AppError> {
    if let Some(value) = max_tokens
        && !(1..=2_000_000).contains(&value)
    {
        return Err(AppError::validation(
            "max_tokens",
            "must be between 1 and 2000000",
        ));
    }
    if let Some(value) = temperature
        && !(0.0..=2.0).contains(&value)
    {
        return Err(AppError::validation(
            "temperature",
            "must be between 0 and 2",
        ));
    }
    if let Some(value) = timeout_secs
        && !(1..=3600).contains(&value)
    {
        return Err(AppError::validation(
            "timeout_secs",
            "must be between 1 and 3600",
        ));
    }
    for (field, value) in [
        ("cost_per_1m_input", cost_per_1m_input),
        ("cost_per_1m_output", cost_per_1m_output),
        ("daily_budget_usd", daily_budget_usd),
    ] {
        if let Some(value) = value
            && (!value.is_finite() || value < 0.0)
        {
            return Err(AppError::validation(field, "must be non-negative"));
        }
    }
    Ok(())
}

fn validate_new_model_config(req: &NewModelConfig) -> Result<(), AppError> {
    validation::validate_name("name", &req.name)?;
    validate_provider(&req.provider)?;
    validate_model_id(&req.model_id)?;
    validate_model_limits(
        req.max_tokens,
        req.temperature,
        req.timeout_secs,
        req.cost_per_1m_input,
        req.cost_per_1m_output,
        req.daily_budget_usd,
    )?;
    validate_optional_env("api_key_env", req.api_key_env.as_deref())?;
    validate_optional_base_url(req.base_url.as_deref())
}

fn validate_model_config_update(req: &ModelConfigUpdate) -> Result<(), AppError> {
    if let Some(name) = &req.name {
        validation::validate_name("name", name)?;
    }
    if let Some(provider) = &req.provider {
        validate_provider(provider)?;
    }
    if let Some(model_id) = &req.model_id {
        validate_model_id(model_id)?;
    }
    validate_model_limits(
        req.max_tokens,
        req.temperature,
        req.timeout_secs,
        req.cost_per_1m_input,
        req.cost_per_1m_output,
        req.daily_budget_usd,
    )?;
    validate_optional_env("api_key_env", req.api_key_env.as_deref())?;
    validate_optional_base_url(req.base_url.as_deref())
}

fn validate_operation(value: &str) -> Result<(), AppError> {
    if value == "*" {
        return Ok(());
    }
    if value.is_empty() || value.len() > 128 {
        return Err(AppError::validation(
            "operation",
            "must be '*' or a non-empty operation key",
        ));
    }
    if !value.chars().enumerate().all(|(idx, ch)| {
        if idx == 0 {
            ch.is_ascii_lowercase()
        } else {
            ch.is_ascii_lowercase()
                || ch.is_ascii_digit()
                || ch == '_'
                || ch == '-'
                || ch == '.'
                || ch == ':'
        }
    }) {
        return Err(AppError::validation(
            "operation",
            "must use a stable lowercase operation key",
        ));
    }
    Ok(())
}

fn validate_new_routing_rule(req: &NewRoutingRule) -> Result<(), AppError> {
    validate_operation(&req.operation)
}

fn validate_routing_rule_update(req: &RoutingRuleUpdate) -> Result<(), AppError> {
    if let Some(operation) = &req.operation {
        validate_operation(operation)?;
    }
    Ok(())
}

fn validate_test_model_request(req: &TestModelRequest) -> Result<(), AppError> {
    validate_provider(&req.provider)?;
    validate_model_id(&req.model_id)?;
    validate_optional_env("api_key_env", req.api_key_env.as_deref())?;
    validate_optional_base_url(req.base_url.as_deref())
}

// ---------------------------------------------------------------------------
// GET /api/models/configs — list model configs
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/models/configs",
    params(ListConfigsParams),
    responses((status = 200, description = "Model configs", body = Vec<ModelConfig>)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn list_model_configs(
    State(state): State<AppState>,
    _principal: Principal,
    Query(params): Query<ListConfigsParams>,
) -> Result<Json<ApiResponse<Vec<ModelConfig>>>, AppError> {
    let configs = state
        .store
        .list_model_configs(params.workspace_id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(configs))
}

// ---------------------------------------------------------------------------
// POST /api/models/configs — create a model config
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/models/configs",
    request_body = NewModelConfig,
    responses((status = 201, description = "Model config created", body = ModelConfig)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn create_model_config(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<NewModelConfig>,
) -> Result<(StatusCode, Json<ApiResponse<ModelConfig>>), AppError> {
    principal.require_admin()?;
    validate_new_model_config(&req)?;

    let config = state
        .store
        .create_model_config(&req)
        .await
        .map_err(AppError::from)?;

    // Invalidate caches so new config takes effect immediately
    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(config_id = %config.id, name = %config.name, "Model config created");

    Ok((StatusCode::CREATED, ApiResponse::of(config)))
}

// ---------------------------------------------------------------------------
// PATCH /api/models/configs/{id} — update a model config
// ---------------------------------------------------------------------------

#[utoipa::path(
    patch,
    path = "/api/models/configs/{id}",
    params(("id" = Uuid, Path, description = "Model config ID")),
    request_body = ModelConfigUpdate,
    responses((status = 200, description = "Updated model config", body = ModelConfig)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn update_model_config(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ModelConfigUpdate>,
) -> Result<Json<ApiResponse<ModelConfig>>, AppError> {
    principal.require_admin()?;
    validate_model_config_update(&req)?;

    let config = state
        .store
        .update_model_config(id, &req)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(config_id = %id, "Model config updated");

    Ok(ApiResponse::of(config))
}

// ---------------------------------------------------------------------------
// DELETE /api/models/configs/{id} — delete a model config
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/models/configs/{id}",
    params(("id" = Uuid, Path, description = "Model config ID")),
    responses(
        (status = 204, description = "Model config deleted"),
        (status = 404, description = "Config not found"),
    ),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn delete_model_config(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;

    let deleted = state
        .store
        .delete_model_config(id)
        .await
        .map_err(AppError::from)?;
    if !deleted {
        return Err(AppError::not_found("Model config"));
    }

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(config_id = %id, "Model config deleted");

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/models/routing-rules — list routing rules
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/models/routing-rules",
    operation_id = "list_model_routing_rules",
    params(ListRulesParams),
    responses((status = 200, description = "Routing rules", body = Vec<ModelRoutingRule>)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn list_routing_rules(
    State(state): State<AppState>,
    _principal: Principal,
    Query(params): Query<ListRulesParams>,
) -> Result<Json<ApiResponse<Vec<ModelRoutingRule>>>, AppError> {
    let rules = state
        .store
        .list_routing_rules(params.workspace_id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(rules))
}

// ---------------------------------------------------------------------------
// POST /api/models/routing-rules — create a routing rule
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/models/routing-rules",
    request_body = NewRoutingRule,
    responses((status = 201, description = "Routing rule created", body = ModelRoutingRule)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn create_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<NewRoutingRule>,
) -> Result<(StatusCode, Json<ApiResponse<ModelRoutingRule>>), AppError> {
    principal.require_admin()?;
    validate_new_routing_rule(&req)?;

    let rule = state
        .store
        .create_routing_rule(&req)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(rule_id = %rule.id, operation = %rule.operation, "Routing rule created");

    Ok((StatusCode::CREATED, ApiResponse::of(rule)))
}

// ---------------------------------------------------------------------------
// PATCH /api/models/routing-rules/{id} — update a routing rule
// ---------------------------------------------------------------------------

#[utoipa::path(
    patch,
    path = "/api/models/routing-rules/{id}",
    params(("id" = Uuid, Path, description = "Routing rule ID")),
    request_body = RoutingRuleUpdate,
    responses((status = 200, description = "Updated routing rule", body = ModelRoutingRule)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn update_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<RoutingRuleUpdate>,
) -> Result<Json<ApiResponse<ModelRoutingRule>>, AppError> {
    principal.require_admin()?;
    validate_routing_rule_update(&req)?;

    let rule = state
        .store
        .update_routing_rule(id, &req)
        .await
        .map_err(AppError::from)?;

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(rule_id = %id, "Routing rule updated");

    Ok(ApiResponse::of(rule))
}

// ---------------------------------------------------------------------------
// DELETE /api/models/routing-rules/{id} — delete a routing rule
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/models/routing-rules/{id}",
    operation_id = "delete_model_routing_rule",
    params(("id" = Uuid, Path, description = "Routing rule ID")),
    responses(
        (status = 204, description = "Routing rule deleted"),
        (status = 404, description = "Rule not found"),
    ),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn delete_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;

    let deleted = state
        .store
        .delete_routing_rule(id)
        .await
        .map_err(AppError::from)?;
    if !deleted {
        return Err(AppError::not_found("Routing rule"));
    }

    state.model_router.invalidate().await;
    state.client_pool.invalidate_all();

    tracing::info!(rule_id = %id, "Routing rule deleted");

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/models/test — test model connection
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/models/test",
    request_body = TestModelRequest,
    responses((status = 200, description = "Connection probe result", body = TestModelResponse)),
    security(("api_key" = [])),
    tag = "Models",
)]
pub(crate) async fn test_model_connection(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<TestModelRequest>,
) -> Result<Json<ApiResponse<TestModelResponse>>, AppError> {
    principal.require_admin()?;
    validate_test_model_request(&req)?;

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
            use branchforge::{Message, ModelRequest};

            let request = ModelRequest::new(&provider_config.model, vec![Message::user("Say OK")])
                .with_max_tokens(16);

            match client.send(&request).await {
                Ok(_) => Ok(ApiResponse::of(TestModelResponse {
                    ok: true,
                    message: format!(
                        "Successfully connected to {} / {}",
                        req.provider, req.model_id
                    ),
                })),
                Err(e) => Ok(ApiResponse::of(TestModelResponse {
                    ok: false,
                    message: format!("Client created but request failed: {e}"),
                })),
            }
        }
        Err(e) => Ok(ApiResponse::of(TestModelResponse {
            ok: false,
            message: format!("Failed to create client: {e}"),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_config_validation_rejects_unstable_identity_fields() {
        assert!(validate_provider("anthropic").is_ok());
        assert!(validate_provider("bedrock-global").is_ok());
        assert!(validate_provider("Anthropic").is_err());
        assert!(validate_provider("").is_err());

        assert!(validate_optional_env("api_key_env", Some("ANTHROPIC_API_KEY")).is_ok());
        assert!(validate_optional_env("api_key_env", Some("anthropic_key")).is_err());

        assert!(validate_optional_base_url(Some("https://api.example.com/v1")).is_ok());
        assert!(validate_optional_base_url(Some("ftp://api.example.com")).is_err());
    }

    #[test]
    fn model_config_validation_rejects_invalid_limits() {
        assert!(validate_model_limits(Some(1), Some(0.0), Some(1), Some(0.0), None, None).is_ok());
        assert!(validate_model_limits(Some(0), None, None, None, None, None).is_err());
        assert!(validate_model_limits(None, Some(2.1), None, None, None, None).is_err());
        assert!(validate_model_limits(None, None, Some(0), None, None, None).is_err());
        assert!(validate_model_limits(None, None, None, Some(-0.1), None, None).is_err());
    }

    #[test]
    fn routing_rule_validation_rejects_unstable_operation_keys() {
        assert!(validate_operation("*").is_ok());
        assert!(validate_operation("design_ontology").is_ok());
        assert!(validate_operation("ontology.design:extract").is_ok());
        assert!(validate_operation("DesignOntology").is_err());
        assert!(validate_operation("").is_err());
    }
}

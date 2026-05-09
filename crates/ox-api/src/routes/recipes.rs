use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::store::CursorParams;
use ox_store::{AnalysisRecipe, RecipeExecutionResult, RecipeStatus};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::validation;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// POST /api/recipes — save an analysis recipe
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateRecipeRequest {
    pub name: String,
    pub description: String,
    pub algorithm_type: String,
    pub code_template: String,
    /// Free-form parameter map. The recipe runner reads keys per
    /// algorithm; the API layer doesn't constrain the shape.
    #[serde(default)]
    #[schema(value_type = HashMap<String, Object>, additional_properties)]
    pub parameters: serde_json::Value,
    /// Column names the recipe needs from the source. Order is
    /// preserved end-to-end.
    #[serde(default)]
    pub required_columns: Vec<String>,
    #[serde(default)]
    pub output_description: String,
}

#[utoipa::path(
    post,
    path = "/api/recipes",
    request_body = CreateRecipeRequest,
    responses(
        (status = 200, description = "Recipe created", body = AnalysisRecipe),
        (status = 400, description = "Validation failure"),
    ),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn create_recipe(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateRecipeRequest>,
) -> Result<Json<ApiResponse<AnalysisRecipe>>, AppError> {
    principal.require_designer()?;
    validation::validate_name("name", &req.name)?;
    validation::validate_description("description", &req.description)?;
    validation::validate_code("code_template", &req.code_template)?;

    let recipe = AnalysisRecipe {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        name: req.name,
        description: req.description,
        algorithm_type: req.algorithm_type,
        code_template: req.code_template,
        parameters: req.parameters,
        required_columns: serde_json::Value::from(req.required_columns),
        output_description: req.output_description,
        created_by: principal.id,
        created_at: Utc::now(),
        version: 1,
        status: RecipeStatus::Draft,
        parent_id: None,
    };

    state
        .store
        .upsert_recipe(&recipe)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(recipe))
}

// ---------------------------------------------------------------------------
// GET /api/recipes — list analysis recipes
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct RecipesCursorQuery {
    #[serde(default = "default_recipes_limit")]
    pub limit: u32,
    pub cursor: Option<String>,
}

fn default_recipes_limit() -> u32 {
    50
}

impl From<RecipesCursorQuery> for CursorParams {
    fn from(q: RecipesCursorQuery) -> Self {
        Self {
            limit: q.limit,
            cursor: q.cursor,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/recipes",
    params(RecipesCursorQuery),
    responses((status = 200, description = "Analysis recipes", body = crate::openapi::AnalysisRecipePage)),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn list_recipes(
    State(state): State<AppState>,
    _principal: Principal,
    axum::extract::Query(pagination): axum::extract::Query<RecipesCursorQuery>,
) -> Result<Json<ApiResponse<Vec<AnalysisRecipe>>>, AppError> {
    let pagination: CursorParams = pagination.into();
    let page = state
        .store
        .list_recipes(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id — get a single recipe
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/recipes/{id}",
    params(("id" = Uuid, Path, description = "Recipe ID")),
    responses(
        (status = 200, description = "Recipe", body = AnalysisRecipe),
        (status = 404, description = "Recipe not found"),
    ),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn get_recipe(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<AnalysisRecipe>>, AppError> {
    let recipe = state
        .store
        .get_recipe(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Analysis recipe"))?;
    Ok(ApiResponse::of(recipe))
}

// ---------------------------------------------------------------------------
// DELETE /api/recipes/:id — delete a recipe
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/recipes/{id}",
    params(("id" = Uuid, Path, description = "Recipe ID")),
    responses(
        (status = 204, description = "Recipe deleted"),
        (status = 403, description = "Caller does not own the recipe"),
        (status = 404, description = "Recipe not found"),
    ),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn delete_recipe(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_designer()?;

    let recipe = state
        .store
        .get_recipe(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Analysis recipe"))?;
    principal.require_owner(&recipe.created_by, "recipe")?;

    let deleted = state
        .store
        .delete_recipe(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Analysis recipe"))
    }
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id/results — list past results for a recipe
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/recipes/{id}/results",
    params(("id" = Uuid, Path, description = "Recipe ID")),
    responses((status = 200, description = "Recent analysis results", body = Vec<RecipeExecutionResult>)),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn list_recipe_results(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<RecipeExecutionResult>>>, AppError> {
    let results = state
        .store
        .list_analysis_results(id, 20)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(results))
}

// ---------------------------------------------------------------------------
// PATCH /api/recipes/:id/status — update recipe status (approve/deprecate)
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RecipeStatusUpdateRequest {
    pub status: RecipeStatus,
}

#[utoipa::path(
    patch,
    path = "/api/recipes/{id}/status",
    params(("id" = Uuid, Path, description = "Recipe ID")),
    request_body = RecipeStatusUpdateRequest,
    responses(
        (status = 204, description = "Status updated"),
        (status = 400, description = "Invalid status"),
        (status = 404, description = "Recipe not found"),
    ),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn update_recipe_status(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<RecipeStatusUpdateRequest>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;

    // Verify recipe exists
    state
        .store
        .get_recipe(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Analysis recipe"))?;

    state
        .store
        .update_recipe_status(id, req.status)
        .await
        .map_err(AppError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/recipes/:id/versions — create a new version of a recipe
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/recipes/{id}/versions",
    params(("id" = Uuid, Path, description = "Parent recipe ID")),
    request_body = CreateRecipeRequest,
    responses(
        (status = 200, description = "New recipe version created", body = AnalysisRecipe),
        (status = 404, description = "Parent recipe not found"),
    ),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn create_recipe_version(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(parent_id): Path<Uuid>,
    Json(req): Json<CreateRecipeRequest>,
) -> Result<Json<ApiResponse<AnalysisRecipe>>, AppError> {
    principal.require_designer()?;
    validation::validate_name("name", &req.name)?;
    validation::validate_description("description", &req.description)?;
    validation::validate_code("code_template", &req.code_template)?;

    // Load parent to determine next version number
    let parent = state
        .store
        .get_recipe(parent_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Parent recipe"))?;

    let recipe = AnalysisRecipe {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        name: req.name,
        description: req.description,
        algorithm_type: req.algorithm_type,
        code_template: req.code_template,
        parameters: req.parameters,
        required_columns: serde_json::Value::from(req.required_columns),
        output_description: req.output_description,
        created_by: principal.id,
        created_at: Utc::now(),
        version: parent.version + 1,
        status: RecipeStatus::Draft,
        parent_id: Some(parent_id),
    };

    state
        .store
        .create_recipe_version(&recipe)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(recipe))
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id/versions — list all versions of a recipe
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/recipes/{id}/versions",
    params(("id" = Uuid, Path, description = "Recipe ID")),
    responses((status = 200, description = "Recipe versions", body = Vec<AnalysisRecipe>)),
    security(("api_key" = [])),
    tag = "Recipes",
)]
pub(crate) async fn list_recipe_versions(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<AnalysisRecipe>>>, AppError> {
    let versions = state
        .store
        .list_recipe_versions(id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(versions))
}

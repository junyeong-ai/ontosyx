use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::store::CursorParams;
use ox_store::{AnalysisRecipe, AnalysisResult};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::validation;

// ---------------------------------------------------------------------------
// POST /api/recipes — save an analysis recipe
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RecipeCreateRequest {
    pub name: String,
    pub description: String,
    pub algorithm_type: String,
    pub code_template: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub required_columns: serde_json::Value,
    #[serde(default)]
    pub output_description: String,
}

pub(crate) async fn create_recipe(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<RecipeCreateRequest>,
) -> Result<Json<AnalysisRecipe>, AppError> {
    principal.require_designer()?;
    validation::validate_name("name", &req.name)?;
    validation::validate_description("description", &req.description)?;
    validation::validate_code("code_template", &req.code_template)?;

    let recipe = AnalysisRecipe {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        algorithm_type: req.algorithm_type,
        code_template: req.code_template,
        parameters: req.parameters,
        required_columns: req.required_columns,
        output_description: req.output_description,
        created_by: principal.id,
        created_at: Utc::now(),
        version: 1,
        status: "draft".to_string(),
        parent_id: None,
    };

    state
        .store
        .upsert_recipe(&recipe)
        .await
        .map_err(AppError::from)?;

    Ok(Json(recipe))
}

// ---------------------------------------------------------------------------
// GET /api/recipes — list analysis recipes
// ---------------------------------------------------------------------------

pub(crate) async fn list_recipes(
    State(state): State<AppState>,
    _principal: Principal,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ox_store::store::CursorPage<AnalysisRecipe>>, AppError> {
    let page = state
        .store
        .list_recipes(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id — get a single recipe
// ---------------------------------------------------------------------------

pub(crate) async fn get_recipe(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<AnalysisRecipe>, AppError> {
    let recipe = state
        .store
        .get_recipe(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Analysis recipe"))?;
    Ok(Json(recipe))
}

// ---------------------------------------------------------------------------
// DELETE /api/recipes/:id — delete a recipe
// ---------------------------------------------------------------------------

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

pub(crate) async fn list_recipe_results(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AnalysisResult>>, AppError> {
    let results = state
        .store
        .list_analysis_results(id, 20)
        .await
        .map_err(AppError::from)?;
    Ok(Json(results))
}

// ---------------------------------------------------------------------------
// PATCH /api/recipes/:id/status — update recipe status (approve/deprecate)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct RecipeStatusUpdateRequest {
    pub status: String,
}

pub(crate) async fn update_recipe_status(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<RecipeStatusUpdateRequest>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;

    let valid_statuses = ["draft", "approved", "deprecated"];
    if !valid_statuses.contains(&req.status.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid status: {}. Must be one of: draft, approved, deprecated",
            req.status
        )));
    }

    // Verify recipe exists
    state
        .store
        .get_recipe(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Analysis recipe"))?;

    state
        .store
        .update_recipe_status(id, &req.status)
        .await
        .map_err(AppError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/recipes/:id/versions — create a new version of a recipe
// ---------------------------------------------------------------------------

pub(crate) async fn create_recipe_version(
    State(state): State<AppState>,
    principal: Principal,
    Path(parent_id): Path<Uuid>,
    Json(req): Json<RecipeCreateRequest>,
) -> Result<Json<AnalysisRecipe>, AppError> {
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
        name: req.name,
        description: req.description,
        algorithm_type: req.algorithm_type,
        code_template: req.code_template,
        parameters: req.parameters,
        required_columns: req.required_columns,
        output_description: req.output_description,
        created_by: principal.id,
        created_at: Utc::now(),
        version: parent.version + 1,
        status: "draft".to_string(),
        parent_id: Some(parent_id),
    };

    state
        .store
        .create_recipe_version(&recipe)
        .await
        .map_err(AppError::from)?;

    Ok(Json(recipe))
}

// ---------------------------------------------------------------------------
// GET /api/recipes/:id/versions — list all versions of a recipe
// ---------------------------------------------------------------------------

pub(crate) async fn list_recipe_versions(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AnalysisRecipe>>, AppError> {
    let versions = state
        .store
        .list_recipe_versions(id)
        .await
        .map_err(AppError::from)?;
    Ok(Json(versions))
}

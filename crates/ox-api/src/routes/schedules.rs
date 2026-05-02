use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::ScheduledTask;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::schedule;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/recipes/:id/schedule — create a scheduled task for a recipe
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateScheduleRequest {
    pub cron_expression: String,
    pub ontology_lineage_id: Option<String>,
    pub description: Option<String>,
    pub webhook_url: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/recipes/{id}/schedule",
    params(("id" = Uuid, Path, description = "Recipe ID")),
    request_body = CreateScheduleRequest,
    responses(
        (status = 201, description = "Schedule created", body = Object),
        (status = 400, description = "Invalid cron expression"),
        (status = 404, description = "Recipe not found"),
    ),
    security(("api_key" = [])),
    tag = "Schedules",
)]
pub(crate) async fn create_schedule(
    State(state): State<AppState>,
    principal: Principal,
    Path(recipe_id): Path<Uuid>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ScheduledTask>>), AppError> {
    principal.require_designer()?;

    // Verify recipe exists
    state
        .store
        .get_recipe(recipe_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Analysis recipe"))?;

    // Validate cron expression and compute first run
    let next_run =
        schedule::next_run_from_cron(&req.cron_expression, Utc::now()).ok_or_else(|| {
            AppError::bad_request(format!(
                "Invalid cron expression: '{}'",
                req.cron_expression
            ))
        })?;

    let task = ScheduledTask {
        id: Uuid::new_v4(),
        recipe_id,
        ontology_lineage_id: req.ontology_lineage_id,
        cron_expression: req.cron_expression,
        description: req.description,
        enabled: true,
        last_run_at: None,
        next_run_at: next_run,
        last_status: None,
        webhook_url: req.webhook_url,
        created_by: principal.id,
        created_at: Utc::now(),
    };

    state
        .store
        .create_scheduled_task(&task)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::CREATED, ApiResponse::of(task)))
}

// ---------------------------------------------------------------------------
// GET /api/scheduled-tasks — list all scheduled tasks
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ScheduleListParams {
    pub recipe_id: Option<Uuid>,
}

#[utoipa::path(
    get,
    path = "/api/scheduled-tasks",
    params(ScheduleListParams),
    responses((status = 200, description = "Scheduled tasks", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Schedules",
)]
pub(crate) async fn list_schedules(
    State(state): State<AppState>,
    _principal: Principal,
    Query(query): Query<ScheduleListParams>,
) -> Result<Json<ApiResponse<Vec<ScheduledTask>>>, AppError> {
    let tasks = state
        .store
        .list_scheduled_tasks(query.recipe_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(tasks))
}

// ---------------------------------------------------------------------------
// GET /api/scheduled-tasks/:id — get a single scheduled task
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/scheduled-tasks/{id}",
    params(("id" = Uuid, Path, description = "Scheduled task ID")),
    responses(
        (status = 200, description = "Scheduled task", body = Object),
        (status = 404, description = "Task not found"),
    ),
    security(("api_key" = [])),
    tag = "Schedules",
)]
pub(crate) async fn get_schedule(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<ScheduledTask>>, AppError> {
    let task = state
        .store
        .get_scheduled_task(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Scheduled task"))?;
    Ok(ApiResponse::of(task))
}

// ---------------------------------------------------------------------------
// PATCH /api/scheduled-tasks/:id — update a scheduled task (enable/disable)
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateScheduleRequest {
    pub enabled: bool,
}

#[utoipa::path(
    patch,
    path = "/api/scheduled-tasks/{id}",
    params(("id" = Uuid, Path, description = "Scheduled task ID")),
    request_body = UpdateScheduleRequest,
    responses(
        (status = 204, description = "Schedule updated"),
        (status = 403, description = "Caller does not own the task"),
        (status = 404, description = "Task not found"),
    ),
    security(("api_key" = [])),
    tag = "Schedules",
)]
pub(crate) async fn update_schedule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;

    let task = state
        .store
        .get_scheduled_task(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Scheduled task"))?;
    principal.require_owner(&task.created_by, "scheduled task")?;

    state
        .store
        .update_scheduled_task_enabled(id, req.enabled)
        .await
        .map_err(AppError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/scheduled-tasks/:id — delete a scheduled task
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/scheduled-tasks/{id}",
    params(("id" = Uuid, Path, description = "Scheduled task ID")),
    responses(
        (status = 204, description = "Schedule deleted"),
        (status = 403, description = "Caller does not own the task"),
        (status = 404, description = "Task not found"),
    ),
    security(("api_key" = [])),
    tag = "Schedules",
)]
pub(crate) async fn delete_schedule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;

    let task = state
        .store
        .get_scheduled_task(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Scheduled task"))?;
    principal.require_owner(&task.created_by, "scheduled task")?;

    let deleted = state
        .store
        .delete_scheduled_task(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Scheduled task"))
    }
}

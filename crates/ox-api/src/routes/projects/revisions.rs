use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use tracing::warn;
use uuid::Uuid;

use ox_core::{OntologyDiff, OntologyIR, compute_diff};
use ox_store::{DesignProject, OntologySnapshot, OntologySnapshotSummary};

use crate::error::AppError;
use crate::principal::Principal;
use crate::routes::projects::helpers::{load_mutable_project, reload_project};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/projects/:id/revisions — list ontology revision history
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/projects/{id}/revisions",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "List of ontology revision snapshots", body = Object),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub async fn list_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<OntologySnapshotSummary>>, AppError> {
    // Verify project exists
    let _ = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let snapshots = state
        .store
        .list_ontology_snapshots(id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(snapshots))
}

// ---------------------------------------------------------------------------
// GET /api/projects/:id/revisions/:rev — get a specific revision snapshot
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/projects/{id}/revisions/{rev}",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev" = i32, Path, description = "Revision number"),
    ),
    responses(
        (status = 200, description = "Ontology revision snapshot", body = Object),
        (status = 404, description = "Revision not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub async fn get_revision(
    State(state): State<AppState>,
    Path((id, rev)): Path<(Uuid, i32)>,
) -> Result<Json<OntologySnapshot>, AppError> {
    let snapshot = state
        .store
        .get_ontology_snapshot(id, rev)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    Ok(Json(snapshot))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/revisions/:rev/restore — restore a previous revision
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectRestoreResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
}

#[utoipa::path(
    post,
    path = "/api/projects/{id}/revisions/{rev}/restore",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev" = i32, Path, description = "Revision number to restore"),
    ),
    responses(
        (status = 200, description = "Revision restored", body = ProjectRestoreResponse),
        (status = 404, description = "Project or revision not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub async fn restore_revision(
    State(state): State<AppState>,
    principal: Principal,
    Path((id, rev)): Path<(Uuid, i32)>,
) -> Result<Json<ProjectRestoreResponse>, AppError> {
    principal.require_designer()?;
    let project = load_mutable_project(&state, id).await?;

    // Snapshot current state before restore (best-effort)
    if let Some(ont) = &project.ontology
        && let Err(e) = state
            .store
            .create_ontology_snapshot(
                id,
                project.revision,
                ont,
                project.source_mapping.as_ref(),
                project.quality_report.as_ref(),
            )
            .await
    {
        warn!(project_id = %id, error = %e, "Failed to save ontology snapshot before restore");
    }

    let snapshot = state
        .store
        .get_ontology_snapshot(id, rev)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    state
        .store
        .update_design_result(
            id,
            &snapshot.ontology,
            snapshot.source_mapping.as_ref(),
            snapshot.quality_report.as_ref(),
            project.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok(Json(ProjectRestoreResponse { project: updated }))
}

// ---------------------------------------------------------------------------
// GET /api/projects/:id/revisions/:rev1/diff/:rev2 — diff between two revisions
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/projects/{id}/revisions/{rev1}/diff/{rev2}",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev1" = i32, Path, description = "Base revision number"),
        ("rev2" = i32, Path, description = "Target revision number"),
    ),
    responses(
        (status = 200, description = "Ontology diff between two revisions", body = Object),
        (status = 404, description = "Revision not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub async fn diff_revisions(
    State(state): State<AppState>,
    Path((id, rev1, rev2)): Path<(Uuid, i32, i32)>,
) -> Result<Json<OntologyDiff>, AppError> {
    let snap1 = state
        .store
        .get_ontology_snapshot(id, rev1)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let snap2 = state
        .store
        .get_ontology_snapshot(id, rev2)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let old: OntologyIR = serde_json::from_value(snap1.ontology).map_err(|e| {
        AppError::internal(format!("Failed to parse revision {rev1} ontology: {e}"))
    })?;
    let new: OntologyIR = serde_json::from_value(snap2.ontology).map_err(|e| {
        AppError::internal(format!("Failed to parse revision {rev2} ontology: {e}"))
    })?;

    Ok(Json(compute_diff(&old, &new)))
}

// ---------------------------------------------------------------------------
// GET /api/projects/:id/diff/current — diff current ontology vs latest snapshot
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/projects/{id}/diff/current",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Diff between current ontology and latest snapshot", body = Object),
        (status = 400, description = "Project has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found or no snapshots", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub async fn diff_current(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OntologyDiff>, AppError> {
    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let current_ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let current: OntologyIR = serde_json::from_value(current_ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse current ontology: {e}")))?;

    let snapshots = state
        .store
        .list_ontology_snapshots(id)
        .await
        .map_err(AppError::from)?;

    let latest = snapshots
        .first()
        .ok_or_else(|| AppError::not_found("No revision snapshots exist for this project"))?;

    let snapshot = state
        .store
        .get_ontology_snapshot(id, latest.revision)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let old: OntologyIR = serde_json::from_value(snapshot.ontology)
        .map_err(|e| AppError::internal(format!("Failed to parse snapshot ontology: {e}")))?;

    Ok(Json(compute_diff(&old, &current)))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/revisions/:rev/migrate — migrate schema between revisions
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ProjectMigrateRequest {
    /// If true, return migration plan without executing it
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectMigrateResponse {
    /// Forward DDL statements
    pub up: Vec<String>,
    /// Rollback DDL statements
    pub down: Vec<String>,
    /// Non-breaking warnings
    pub warnings: Vec<String>,
    /// Breaking changes requiring confirmation
    pub breaking_changes: Vec<String>,
    /// Whether the migration was executed
    pub executed: bool,
}

#[utoipa::path(
    post,
    path = "/api/projects/{id}/revisions/{rev}/migrate",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev" = i32, Path, description = "Base revision (deployed state) — migration goes FROM this revision TO current ontology"),
    ),
    request_body = ProjectMigrateRequest,
    responses(
        (status = 200, description = "Migration plan or execution result", body = ProjectMigrateResponse),
        (status = 400, description = "No ontology in project or revision", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project or revision not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub async fn migrate_schema(
    State(state): State<AppState>,
    principal: Principal,
    Path((id, rev)): Path<(Uuid, i32)>,
    Json(req): Json<ProjectMigrateRequest>,
) -> Result<Json<ProjectMigrateResponse>, AppError> {
    principal.require_designer()?;

    // Load current project ontology
    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let current_ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let current: OntologyIR = serde_json::from_value(current_ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse current ontology: {e}")))?;

    // Load target revision ontology
    let snapshot = state
        .store
        .get_ontology_snapshot(id, rev)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let old: OntologyIR = serde_json::from_value(snapshot.ontology)
        .map_err(|e| AppError::internal(format!("Failed to parse revision {rev} ontology: {e}")))?;

    // Compute diff (old revision → current)
    let diff = compute_diff(&old, &current);

    if diff.is_empty() {
        return Ok(Json(ProjectMigrateResponse {
            up: vec![],
            down: vec![],
            warnings: vec![],
            breaking_changes: vec![],
            executed: false,
        }));
    }

    // Compile migration plan
    let plan = ox_compiler::cypher::migration::compile_migration(&diff, &old, &current);

    if req.dry_run || !plan.breaking_changes.is_empty() {
        return Ok(Json(ProjectMigrateResponse {
            up: plan.up,
            down: plan.down,
            warnings: plan.warnings,
            breaking_changes: plan.breaking_changes,
            executed: false,
        }));
    }

    // Nothing to execute if up is empty (diff only produced warnings)
    if plan.up.is_empty() {
        return Ok(Json(ProjectMigrateResponse {
            up: plan.up,
            down: plan.down,
            warnings: plan.warnings,
            breaking_changes: plan.breaking_changes,
            executed: false,
        }));
    }

    // Execute forward migration
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    runtime
        .execute_schema(&plan.up)
        .await
        .map_err(AppError::from)?;

    tracing::info!(
        project_id = %id,
        from_revision = rev,
        statements = plan.up.len(),
        "Schema migration executed"
    );

    Ok(Json(ProjectMigrateResponse {
        up: plan.up,
        down: plan.down,
        warnings: plan.warnings,
        breaking_changes: plan.breaking_changes,
        executed: true,
    }))
}

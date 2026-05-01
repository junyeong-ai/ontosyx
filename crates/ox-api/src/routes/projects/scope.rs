use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_ontology::ir::OntologyIR;
use ox_source::AnalysisScope;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

use super::helpers::{load_mutable_project, reload_project};
use super::types::ProjectView;

// ---------------------------------------------------------------------------
// POST /api/projects/:id/scope/include
// POST /api/projects/:id/scope/defer
//
// Per-table promotion / demotion of `AnalysisScope` entries. The
// staged-bootstrap flow (`AnalyzeSelection::Staged`) populates
// `deferred` with every unpicked table; these endpoints close the
// loop so the operator can promote a deferred table into `included`
// or move a modeled-but-not-needed table back to deferred — without
// re-running introspection or LLM design.
//
// Both routes update only `analysis_scope` (no ontology / source
// schema / profile churn). Optimistic CAS on `revision` matches the
// rest of the project mutation surface.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct IncludeScopeTablesRequest {
    /// Tables to promote from `deferred` (or first-time-seen) into
    /// `included`. Names must already appear in the project's last
    /// introspection — this endpoint does not introspect; it
    /// reclassifies.
    pub tables: Vec<String>,
    pub expected_revision: i32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeferScopeTablesRequest {
    /// Tables to move from `included` to `deferred`.
    pub tables: Vec<String>,
    /// Why the operator is deferring — surfaced in the FE deferred
    /// tab next to the timestamp.
    pub reason: String,
    pub expected_revision: i32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ScopeUpdateResponse {
    pub project: ProjectView,
}

/// Promote tables from deferred (or first-time) into included.
#[utoipa::path(
    post,
    path = "/api/projects/{id}/scope/include",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = IncludeScopeTablesRequest,
    responses(
        (status = 200, description = "Scope updated", body = ScopeUpdateResponse),
        (status = 400, description = "Empty tables list", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Revision mismatch", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn include_scope_tables(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<IncludeScopeTablesRequest>,
) -> Result<Json<ApiResponse<ScopeUpdateResponse>>, AppError> {
    principal.require_designer()?;
    if req.tables.is_empty() {
        return Err(AppError::bad_request(
            "scope/include requires at least one table",
        ));
    }

    let project = load_mutable_project(&state, id).await?;
    let mut scope: AnalysisScope =
        serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default();

    // Drive promotion through `record_selection(Subset)` so the
    // include / clear-deferred logic is the same code path the
    // analyze flow uses. The empty all-tables set is correct — we
    // only need the helper's Subset arm, which ignores it.
    let selection = ox_source::AnalyzeSelection::Subset {
        tables: req.tables.iter().cloned().collect::<BTreeSet<_>>(),
    };
    scope.record_selection(&selection, &BTreeSet::new(), chrono::Utc::now());

    let scope_json = AppError::to_json(&scope)?;
    state
        .store
        .update_analysis_scope(id, &scope_json, req.expected_revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;
    Ok(ApiResponse::of(ScopeUpdateResponse {
        project: ProjectView::from_project(updated),
    }))
}

/// Move tables from included to deferred. Rejected when the
/// project's ontology already binds a NodeType to a table (the
/// caller must retract those nodes first via `Reduce` / edit ops).
#[utoipa::path(
    post,
    path = "/api/projects/{id}/scope/defer",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = DeferScopeTablesRequest,
    responses(
        (status = 200, description = "Scope updated", body = ScopeUpdateResponse),
        (status = 400, description = "Empty tables list / empty reason", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Table is currently modeled / revision mismatch", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn defer_scope_tables(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<DeferScopeTablesRequest>,
) -> Result<Json<ApiResponse<ScopeUpdateResponse>>, AppError> {
    principal.require_designer()?;
    if req.tables.is_empty() {
        return Err(AppError::bad_request(
            "scope/defer requires at least one table",
        ));
    }
    if req.reason.trim().is_empty() {
        return Err(AppError::bad_request(
            "scope/defer requires a non-empty reason",
        ));
    }

    let project = load_mutable_project(&state, id).await?;

    // If the ontology models any of these tables, refuse — moving
    // them to `deferred` while NodeType / mappings still bind to
    // them would leave the project in a state where the FE shows
    // the table as not-modeled but the canvas still has the node.
    // The caller must retract via `Reduce` / edit ops first.
    if let Some(ontology_json) = project.ontology.as_ref() {
        let ontology: OntologyIR = serde_json::from_value(ontology_json.clone())
            .map_err(|e| AppError::internal(format!("Corrupt ontology: {e}")))?;
        let modeled: BTreeSet<&str> = ontology
            .object_mappings()
            .iter()
            .map(|om| om.relation.as_str())
            .collect();
        let blocked: Vec<&String> = req
            .tables
            .iter()
            .filter(|t| modeled.contains(t.as_str()))
            .collect();
        if !blocked.is_empty() {
            return Err(AppError::conflict(format!(
                "cannot defer modeled tables: {} — retract the bound NodeType first",
                blocked
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    let mut scope: AnalysisScope =
        serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default();
    let now = chrono::Utc::now();
    for table in &req.tables {
        scope.included.remove(table);
        // Replace any prior deferral entry so the new reason +
        // timestamp win — operator may be re-deferring with fresh
        // context.
        scope.deferred.retain(|d| &d.table != table);
        scope.deferred.push(ox_source::DeferredTable {
            table: table.clone(),
            reason: req.reason.clone(),
            deferred_at: now,
            revisit_at: None,
        });
    }

    let scope_json = AppError::to_json(&scope)?;
    state
        .store
        .update_analysis_scope(id, &scope_json, req.expected_revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;
    Ok(ApiResponse::of(ScopeUpdateResponse {
        project: ProjectView::from_project(updated),
    }))
}

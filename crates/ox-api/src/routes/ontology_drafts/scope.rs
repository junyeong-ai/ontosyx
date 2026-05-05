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

use super::helpers::{load_mutable_ontology_draft, reload_ontology_draft};
use super::types::OntologyDraftView;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct IncludeScopeTablesRequest {
    pub tables: Vec<String>,
    pub expected_revision: i32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeferScopeTablesRequest {
    pub tables: Vec<String>,
    pub reason: String,
    pub expected_revision: i32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ScopeUpdateResponse {
    pub project: OntologyDraftView,
}

/// Promote tables from deferred (or first-time) into included.
#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/scope/include",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = IncludeScopeTablesRequest,
    responses(
        (status = 200, description = "Scope updated", body = ScopeUpdateResponse),
        (status = 400, description = "Empty tables list", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Revision mismatch", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn include_scope_tables(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<IncludeScopeTablesRequest>,
) -> Result<Json<ApiResponse<ScopeUpdateResponse>>, AppError> {
    principal.require_designer()?;
    if req.tables.is_empty() {
        return Err(AppError::required_field_empty("tables"));
    }

    let project = load_mutable_ontology_draft(&state, id).await?;
    let mut scope: AnalysisScope =
        serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default();

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

    let updated = reload_ontology_draft(&state, id).await?;
    Ok(ApiResponse::of(ScopeUpdateResponse {
        project: OntologyDraftView::from_ontology_draft(updated),
    }))
}

/// Move tables from included to deferred. Rejected with 409 when
/// the project's ontology already binds a NodeType to a target
/// table — the caller must retract those nodes first.
#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/scope/defer",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = DeferScopeTablesRequest,
    responses(
        (status = 200, description = "Scope updated", body = ScopeUpdateResponse),
        (status = 400, description = "Empty tables list / empty reason", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Table is currently modeled / revision mismatch", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn defer_scope_tables(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<DeferScopeTablesRequest>,
) -> Result<Json<ApiResponse<ScopeUpdateResponse>>, AppError> {
    principal.require_designer()?;
    if req.tables.is_empty() {
        return Err(AppError::required_field_empty("tables"));
    }
    if req.reason.trim().is_empty() {
        return Err(AppError::required_field_empty("reason"));
    }

    let project = load_mutable_ontology_draft(&state, id).await?;

    if let Some(ontology_json) = project.ontology.as_ref() {
        let ontology: OntologyIR = serde_json::from_value(ontology_json.clone())
            .map_err(|e| AppError::internal(format!("deserialize ontology: {e}")))?;
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
            let blocked_list: Vec<&str> = blocked.iter().map(|s| s.as_str()).collect();
            return Err(AppError::scope_defer_modeled_tables(&blocked_list));
        }
    }

    let mut scope: AnalysisScope =
        serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default();
    let now = chrono::Utc::now();
    for table in &req.tables {
        scope.included.remove(table);
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

    let updated = reload_ontology_draft(&state, id).await?;
    Ok(ApiResponse::of(ScopeUpdateResponse {
        project: OntologyDraftView::from_ontology_draft(updated),
    }))
}

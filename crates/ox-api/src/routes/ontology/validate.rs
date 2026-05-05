//! `GET /api/ontology/validate` — return the structural diagnostics
//! produced by [`OntologyIR::validate`] over the workspace's
//! current-version IR.
//!
//! The endpoint exposes the same vector the `/edits` apply-path
//! uses to reject invalid mutations, so admin forms can surface
//! the same dangling-reference warnings inline (rule mentions a
//! missing GlossaryTerm, mapping references a missing NodeType,
//! …) without waiting for a save attempt to fail.
//!
//! Wire shape: `Vec<DiagnosticMessage>` — empty on a validating
//! snapshot. Each diagnostic carries a stable `code` and `params`
//! map; the FE i18n catalogue owns the locale.

use axum::Json;
use axum::extract::State;

use ox_core::diagnostic::DiagnosticMessage;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/ontology/validate",
    responses(
        (status = 200, description = "Structural validation diagnostics for the current IR", body = Vec<DiagnosticMessage>),
        (status = 404, description = "Workspace has no ontology, or the ontology has no committed version"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn get_ontology_validate(
    State(state): State<AppState>,
    _principal: Principal,
) -> Result<Json<ApiResponse<Vec<DiagnosticMessage>>>, AppError> {
    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let current = state
        .store
        .find_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ontology has no committed version"))?;
    let ir = state
        .store
        .get_ontology_ir(current.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;

    Ok(ApiResponse::of(ir.validate()))
}

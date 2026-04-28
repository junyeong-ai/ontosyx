//! `GET /api/ontologies/{id}/dependencies` — return the schema-level
//! [`SchemaDependencyGraph`] of the current-version IR.
//!
//! Distinct from `cross-refs` (the Complete Map's six-axis flow):
//! the dependency graph is the inverted reference index used by the
//! editor's Inspector ("which entities depend on this property?")
//! and the standalone `/dependencies/{ref}` impact-analysis view.
//!
//! The endpoint serialises the entire graph in a single response
//! because (a) it's small (entity-count × ~5 references on
//! average), (b) the FE caches and re-queries by client-side
//! lookup, and (c) deltas are produced server-side on commit
//! anyway. A future scoped endpoint can land if entity-by-entity
//! pagination becomes meaningful.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use ox_ontology::SchemaDependencyGraph;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/ontologies/{id}/dependencies",
    params(("id" = Uuid, Path, description = "Ontology identity id")),
    responses(
        (status = 200, description = "Schema dependency graph", body = Object),
        (status = 404, description = "Ontology not found or has no committed version"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn get_ontology_dependencies(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SchemaDependencyGraph>>, AppError> {
    let identity = state
        .store
        .get_ontology(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let current = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ontology has no committed version"))?;
    let ir = state
        .store
        .get_ontology_ir(current.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;

    Ok(ApiResponse::of(SchemaDependencyGraph::build(&ir)))
}

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::ElementVerification;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct VerifyElementRequest {
    pub element_id: String,
    pub element_kind: String,
    pub review_notes: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct VerifyElementResponse {
    pub id: Uuid,
}

/// POST /api/ontologies/{id}/verifications — mark an element as verified
#[utoipa::path(
    post,
    path = "/api/ontologies/{id}/verifications",
    params(("id" = String, Path, description = "Ontology lineage ID")),
    request_body = VerifyElementRequest,
    responses(
        (status = 200, description = "Verification recorded", body = VerifyElementResponse),
        (status = 400, description = "Invalid element_kind"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn verify_element(
    State(state): State<AppState>,
    principal: Principal,
    Path(ontology_lineage_id): Path<String>,
    Json(req): Json<VerifyElementRequest>,
) -> Result<Json<ApiResponse<VerifyElementResponse>>, AppError> {
    if !matches!(req.element_kind.as_str(), "node" | "edge" | "property") {
        return Err(AppError::invalid_enum_value(
            "element_kind",
            req.element_kind.clone(),
            &["node", "edge", "property"],
        ));
    }

    let user = state
        .store
        .get_user_by_provider("ontosyx", &principal.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    let verification = ElementVerification {
        id: Uuid::new_v4(),
        ontology_lineage_id: ontology_lineage_id.clone(),
        element_id: req.element_id,
        element_kind: req.element_kind,
        verified_by: user.id,
        verified_by_name: None,
        review_notes: req.review_notes,
        invalidated_at: None,
        invalidation_reason: None,
        created_at: chrono::Utc::now(),
    };

    let id = state
        .store
        .verify_element(&verification)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(VerifyElementResponse { id }))
}

/// GET /api/ontologies/{id}/verifications — list active verifications
#[utoipa::path(
    get,
    path = "/api/ontologies/{id}/verifications",
    params(("id" = String, Path, description = "Ontology lineage ID")),
    responses((status = 200, description = "Active verifications", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn list_verifications(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_lineage_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<ElementVerification>>>, AppError> {
    let verifications = state
        .store
        .list_verifications(&ontology_lineage_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(verifications))
}

/// DELETE /api/ontologies/{id}/verifications/{element_id} — revoke verification
#[utoipa::path(
    delete,
    path = "/api/ontologies/{id}/verifications/{element_id}",
    params(
        ("id" = String, Path, description = "Ontology lineage ID"),
        ("element_id" = String, Path, description = "Element ID"),
    ),
    responses((status = 204, description = "Verification revoked")),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn delete_verification(
    State(state): State<AppState>,
    principal: Principal,
    Path((ontology_lineage_id, element_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let user = state
        .store
        .get_user_by_provider("ontosyx", &principal.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    state
        .store
        .delete_verification(&ontology_lineage_id, &element_id, user.id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

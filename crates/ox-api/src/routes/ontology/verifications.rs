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

#[derive(Deserialize)]
pub struct VerifyElementRequest {
    pub element_id: String,
    pub element_kind: String,
    pub review_notes: Option<String>,
}

/// POST /api/ontology/{id}/verifications — mark an element as verified
pub(crate) async fn verify_element(
    State(state): State<AppState>,
    principal: Principal,
    Path(ontology_id): Path<String>,
    Json(req): Json<VerifyElementRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if !matches!(req.element_kind.as_str(), "node" | "edge" | "property") {
        return Err(AppError::bad_request(
            "element_kind must be 'node', 'edge', or 'property'",
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
        ontology_id: ontology_id.clone(),
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

    Ok(ApiResponse::of(serde_json::json!({ "id": id })))
}

/// GET /api/ontology/{id}/verifications — list active verifications
pub(crate) async fn list_verifications(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<ElementVerification>>>, AppError> {
    let verifications = state
        .store
        .get_verifications(&ontology_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(verifications))
}

/// DELETE /api/ontology/{id}/verifications/{element_id} — revoke verification
pub(crate) async fn delete_verification(
    State(state): State<AppState>,
    principal: Principal,
    Path((ontology_id, element_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let user = state
        .store
        .get_user_by_provider("ontosyx", &principal.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    state
        .store
        .delete_verification(&ontology_id, &element_id, user.id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

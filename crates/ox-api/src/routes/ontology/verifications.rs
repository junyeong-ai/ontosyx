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

/// Resolve the workspace ontology's lineage id — the stable handle
/// the verification table indexes on. Errors with 404 when the
/// workspace has no ontology.
async fn workspace_lineage_id(state: &AppState) -> Result<String, AppError> {
    state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .map(|o| o.lineage_id)
        .ok_or_else(|| AppError::not_found("Ontology"))
}

/// POST /api/ontology/verifications — mark an element as verified
#[utoipa::path(
    post,
    path = "/api/ontology/verifications",
    request_body = VerifyElementRequest,
    responses(
        (status = 200, description = "Verification recorded", body = VerifyElementResponse),
        (status = 400, description = "Invalid element_kind"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn verify_element(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<VerifyElementRequest>,
) -> Result<Json<ApiResponse<VerifyElementResponse>>, AppError> {
    if !matches!(req.element_kind.as_str(), "node" | "edge" | "property") {
        return Err(AppError::invalid_enum_value(
            "element_kind",
            req.element_kind.clone(),
            &["node", "edge", "property"],
        ));
    }

    let ontology_lineage_id = workspace_lineage_id(&state).await?;

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

/// GET /api/ontology/verifications — list active verifications
#[utoipa::path(
    get,
    path = "/api/ontology/verifications",
    responses((status = 200, description = "Active verifications", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn list_verifications(
    State(state): State<AppState>,
    _principal: Principal,
) -> Result<Json<ApiResponse<Vec<ElementVerification>>>, AppError> {
    let ontology_lineage_id = workspace_lineage_id(&state).await?;
    let verifications = state
        .store
        .list_verifications(&ontology_lineage_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(verifications))
}

/// DELETE /api/ontology/verifications/{element_id} — revoke verification
#[utoipa::path(
    delete,
    path = "/api/ontology/verifications/{element_id}",
    params(
        ("element_id" = String, Path, description = "Element ID"),
    ),
    responses((status = 204, description = "Verification revoked")),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn delete_verification(
    State(state): State<AppState>,
    principal: Principal,
    Path(element_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let ontology_lineage_id = workspace_lineage_id(&state).await?;
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

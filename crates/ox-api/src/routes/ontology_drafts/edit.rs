use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use tokio::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

use ox_ontology::ir::OntologyIR;

use ox_brain::model_resolver::operation;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

use super::helpers::{
    assess_quality_from_ontology_draft, get_design_options, load_mutable_ontology_draft,
    reload_ontology_draft,
};
use super::types::{EditOntologyDraftRequest, EditOntologyDraftResponse, OntologyDraftView};

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/edit
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/edit",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = EditOntologyDraftRequest,
    responses(
        (status = 200, description = "Edit commands generated and optionally applied", body = EditOntologyDraftResponse),
        (status = 400, description = "Empty request or no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Command validation failed", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "LLM timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn edit_ontology_draft(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<EditOntologyDraftRequest>,
) -> Result<Json<ApiResponse<EditOntologyDraftResponse>>, AppError> {
    principal.require_designer()?;
    // Validate input
    if req.user_request.trim().is_empty() {
        return Err(AppError::required_field_empty("user_request"));
    }

    let project = load_mutable_ontology_draft(&state, id).await?;

    // Ontology draft must have an ontology (status "designed", or "analyzed" with ontology)
    let ontology: OntologyIR = match project.ontology.as_ref() {
        None => return Err(AppError::no_ontology()),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| AppError::internal(format!("Corrupt ontology in project: {e}")))?,
    };

    // Generate edit commands via Brain
    let timeout =
        std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
    let edit_started = Instant::now();
    info!(ontology_draft_id = %id, "Generating edit commands");

    let edit_output = tokio::time::timeout(
        timeout,
        state.brain.generate_edit_commands(
            &ontology,
            &req.user_request,
            &entelix::ExecutionContext::default(),
        ),
    )
    .await
    .map_err(|_| {
        warn!(
            ontology_draft_id = %id,
            elapsed_ms = edit_started.elapsed().as_millis() as u64,
            timeout_secs = timeout.as_secs(),
            "Edit command generation timed out"
        );
        AppError::timeout(format!(
            "Edit command generation timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(AppError::from)?;

    let edit_duration_ms = edit_started.elapsed().as_millis() as i64;
    info!(
        ontology_draft_id = %id,
        edit_ms = edit_duration_ms,
        command_count = edit_output.commands.len(),
        "Edit commands generated"
    );

    // Record metering (fire-and-forget)
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        let meter_provider = edit_output.provider.clone();
        let meter_model = edit_output.model.clone();
        ox_context::spawn_scoped(async move {
            if let Err(error) = meter_store
                .record_usage(
                    meter_user,
                    "llm",
                    Some(&meter_provider),
                    Some(&meter_model),
                    Some(operation::EDIT_ONTOLOGY),
                    0,
                    0,
                    edit_duration_ms,
                    0.0,
                    serde_json::json!({}),
                )
                .await
            {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    if edit_output.commands.is_empty() {
        return Ok(ApiResponse::of(EditOntologyDraftResponse {
            project: Some(OntologyDraftView::from_ontology_draft(project)),
            commands: vec![],
            explanation: edit_output.explanation,
        }));
    }

    // Validate commands by executing them sequentially on a clone
    let mut validated_ontology = ontology.clone();
    for (i, cmd) in edit_output.commands.iter().enumerate() {
        match cmd.execute(&validated_ontology) {
            Ok(result) => validated_ontology = result.new_ontology,
            Err(e) => {
                return Err(AppError::edit_operation_rejected(format!(
                    "commands[{}]: {e}",
                    i + 1
                )));
            }
        }
    }

    if req.dry_run {
        return Ok(ApiResponse::of(EditOntologyDraftResponse {
            project: None,
            commands: edit_output.commands,
            explanation: edit_output.explanation,
        }));
    }

    // Snapshot current state before mutation (best-effort)
    if let Some(ont) = &project.ontology
        && let Err(e) = state
            .store
            .create_ontology_snapshot(id, project.revision, ont, project.quality_report.as_ref())
            .await
    {
        warn!(ontology_draft_id = %id, error = %e, "Failed to save ontology snapshot");
    }

    // Apply: save updated ontology with quality re-assessment
    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_ontology_draft(
        &project,
        &validated_ontology,
        &opts.excluded_tables,
        &opts.column_clarifications,
    )?;

    let ontology_json = AppError::to_json(&validated_ontology)?;
    let qr_json = AppError::to_json(&quality_report)?;

    state
        .store
        .update_design_result(id, &ontology_json, Some(&qr_json), req.revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_ontology_draft(&state, id).await?;

    info!(
        ontology_draft_id = %id,
        total_ms = edit_started.elapsed().as_millis() as u64,
        "Edit completed"
    );

    Ok(ApiResponse::of(EditOntologyDraftResponse {
        project: Some(OntologyDraftView::from_ontology_draft(updated)),
        commands: edit_output.commands,
        explanation: edit_output.explanation,
    }))
}

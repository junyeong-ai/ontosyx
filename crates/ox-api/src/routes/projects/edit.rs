use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use tokio::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

use ox_core::ontology_ir::OntologyIR;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

use super::helpers::{assess_quality_from_project, get_design_options, load_mutable_project, reload_project};
use super::types::{ProjectEditRequest, ProjectEditResponse};

// ---------------------------------------------------------------------------
// POST /api/projects/:id/edit
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/edit",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectEditRequest,
    responses(
        (status = 200, description = "Edit commands generated and optionally applied", body = ProjectEditResponse),
        (status = 400, description = "Empty request or no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Command validation failed", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "LLM timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn edit_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectEditRequest>,
) -> Result<Json<ProjectEditResponse>, AppError> {
    principal.require_designer()?;
    // Validate input
    if req.user_request.trim().is_empty() {
        return Err(AppError::bad_request("user_request must not be empty"));
    }

    let project = load_mutable_project(&state, id).await?;

    // Project must have an ontology (status "designed", or "analyzed" with ontology)
    let ontology: OntologyIR = match project.ontology.as_ref() {
        None => return Err(AppError::no_ontology()),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
            AppError::internal(format!("Corrupt ontology in project: {e}"))
        })?,
    };

    // Generate edit commands via Brain
    let timeout = std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
    let edit_started = Instant::now();
    info!(project_id = %id, "Generating edit commands");

    let edit_output = tokio::time::timeout(
        timeout,
        state
            .brain
            .generate_edit_commands(&ontology, &req.user_request),
    )
    .await
    .map_err(|_| {
        warn!(
            project_id = %id,
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
        project_id = %id,
        edit_ms = edit_duration_ms,
        command_count = edit_output.commands.len(),
        "Edit commands generated"
    );

    // Record metering (fire-and-forget)
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        crate::spawn_scoped::spawn_scoped(async move {
            let _ = meter_store.record_usage(
                meter_user,
                "llm",
                Some("anthropic"),
                None,
                Some("edit"),
                0,
                0,
                edit_duration_ms,
                0.0,
                serde_json::json!({}),
            ).await;
        });
    }

    if edit_output.commands.is_empty() {
        return Ok(Json(ProjectEditResponse {
            project: Some(project),
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
                return Err(AppError::unprocessable(format!(
                    "Command {} failed validation: {e}",
                    i + 1
                )));
            }
        }
    }

    if req.dry_run {
        return Ok(Json(ProjectEditResponse {
            project: None,
            commands: edit_output.commands,
            explanation: edit_output.explanation,
        }));
    }

    // Snapshot current state before mutation (best-effort)
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
        warn!(project_id = %id, error = %e, "Failed to save ontology snapshot");
    }

    // Apply: save updated ontology with quality re-assessment
    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_project(
        &project,
        &validated_ontology,
        &opts.excluded_tables,
        &opts.column_clarifications,
    )?;

    let ontology_json = AppError::to_json(&validated_ontology)?;
    let qr_json = AppError::to_json(&quality_report)?;

    state
        .store
        .update_design_result(
            id,
            &ontology_json,
            project.source_mapping.as_ref(),
            Some(&qr_json),
            req.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    info!(
        project_id = %id,
        total_ms = edit_started.elapsed().as_millis() as u64,
        "Edit completed"
    );

    Ok(Json(ProjectEditResponse {
        project: Some(updated),
        commands: edit_output.commands,
        explanation: edit_output.explanation,
    }))
}

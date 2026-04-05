use axum::Json;
use axum::extract::{Path, State};
use tracing::warn;
use uuid::Uuid;

use ox_core::design_project::{SourceConfig, SourceTypeKind};
use ox_store::store::AnalysisSnapshot;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

use super::helpers::{
    analyze_code_repository, analyze_source, get_design_options, load_mutable_project,
    prune_decisions, reload_project, run_repo_enrichment, skipped_repo_summary,
};
use super::types::{ProjectReanalyzeRequest, ProjectReanalyzeResponse, ProjectSource};

// ---------------------------------------------------------------------------
// POST /api/projects/:id/reanalyze
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/reanalyze",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectReanalyzeRequest,
    responses(
        (status = 200, description = "Source re-analyzed", body = ProjectReanalyzeResponse),
        (status = 400, description = "Source type mismatch", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn reanalyze_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectReanalyzeRequest>,
) -> Result<Json<ProjectReanalyzeResponse>, AppError> {
    principal.require_designer()?;
    let project = load_mutable_project(&state, id).await?;

    let stored_config: SourceConfig = serde_json::from_value(project.source_config.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt source_config: {e}")))?;

    // Validate source type matches
    let new_source_type = match &req.source {
        ProjectSource::Text { .. } => SourceTypeKind::Text,
        ProjectSource::Csv { .. } => SourceTypeKind::Csv,
        ProjectSource::Json { .. } => SourceTypeKind::Json,
        ProjectSource::Postgresql { .. } => SourceTypeKind::Postgresql,
        ProjectSource::Mysql { .. } => SourceTypeKind::Mysql,
        ProjectSource::Mongodb { .. } => SourceTypeKind::Mongodb,
        ProjectSource::Snowflake { .. } => SourceTypeKind::Snowflake,
        ProjectSource::Bigquery { .. } => SourceTypeKind::Bigquery,
        ProjectSource::Duckdb { .. } => SourceTypeKind::DuckDb,
        ProjectSource::CodeRepository { .. } => SourceTypeKind::CodeRepository,
    };

    if new_source_type != stored_config.source_type {
        return Err(AppError::bad_request(format!(
            "Source type mismatch: project is '{}' but reanalyze got '{}'",
            stored_config.source_type, new_source_type
        )));
    }

    // Re-analyze (CodeRepository has a separate path requiring LLM calls)
    let (source_config, source_data, source_schema, source_profile, report) =
        if let ProjectSource::CodeRepository { url } = req.source {
            let (config, schema, profile, report) = analyze_code_repository(&state, &url).await?;
            (config, None, Some(schema), Some(profile), Some(report))
        } else {
            let (config, data, schema, profile, report) =
                analyze_source(req.source, &state.introspector_registry).await?;

            let mut report = report;

            // Optional repo enrichment (non-fatal — failures recorded in repo_summary)
            if let (Some(source), Some(rpt)) = (&req.repo_source, &mut report) {
                match source.validate(
                    &state.repo_policy.allowed_roots,
                    &state.repo_policy.allowed_git_hosts,
                ) {
                    Ok(validated) => run_repo_enrichment(&state, &validated, rpt).await,
                    Err(reason) => {
                        let reason = reason.to_string();
                        warn!(reason, "Repo enrichment skipped");
                        rpt.repo_summary = Some(skipped_repo_summary(reason));
                    }
                }
            }

            (config, data, schema, profile, report)
        };

    // Detect source identity change via fingerprint comparison
    let source_identity_changed = {
        let old_fp = stored_config.source_fingerprint.as_deref();
        let new_fp = source_config.source_fingerprint.as_deref();
        match (old_fp, new_fp) {
            (Some(a), Some(b)) => a != b,
            // No fingerprint on either side -> treat as potentially changed
            _ => true,
        }
    };

    // Prune invalidated decisions
    let old_opts = get_design_options(&project);
    let (pruned_opts, invalidated) =
        prune_decisions(old_opts, source_schema.as_ref(), source_identity_changed);

    // Persist
    let snapshot = AnalysisSnapshot {
        source_config: AppError::to_json(&source_config)?,
        source_data,
        source_schema: source_schema
            .map(|s| AppError::to_json(&s))
            .transpose()?
            .unwrap_or(serde_json::Value::Null),
        source_profile: source_profile
            .map(|p| AppError::to_json(&p))
            .transpose()?
            .unwrap_or(serde_json::Value::Null),
        analysis_report: report
            .map(|r| AppError::to_json(&r))
            .transpose()?
            .unwrap_or(serde_json::Value::Null),
        design_options: AppError::to_json(&pruned_opts)?,
    };

    state
        .store
        .replace_analysis_snapshot(id, &snapshot, req.revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok(Json(ProjectReanalyzeResponse {
        project: updated,
        invalidated_decisions: invalidated,
    }))
}

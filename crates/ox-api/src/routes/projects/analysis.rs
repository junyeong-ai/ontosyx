use axum::Json;
use axum::extract::{Path, State};
use tracing::warn;
use uuid::Uuid;

use ox_ontology::design_project::{SourceConfig, SourceTypeKind};
use ox_ontology::mapping::SourceId;
use ox_store::store::AnalysisSnapshot;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

use super::helpers::{
    analyze_code_repository, analyze_source, get_design_options, load_mutable_project,
    prune_decisions, reload_project, run_repo_enrichment, skipped_repo_summary,
};
use super::types::{ProjectSource, ProjectView, ReanalyzeProjectRequest, ReanalyzeProjectResponse};

// ---------------------------------------------------------------------------
// POST /api/projects/:id/reanalyze
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/reanalyze",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ReanalyzeProjectRequest,
    responses(
        (status = 200, description = "Source re-analyzed", body = ReanalyzeProjectResponse),
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
    Json(req): Json<ReanalyzeProjectRequest>,
) -> Result<Json<ApiResponse<ReanalyzeProjectResponse>>, AppError> {
    principal.require_designer()?;
    req.selection.validate().map_err(AppError::from)?;
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
    let (source_config, source_data, source_schema, source_profile, mut report) =
        if let ProjectSource::CodeRepository { url } = req.source {
            let (config, schema, profile, report) = analyze_code_repository(&state, &url).await?;
            (config, None, Some(schema), Some(profile), Some(report))
        } else {
            let analyzed =
                analyze_source(req.source, &state.adapter_registry, req.selection.clone(), None).await?;
            let mut report = analyzed.report;

            // Optional repo enrichment (non-fatal — failures recorded in repo_summary)
            if let (Some(source), Some(rpt)) = (&req.repo_source, &mut report) {
                match source.validate(
                    &state.repo_policy.allowed_roots,
                    &state.repo_policy.allowed_git_hosts,
                ) {
                    Ok(validated) => run_repo_enrichment(&state, &validated, rpt).await,
                    Err(reason) => {
                        warn!(reason = %reason, "Repo enrichment skipped");
                        rpt.repo_summary = Some(skipped_repo_summary(
                            ox_ontology::source_analysis::RepoFailureKind::PolicyRejected,
                        ));
                    }
                }
            }

            (
                analyzed.config,
                analyzed.raw_data,
                analyzed.schema,
                analyzed.profile,
                report,
            )
        };

    // Eager drift detection: compare the new profile against the
    // existing ontology's value-set bindings. Skipped on source
    // identity change (different source = different bindings; no
    // meaningful drift comparison) and when the project has no
    // ontology yet.
    let existing_ontology: Option<ox_ontology::OntologyIR> = project
        .ontology
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    if let (Some(profile), Some(report), Some(ontology)) =
        (&source_profile, report.as_mut(), existing_ontology.as_ref())
    {
        let drift_warnings = ox_ontology::detect_value_set_drift(ontology, profile);
        if !drift_warnings.is_empty() {
            warn!(
                project_id = %id,
                drift_count = drift_warnings.len(),
                "Value-set drift detected — derived rules may silently reject samples"
            );
            report.analysis_warnings.extend(drift_warnings);
        }
    }

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

    // Pre-compute fresh per-table fingerprints once and reuse for
    // both drift detection and the rolled-forward scope. The
    // fingerprint is a SHA-256 over the canonical column shape; a
    // mismatch against the prior baseline means the source-side
    // table changed (column added / dropped / retyped, nullability
    // flipped) since the last analysis.
    let now = chrono::Utc::now();
    let fresh_fingerprints: std::collections::BTreeMap<String, String> = source_schema
        .as_ref()
        .map(|s| {
            s.tables
                .iter()
                .map(|t| {
                    (
                        t.name.clone(),
                        ox_core::source_schema::table_fingerprint(t),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // Eager schema-drift detection: when a same-source re-analysis
    // observes a fingerprint change for a previously-known table,
    // the derived ontology may now disagree with the live source
    // shape (column dropped, type changed, nullability flipped, or
    // the table itself disappeared). Surface as a warning so the
    // operator reviews mappings before the next deploy. Skipped on
    // source-identity change — different source ⇒ scope resets and
    // the prior fingerprints refer to a different physical source.
    if !source_identity_changed
        && !fresh_fingerprints.is_empty()
        && let Some(rpt) = report.as_mut()
    {
        let prior_scope: ox_source::AnalysisScope =
            serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default();
        let drift_warnings = prior_scope.detect_drift(&fresh_fingerprints);
        if !drift_warnings.is_empty() {
            warn!(
                project_id = %id,
                drift_count = drift_warnings.len(),
                "Table schema drift detected"
            );
            rpt.analysis_warnings.extend(drift_warnings);
        }
    }

    // Roll the project's analysis scope forward against the new
    // selection. A source identity change (different fingerprint)
    // resets the scope — the prior scope's `included` / `deferred`
    // refer to a different physical source and would mislead the
    // FE; same source folds the new selection into the prior scope
    // so deferred tables and history persist.
    let scope_json = {
        let mut scope = if source_identity_changed {
            ox_source::AnalysisScope::default()
        } else {
            serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default()
        };
        let all_tables: std::collections::BTreeSet<String> = source_schema
            .as_ref()
            .map(|s| s.tables.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default();
        scope.record_selection(&req.selection, &all_tables, now);
        scope.record_fingerprints(fresh_fingerprints);
        AppError::to_json(&scope)?
    };

    // Persist. `source_id` is recomputed from the (possibly new)
    // source_config via the canonical rule — when the fingerprint
    // shifts, downstream caches (federation plan-cache, ambiguity
    // detection) see a fresh id and invalidate naturally.
    let snapshot = AnalysisSnapshot {
        source_id: SourceId::from_source_config(&source_config).to_string(),
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
        analysis_scope: scope_json,
    };

    state
        .store
        .replace_analysis_snapshot(id, &snapshot, req.revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok(ApiResponse::of(ReanalyzeProjectResponse {
        project: ProjectView::from_project(updated),
        invalidated_decisions: invalidated,
    }))
}

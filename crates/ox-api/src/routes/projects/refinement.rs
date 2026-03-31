use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use tokio::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::validation::validate_ontology_input;
use ox_core::design_project::{DesignProjectStatus, SourceConfig};
use ox_core::ontology_ir::OntologyIR;
use ox_core::source_analysis::DesignOptions;
use ox_runtime::profiler;
use ox_source::analyzer::build_design_context;

use super::helpers::{
    LlmInputContext, assess_quality_from_project, assess_quality_from_project_with_mapping,
    build_llm_input, build_refinement_context, build_source_schema_summary, get_design_options,
    load_mutable_project, load_project_in_status, maybe_require_review, reload_project,
};
use super::types::{
    ProjectReconcileRequest, ProjectDesignRequest, ProjectDesignResponse, ProjectRefineRequest, ProjectRefineResponse,
};

// ---------------------------------------------------------------------------
// POST /api/projects/:id/design
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/design",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectDesignRequest,
    responses(
        (status = 200, description = "Ontology designed", body = ProjectDesignResponse),
        (status = 400, description = "Invalid input or large schema gate", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "LLM timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn design_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectDesignRequest>,
) -> Result<Json<ProjectDesignResponse>, AppError> {
    principal.require_designer()?;
    let project = load_mutable_project(&state, id).await?;

    // Deserialize stored data
    let source_config: SourceConfig = serde_json::from_value(project.source_config.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt source_config: {e}")))?;

    let effective_opts: DesignOptions = serde_json::from_value(project.design_options.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt design_options: {e}")))?;

    // Read runtime-tunable config (scoped guard — released before async operations)
    let (gate_threshold, sample_data) = {
        let sys_config = state.system_config.read().await;
        let threshold = sys_config.large_schema_gate_threshold();
        let ctx = LlmInputContext::from_project(&project);
        let data = build_llm_input(&ctx, &source_config, &effective_opts, &sys_config)?;
        (threshold, data)
    };

    if sample_data.trim().is_empty() {
        return Err(AppError::empty_source_data());
    }

    // Deserialize analysis report once for review gate, large schema gate, and design context.
    let analysis_report = project
        .analysis_report
        .as_ref()
        .map(|v| {
            serde_json::from_value::<ox_core::source_analysis::SourceAnalysisReport>(v.clone())
                .map_err(|e| AppError::internal(format!("Corrupt analysis_report: {e}")))
        })
        .transpose()?;

    // Review gate: use the stored analysis report (which reflects repo enrichment)
    // rather than reconstructing from schema+profile (which would lose repo resolutions).
    if let Some(report) = &analysis_report {
        maybe_require_review(report, &effective_opts)?;

        // Large schema governance gate
        if !req.acknowledge_large_schema
            && let Some(warning) = &report.large_schema_warning
            && warning.table_count >= gate_threshold
        {
            return Err(AppError::bad_request(format!(
                "Schema has {} tables (limit: {gate_threshold}). Use excluded_tables to reduce scope, \
                 or set acknowledge_large_schema=true to proceed.",
                warning.table_count
            )));
        }
    }

    // Build LLM context (include repo field_hints/domain_notes when available)
    let repo_summary = analysis_report
        .as_ref()
        .and_then(|r| r.repo_summary.as_ref());
    let effective_context = build_design_context(&req.context, &effective_opts, repo_summary);

    info!(project_id = %id, "Designing ontology from stored snapshot");

    let timeout = std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
    let design_started = Instant::now();
    let (ontology, source_mapping) = tokio::time::timeout(
        timeout,
        state
            .brain
            .design_ontology(&sample_data, &effective_context),
    )
    .await
    .map_err(|_| {
        warn!(
            project_id = %id,
            elapsed_ms = design_started.elapsed().as_millis() as u64,
            timeout_secs = timeout.as_secs(),
            "Design LLM call timed out"
        );
        AppError::timeout(format!(
            "Ontology design timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(AppError::from)?;

    let design_duration_ms = design_started.elapsed().as_millis() as i64;
    info!(
        project_id = %id,
        design_ms = design_duration_ms,
        "LLM design completed"
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
                Some("design"),
                0,
                0,
                design_duration_ms,
                0.0,
                serde_json::json!({}),
            ).await;
        });
    }

    // Assess quality (use the fresh source_mapping, not yet persisted)
    let quality_report = assess_quality_from_project_with_mapping(
        &project,
        &ontology,
        &source_mapping,
        &effective_opts.excluded_tables,
        &effective_opts.column_clarifications,
    )?;

    // Persist
    let ontology_json = AppError::to_json(&ontology)?;
    let sm_json = AppError::to_json(&source_mapping)?;
    let qr_json = AppError::to_json(&quality_report)?;
    state
        .store
        .update_design_result(
            id,
            &ontology_json,
            Some(&sm_json),
            Some(&qr_json),
            req.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok(Json(ProjectDesignResponse { project: updated }))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/refine
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/refine",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectRefineRequest,
    responses(
        (status = 200, description = "Ontology refined", body = ProjectRefineResponse),
        (status = 400, description = "No runtime or additional context", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Uncertain reconcile matches", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "LLM timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn refine_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectRefineRequest>,
) -> Result<Json<ProjectRefineResponse>, AppError> {
    principal.require_designer()?;
    let project = load_project_in_status(&state, id, DesignProjectStatus::Designed).await?;

    let ontology: OntologyIR = project
        .ontology
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(AppError::no_ontology)?;

    validate_ontology_input(&ontology)?;

    let has_additional_context = req
        .additional_context
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());

    let (large_ontology_threshold, profiling_timeout_secs, refine_timeout_secs) = {
        let sys_config = state.system_config.read().await;
        (
            sys_config.large_ontology_threshold(),
            sys_config.profiling_timeout_secs(),
            sys_config.refine_timeout_secs(),
        )
    };

    let profiling_timeout = std::time::Duration::from_secs(profiling_timeout_secs);

    // Graph profile (optional, non-fatal)
    let refine_started = Instant::now();
    let node_count = ontology.node_types.len();
    let profile_config = profiler::ProfileConfig::for_ontology_size(node_count);
    let graph_profile = if let Some(runtime) = &state.runtime {
        info!(project_id = %id, "Profiling graph data for refinement");
        let profile_started = Instant::now();
        match tokio::time::timeout(
            profiling_timeout,
            profiler::profile_graph(runtime.as_ref(), &ontology, &profile_config),
        )
        .await
        {
            Ok(Ok(profile)) => {
                info!(
                    project_id = %id,
                    profiling_ms = profile_started.elapsed().as_millis() as u64,
                    "Graph profiling succeeded"
                );
                let serialize_result = if node_count >= large_ontology_threshold {
                    serde_json::to_string(&profile)
                } else {
                    serde_json::to_string_pretty(&profile)
                };
                match serialize_result {
                    Ok(json) => Some((
                        json,
                        profile.node_profiles.len(),
                        profile.edge_profiles.len(),
                    )),
                    Err(e) => {
                        warn!(
                            "Graph profile serialization failed: {e} — proceeding without profile"
                        );
                        None
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Graph profiling failed: {e} — proceeding without profile");
                None
            }
            Err(_) => {
                warn!(
                    "Graph profiling timed out after {}s — proceeding without profile",
                    profiling_timeout_secs
                );
                None
            }
        }
    } else {
        None
    };

    // When no graph profile and no additional context, fall back to source schema
    // so the LLM still has enough information to refine descriptions, relationships, etc.
    let schema_fallback = if graph_profile.is_none() && !has_additional_context {
        if let Some(schema_val) = &project.source_schema {
            match serde_json::from_value::<ox_core::source_schema::SourceSchema>(schema_val.clone())
            {
                Ok(schema) => {
                    info!("No graph runtime or additional context — using source schema for refinement");
                    let summary = build_source_schema_summary(&schema);
                    Some(summary)
                }
                Err(e) => {
                    warn!("Failed to parse source schema for refinement fallback: {e}");
                    return Err(AppError::bad_request(
                        "No graph runtime, additional context, or valid source schema for refinement",
                    ));
                }
            }
        } else {
            return Err(AppError::bad_request(
                "No graph runtime, additional context, or source schema for refinement",
            ));
        }
    } else {
        None
    };

    let refinement_context = build_refinement_context(
        graph_profile.as_ref().map(|(json, _, _)| json.as_str()),
        req.additional_context
            .as_deref()
            .or(schema_fallback.as_deref()),
    );

    let timeout = std::time::Duration::from_secs(refine_timeout_secs);
    info!(
        project_id = %id,
        profiling_elapsed_ms = refine_started.elapsed().as_millis() as u64,
        timeout_secs = timeout.as_secs(),
        "Starting LLM refinement"
    );
    let llm_started = Instant::now();
    let (llm_refined, refined_mapping) = tokio::time::timeout(
        timeout,
        state.brain.refine_ontology(&ontology, &refinement_context),
    )
    .await
    .map_err(|_| {
        let total = refine_started.elapsed();
        warn!(
            project_id = %id,
            total_elapsed_ms = total.as_millis() as u64,
            llm_elapsed_ms = llm_started.elapsed().as_millis() as u64,
            timeout_secs = timeout.as_secs(),
            "Refinement LLM call timed out"
        );
        AppError::timeout(format!("Refinement timed out after {}s", timeout.as_secs()))
    })?
    .map_err(AppError::from)?;
    let refine_duration_ms = llm_started.elapsed().as_millis() as i64;
    info!(
        project_id = %id,
        llm_ms = refine_duration_ms,
        "LLM refinement completed"
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
                Some("refine"),
                0,
                0,
                refine_duration_ms,
                0.0,
                serde_json::json!({}),
            ).await;
        });
    }

    // Reconcile LLM output against original to preserve lineage IDs
    let reconciled = ox_core::ontology_command::reconcile_refined(&ontology, llm_refined);
    let _ = refined_mapping; // Source mapping preserved from project; refine does not change it

    // Fail-closed: reject if uncertain matches are present.
    // Return 422 with the full ReconcileReport + reconciled ontology so the FE
    // can apply user accept/reject decisions without re-running the LLM.
    if !reconciled.report.uncertain_matches.is_empty() {
        let details = serde_json::json!({
            "report": reconciled.report,
            "reconciled_ontology": reconciled.ontology,
        });
        return Err(AppError::unprocessable_with_details(
            "uncertain_reconcile",
            format!(
                "Refinement produced {} uncertain ID match(es) that require review",
                reconciled.report.uncertain_matches.len()
            ),
            details,
        ));
    }

    let refined = reconciled.ontology;

    let profile_summary = match (&graph_profile, has_additional_context, &schema_fallback) {
        (Some((_, n, e)), true, _) => {
            format!("Profiled {n} node types, {e} edge types; applied additional context")
        }
        (Some((_, n, e)), false, _) => format!("Profiled {n} node types, {e} edge types"),
        (None, _, Some(_)) => {
            "Refined from source schema (no graph runtime)".to_string()
        }
        (None, _, None) => "Refined from additional context (no graph data)".to_string(),
    };

    // Re-assess quality after refinement
    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_project(
        &project,
        &refined,
        &opts.excluded_tables,
        &opts.column_clarifications,
    )?;

    let ontology_json = AppError::to_json(&refined)?;
    let qr_json = AppError::to_json(&quality_report)?;
    // Preserve the existing source_mapping (refine does not change it)
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
        total_ms = refine_started.elapsed().as_millis() as u64,
        "Refine completed"
    );

    Ok(Json(ProjectRefineResponse {
        project: updated,
        profile_summary,
        reconcile_report: reconciled.report,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/apply-reconcile
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/apply-reconcile",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectReconcileRequest,
    responses(
        (status = 200, description = "Reconcile decisions applied", body = ProjectRefineResponse),
        (status = 400, description = "Invalid decisions", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Reconciled ontology invalid", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn apply_reconcile(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectReconcileRequest>,
) -> Result<Json<ProjectRefineResponse>, AppError> {
    principal.require_designer()?;
    let project = load_project_in_status(&state, id, DesignProjectStatus::Designed).await?;

    // Validate all decisions reference valid uncertain matches
    for decision in &req.decisions {
        if !req
            .uncertain_matches
            .iter()
            .any(|m| m.original_id == decision.original_id)
        {
            return Err(AppError::bad_request(format!(
                "Decision references unknown original_id '{}'",
                decision.original_id
            )));
        }
    }

    // Apply user decisions
    let finalized = ox_core::ontology_command::apply_match_decisions(
        req.reconciled_ontology,
        &req.decisions,
        &req.uncertain_matches,
    );

    // Validate
    let errors = finalized.validate();
    if !errors.is_empty() {
        return Err(AppError::unprocessable(format!(
            "Reconciled ontology is invalid: {}",
            errors.join("; ")
        )));
    }

    // Quality assessment
    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_project(
        &project,
        &finalized,
        &opts.excluded_tables,
        &opts.column_clarifications,
    )?;

    let ontology_json = AppError::to_json(&finalized)?;
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

    Ok(Json(ProjectRefineResponse {
        project: updated,
        profile_summary: "Applied reconcile decisions".to_string(),
        reconcile_report: ox_core::ReconcileReport {
            preserved_ids: vec![],
            generated_ids: vec![],
            uncertain_matches: vec![],
            deleted_entities: vec![],
            confidence: ox_core::ReconcileConfidence::High,
        },
    }))
}


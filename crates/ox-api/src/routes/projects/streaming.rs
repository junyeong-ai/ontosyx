use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, State},
    response::sse::{Event, Sse},
};
use futures_core::Stream;
use serde::Serialize;
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
    build_batch_llm_input, build_llm_input, build_refinement_context, build_source_schema_summary,
    find_uncovered_cross_fks, format_cross_fks, format_existing_edges_for_resolution,
    format_existing_nodes, format_node_labels_for_resolution, format_uncovered_fks,
    get_design_options, load_mutable_project, load_project_in_status, maybe_require_review,
    merge_input_irs, reload_project,
};
use super::types::{ProjectDesignRequest, ProjectDesignResponse, ProjectRefineRequest, ProjectRefineResponse};

// ---------------------------------------------------------------------------
// SSE event helpers
// ---------------------------------------------------------------------------

fn sse_phase(phase: &str, detail: Option<&str>) -> String {
    match detail {
        Some(d) => serde_json::json!({ "phase": phase, "detail": d }).to_string(),
        None => serde_json::json!({ "phase": phase }).to_string(),
    }
}

fn sse_error(error_type: &str, message: &str) -> String {
    serde_json::json!({
        "error": { "type": error_type, "message": message }
    })
    .to_string()
}

fn sse_result<T: Serialize>(data: &T) -> String {
    serde_json::to_string(data).unwrap_or_else(|e| {
        serde_json::json!({
            "error": { "type": "serialization_error", "message": e.to_string() }
        })
        .to_string()
    })
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/design/stream — SSE streaming design
//
// SSE event flow:
//   phase   → { phase: "validating" }
//   phase   → { phase: "designing", detail: "..." }
//   phase   → { phase: "assessing_quality" }
//   phase   → { phase: "persisting" }
//   result  → ProjectDesignResponse
//   error   → { error: { type, message } }
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/design/stream",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectDesignRequest,
    responses(
        (status = 200, description = "SSE stream: phase* -> result events", content_type = "text/event-stream"),
        (status = 400, description = "Invalid input", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
#[tracing::instrument(skip(state, principal, req), fields(project_id = %id))]
pub(crate) async fn design_project_stream(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectDesignRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    principal.require_designer()?;
    // Validate eagerly before entering stream (allows ? error propagation)
    let project = load_mutable_project(&state, id).await?;

    let source_config: SourceConfig = serde_json::from_value(project.source_config.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt source_config: {e}")))?;

    let effective_opts: DesignOptions = serde_json::from_value(project.design_options.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt design_options: {e}")))?;

    let (gate_threshold, batch_size, sys_config_snapshot) = {
        let sys_config = state.system_config.read().await;
        let threshold = sys_config.large_schema_gate_threshold();
        let bs = sys_config.batch_size();
        let snapshot = sys_config.clone();
        (threshold, bs, snapshot)
    };

    let analysis_report = project
        .analysis_report
        .as_ref()
        .map(|v| {
            serde_json::from_value::<ox_core::source_analysis::SourceAnalysisReport>(v.clone())
                .map_err(|e| AppError::internal(format!("Corrupt analysis_report: {e}")))
        })
        .transpose()?;

    if let Some(report) = &analysis_report {
        maybe_require_review(report, &effective_opts)?;

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

    let repo_summary = analysis_report
        .as_ref()
        .and_then(|r| r.repo_summary.as_ref());
    let effective_context = build_design_context(&req.context, &effective_opts, repo_summary);

    // Parse schema + profile for batch path (may not exist for text sources)
    let schema_and_profile: Option<(
        ox_core::source_schema::SourceSchema,
        ox_core::source_schema::SourceProfile,
    )> = project
        .source_schema
        .as_ref()
        .and_then(|sv| serde_json::from_value(sv.clone()).ok())
        .and_then(|schema: ox_core::source_schema::SourceSchema| {
            project
                .source_profile
                .as_ref()
                .and_then(|pv| serde_json::from_value(pv.clone()).ok())
                .map(|profile| (schema, profile))
        });

    let total_tables = schema_and_profile
        .as_ref()
        .map(|(s, _)| s.tables.len())
        .unwrap_or(0);

    // Structured sources (schema+profile available) always use batch pipeline.
    // Text sources (no schema) use direct single-call design.
    let use_batch = schema_and_profile.is_some();

    let implied_rels: Vec<ox_core::source_analysis::ImpliedRelationship> = analysis_report
        .as_ref()
        .map(|r| r.implied_relationships.clone())
        .unwrap_or_default();

    let revision = req.revision;

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("phase").data(sse_phase("validating", None)));

        info!(project_id = %id, total_tables, use_batch, "Designing ontology (stream) from stored snapshot");

        let timeout = std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
        let design_started = Instant::now();

        let design_result: Result<(OntologyIR, ox_core::SourceMapping), ox_core::OxError> = if !use_batch {
            // === Text source path (no schema to cluster) ===
            let sample_data = {
                let ctx = LlmInputContext::from_project(&project);
                match build_llm_input(&ctx, &source_config, &effective_opts, &sys_config_snapshot) {
                    Ok(data) if !data.trim().is_empty() => data,
                    Ok(_) => {
                        yield Ok(Event::default().event("error").data(
                            sse_error("validation_error", "Source data is empty")
                        ));
                        return;
                    }
                    Err(e) => {
                        yield Ok(Event::default().event("error").data(
                            sse_error("validation_error", &format!("{e:?}"))
                        ));
                        return;
                    }
                }
            };

            yield Ok(Event::default().event("phase").data(
                sse_phase("designing", Some("LLM is generating the ontology..."))
            ));

            match tokio::time::timeout(
                timeout,
                state.brain.design_ontology(&sample_data, &effective_context),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    warn!(
                        project_id = %id,
                        elapsed_ms = design_started.elapsed().as_millis() as u64,
                        "Design LLM call timed out (stream)"
                    );
                    yield Ok(Event::default().event("error").data(
                        sse_error("timeout", &format!(
                            "Ontology design timed out after {}s",
                            timeout.as_secs()
                        ))
                    ));
                    return;
                }
            }
        } else {
            // === Divide-and-conquer path (structured sources) ===
            let (raw_schema, raw_profile) = schema_and_profile.as_ref().unwrap();

            // Pre-process: filter excluded tables and apply PII masking
            let mut schema = raw_schema.clone();
            let mut profile = raw_profile.clone();
            if !effective_opts.excluded_tables.is_empty() {
                let excluded: std::collections::HashSet<&str> =
                    effective_opts.excluded_tables.iter().map(|s| s.as_str()).collect();
                schema.tables.retain(|t| !excluded.contains(t.name.as_str()));
                schema.foreign_keys.retain(|fk| {
                    !excluded.contains(fk.from_table.as_str()) && !excluded.contains(fk.to_table.as_str())
                });
                profile.table_profiles.retain(|tp| !excluded.contains(tp.table_name.as_str()));
            }
            if effective_opts.pii_decisions.iter().any(|d| {
                matches!(d.decision, ox_core::source_analysis::PiiDecision::Mask | ox_core::source_analysis::PiiDecision::Exclude)
            }) {
                ox_source::analyzer::apply_pii_masking(&mut profile, &effective_opts.pii_decisions);
            }

            let effective_tables = schema.tables.len();

            // Phase 1: Clustering
            yield Ok(Event::default().event("phase").data(
                sse_phase("clustering", Some(&format!("Analyzing {} table relationships...", effective_tables)))
            ));

            let plan = ox_core::cluster_tables(&schema, &implied_rels, batch_size);
            let all_cross_fks: Vec<ox_core::source_schema::ForeignKeyDef> = {
                let mut seen = std::collections::HashSet::new();
                plan.clusters
                    .iter()
                    .flat_map(|c| c.cross_fks.iter())
                    .filter(|fk| seen.insert((fk.from_table.clone(), fk.from_column.clone(), fk.to_table.clone())))
                    .cloned()
                    .collect()
            };

            info!(
                project_id = %id,
                cluster_count = plan.clusters.len(),
                parallel_levels = plan.levels.len(),
                cross_fk_count = all_cross_fks.len(),
                "Table clustering complete"
            );

            // Phase 2: Level-by-level parallel batch design
            let mut batch_results: Vec<ox_core::OntologyInputIR> = Vec::new();
            let mut completed = 0usize;
            let total_clusters = plan.clusters.len();

            for (level_idx, level) in plan.levels.iter().enumerate() {
                let level_size = level.len();
                if level_size == 1 {
                    // Single cluster in level — run directly (no JoinSet overhead)
                    let cluster_id = level[0];
                    let cluster = &plan.clusters[cluster_id];
                    completed += 1;
                    let detail = format!(
                        "{}/{} ({} tables)",
                        completed, total_clusters, cluster.tables.len(),
                    );
                    yield Ok(Event::default().event("phase").data(
                        sse_phase("designing", Some(&detail))
                    ));

                    let batch_input = match build_batch_llm_input(&schema, &profile, cluster, &sys_config_snapshot) {
                        Ok(data) => data,
                        Err(e) => {
                            yield Ok(Event::default().event("error").data(
                                sse_error("design_error", &format!("Cluster {} input failed: {e:?}", cluster_id))
                            ));
                            return;
                        }
                    };
                    let existing = format_existing_nodes(&batch_results);
                    let cross = format_cross_fks(&cluster.cross_fks, cluster, &batch_results);

                    match tokio::time::timeout(
                        timeout,
                        state.brain.design_ontology_batch(&batch_input, &effective_context, &existing, &cross),
                    ).await {
                        Ok(Ok(ir)) => {
                            info!(project_id = %id, cluster = cluster_id, nodes = ir.node_types.len(), "Batch completed");
                            batch_results.push(ir);
                        }
                        Ok(Err(e)) => {
                            yield Ok(Event::default().event("error").data(sse_error("design_error", &e.to_string())));
                            return;
                        }
                        Err(_) => {
                            yield Ok(Event::default().event("error").data(sse_error("timeout", &format!("Cluster {} timed out", cluster_id))));
                            return;
                        }
                    }
                } else {
                    // Multiple independent clusters — run in parallel
                    let detail = format!(
                        "Level {}/{}: {} clusters in parallel ({}/{})",
                        level_idx + 1, plan.levels.len(), level_size, completed + 1, total_clusters,
                    );
                    yield Ok(Event::default().event("phase").data(
                        sse_phase("designing", Some(&detail))
                    ));

                    // Snapshot current batch_results for all parallel tasks in this level
                    let existing = format_existing_nodes(&batch_results);

                    // Prepare inputs for all clusters in this level
                    let mut tasks: Vec<(usize, String, String)> = Vec::new();
                    for &cluster_id in level {
                        let cluster = &plan.clusters[cluster_id];
                        let batch_input = match build_batch_llm_input(&schema, &profile, cluster, &sys_config_snapshot) {
                            Ok(data) => data,
                            Err(e) => {
                                yield Ok(Event::default().event("error").data(
                                    sse_error("design_error", &format!("Cluster {} input failed: {e:?}", cluster_id))
                                ));
                                return;
                            }
                        };
                        let cross = format_cross_fks(&cluster.cross_fks, cluster, &batch_results);
                        tasks.push((cluster_id, batch_input, cross));
                    }

                    // Run LLM calls with bounded concurrency to avoid API rate limits
                    let max_concurrent = 5;
                    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
                    let futs: Vec<_> = tasks.iter().map(|(_, batch_input, cross)| {
                        let sem = semaphore.clone();
                        let brain = state.brain.clone();
                        let ctx = effective_context.clone();
                        let ex = existing.clone();
                        let bi = batch_input.clone();
                        let cr = cross.clone();
                        let t = timeout;
                        async move {
                            let _permit = sem.acquire().await.unwrap();
                            tokio::time::timeout(t, brain.design_ontology_batch(&bi, &ctx, &ex, &cr)).await
                        }
                    }).collect();

                    let results = futures::future::join_all(futs).await;

                    let mut level_results: Vec<(usize, ox_core::OntologyInputIR)> = Vec::new();
                    for (idx, result) in results.into_iter().enumerate() {
                        let cluster_id = tasks[idx].0;
                        match result {
                            Ok(Ok(ir)) => {
                                info!(project_id = %id, cluster = cluster_id, nodes = ir.node_types.len(), "Parallel batch completed");
                                level_results.push((cluster_id, ir));
                            }
                            Ok(Err(e)) => {
                                yield Ok(Event::default().event("error").data(sse_error("design_error", &e.to_string())));
                                return;
                            }
                            Err(_) => {
                                yield Ok(Event::default().event("error").data(sse_error("timeout", &format!("Cluster {} timed out", cluster_id))));
                                return;
                            }
                        }
                    }

                    completed += level_results.len();
                    yield Ok(Event::default().event("phase").data(
                        sse_phase("designing", Some(&format!("{}/{} clusters complete", completed, total_clusters)))
                    ));

                    // Sort by cluster_id for deterministic merge order
                    level_results.sort_by_key(|(id, _)| *id);
                    for (_, ir) in level_results {
                        batch_results.push(ir);
                    }
                }
            }

            // Phase 3: Merge InputIRs
            yield Ok(Event::default().event("phase").data(
                sse_phase("merging", Some("Merging partial ontologies..."))
            ));

            let project_name = project.title.clone().unwrap_or_default();
            let description: Option<String> = None;
            let mut merged = merge_input_irs(
                batch_results,
                &project_name,
                description.as_deref(),
            );

            info!(
                project_id = %id,
                merged_nodes = merged.node_types.len(),
                merged_edges = merged.edge_types.len(),
                "InputIR merge complete"
            );

            // Phase 4: Cross-domain edge resolution (conditional)
            let uncovered = find_uncovered_cross_fks(&merged, &all_cross_fks);
            if !uncovered.is_empty() {
                yield Ok(Event::default().event("phase").data(
                    sse_phase("resolving_edges", Some(&format!(
                        "{} uncovered cross-domain FKs", uncovered.len()
                    )))
                ));

                let node_labels = format_node_labels_for_resolution(&merged);
                let existing_edges = format_existing_edges_for_resolution(&merged);
                let uncovered_text = format_uncovered_fks(&uncovered, &merged);

                match tokio::time::timeout(
                    timeout,
                    state.brain.resolve_cross_edges(
                        &node_labels, &existing_edges, &uncovered_text,
                    ),
                )
                .await
                {
                    Ok(Ok(extra_edges)) => {
                        info!(
                            project_id = %id,
                            resolved_edges = extra_edges.len(),
                            "Cross-domain edge resolution complete"
                        );
                        merged.edge_types.extend(extra_edges);
                    }
                    Ok(Err(e)) => {
                        warn!(project_id = %id, error = %e, "Edge resolution failed — continuing with existing edges");
                        yield Ok(Event::default().event("phase").data(
                            sse_phase("resolving_edges", Some("Edge resolution failed — some cross-domain edges may be missing"))
                        ));
                    }
                    Err(_) => {
                        warn!(project_id = %id, "Edge resolution timed out — continuing with existing edges");
                        yield Ok(Event::default().event("phase").data(
                            sse_phase("resolving_edges", Some("Edge resolution timed out — some cross-domain edges may be missing"))
                        ));
                    }
                }
            }

            // Phase 5: Normalize (single pass)
            match ox_core::normalize(merged) {
                Ok(nr) => {
                    let errors = nr.ontology.validate();
                    if !errors.is_empty() {
                        Err(ox_core::OxError::Ontology {
                            message: format!("Batch-designed ontology validation errors: {}", errors.join("; ")),
                        })
                    } else {
                        Ok((nr.ontology, nr.source_mapping))
                    }
                }
                Err(errors) => Err(ox_core::OxError::Ontology {
                    message: format!("Batch-designed ontology normalization failed: {}", errors.join("; ")),
                }),
            }
        };

        let (ontology, source_mapping) = match design_result {
            Ok(result) => result,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("design_error", &e.to_string())
                ));
                return;
            }
        };

        let design_ms = design_started.elapsed().as_millis() as u64;
        info!(project_id = %id, design_ms, "LLM design completed (stream)");

        yield Ok(Event::default().event("phase").data(
            sse_phase("assessing_quality", None)
        ));

        let quality_report = match assess_quality_from_project_with_mapping(
            &project,
            &ontology,
            &source_mapping,
            &effective_opts.excluded_tables,
            &effective_opts.column_clarifications,
        ) {
            Ok(qr) => qr,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("quality_error", &format!("{e:?}"))
                ));
                return;
            }
        };

        yield Ok(Event::default().event("phase").data(
            sse_phase("persisting", None)
        ));

        let ontology_json = match AppError::to_json(&ontology) {
            Ok(v) => v,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("serialization_error", &format!("{e:?}"))
                ));
                return;
            }
        };
        let sm_json = match AppError::to_json(&source_mapping) {
            Ok(v) => v,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("serialization_error", &format!("{e:?}"))
                ));
                return;
            }
        };
        let qr_json = match AppError::to_json(&quality_report) {
            Ok(v) => v,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("serialization_error", &format!("{e:?}"))
                ));
                return;
            }
        };

        if let Err(e) = state
            .store
            .update_design_result(id, &ontology_json, Some(&sm_json), Some(&qr_json), revision)
            .await
        {
            yield Ok(Event::default().event("error").data(
                sse_error("persist_error", &e.to_string())
            ));
            return;
        }

        let updated = match reload_project(&state, id).await {
            Ok(p) => p,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("internal_error", &format!("{e:?}"))
                ));
                return;
            }
        };

        yield Ok(Event::default().event("result").data(
            sse_result(&ProjectDesignResponse { project: updated })
        ));
    };

    Ok(Sse::new(stream))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/refine/stream — SSE streaming refinement
//
// SSE event flow:
//   phase              → { phase: "validating" }
//   phase              → { phase: "profiling", detail: "..." }
//   phase              → { phase: "profiling_complete", detail: "..." }
//   phase              → { phase: "refining", detail: "..." }
//   phase              → { phase: "reconciling" }
//   phase              → { phase: "assessing_quality" }
//   phase              → { phase: "persisting" }
//   result             → ProjectRefineResponse
//   uncertain_reconcile → { report, reconciled_ontology }
//   error              → { error: { type, message } }
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/refine/stream",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectRefineRequest,
    responses(
        (status = 200, description = "SSE stream: phase* -> result/uncertain_reconcile events", content_type = "text/event-stream"),
        (status = 400, description = "No runtime or context", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
#[tracing::instrument(skip(state, principal, req), fields(project_id = %id))]
pub(crate) async fn refine_project_stream(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectRefineRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    principal.require_designer()?;
    // Validate eagerly
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

    let revision = req.revision;
    let additional_context = req.additional_context.clone();
    let (large_ontology_threshold, profiling_timeout_secs, refine_timeout_secs) = {
        let sys_config = state.system_config.read().await;
        (
            sys_config.large_ontology_threshold(),
            sys_config.profiling_timeout_secs(),
            sys_config.refine_timeout_secs(),
        )
    };
    // Dynamic profiling timeout based on ontology size
    let dynamic_timeout_secs = if profiling_timeout_secs > 0 {
        let node_count = ontology.node_types.len();
        match node_count {
            0..=100 => profiling_timeout_secs,
            101..=500 => profiling_timeout_secs.max(120),
            _ => profiling_timeout_secs.max(180),
        }
    } else {
        profiling_timeout_secs
    };
    let profiling_timeout = std::time::Duration::from_secs(dynamic_timeout_secs);
    // Clone source_schema before entering stream for schema fallback
    let source_schema_val = project.source_schema.clone();

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("phase").data(sse_phase("validating", None)));

        let refine_started = Instant::now();
        let node_count = ontology.node_types.len();
        let profile_config = profiler::ProfileConfig::for_ontology_size(node_count);

        // Graph profiling (optional, non-fatal)
        let graph_profile = if let Some(runtime) = &state.runtime {
            yield Ok(Event::default().event("phase").data(
                sse_phase("profiling", Some(&format!(
                    "Profiling {} node types against graph database...",
                    node_count
                )))
            ));

            let profile_started = Instant::now();
            match tokio::time::timeout(
                profiling_timeout,
                profiler::profile_graph(runtime.as_ref(), &ontology, &profile_config),
            )
            .await
            {
                Ok(Ok(profile)) => {
                    let profiling_ms = profile_started.elapsed().as_millis() as u64;
                    info!(project_id = %id, profiling_ms, "Graph profiling succeeded (stream)");

                    let n = profile.node_profiles.len();
                    let e = profile.edge_profiles.len();

                    yield Ok(Event::default().event("phase").data(
                        sse_phase("profiling_complete", Some(&format!(
                            "Profiled {n} node types, {e} edge types in {profiling_ms}ms"
                        )))
                    ));

                    let serialize_result = if node_count >= large_ontology_threshold {
                        serde_json::to_string(&profile)
                    } else {
                        serde_json::to_string_pretty(&profile)
                    };
                    match serialize_result {
                        Ok(json) => Some((json, n, e)),
                        Err(err) => {
                            warn!("Graph profile serialization failed: {err} — proceeding without profile");
                            None
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("Graph profiling failed: {e} — proceeding without profile");
                    yield Ok(Event::default().event("phase").data(
                        sse_phase("profiling_complete", Some("Profiling failed, proceeding without graph data"))
                    ));
                    None
                }
                Err(_) => {
                    warn!(
                        "Graph profiling timed out after {}s — proceeding without profile",
                        profiling_timeout_secs
                    );
                    yield Ok(Event::default().event("phase").data(
                        sse_phase("profiling_complete", Some(&format!(
                            "Profiling timed out after {}s, proceeding without graph data",
                            profiling_timeout_secs
                        )))
                    ));
                    None
                }
            }
        } else {
            None
        };

        // When no graph profile and no additional context, fall back to source schema
        let schema_fallback = if graph_profile.is_none() && !has_additional_context {
            if let Some(schema_val) = &source_schema_val {
                match serde_json::from_value::<ox_core::source_schema::SourceSchema>(schema_val.clone()) {
                    Ok(schema) => {
                        info!("No graph runtime or additional context — using source schema for refinement (stream)");
                        Some(build_source_schema_summary(&schema))
                    }
                    Err(_) => {
                        yield Ok(Event::default().event("error").data(
                            sse_error("bad_request", "No graph runtime, additional context, or valid source schema for refinement")
                        ));
                        return;
                    }
                }
            } else {
                yield Ok(Event::default().event("error").data(
                    sse_error("bad_request", "No graph runtime, additional context, or source schema for refinement")
                ));
                return;
            }
        } else {
            None
        };

        let refinement_context = build_refinement_context(
            graph_profile.as_ref().map(|(json, _, _)| json.as_str()),
            additional_context.as_deref().or(schema_fallback.as_deref()),
        );

        let timeout = std::time::Duration::from_secs(refine_timeout_secs);

        yield Ok(Event::default().event("phase").data(
            sse_phase("refining", Some("LLM is refining the ontology..."))
        ));

        let llm_started = Instant::now();
        let llm_result = tokio::time::timeout(
            timeout,
            state.brain.refine_ontology(&ontology, &refinement_context),
        )
        .await;

        let (llm_refined, refined_mapping) = match llm_result {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("refine_error", &e.to_string())
                ));
                return;
            }
            Err(_) => {
                let total = refine_started.elapsed();
                warn!(
                    project_id = %id,
                    total_elapsed_ms = total.as_millis() as u64,
                    llm_elapsed_ms = llm_started.elapsed().as_millis() as u64,
                    "Refinement LLM call timed out (stream)"
                );
                yield Ok(Event::default().event("error").data(
                    sse_error("timeout", &format!(
                        "Refinement timed out after {}s",
                        timeout.as_secs()
                    ))
                ));
                return;
            }
        };

        let llm_ms = llm_started.elapsed().as_millis() as u64;
        info!(project_id = %id, llm_ms, "LLM refinement completed (stream)");

        yield Ok(Event::default().event("phase").data(
            sse_phase("reconciling", None)
        ));

        let reconciled = ox_core::ontology_command::reconcile_refined(&ontology, llm_refined);
        let _ = refined_mapping;

        // Fail-closed: return uncertain matches as special SSE event
        if !reconciled.report.uncertain_matches.is_empty() {
            let details = serde_json::json!({
                "report": reconciled.report,
                "reconciled_ontology": reconciled.ontology,
            });
            yield Ok(Event::default().event("uncertain_reconcile").data(
                details.to_string()
            ));
            return;
        }

        let refined = reconciled.ontology;

        yield Ok(Event::default().event("phase").data(
            sse_phase("assessing_quality", None)
        ));

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

        let opts = get_design_options(&project);
        let quality_report = match assess_quality_from_project(
            &project, &refined, &opts.excluded_tables, &opts.column_clarifications,
        ) {
            Ok(qr) => qr,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("quality_error", &format!("{e:?}"))
                ));
                return;
            }
        };

        yield Ok(Event::default().event("phase").data(
            sse_phase("persisting", None)
        ));

        let ontology_json = match AppError::to_json(&refined) {
            Ok(v) => v,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("serialization_error", &format!("{e:?}"))
                ));
                return;
            }
        };
        let qr_json = match AppError::to_json(&quality_report) {
            Ok(v) => v,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("serialization_error", &format!("{e:?}"))
                ));
                return;
            }
        };

        if let Err(e) = state
            .store
            .update_design_result(
                id,
                &ontology_json,
                project.source_mapping.as_ref(),
                Some(&qr_json),
                revision,
            )
            .await
        {
            yield Ok(Event::default().event("error").data(
                sse_error("persist_error", &e.to_string())
            ));
            return;
        }

        let updated = match reload_project(&state, id).await {
            Ok(p) => p,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("internal_error", &format!("{e:?}"))
                ));
                return;
            }
        };

        let total_ms = refine_started.elapsed().as_millis() as u64;
        info!(project_id = %id, total_ms, "Refine completed (stream)");

        yield Ok(Event::default().event("result").data(
            sse_result(&ProjectRefineResponse {
                project: updated,
                profile_summary,
                reconcile_report: reconciled.report,
            })
        ));
    };

    Ok(Sse::new(stream))
}

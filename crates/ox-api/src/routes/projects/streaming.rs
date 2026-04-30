use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
};
use chrono::{Duration as ChronoDuration, Utc};
use futures_core::Stream;
use serde::Serialize;
use tokio::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

/// How long a draft cluster checkpoint stays cached before the
/// daily cleanup cron sweeps it. ADR-0027 — long enough that a
/// session of design retries hits the cache, short enough that
/// abandoned designs don't accumulate.
const DRAFT_CHECKPOINT_TTL_HOURS: i64 = 24;

/// Author + persist one draft cluster checkpoint. Failures log
/// loudly but never propagate — the LLM call already succeeded
/// and `batch_results` carries the output forward. A later replay
/// of the same cluster will hit the LLM again rather than the
/// cache, which is the desirable degraded behaviour.
///
/// Bounded on `DraftClusterCheckpointStore` (not the full Store
/// supertrait) so the helper stays narrow on the surface it
/// actually consumes.
async fn persist_cluster_checkpoint<S>(
    store: &S,
    workspace_id: Uuid,
    project_id: Uuid,
    source_id: &str,
    signature: &str,
    cluster_id: usize,
    output: &ox_ontology::input::InputOntologyDef,
) where
    S: ox_store::DraftClusterCheckpointStore + ?Sized,
{
    let serialized = match serde_json::to_value(output) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                project_id = %project_id,
                cluster = cluster_id,
                error = %e,
                "Cluster checkpoint serialise failed; skipping persist"
            );
            return;
        }
    };
    let row = ox_store::DraftClusterCheckpointRow {
        id: Uuid::new_v4(),
        workspace_id,
        project_id,
        source_id: source_id.to_string(),
        signature: signature.to_string(),
        cluster_id: cluster_id as i32,
        output: serialized,
        created_at: Utc::now(),
        expires_at: Utc::now() + ChronoDuration::hours(DRAFT_CHECKPOINT_TTL_HOURS),
    };
    if let Err(e) = store.upsert_draft_cluster_checkpoint(&row).await {
        warn!(
            project_id = %project_id,
            cluster = cluster_id,
            error = %e,
            "Cluster checkpoint persist failed"
        );
    }
}

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::validation::validate_ontology_input;
use ox_brain::DesignOntologyOutput;
use ox_ontology::source_mapping::ArtifactProvenance;
use ox_ontology::design_project::{DesignProjectStatus, SourceConfig};
use ox_ontology::ir::OntologyIR;
use ox_ontology::source_analysis::DesignOptions;
use ox_runtime::profiler;
use ox_source::analyzer::build_design_context;

use crate::spawn_scoped::{WsScope, scope_stream};
use super::helpers::artifact::persist_design_artifact;
use super::helpers::{
    LlmInputContext, assess_quality_from_project, assess_quality_from_project_with_mapping,
    build_batch_llm_input, build_llm_input, build_refinement_context, build_source_schema_summary,
    enforce_design_gates, find_uncovered_cross_fks, format_cross_fks,
    format_existing_edges_for_resolution, format_existing_nodes, format_node_labels_for_resolution,
    format_uncovered_fks, get_design_options, load_analysis_report, load_mutable_project,
    load_project_in_status, merge_input_irs, reload_project,
};
use super::types::{
    DesignProjectRequest, DesignProjectResponse, ProjectView, RefineProjectRequest,
    RefineProjectResponse,
};

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
//   result  → DesignProjectResponse
//   error   → { error: { type, message } }
//
// ADR-0027 store integration: the BE checkpoint replay path is
// wired below. Each cluster's `InputOntologyDef` is cached in
// `draft_cluster_checkpoints` keyed by
// `ClusterSignature::from_cluster(cluster, prompt_template_hash)`;
// a transient failure on cluster K no longer discards 0..K's
// output. Retry replays the cached entries and only re-runs the
// uncompleted clusters. Successful design completion drops the
// project's checkpoints; the daily cleanup cron sweeps any rows
// past `expires_at` (24h TTL).
//
// ADR-0053 progressive streaming (FE half) is the remaining slice.
// With the BE store now in place, the FE can render per-cluster
// outcome events as each cluster completes (cache-hit vs
// cache-miss) and surface partial-progress retry recovery to the
// operator. That work lives in the FE; the BE contract is now
// stable for it to ride.
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/design/stream",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = DesignProjectRequest,
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
    Json(req): Json<DesignProjectRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    principal.require_designer()?;
    // Validate eagerly before entering stream (allows ? error propagation)
    let project = load_mutable_project(&state, id).await?;

    let source_config: SourceConfig = serde_json::from_value(project.source_config.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt source_config: {e}")))?;

    let effective_opts: DesignOptions = serde_json::from_value(project.design_options.clone())
        .map_err(|e| AppError::bad_request(format!("Corrupt design_options: {e}")))?;

    let (batch_size, sys_config_snapshot) = {
        let sys_config = state.system_config.read().await;
        let bs = sys_config.batch_size();
        let snapshot = sys_config.clone();
        (bs, snapshot)
    };

    let analysis_report = load_analysis_report(&project);

    if let Some(report) = &analysis_report {
        enforce_design_gates(report, &effective_opts)?;
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

    let implied_rels: Vec<ox_ontology::source_analysis::ImpliedRelationship> = analysis_report
        .as_ref()
        .map(|r| r.implied_relationships.clone())
        .unwrap_or_default();

    let revision = req.revision;

    // Capture workspace scope synchronously — the SSE stream is driven
    // by axum *after* the workspace_context middleware's
    // `WORKSPACE_ID.scope` exits, so every store/runtime call inside
    // the body below would otherwise see no task-locals (and post-B6,
    // would return `MissingContext`). `scope_stream` re-enters the
    // captured scope on every poll.
    let ws_scope = WsScope::capture();

    // Workspace id for the draft cluster checkpoint rows (ADR-0027).
    // System / None scopes (cron-driven design replays, if they ever
    // exist) skip the checkpoint flow — the row's `workspace_id`
    // column has no meaningful value to populate. Captured here in
    // the synchronous prologue, before scope_stream takes ownership
    // of `ws_scope`.
    let checkpoint_workspace_id: Option<Uuid> = match &ws_scope {
        WsScope::Workspace(id) => Some(*id),
        WsScope::System | WsScope::None => None,
    };

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("phase").data(sse_phase("validating", None)));

        info!(project_id = %id, total_tables, use_batch, "Designing ontology (stream) from stored snapshot");

        let timeout = std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
        let design_started = Instant::now();

        // Canonical source identity — stamped onto every ObjectMappingDef
        // produced by normalization so the IR alone is a complete mapping.
        let source_id = ox_ontology::mapping::SourceId::new(project.source_id.clone());

        let design_result: Result<DesignOntologyOutput, ox_core::OxError> = if !use_batch {
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
                state.brain.design_ontology(&ox_brain::DesignOntologyInput::bare(
                    &sample_data,
                    &effective_context,
                    &source_id,
                )),
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
        } else if let Some((raw_schema, raw_profile)) = schema_and_profile.as_ref() {
            // === Divide-and-conquer path (structured sources) ===
            //
            // Capture provenance once at the top of the batch loop:
            // every cluster shares the same prompt + model resolution
            // (the routing rule is keyed on the operation name, not
            // the cluster), so a single lookup covers every emission.
            // The batch path runs `design_ontology_batch` instructions
            // — record that as the prompt id even though the model
            // routing key is `design_ontology` (rules cascade by
            // operation, not by template).
            let batch_provenance: ArtifactProvenance = match state
                .brain
                .resolve_design_provenance(
                    "design_ontology_batch",
                    "design_ontology",
                )
                .await
            {
                Ok(a) => a,
                Err(e) => {
                    yield Ok(Event::default().event("error").data(
                        sse_error("design_error", &format!(
                            "Failed to resolve design provenance: {e}"
                        ))
                    ));
                    return;
                }
            };

            // Prompt template hash for the cluster signature
            // (ADR-0027). Folds the template body into the cache key
            // so an admin who edits the prompt without bumping
            // `prompt_version` cleanly invalidates every cached
            // checkpoint authored under the prior body. Computed
            // once for the whole batch; every cluster signature
            // pulls in the same hash.
            let prompt_template_hash = match state
                .brain
                .design_prompt_template_hash("design_ontology_batch")
                .await
            {
                Ok(h) => h,
                Err(e) => {
                    yield Ok(Event::default().event("error").data(
                        sse_error("design_error", &format!(
                            "Failed to compute prompt template hash: {e}"
                        ))
                    ));
                    return;
                }
            };
            let project_source_id = project.source_id.clone();

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
            if !effective_opts.excluded_columns.is_empty() {
                let mut by_table: std::collections::HashMap<&str, std::collections::HashSet<&str>> =
                    std::collections::HashMap::new();
                for entry in &effective_opts.excluded_columns {
                    by_table
                        .entry(entry.table.as_str())
                        .or_default()
                        .insert(entry.column.as_str());
                }
                for table in &mut schema.tables {
                    if let Some(cols) = by_table.get(table.name.as_str()) {
                        table.columns.retain(|c| !cols.contains(c.name.as_str()));
                    }
                }
                schema.foreign_keys.retain(|fk| {
                    let from_excluded = by_table
                        .get(fk.from_table.as_str())
                        .map(|s| s.contains(fk.from_column.as_str()))
                        .unwrap_or(false);
                    let to_excluded = by_table
                        .get(fk.to_table.as_str())
                        .map(|s| s.contains(fk.to_column.as_str()))
                        .unwrap_or(false);
                    !from_excluded && !to_excluded
                });
                for tp in &mut profile.table_profiles {
                    if let Some(cols) = by_table.get(tp.table_name.as_str()) {
                        tp.column_stats.retain(|cs| !cols.contains(cs.column_name.as_str()));
                    }
                }
            }
            if !effective_opts.pii_annotations.is_empty() {
                let mut by_target: std::collections::HashMap<
                    (&str, &str),
                    &ox_ontology::ir::PiiKind,
                > = std::collections::HashMap::new();
                for ann in &effective_opts.pii_annotations {
                    by_target.insert((ann.table.as_str(), ann.column.as_str()), &ann.kind);
                }
                for tp in &mut profile.table_profiles {
                    for cs in &mut tp.column_stats {
                        if let Some(kind) = by_target
                            .get(&(tp.table_name.as_str(), cs.column_name.as_str()))
                        {
                            ox_ontology::pii::redact_column_stats(cs, kind);
                        }
                    }
                }
            }

            let effective_tables = schema.tables.len();

            // Phase 1: Clustering
            yield Ok(Event::default().event("phase").data(
                sse_phase("clustering", Some(&format!("Analyzing {} table relationships...", effective_tables)))
            ));

            let plan = ox_ontology::cluster_tables(&schema, &implied_rels, batch_size);
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
            let mut batch_results: Vec<ox_ontology::InputOntologyDef> = Vec::new();
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

                    // ADR-0027 — checkpoint cache lookup before the
                    // LLM call. The signature folds tables + FKs +
                    // prompt template hash, so a re-run with the
                    // same inputs replays from cache.
                    let signature = ox_ontology::cluster_checkpoint::ClusterSignature::from_cluster(
                        cluster, &prompt_template_hash,
                    );
                    let cached = match state
                        .store
                        .find_draft_cluster_checkpoint_by_signature(
                            id,
                            &project_source_id,
                            signature.as_str(),
                        )
                        .await
                    {
                        Ok(Some(row)) => match serde_json::from_value::<
                            ox_ontology::input::InputOntologyDef,
                        >(row.output)
                        {
                            Ok(ir) => Some(ir),
                            Err(e) => {
                                warn!(
                                    project_id = %id,
                                    cluster = cluster_id,
                                    error = %e,
                                    "Checkpoint deserialise failed; rerunning LLM"
                                );
                                None
                            }
                        },
                        Ok(None) => None,
                        Err(e) => {
                            warn!(
                                project_id = %id,
                                cluster = cluster_id,
                                error = %e,
                                "Checkpoint lookup failed; rerunning LLM"
                            );
                            None
                        }
                    };

                    if let Some(ir) = cached {
                        info!(project_id = %id, cluster = cluster_id, "Cluster checkpoint cache hit");
                        yield Ok(Event::default().event("phase").data(
                            sse_phase("designing", Some(&format!("{} (cached)", detail)))
                        ));
                        batch_results.push(ir);
                    } else {
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
                                if let Some(ws_id) = checkpoint_workspace_id {
                                    persist_cluster_checkpoint(
                                        state.store.as_ref(),
                                        ws_id,
                                        id,
                                        &project_source_id,
                                        signature.as_str(),
                                        cluster_id,
                                        &ir,
                                    )
                                    .await;
                                }
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

                    // ADR-0027 — pre-resolve checkpoints for every
                    // cluster in this level. Cache hits skip the LLM
                    // call entirely; misses go through the parallel
                    // join_all path below. The two halves merge back
                    // by `cluster_id` at the bottom of the level so
                    // the eventual `batch_results` order is
                    // deterministic regardless of the hit/miss split.
                    let mut level_results: Vec<(usize, ox_ontology::InputOntologyDef)> =
                        Vec::new();
                    let mut miss_tasks: Vec<(usize, String, String, String)> = Vec::new();
                    for &cluster_id in level {
                        let cluster = &plan.clusters[cluster_id];
                        let signature = ox_ontology::cluster_checkpoint::ClusterSignature::from_cluster(
                            cluster, &prompt_template_hash,
                        );
                        let cached = match state
                            .store
                            .find_draft_cluster_checkpoint_by_signature(
                                id,
                                &project_source_id,
                                signature.as_str(),
                            )
                            .await
                        {
                            Ok(Some(row)) => match serde_json::from_value::<
                                ox_ontology::input::InputOntologyDef,
                            >(row.output)
                            {
                                Ok(ir) => Some(ir),
                                Err(e) => {
                                    warn!(
                                        project_id = %id,
                                        cluster = cluster_id,
                                        error = %e,
                                        "Checkpoint deserialise failed; rerunning LLM"
                                    );
                                    None
                                }
                            },
                            Ok(None) => None,
                            Err(e) => {
                                warn!(
                                    project_id = %id,
                                    cluster = cluster_id,
                                    error = %e,
                                    "Checkpoint lookup failed; rerunning LLM"
                                );
                                None
                            }
                        };
                        if let Some(ir) = cached {
                            info!(project_id = %id, cluster = cluster_id, "Cluster checkpoint cache hit");
                            level_results.push((cluster_id, ir));
                            continue;
                        }
                        let batch_input = match build_batch_llm_input(
                            &schema,
                            &profile,
                            cluster,
                            &sys_config_snapshot,
                        ) {
                            Ok(data) => data,
                            Err(e) => {
                                yield Ok(Event::default().event("error").data(
                                    sse_error("design_error", &format!("Cluster {} input failed: {e:?}", cluster_id))
                                ));
                                return;
                            }
                        };
                        let cross =
                            format_cross_fks(&cluster.cross_fks, cluster, &batch_results);
                        miss_tasks.push((
                            cluster_id,
                            signature.as_str().to_string(),
                            batch_input,
                            cross,
                        ));
                    }

                    if !miss_tasks.is_empty() {
                        // Run LLM calls with bounded concurrency to avoid API rate limits
                        let max_concurrent = 5;
                        let semaphore =
                            std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
                        let futs: Vec<_> = miss_tasks.iter().map(|(_, _, batch_input, cross)| {
                            let sem = semaphore.clone();
                            let brain = state.brain.clone();
                            let ctx = effective_context.clone();
                            let ex = existing.clone();
                            let bi = batch_input.clone();
                            let cr = cross.clone();
                            let t = timeout;
                            async move {
                                // `acquire().await` only errors when the
                                // semaphore is closed — which we never do in this
                                // scope. If it ever happens, drop the concurrency
                                // bound and proceed so we never silently deadlock.
                                let _permit = sem.acquire().await.ok();
                                tokio::time::timeout(t, brain.design_ontology_batch(&bi, &ctx, &ex, &cr)).await
                            }
                        }).collect();

                        let results = futures::future::join_all(futs).await;

                        for (idx, result) in results.into_iter().enumerate() {
                            let (cluster_id, signature_str, _, _) = &miss_tasks[idx];
                            match result {
                                Ok(Ok(ir)) => {
                                    info!(project_id = %id, cluster = cluster_id, nodes = ir.node_types.len(), "Parallel batch completed");
                                    if let Some(ws_id) = checkpoint_workspace_id {
                                        persist_cluster_checkpoint(
                                            state.store.as_ref(),
                                            ws_id,
                                            id,
                                            &project_source_id,
                                            signature_str,
                                            *cluster_id,
                                            &ir,
                                        )
                                        .await;
                                    }
                                    level_results.push((*cluster_id, ir));
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

            // Phase 5: Normalize (single pass) — source_id stamps the
            // canonical object_mappings emitted by this normalization.
            match ox_ontology::normalize(merged, &source_id) {
                Ok(nr) => {
                    let errors = nr.ontology.validate();
                    if !errors.is_empty() {
                        Err(ox_core::OxError::Ontology {
                            message: format!(
                                "Batch-designed ontology validation errors: {}",
                                ox_core::join_messages(&errors, "; ")
                            ),
                        })
                    } else {
                        Ok(DesignOntologyOutput {
                            ontology: nr.ontology,
                            provenance: batch_provenance.clone(),
                        })
                    }
                }
                Err(errors) => Err(ox_core::OxError::Ontology {
                    message: format!(
                        "Batch-designed ontology normalization failed: {}",
                        ox_core::join_messages(&errors, "; ")
                    ),
                }),
            }
        } else {
            // `use_batch` was derived from `schema_and_profile.is_some()`, so
            // this branch is unreachable by construction. Fail loudly if it
            // ever executes rather than unwrapping silently.
            Err(ox_core::OxError::Runtime {
                message: "internal: batch path entered without schema+profile snapshot".into(),
            })
        };

        let DesignOntologyOutput { mut ontology, provenance } = match design_result {
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

        // Push operator-confirmed PII annotations into the resulting
        // ontology — sets `pii_kind` and `classification` on every
        // matching property via the canonical object-mapping lookup.
        if !effective_opts.pii_annotations.is_empty() {
            let object_mappings = ontology.object_mappings().to_vec();
            let classified = ox_ontology::source_analysis::apply_pii_annotations(
                &mut ontology,
                &effective_opts.pii_annotations,
                &object_mappings,
            );
            if classified > 0 {
                info!(
                    project_id = %id,
                    classified,
                    "Applied PII annotations to ontology properties"
                );
            }
        }

        // Author the source-to-IR mapping artifact for this design run.
        // Hash pivots on the schema we just designed against — the post-
        // exclusion schema, which is what the LLM actually saw. A future
        // re-run with the same operator decisions hashes to the same
        // value and the store collapses to a single row.
        //
        // Text sources skip artifact creation: no SourceSchema = no
        // structural mapping decisions to record. The provenance for
        // text designs lives in audit + ontology metadata instead.
        if let Some(schema_for_artifact) = schema_and_profile.as_ref().map(|(s, _)| s.clone()) {
            persist_design_artifact(
                state.store.as_ref(),
                &ontology,
                &source_id,
                &schema_for_artifact,
                provenance,
                principal.id.clone(),
            )
            .await;
        }

        yield Ok(Event::default().event("phase").data(
            sse_phase("assessing_quality", None)
        ));

        let quality_report = match assess_quality_from_project_with_mapping(
            &project,
            &ontology,
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
            .update_design_result(id, &ontology_json, Some(&qr_json), revision)
            .await
        {
            yield Ok(Event::default().event("error").data(
                sse_error("persist_error", &e.to_string())
            ));
            return;
        }

        // ADR-0027 — design completed; the cached checkpoints are no
        // longer authoritative (the project rolled forward, the next
        // pass starts from the persisted result, not from cache).
        // Drop them eagerly. Failures here are non-fatal: stale rows
        // get swept by the daily cleanup cron via `expires_at`.
        if let Err(e) = state
            .store
            .delete_draft_cluster_checkpoints_for_project(id)
            .await
        {
            warn!(
                project_id = %id,
                error = %e,
                "Failed to drop draft cluster checkpoints after design completion"
            );
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
            sse_result(&DesignProjectResponse {
                project: ProjectView::from_project(updated),
            })
        ));
    };

    Ok(Sse::new(scope_stream(ws_scope, stream))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(30))))
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
//   result             → RefineProjectResponse
//   uncertain_reconcile → { report, reconciled_ontology }
//   error              → { error: { type, message } }
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/refine/stream",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = RefineProjectRequest,
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
    Json(req): Json<RefineProjectRequest>,
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
        let node_count = ontology.node_types().len();
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

    // See `design_project_stream` — capture workspace scope before
    // returning the Sse so per-poll store/runtime calls re-enter
    // task-locals.
    let ws_scope = WsScope::capture();

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("phase").data(sse_phase("validating", None)));

        let refine_started = Instant::now();
        let node_count = ontology.node_types().len();
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

        let source_id = ox_ontology::mapping::SourceId::new(project.source_id.clone());

        let llm_started = Instant::now();
        let llm_result = tokio::time::timeout(
            timeout,
            state.brain.refine_ontology(&ontology, &refinement_context, &source_id),
        )
        .await;

        let llm_refined = match llm_result {
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

        let reconciled = match ox_ontology::command::reconcile_refined(&ontology, llm_refined) {
            Ok(r) => r,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    sse_error("reconcile_error", &format!("{e}"))
                ));
                return;
            }
        };

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
            sse_result(&RefineProjectResponse {
                project: ProjectView::from_project(updated),
                profile_summary,
                reconcile_report: reconciled.report,
            })
        ));
    };

    Ok(Sse::new(scope_stream(ws_scope, stream))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(30))))
}

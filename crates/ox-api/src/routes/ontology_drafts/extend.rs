use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use tokio::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

use ox_ontology::ontology_draft::{OntologyDraftStatus, SourceHistoryEntry};
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::SourceId;
use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_source::AnalysisResult;
use ox_source::analyzer::build_design_context;

use super::helpers::artifact::persist_design_artifact;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

use super::helpers::{
    LlmInputContext, analyze_code_repository, analyze_source, build_llm_input, get_design_options,
    load_ontology_draft_in_status, reload_ontology_draft,
};
use super::types::{ExtendOntologyDraftRequest, ExtendOntologyDraftResponse, DataSourceSpec, OntologyDraftView};

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/extend
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/extend",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = ExtendOntologyDraftRequest,
    responses(
        (status = 200, description = "Ontology extended with new source", body = ExtendOntologyDraftResponse),
        (status = 400, description = "No ontology or empty source data", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "LLM timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn extend_ontology_draft(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ExtendOntologyDraftRequest>,
) -> Result<Json<ApiResponse<ExtendOntologyDraftResponse>>, AppError> {
    principal.require_designer()?;
    req.selection.validate().map_err(AppError::from)?;
    let project = load_ontology_draft_in_status(&state, id, OntologyDraftStatus::Designed).await?;

    // Snapshot current state before mutation (best-effort)
    if let Some(ont) = &project.ontology
        && let Err(e) = state
            .store
            .create_ontology_snapshot(
                id,
                project.revision,
                ont,
                project.quality_report.as_ref(),
            )
            .await
    {
        warn!(ontology_draft_id = %id, error = %e, "Failed to save ontology snapshot");
    }

    // Existing ontology is required
    let existing_ontology: OntologyIR = match project.ontology.as_ref() {
        None => return Err(AppError::no_ontology()),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| AppError::internal(format!("Corrupt ontology in project: {e}")))?,
    };

    // 1. Introspect the new source (including Code Repository).
    //
    // When the project carries a structured `source_schema` /
    // `source_profile` from a previous introspection, reconstruct
    // it as the kernel's `baseline` so an
    // `AnalyzeSelection::Extend { tables }` can recover cross-
    // baseline foreign keys (an Order table newly added to a
    // baseline that already had Customer must surface the Order →
    // Customer FK even though only Order is in the new scope).
    // Code-Repository / Text sources skip the baseline.
    let source_url = match &req.source {
        DataSourceSpec::CodeRepository { url } => Some(url.clone()),
        _ => None,
    };
    let baseline = build_extend_baseline(&project);
    let (new_source_config, new_source_data, new_source_schema, new_source_profile, mut new_report) =
        if let Some(url) = &source_url {
            let (config, schema, profile, report) = analyze_code_repository(&state, url).await?;
            (config, None, Some(schema), Some(profile), Some(report))
        } else {
            let analyzed = analyze_source(
                req.source,
                &state.adapter_registry,
                req.selection.clone(),
                baseline.as_ref(),
            )
            .await?;
            (
                analyzed.config,
                analyzed.raw_data,
                analyzed.schema,
                analyzed.profile,
                analyzed.report,
            )
        };

    // Eager drift detection: compare the new profile against the
    // existing ontology's value-set bindings. A new sample value
    // outside the bound set means the derived `InValueSet` rule
    // would silently reject it on write — surface as an analysis
    // warning so the operator reviews the binding before the next
    // deploy.
    if let (Some(profile), Some(report)) = (&new_source_profile, new_report.as_mut()) {
        let drift_warnings =
            ox_ontology::detect_value_set_drift(&existing_ontology, profile);
        if !drift_warnings.is_empty() {
            warn!(
                ontology_draft_id = %id,
                drift_count = drift_warnings.len(),
                "Value-set drift detected — derived rules may silently reject samples"
            );
            report.analysis_warnings.extend(drift_warnings);
        }
    }

    // 2. Build LLM input directly from the new source data (no temp struct needed)
    let existing_opts = get_design_options(&project);
    let new_schema_json = new_source_schema
        .as_ref()
        .map(AppError::to_json)
        .transpose()?;
    let new_profile_json = new_source_profile
        .as_ref()
        .map(AppError::to_json)
        .transpose()?;
    let new_report_json = new_report.as_ref().map(AppError::to_json).transpose()?;

    let sample_data = {
        let ctx = LlmInputContext {
            source_data: new_source_data.as_deref(),
            source_schema: new_schema_json.as_ref(),
            source_profile: new_profile_json.as_ref(),
            analysis_report: new_report_json.as_ref(),
        };
        let sys_config = state.system_config.read().await;
        build_llm_input(&ctx, &new_source_config, &existing_opts, &sys_config)?
    };

    // Log any warnings from the new source analysis
    if let Some(report) = &new_report {
        if !report.analysis_warnings.is_empty() {
            warn!(
                ontology_draft_id = %id,
                warning_count = report.analysis_warnings.len(),
                "New source analysis produced warnings"
            );
        }
        if report.is_partial() {
            warn!(ontology_draft_id = %id, "New source analysis is partial");
        }
    }

    if sample_data.trim().is_empty() {
        return Err(AppError::empty_source_data());
    }

    // 3. Build context — the LLM also receives the existing
    //    ontology as a structured `existing_ontology` field on
    //    `DesignOntologyInput` (compact label list rendered by
    //    `render_existing_ontology_section`), so we no longer dump
    //    the full IR JSON into the freeform context. Stays much
    //    cheaper on the token budget and gives the model a cleaner
    //    "extend, don't redefine" signal.
    let context = "You are extending an existing ontology with data from a new source. \
         Design new entities and relationships for the new source data. You may create \
         edges connecting new entities to existing ones where appropriate. Do NOT \
         duplicate entities that already exist in the current ontology — the existing \
         labels appear in the `Existing Ontology (extension mode)` section below.";

    let effective_context = build_design_context(context, &existing_opts, None);

    // 4. Call design_ontology with the new source data + every
    //    domain artefact the existing ontology already carries.
    //    Passing `glossary` and `code_systems` slices lets the LLM
    //    reuse canonical terms instead of inventing parallel ones;
    //    `existing_ontology` flips the prompt into extension mode.
    info!(ontology_draft_id = %id, "Extending ontology with new source");

    let timeout =
        std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
    let design_started = Instant::now();
    // The extend flow attaches a NEW source to an existing project.
    // SourceId for the new source comes from its config fingerprint
    // so federation plan caches keyed by source agree with the
    // object_mappings that normalize() stamps with this id.
    let new_source_id = SourceId::from_source_config(&new_source_config);
    let design_input = ox_brain::DesignOntologyInput {
        sample_data: &sample_data,
        context: &effective_context,
        source_id: &new_source_id,
        glossary_terms: existing_ontology.glossary(),
        code_systems: existing_ontology.code_systems(),
        // Ambiguity wiring lands with the planner-diagnostic hook —
        // until that lands, no pre-detected ambiguities are surfaced
        // to the design call from the extend path.
        ambiguity_hints: &[],
        existing_ontology: Some(&existing_ontology),
    };
    let design_output = tokio::time::timeout(
        timeout,
        state.brain.design_ontology(&design_input),
    )
    .await
    .map_err(|_| {
        warn!(
            ontology_draft_id = %id,
            elapsed_ms = design_started.elapsed().as_millis() as u64,
            timeout_secs = timeout.as_secs(),
            "Extend LLM call timed out"
        );
        AppError::timeout(format!(
            "Ontology extension timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(AppError::from)?;

    let ox_brain::DesignOntologyOutput { ontology: new_ontology, provenance } = design_output;

    info!(
        ontology_draft_id = %id,
        design_ms = design_started.elapsed().as_millis() as u64,
        new_nodes = new_ontology.node_types().len(),
        new_edges = new_ontology.edge_types().len(),
        "LLM extension design completed"
    );

    // Author the source-to-IR mapping artifact for the new source.
    // Captures the LLM's per-property and per-edge decisions against
    // the schema hash so a future replay can short-circuit re-prompt.
    if let Some(schema) = new_source_schema.as_ref() {
        persist_design_artifact(
            state.store.as_ref(),
            &new_ontology,
            &new_source_id,
            schema,
            provenance,
            principal.id.clone(),
        )
        .await;
    }

    // 5. Reconcile: merge new ontology with existing (preserves existing IDs).
    //    ObjectMappingDef entries on both sides carry distinct
    //    `source_id` stamps, so the reconcile + canonical mapping
    //    layer resolves multi-source topology naturally — federation
    //    planner already honours precedence + id uniqueness.
    let reconciled = ox_ontology::command::reconcile_refined(&existing_ontology, new_ontology)
        .map_err(|e| AppError::internal(format!("Reconcile produced invalid ontology: {e}")))?;

    let merged = reconciled.ontology;

    // 6. Merge source schemas and profiles so quality assessment covers both sources
    let mut merged_schema: SourceSchema = project
        .source_schema
        .as_ref()
        .map(|v| serde_json::from_value::<SourceSchema>(v.clone()))
        .transpose()
        .map_err(|e| AppError::internal(format!("Corrupt source_schema: {e}")))?
        .unwrap_or_else(|| SourceSchema {
            source_type: String::new(),
            tables: Vec::new(),
            foreign_keys: Vec::new(),
        });
    if let Some(new_schema) = &new_source_schema {
        for table in &new_schema.tables {
            if !merged_schema.tables.iter().any(|t| t.name == table.name) {
                merged_schema.tables.push(table.clone());
            }
        }
        for fk in &new_schema.foreign_keys {
            if !merged_schema.foreign_keys.iter().any(|f| {
                f.from_table == fk.from_table
                    && f.from_column == fk.from_column
                    && f.to_table == fk.to_table
            }) {
                merged_schema.foreign_keys.push(fk.clone());
            }
        }
    }

    let mut merged_profile: SourceProfile = project
        .source_profile
        .as_ref()
        .map(|v| serde_json::from_value::<SourceProfile>(v.clone()))
        .transpose()
        .map_err(|e| AppError::internal(format!("Corrupt source_profile: {e}")))?
        .unwrap_or_else(|| SourceProfile {
            table_profiles: Vec::new(),
        });
    if let Some(new_profile) = &new_source_profile {
        for stat in &new_profile.table_profiles {
            if !merged_profile
                .table_profiles
                .iter()
                .any(|s| s.table_name == stat.table_name)
            {
                merged_profile.table_profiles.push(stat.clone());
            }
        }
    }

    // 8. Re-assess quality with merged schema/profile. Quality reads
    //    mapping information directly from the canonical
    //    ObjectMappingDef list on the merged ontology.
    let quality_report = ox_ontology::quality::assess_quality(
        &merged,
        Some(&merged_schema),
        Some(&merged_profile),
        merged.object_mappings(),
        &existing_opts.excluded_tables,
        &existing_opts.column_clarifications,
        &ox_ontology::quality::QualityConfig::default(),
    );

    // 9. Build source history entry for the new source
    let new_history_entry = SourceHistoryEntry {
        source_type: new_source_config.source_type.clone(),
        added_at: Utc::now(),
        schema_name: new_source_config.schema_name.clone(),
        url: source_url,
        fingerprint: new_source_config.source_fingerprint.clone(),
    };

    let mut history: Vec<SourceHistoryEntry> =
        serde_json::from_value(project.source_history.clone()).unwrap_or_default();
    history.push(new_history_entry);

    // Roll the analysis scope forward: extend always preserves the
    // prior scope, adds the newly-selected tables to `included`,
    // and refreshes every fingerprint against the merged schema.
    let now = chrono::Utc::now();
    let scope_json = {
        let mut scope: ox_source::AnalysisScope =
            serde_json::from_value(project.analysis_scope.clone()).unwrap_or_default();
        let all_tables: std::collections::BTreeSet<String> = merged_schema
            .tables
            .iter()
            .map(|t| t.name.clone())
            .collect();
        scope.record_selection(&req.selection, &all_tables, now);
        scope.record_fingerprints(merged_schema.tables.iter().map(|t| {
            (
                t.name.clone(),
                ox_core::source_schema::table_fingerprint(t),
            )
        }));
        AppError::to_json(&scope)?
    };

    // 10. Persist — merged schema/profile + updated source history
    //     and the rolled-forward analysis scope. Mapping state lives
    //     inside `ontology` (object_mappings), so ExtendResult no
    //     longer carries a separate blob.
    let extend_result = ox_store::store::ExtendResult {
        ontology: AppError::to_json(&merged)?,
        quality_report: AppError::to_json(&quality_report)?,
        source_schema: AppError::to_json(&merged_schema)?,
        source_profile: AppError::to_json(&merged_profile)?,
        source_history: AppError::to_json(&history)?,
        analysis_scope: scope_json,
    };
    state
        .store
        .update_extend_result(id, &extend_result, req.revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_ontology_draft(&state, id).await?;

    info!(
        ontology_draft_id = %id,
        total_ms = design_started.elapsed().as_millis() as u64,
        "Extend completed"
    );

    Ok(ApiResponse::of(ExtendOntologyDraftResponse {
        project: OntologyDraftView::from_ontology_draft(updated),
        reconcile_report: reconciled.report,
    }))
}

/// Reconstruct the kernel's `AnalysisResult` baseline from the
/// project's stored introspection rows so the kernel's
/// `analyze_extension` path runs the cross-baseline foreign-key
/// recovery.
///
/// Returns `None` when the project lacks a structured schema /
/// profile (text-source projects, projects whose initial
/// introspection failed). Decode failures also fall back to `None`
/// — the extend call still succeeds, just without cross-baseline
/// FK recovery; the alternative (rejecting the request) would
/// hand-stitch an unrecoverable failure mode for a corruption the
/// operator cannot fix from the FE.
fn build_extend_baseline(project: &ox_store::OntologyDraft) -> Option<AnalysisResult> {
    let schema_json = project.source_schema.as_ref()?;
    let profile_json = project.source_profile.as_ref()?;
    let schema: SourceSchema = serde_json::from_value(schema_json.clone()).ok()?;
    let profile: SourceProfile = serde_json::from_value(profile_json.clone()).ok()?;
    Some(AnalysisResult {
        schema,
        profile,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::build_extend_baseline;

    fn schema_value() -> serde_json::Value {
        serde_json::json!({
            "source_type": "postgresql",
            "tables": [
                {
                    "name": "users",
                    "columns": [],
                    "primary_key": [],
                }
            ],
            "foreign_keys": [],
        })
    }

    fn profile_value() -> serde_json::Value {
        serde_json::json!({"table_profiles": []})
    }

    fn project(
        schema: Option<serde_json::Value>,
        profile: Option<serde_json::Value>,
    ) -> ox_store::OntologyDraft {
        ox_store::OntologyDraft {
            id: uuid::Uuid::new_v4(),
            status: "designed".to_string(),
            revision: 1,
            user_id: "u-1".to_string(),
            title: None,
            source_config: serde_json::json!({}),
            source_id: "src-1".to_string(),
            source_data: None,
            source_schema: schema,
            source_profile: profile,
            analysis_report: None,
            design_options: serde_json::json!({}),
            analysis_scope: serde_json::json!({}),
            ontology: None,
            quality_report: None,
            parent_version_id: None,
            committed_version_id: None,
            source_history: serde_json::json!([]),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            analyzed_at: None,
        }
    }

    #[test]
    fn baseline_round_trips_when_project_has_schema_and_profile() {
        let baseline = build_extend_baseline(&project(
            Some(schema_value()),
            Some(profile_value()),
        ))
        .expect("baseline rebuilds when both rows present");
        assert_eq!(baseline.schema.tables.len(), 1);
        assert_eq!(baseline.schema.tables[0].name, "users");
    }

    #[test]
    fn baseline_returns_none_when_either_row_missing() {
        // Schema present, profile missing — extend cannot recover
        // FKs without sample-driven profile, so baseline collapses
        // to None and the extend call proceeds without
        // cross-baseline recovery (the same shape as a fresh
        // project's first source).
        assert!(build_extend_baseline(&project(Some(schema_value()), None)).is_none());
        assert!(build_extend_baseline(&project(None, Some(profile_value()))).is_none());
        assert!(build_extend_baseline(&project(None, None)).is_none());
    }

    #[test]
    fn baseline_returns_none_on_corrupt_schema_payload() {
        // A wire shape that doesn't deserialise must not crash the
        // extend handler — fall through to the no-baseline path.
        let baseline = build_extend_baseline(&project(
            Some(serde_json::json!({"this_is_not_a_schema": true})),
            Some(profile_value()),
        ));
        assert!(baseline.is_none());
    }
}

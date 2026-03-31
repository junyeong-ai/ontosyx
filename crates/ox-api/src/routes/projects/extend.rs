use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use tokio::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

use ox_core::design_project::{DesignProjectStatus, SourceHistoryEntry};
use ox_core::ontology_ir::OntologyIR;
use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_source::analyzer::build_design_context;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

use super::helpers::{
    LlmInputContext, analyze_code_repository, analyze_source, build_llm_input, get_design_options,
    load_project_in_status, reload_project,
};
use super::types::{ProjectExtendRequest, ProjectExtendResponse, ProjectSource};

// ---------------------------------------------------------------------------
// POST /api/projects/:id/extend
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/extend",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectExtendRequest,
    responses(
        (status = 200, description = "Ontology extended with new source", body = ProjectExtendResponse),
        (status = 400, description = "No ontology or empty source data", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "LLM timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn extend_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectExtendRequest>,
) -> Result<Json<ProjectExtendResponse>, AppError> {
    principal.require_designer()?;
    let project = load_project_in_status(&state, id, DesignProjectStatus::Designed).await?;

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

    // Existing ontology is required
    let existing_ontology: OntologyIR = match project.ontology.as_ref() {
        None => return Err(AppError::no_ontology()),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| AppError::internal(format!("Corrupt ontology in project: {e}")))?,
    };

    // 1. Introspect the new source (including Code Repository)
    // Extract URL before source is consumed (for source history)
    let source_url = match &req.source {
        ProjectSource::CodeRepository { url } => Some(url.clone()),
        _ => None,
    };
    let (new_source_config, new_source_data, new_source_schema, new_source_profile, new_report) =
        if let Some(url) = &source_url {
            let (config, schema, profile, report) = analyze_code_repository(&state, url).await?;
            (config, None, Some(schema), Some(profile), Some(report))
        } else {
            analyze_source(req.source, &state.introspector_registry).await?
        };

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
                project_id = %id,
                warning_count = report.analysis_warnings.len(),
                "New source analysis produced warnings"
            );
        }
        if report.is_partial() {
            warn!(project_id = %id, "New source analysis is partial");
        }
    }

    if sample_data.trim().is_empty() {
        return Err(AppError::empty_source_data());
    }

    // 3. Build context: include the existing ontology so the LLM knows what already exists
    let existing_ontology_json = serde_json::to_string_pretty(&existing_ontology)
        .map_err(|e| AppError::internal(format!("Failed to serialize existing ontology: {e}")))?;

    let context = format!(
        "You are extending an existing ontology with data from a new source. \
         The existing ontology is provided below. Design new entities and relationships \
         for the new source data. You may create edges connecting new entities to existing ones \
         where appropriate. Do NOT duplicate entities that already exist in the current ontology.\n\n\
         EXISTING ONTOLOGY:\n{existing_ontology_json}"
    );

    let effective_context = build_design_context(&context, &existing_opts, None);

    // 4. Call design_ontology with the new source data + existing ontology as context
    info!(project_id = %id, "Extending ontology with new source");

    let timeout =
        std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());
    let design_started = Instant::now();
    let (new_ontology, new_source_mapping) = tokio::time::timeout(
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
            "Extend LLM call timed out"
        );
        AppError::timeout(format!(
            "Ontology extension timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(AppError::from)?;

    info!(
        project_id = %id,
        design_ms = design_started.elapsed().as_millis() as u64,
        new_nodes = new_ontology.node_types.len(),
        new_edges = new_ontology.edge_types.len(),
        "LLM extension design completed"
    );

    // 5. Reconcile: merge new ontology with existing (preserves existing IDs)
    let reconciled = ox_core::ontology_command::reconcile_refined(&existing_ontology, new_ontology);

    let merged = reconciled.ontology;

    // 6. Merge source mappings: existing + new
    let existing_mapping: ox_core::source_mapping::SourceMapping = project
        .source_mapping
        .as_ref()
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| AppError::internal(format!("Corrupt source_mapping: {e}")))?
        .unwrap_or_default();

    let mut merged_mapping = existing_mapping;
    for (k, v) in new_source_mapping.node_tables {
        merged_mapping.node_tables.entry(k).or_insert(v);
    }
    for (k, v) in new_source_mapping.property_columns {
        merged_mapping.property_columns.entry(k).or_insert(v);
    }

    // 7. Merge source schemas and profiles so quality assessment covers both sources
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

    // 8. Re-assess quality with merged schema/profile
    let quality_report = ox_core::quality::assess_quality(
        &merged,
        Some(&merged_schema),
        Some(&merged_profile),
        &merged_mapping,
        &existing_opts.excluded_tables,
        &existing_opts.column_clarifications,
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

    // 10. Persist — includes merged source schema/profile and updated source history
    let extend_result = ox_store::store::ExtendResult {
        ontology: AppError::to_json(&merged)?,
        source_mapping: AppError::to_json(&merged_mapping)?,
        quality_report: AppError::to_json(&quality_report)?,
        source_schema: AppError::to_json(&merged_schema)?,
        source_profile: AppError::to_json(&merged_profile)?,
        source_history: AppError::to_json(&history)?,
    };
    state
        .store
        .update_extend_result(id, &extend_result, req.revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    info!(
        project_id = %id,
        total_ms = design_started.elapsed().as_millis() as u64,
        "Extend completed"
    );

    Ok(Json(ProjectExtendResponse {
        project: updated,
        reconcile_report: reconciled.report,
    }))
}

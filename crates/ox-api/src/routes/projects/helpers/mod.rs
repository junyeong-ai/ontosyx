pub(crate) mod artifact;
mod decisions;
mod fingerprint;
mod llm;
mod quality;
mod repo;
mod source;

use uuid::Uuid;

use ox_ontology::design_project::DesignProjectStatus;
use ox_store::DesignProject;

use crate::error::AppError;
use crate::state::AppState;

// Re-export all public items so that `super::helpers::{...}` imports continue to work.
pub(crate) use self::decisions::{
    build_refinement_context, build_source_schema_summary, enforce_design_gates, prune_decisions,
    validate_decisions,
};
pub(crate) use self::llm::{
    LlmInputContext, build_batch_llm_input, build_llm_input, find_uncovered_cross_fks,
    format_cross_fks, format_existing_edges_for_resolution, format_existing_nodes,
    format_node_labels_for_resolution, format_uncovered_fks, merge_input_irs,
};
pub(crate) use self::quality::{
    assess_quality_from_project, assess_quality_from_project_with_mapping,
};
pub(crate) use self::repo::{analyze_code_repository, run_repo_enrichment, skipped_repo_summary};
pub(crate) use self::source::{analyze_source, build_adapter};

/// Extract `DesignOptions` from a project's JSON field, falling back to defaults.
pub(crate) fn get_design_options(
    project: &ox_store::DesignProject,
) -> ox_ontology::source_analysis::DesignOptions {
    serde_json::from_value(project.design_options.clone()).unwrap_or_default()
}

/// Deserialise the project's stored `analysis_report` JSON against the
/// current wire shape. Returns `Ok(None)` when the project has no
/// report yet (typical for `BaseOntology`-origin projects) **or**
/// when the persisted JSON cannot be parsed against the current
/// schema. The latter case is treated as graceful degradation rather
/// than a hard failure: the design / refine / extend handlers
/// already model "no analysis report" as a valid state, so a row
/// written under an older schema simply joins that branch — gates
/// don't enforce, but the operator can proceed and re-run analysis
/// from the workflow when the project reaches `designed` status.
///
/// The parse error is logged at `warn` so operators / observability
/// see the schema drift without surfacing it as a blocking 422.
pub(crate) fn load_analysis_report(
    project: &ox_store::DesignProject,
) -> Option<ox_ontology::source_analysis::SourceAnalysisReport> {
    let value = project.analysis_report.as_ref()?;
    match serde_json::from_value::<ox_ontology::source_analysis::SourceAnalysisReport>(
        value.clone(),
    ) {
        Ok(report) => Some(report),
        Err(error) => {
            tracing::warn!(
                project_id = %project.id,
                ?error,
                "Stored analysis_report does not match the current wire shape — \
                 treating as absent. Re-run analyse to refresh."
            );
            None
        }
    }
}

/// Load a project for mutation. Completed projects are allowed — editing
/// a completed project will revert it to "designed" status (unpublish).
pub(crate) async fn load_mutable_project(
    state: &AppState,
    id: Uuid,
) -> Result<DesignProject, AppError> {
    reload_project(state, id).await
}

/// Load a project that must be in a specific status.
pub(crate) async fn load_project_in_status(
    state: &AppState,
    id: Uuid,
    required: DesignProjectStatus,
) -> Result<DesignProject, AppError> {
    let project = load_mutable_project(state, id).await?;

    if project.status.parse::<DesignProjectStatus>().ok() != Some(required) {
        return Err(AppError::bad_request(format!(
            "Project must be in '{}' status",
            required
        )));
    }

    Ok(project)
}

/// Reload a project from the store (typically after a mutation).
pub(crate) async fn reload_project(state: &AppState, id: Uuid) -> Result<DesignProject, AppError> {
    state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)
}

/// Pick the next ontology version tag after `previous`. Integer-only
/// deployments bump the counter by one; everything else falls back to
/// `{previous}+<epoch>` so the TEXT column stays unique without forcing
/// a semver parser on callers. Shared between the design-lifecycle
/// completion flow and ad-hoc schema-ops paths (enrichment, etc).
pub(crate) fn next_ontology_version_tag(previous: &str) -> String {
    match previous.parse::<u64>() {
        Ok(n) => (n + 1).to_string(),
        Err(_) => format!("{previous}+{}", chrono::Utc::now().timestamp()),
    }
}

mod decisions;
mod fingerprint;
mod llm;
mod quality;
mod repo;
mod source;

use uuid::Uuid;

use ox_core::design_project::DesignProjectStatus;
use ox_store::DesignProject;

use crate::error::AppError;
use crate::state::AppState;

// Re-export all public items so that `super::helpers::{...}` imports continue to work.
pub(crate) use self::decisions::{
    build_refinement_context, build_source_schema_summary, maybe_require_review, prune_decisions,
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
pub(crate) use self::source::analyze_source;

/// Extract `DesignOptions` from a project's JSON field, falling back to defaults.
pub(crate) fn get_design_options(
    project: &ox_store::DesignProject,
) -> ox_core::source_analysis::DesignOptions {
    serde_json::from_value(project.design_options.clone()).unwrap_or_default()
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
pub(crate) async fn reload_project(
    state: &AppState,
    id: Uuid,
) -> Result<DesignProject, AppError> {
    state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)
}

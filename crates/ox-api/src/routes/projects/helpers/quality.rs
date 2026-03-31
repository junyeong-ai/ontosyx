use ox_core::ontology_ir::OntologyIR;
use ox_core::quality::{OntologyQualityReport, assess_quality};
use ox_core::source_analysis::ColumnClarification;
use ox_core::source_mapping::SourceMapping;
use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_store::DesignProject;

use crate::error::AppError;

/// Assess quality from a project's stored source schema, profile, and source mapping.
/// Always produces a quality report. For sources without schema/profile (e.g., text),
/// ontology-level checks (missing descriptions, etc.) still run.
pub(crate) fn assess_quality_from_project(
    project: &DesignProject,
    ontology: &OntologyIR,
    excluded_tables: &[String],
    column_clarifications: &[ColumnClarification],
) -> Result<OntologyQualityReport, AppError> {
    let mapping: SourceMapping = project
        .source_mapping
        .as_ref()
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| AppError::internal(format!("Corrupt source_mapping in project: {e}")))?
        .unwrap_or_default();
    assess_quality_from_project_with_mapping(
        project,
        ontology,
        &mapping,
        excluded_tables,
        column_clarifications,
    )
}

pub(crate) fn assess_quality_from_project_with_mapping(
    project: &DesignProject,
    ontology: &OntologyIR,
    source_mapping: &SourceMapping,
    excluded_tables: &[String],
    column_clarifications: &[ColumnClarification],
) -> Result<OntologyQualityReport, AppError> {
    let schema: Option<SourceSchema> = project
        .source_schema
        .as_ref()
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| AppError::internal(format!("Corrupt source_schema in project: {e}")))?;

    let profile: Option<SourceProfile> = project
        .source_profile
        .as_ref()
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| AppError::internal(format!("Corrupt source_profile in project: {e}")))?;

    Ok(assess_quality(
        ontology,
        schema.as_ref(),
        profile.as_ref(),
        source_mapping,
        excluded_tables,
        column_clarifications,
    ))
}

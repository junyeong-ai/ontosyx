use ox_ontology::ir::OntologyIR;
use ox_ontology::quality::{OntologyQualityReport, assess_quality};
use ox_ontology::source_analysis::ColumnClarification;
use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_store::OntologyDraft;

use crate::error::AppError;

/// Assess quality from a project's stored source schema, profile, and
/// the canonical object_mappings carried inside the ontology itself.
/// For sources without schema/profile (e.g. text), ontology-level
/// checks (missing descriptions, etc.) still run.
pub(crate) fn assess_quality_from_ontology_draft(
    project: &OntologyDraft,
    ontology: &OntologyIR,
    excluded_tables: &[String],
    column_clarifications: &[ColumnClarification],
) -> Result<OntologyQualityReport, AppError> {
    assess_quality_from_ontology_draft_with_mapping(
        project,
        ontology,
        excluded_tables,
        column_clarifications,
    )
}

/// The `_with_mapping` suffix is a historical name kept so existing
/// call sites don't churn — mapping now travels on the ontology IR,
/// not through a separate parameter. New call sites should prefer
/// `assess_quality_from_ontology_draft`; the two are identical.
pub(crate) fn assess_quality_from_ontology_draft_with_mapping(
    project: &OntologyDraft,
    ontology: &OntologyIR,
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
        ontology.object_mappings(),
        excluded_tables,
        column_clarifications,
        &ox_ontology::quality::QualityConfig::default(),
    ))
}

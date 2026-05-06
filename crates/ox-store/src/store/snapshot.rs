//! Grouped parameter shapes for the [`super::OntologyDraftStore`]
//! analyse / extend / reanalyse paths. Bundled here so each
//! signature stays parameter-light without sprawling across
//! positional arguments.

/// Grouped parameters for `replace_analysis_snapshot`.
pub struct AnalysisSnapshot {
    pub source_config: serde_json::Value,
    /// Canonical source identity recomputed from `source_config`
    /// via `SourceId::from_source_config`. Reanalyze rewrites this
    /// when the fingerprint shifts so federation caches invalidate
    /// naturally on source replacement.
    pub source_id: String,
    pub source_data: Option<String>,
    pub source_schema: serde_json::Value,
    pub source_profile: serde_json::Value,
    pub analysis_report: serde_json::Value,
    pub design_options: serde_json::Value,
    /// `ox_source::AnalysisScope` accumulated across the draft's
    /// lifetime — every analyze / extend / reanalyze updates it
    /// with the new selection's tables and the post-introspection
    /// fingerprints so the FE renders progress + drift.
    pub analysis_scope: serde_json::Value,
}

/// Grouped parameters for `update_extend_result`.
pub struct ExtendResult {
    /// Canonical OntologyIR JSON — already carries object_mappings
    /// stamped with each source's SourceId, so no separate mapping
    /// blob travels alongside.
    pub ontology: serde_json::Value,
    pub quality_report: serde_json::Value,
    pub source_schema: serde_json::Value,
    pub source_profile: serde_json::Value,
    pub source_history: serde_json::Value,
    /// Updated [`ox_source::AnalysisScope`] — the prior scope plus
    /// every table the extend selection brought in, with refreshed
    /// fingerprints and `last_introspected_at`.
    pub analysis_scope: serde_json::Value,
}

//! Source-analysis scope helpers owned by the source layer.

pub use ox_core::source_scope::{
    AnalysisScope, AnalyzeSelection, DeferredTable, TableSchemaDrift, TableSchemaDriftKind,
    TableSelection,
};

use ox_ontology::source_analysis::{
    AnalysisPhase, AnalysisWarning, WarningClass, WarningLevel, WarningScope,
};

pub fn table_schema_drift_warnings(
    scope: &AnalysisScope,
    fresh: &std::collections::BTreeMap<String, String>,
) -> Vec<AnalysisWarning> {
    scope
        .detect_table_schema_drift(fresh)
        .into_iter()
        .map(|drift| {
            AnalysisWarning::new(
                WarningLevel::Warning,
                AnalysisPhase::SchemaIntrospection,
                WarningClass::TableSchemaDrift,
                WarningScope::Table { name: drift.table },
            )
            .with_param("kind", drift.kind.as_str())
        })
        .collect()
}

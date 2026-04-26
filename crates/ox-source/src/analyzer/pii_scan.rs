//! Run a [`PiiClassifier`] over an entire schema + profile and
//! collect every suggestion it emits. Pure orchestration — the
//! actual signal logic lives in
//! [`ox_ontology::pii`].

use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_ontology::pii::{PiiClassifier, PiiSignals, PiiSuggestion};

/// Walk every `(table, column)` pair in `schema` and ask
/// `classifier` for a suggestion. Sample values from `profile` (if
/// present for that column) feed the classifier alongside the
/// column's name + raw data type.
///
/// Per-column suggestions land in source order. A column that
/// produces no suggestion is silently absent — callers that need
/// "everything was inspected" can compare emitted entries against
/// the input column count.
pub fn scan_for_pii(
    schema: &SourceSchema,
    profile: &SourceProfile,
    classifier: &dyn PiiClassifier,
) -> Vec<PiiSuggestion> {
    let mut out = Vec::new();
    for table in &schema.tables {
        let table_profile = profile
            .table_profiles
            .iter()
            .find(|tp| tp.table_name == table.name);
        for column in &table.columns {
            let samples: &[String] = table_profile
                .and_then(|tp| {
                    tp.column_stats
                        .iter()
                        .find(|cs| cs.column_name == column.name)
                })
                .map(|cs| cs.sample_values.as_slice())
                .unwrap_or(&[]);
            let signals = PiiSignals {
                table: &table.name,
                column: &column.name,
                data_type: Some(&column.data_type),
                sample_values: samples,
            };
            if let Some(suggestion) = classifier.classify(&signals) {
                out.push(suggestion);
            }
        }
    }
    out
}

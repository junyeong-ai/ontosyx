use std::collections::{HashMap, HashSet};

use ox_core::quality::is_cryptic_short;
use ox_core::source_analysis::{AmbiguityType, AmbiguousColumn};
use ox_core::source_schema::{SourceProfile, SourceSchema};

pub(super) fn detect_ambiguous(
    schema: &SourceSchema,
    profile: &SourceProfile,
) -> Vec<AmbiguousColumn> {
    // Pre-build O(1) lookups to avoid repeated O(n) scans inside the hot column loop.
    let schema_table_map: HashMap<&str, _> =
        schema.tables.iter().map(|t| (t.name.as_str(), t)).collect();
    let declared_fk_set: HashSet<(&str, &str)> = schema
        .foreign_keys
        .iter()
        .map(|fk| (fk.from_table.as_str(), fk.from_column.as_str()))
        .collect();

    let mut results = Vec::new();

    for tp in &profile.table_profiles {
        // Resolve schema table once per table profile — not per column.
        let schema_table = schema_table_map.get(tp.table_name.as_str());

        for cs in &tp.column_stats {
            if cs.sample_values.is_empty() {
                continue;
            }

            // Skip PKs and FK columns (they have no semantic ambiguity)
            if let Some(table) = schema_table {
                let is_pk = table.primary_key.contains(&cs.column_name);
                let is_fk =
                    declared_fk_set.contains(&(tp.table_name.as_str(), cs.column_name.as_str()));
                if is_pk || is_fk {
                    continue;
                }
            }

            // NumericCode: all values parse as i64, low cardinality (2-20).
            //
            // Dual ID guard — intentional defense-in-depth:
            // 1. Schema-based (above): skips declared PKs/FKs via schema metadata.
            // 2. Name-based (below): skips `id` / `*_id` columns not declared in schema
            //    (e.g., implied FKs, schema introspection gaps, cross-DB sources).
            // Both guards are needed — neither alone is sufficient in all environments.
            let is_id_column = cs.column_name == "id" || cs.column_name.ends_with("_id");
            let all_numeric = cs.sample_values.iter().all(|v| v.parse::<i64>().is_ok());
            if !is_id_column && all_numeric && cs.distinct_count >= 2 && cs.distinct_count <= 20 {
                results.push(AmbiguousColumn {
                    table: tp.table_name.clone(),
                    column: cs.column_name.clone(),
                    ambiguity_type: AmbiguityType::NumericCode,
                    sample_values: cs.sample_values.clone(),
                    clarification_prompt: format!(
                        "Column `{}.{}` contains numeric codes [{}]. What does each value mean? \
                         (e.g., 1=active, 2=inactive, 3=suspended)",
                        tp.table_name,
                        cs.column_name,
                        cs.sample_values.join(", ")
                    ),
                    repo_suggestion: None,
                });
                continue;
            }

            // OpaqueShortCode: short uppercase codes mixed with longer strings
            let short_cryptic: Vec<&str> = cs
                .sample_values
                .iter()
                .filter(|v| is_cryptic_short(v))
                .map(String::as_str)
                .collect();
            let has_longer = cs.sample_values.iter().any(|v| v.len() > 2);

            if !short_cryptic.is_empty() && has_longer {
                results.push(AmbiguousColumn {
                    table: tp.table_name.clone(),
                    column: cs.column_name.clone(),
                    ambiguity_type: AmbiguityType::OpaqueShortCode,
                    sample_values: cs.sample_values.clone(),
                    clarification_prompt: format!(
                        "Column `{}.{}` has cryptic code(s) [{}] alongside longer values [{}]. \
                         What do the short codes mean?",
                        tp.table_name,
                        cs.column_name,
                        short_cryptic.join(", "),
                        cs.sample_values
                            .iter()
                            .filter(|v| v.len() > 2)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    repo_suggestion: None,
                });
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::test_utils::make_schema;
    use ox_core::source_schema::{ColumnStats, TableProfile};

    #[test]
    fn detect_ambiguous_numeric_code() {
        let schema = make_schema(&[("orders", &["id", "status"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "orders".to_string(),
                row_count: 5,
                column_stats: vec![ColumnStats {
                    column_name: "status".to_string(),
                    null_count: 0,
                    distinct_count: 3,
                    sample_values: vec!["1".to_string(), "2".to_string(), "3".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let results = detect_ambiguous(&schema, &profile);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].column, "status");
        assert!(matches!(
            results[0].ambiguity_type,
            AmbiguityType::NumericCode
        ));
    }

    #[test]
    fn detect_ambiguous_opaque_short_code() {
        // 'N' alongside 'Regular', 'Town' — classic opaque short code scenario
        let schema = make_schema(&[("stores", &["id", "store_type"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "stores".to_string(),
                row_count: 5,
                column_stats: vec![ColumnStats {
                    column_name: "store_type".to_string(),
                    null_count: 0,
                    distinct_count: 3,
                    sample_values: vec!["N".to_string(), "Regular".to_string(), "Town".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let results = detect_ambiguous(&schema, &profile);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].column, "store_type");
        assert!(matches!(
            results[0].ambiguity_type,
            AmbiguityType::OpaqueShortCode
        ));

        // Pure binary pair ['Y', 'N'] must NOT trigger — has_longer is false
        let schema2 = make_schema(&[("stores", &["id", "is_active"])], &[]);
        let profile_binary = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "stores".to_string(),
                row_count: 5,
                column_stats: vec![ColumnStats {
                    column_name: "is_active".to_string(),
                    null_count: 0,
                    distinct_count: 2,
                    sample_values: vec!["Y".to_string(), "N".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let binary_results = detect_ambiguous(&schema2, &profile_binary);
        assert!(
            binary_results.is_empty(),
            "pure binary ['Y','N'] must not be flagged"
        );
    }

    #[test]
    fn detect_ambiguous_id_column_skipped() {
        // `user_id` ends with `_id` — must not be reported as ambiguous numeric code
        let schema = make_schema(&[("orders", &["id", "user_id"])], &[]);
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "orders".to_string(),
                row_count: 5,
                column_stats: vec![ColumnStats {
                    column_name: "user_id".to_string(),
                    null_count: 0,
                    distinct_count: 5,
                    sample_values: vec!["1".to_string(), "2".to_string(), "3".to_string()],
                    min_value: None,
                    max_value: None,
                }],
            }],
        };
        let results = detect_ambiguous(&schema, &profile);
        assert!(
            results.is_empty(),
            "id/fk columns must not be flagged as ambiguous"
        );
    }
}

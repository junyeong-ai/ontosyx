use std::collections::HashMap;

use ox_core::source_analysis::{TableExclusionReason, TableExclusionSuggestion};
use ox_core::source_schema::{SourceProfile, SourceSchema};

const AUDIT_PREFIXES: &[&str] = &["audit_", "log_", "history_"];
const AUDIT_SUFFIXES: &[&str] = &["_audit", "_log", "_history", "_logs"];
const TEMP_PREFIXES: &[&str] = &["tmp_", "temp_", "bak_"];
const TEMP_SUFFIXES: &[&str] = &["_old", "_backup", "_bak", "_tmp", "_temp"];

pub(super) fn suggest_exclusions(
    schema: &SourceSchema,
    profile: &SourceProfile,
) -> Vec<TableExclusionSuggestion> {
    // Pre-build row count lookup to avoid O(n^2) repeated linear scans
    let row_counts: HashMap<&str, u64> = profile
        .table_profiles
        .iter()
        .map(|tp| (tp.table_name.as_str(), tp.row_count))
        .collect();

    let mut suggestions = Vec::new();

    for table in &schema.tables {
        let name_lower = table.name.to_lowercase();
        let row_count = row_counts.get(table.name.as_str()).copied();

        let is_audit = AUDIT_PREFIXES.iter().any(|p| name_lower.starts_with(p))
            || AUDIT_SUFFIXES.iter().any(|s| name_lower.ends_with(s));

        if is_audit {
            suggestions.push(TableExclusionSuggestion {
                table_name: table.name.clone(),
                reason: TableExclusionReason::AuditLog,
                row_count,
            });
            continue;
        }

        let is_temp = TEMP_PREFIXES.iter().any(|p| name_lower.starts_with(p))
            || TEMP_SUFFIXES.iter().any(|s| name_lower.ends_with(s));

        if is_temp {
            suggestions.push(TableExclusionSuggestion {
                table_name: table.name.clone(),
                reason: TableExclusionReason::Temporary,
                row_count,
            });
            continue;
        }

        if row_count == Some(0) {
            suggestions.push(TableExclusionSuggestion {
                table_name: table.name.clone(),
                reason: TableExclusionReason::Empty,
                row_count: Some(0),
            });
        }
    }

    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::test_utils::make_schema;
    use ox_core::source_schema::TableProfile;

    #[test]
    fn suggest_exclusions_audit_and_temp() {
        let schema = make_schema(
            &[
                ("orders", &["id"]),
                ("orders_audit", &["id"]), // audit suffix
                ("tmp_migrate", &["id"]),  // temp prefix
                ("empty_table", &["id"]),  // zero rows
            ],
            &[],
        );
        let profile = SourceProfile {
            table_profiles: vec![
                TableProfile {
                    table_name: "orders".to_string(),
                    row_count: 100,
                    column_stats: vec![],
                },
                TableProfile {
                    table_name: "orders_audit".to_string(),
                    row_count: 500,
                    column_stats: vec![],
                },
                TableProfile {
                    table_name: "tmp_migrate".to_string(),
                    row_count: 10,
                    column_stats: vec![],
                },
                TableProfile {
                    table_name: "empty_table".to_string(),
                    row_count: 0,
                    column_stats: vec![],
                },
            ],
        };
        let suggestions = suggest_exclusions(&schema, &profile);
        let names: Vec<&str> = suggestions.iter().map(|s| s.table_name.as_str()).collect();

        assert!(
            names.contains(&"orders_audit"),
            "audit table must be suggested"
        );
        assert!(
            names.contains(&"tmp_migrate"),
            "temp table must be suggested"
        );
        assert!(
            names.contains(&"empty_table"),
            "empty table must be suggested"
        );
        assert!(
            !names.contains(&"orders"),
            "normal table must not be suggested"
        );
    }
}

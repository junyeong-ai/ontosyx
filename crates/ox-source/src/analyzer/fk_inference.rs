use std::collections::{HashMap, HashSet};

use ox_core::source_analysis::{ImpliedFkPattern, ImpliedRelationship};
use ox_core::source_schema::SourceSchema;

pub(super) fn infer_implied_fks(schema: &SourceSchema) -> Vec<ImpliedRelationship> {
    let declared_fk_set: HashSet<(&str, &str)> = schema
        .foreign_keys
        .iter()
        .map(|fk| (fk.from_table.as_str(), fk.from_column.as_str()))
        .collect();

    // Pre-build O(1) lookups:
    // - lowercase table name -> original name (for candidate matching)
    // - original name -> SourceTableDef (for PK resolution on match)
    let table_lower_map: HashMap<String, &str> = schema
        .tables
        .iter()
        .map(|t| (t.name.to_lowercase(), t.name.as_str()))
        .collect();
    let table_def_map: HashMap<&str, _> =
        schema.tables.iter().map(|t| (t.name.as_str(), t)).collect();

    let mut results = Vec::new();

    for table in &schema.tables {
        for col in &table.columns {
            // Skip if already a declared FK
            if declared_fk_set.contains(&(table.name.as_str(), col.name.as_str())) {
                continue;
            }

            // Pattern: column ends with `_id`
            let Some(base) = col.name.strip_suffix("_id") else {
                continue;
            };

            // Try exact match and common plural forms (s / es / ies).
            // Safe heuristic: candidate must match an actually-existing table name,
            // so false positives are impossible. Irregular plurals (person->people,
            // child->children) are not covered — these are caught by ORM-based repo
            // enrichment or manual confirmed_relationships.
            let mut candidates = vec![base.to_string(), format!("{base}s"), format!("{base}es")];
            // Irregular plural: y -> ies (category -> categories, country -> countries)
            if let Some(stem) = base.strip_suffix('y') {
                candidates.push(format!("{stem}ies"));
            }

            for candidate in &candidates {
                let candidate_lower = candidate.to_lowercase();
                if let Some(&matched_table) = table_lower_map.get(&candidate_lower) {
                    let pk_col = table_def_map
                        .get(matched_table)
                        .and_then(|t| t.primary_key.first())
                        .map(String::as_str)
                        .unwrap_or("id");

                    results.push(ImpliedRelationship {
                        from_table: table.name.clone(),
                        from_column: col.name.clone(),
                        to_table: matched_table.to_string(),
                        to_column: pk_col.to_string(),
                        confidence: 0.85,
                        pattern: ImpliedFkPattern::EntityIdSuffix,
                        reason: format!(
                            "Column `{}` ends with `_id`, stripped name `{}` matches table `{}`",
                            col.name, base, matched_table
                        ),
                        repo_confirmed: false,
                    });
                    break;
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::test_utils::make_schema;

    #[test]
    fn infer_implied_fks_ies_plural() {
        // `store_category_id` should match `store_categories` table (y->ies)
        let schema = make_schema(
            &[
                ("stores", &["id", "store_category_id"]),
                ("store_categories", &["id"]),
            ],
            &[], // no declared FKs
        );
        let rels = infer_implied_fks(&schema);
        assert_eq!(
            rels.len(),
            1,
            "should infer stores.store_category_id -> store_categories"
        );
        assert_eq!(rels[0].from_table, "stores");
        assert_eq!(rels[0].to_table, "store_categories");
    }

    #[test]
    fn infer_implied_fks_standard_plural() {
        // Regression: `user_id` -> `users` (s-plural) still works after refactor
        let schema = make_schema(&[("orders", &["id", "user_id"]), ("users", &["id"])], &[]);
        let rels = infer_implied_fks(&schema);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].to_table, "users");
    }

    #[test]
    fn infer_implied_fks_es_plural() {
        // `branch_id` -> `branches` (es-plural)
        let schema = make_schema(
            &[("employees", &["id", "branch_id"]), ("branches", &["id"])],
            &[],
        );
        let rels = infer_implied_fks(&schema);
        assert_eq!(
            rels.len(),
            1,
            "should infer employees.branch_id -> branches"
        );
        assert_eq!(rels[0].from_table, "employees");
        assert_eq!(rels[0].to_table, "branches");
    }
}

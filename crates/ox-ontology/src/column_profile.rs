//! Column profile preservation — keep the data-distribution snapshot
//! the introspection kernel produced as a first-class IR collection,
//! not a one-shot input the value-set inferer discards after a single
//! pass.
//!
//! Why this lives in the IR:
//!
//! - **Re-inference without a re-scan.** `propose_value_sets` and
//!   `propose_notation_patterns` consume distribution stats. With the
//!   profile in the IR, an admin re-running the suggestion pipeline
//!   doesn't need to call the source adapter again — same input,
//!   same proposal, deterministic.
//! - **Distribution-change detection across versions.** Two ontology
//!   versions can be diffed for "data distribution drifted" signals
//!   (e.g., a column whose distinct count tripled between v1 and v2)
//!   in addition to schema-shape diffs.
//! - **SHACL constraint suggestions.** Cardinality and value-set
//!   constraints want concrete population numbers to recommend
//!   thresholds; the profile is the right place to read them from.
//!
//! Identity: `(source_id, relation, column)`. The same column profiled
//! against the same source twice replaces the previous entry — the IR
//! always carries the most recent snapshot for a given location.

use chrono::{DateTime, Utc};
use ox_core::source_schema::{ColumnStats, SourceProfile};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::mapping::SourceId;

ox_core::define_id_newtype!(
    /// Type-safe identifier for a column profile entry. Stable across
    /// re-snapshots so the IR-level diff treats them as updates, not
    /// add+remove pairs.
    ColumnProfileId
);

/// One column's distribution snapshot, taken from a specific source
/// at a specific time. Wraps the same [`ColumnStats`] the
/// introspection kernel produces, plus the location identity and the
/// sampling timestamp that locks the snapshot to a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ColumnProfileDef {
    pub id: ColumnProfileId,
    /// Source the profile was sampled from. Matches the `source_id`
    /// used by the matching `ObjectMappingDef`.
    pub source_id: SourceId,
    /// Source-side relation name (table / collection / file relation).
    pub relation: String,
    /// Column name as the source advertises it.
    pub column: String,
    /// The sampled distribution itself — null count, distinct count,
    /// up-to-30 sample values, min / max. Carried verbatim so the
    /// inference pipeline reads the same shape it always has.
    pub stats: ColumnStats,
    /// Wall-clock timestamp of the sampling pass that produced this
    /// snapshot. Used to age out stale profiles in admin UIs and to
    /// attribute distribution diffs to a window.
    pub sampled_at: DateTime<Utc>,
}

impl ColumnProfileDef {
    /// Stable id encoding for `(source_id, relation, column)` so the
    /// IR's HashMap-backed lookup path can treat this triple as a
    /// natural key. Re-sampling the same location produces the same
    /// id, which makes `add_column_profile` an upsert by location.
    pub fn make_id(source_id: &SourceId, relation: &str, column: &str) -> ColumnProfileId {
        ColumnProfileId::from(format!("cp:{source_id}:{relation}:{column}"))
    }
}

/// Convert a [`SourceProfile`] (the analysis-side shape) into one
/// [`ColumnProfileDef`] per profiled column. The caller threads the
/// `source_id` (the kernel's `SourceProfile` is source-agnostic) and
/// `sampled_at` so the IR-side timestamp matches the analysis run.
///
/// Returns `Vec` rather than directly mutating an IR so callers can
/// audit / filter the proposed entries before persisting them through
/// `OntologyIR::add_column_profile`.
pub fn profile_to_column_defs(
    source_id: &SourceId,
    profile: &SourceProfile,
    sampled_at: DateTime<Utc>,
) -> Vec<ColumnProfileDef> {
    let mut out = Vec::new();
    for table in &profile.table_profiles {
        for stats in &table.column_stats {
            out.push(ColumnProfileDef {
                id: ColumnProfileDef::make_id(source_id, &table.table_name, &stats.column_name),
                source_id: source_id.clone(),
                relation: table.table_name.clone(),
                column: stats.column_name.clone(),
                stats: stats.clone(),
                sampled_at,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::source_schema::{TableProfile};

    #[test]
    fn make_id_is_deterministic() {
        let s = SourceId::from("src");
        let a = ColumnProfileDef::make_id(&s, "users", "email");
        let b = ColumnProfileDef::make_id(&s, "users", "email");
        assert_eq!(a, b);
        assert_ne!(a, ColumnProfileDef::make_id(&s, "users", "name"));
        assert_ne!(a, ColumnProfileDef::make_id(&s, "orders", "email"));
        assert_ne!(
            a,
            ColumnProfileDef::make_id(&SourceId::from("other"), "users", "email"),
        );
    }

    #[test]
    fn ir_ingest_source_profile_creates_one_entry_per_column_and_upserts_on_repeat() {
        use crate::ir::OntologyIR;

        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "users".into(),
                row_count: 10,
                column_stats: vec![ColumnStats {
                    column_name: "email".into(),
                    null_count: 0,
                    distinct_count: 8,
                    sample_values: vec![],
                    min_value: None,
                    max_value: None,
                    pii_redacted: None,
                }],
            }],
        };
        let mut ir = OntologyIR::new(
            "ont-1".into(),
            "Test".into(),
            ox_core::i18n::LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        let n = ir.ingest_source_profile(&SourceId::from("pg"), &profile, Utc::now());
        assert_eq!(n, 1);
        assert_eq!(ir.column_profiles().len(), 1);

        // Re-ingest with a different distinct_count — upsert keeps the
        // collection size at one and the latest entry wins.
        let mut profile2 = profile.clone();
        profile2.table_profiles[0].column_stats[0].distinct_count = 9;
        let n2 = ir.ingest_source_profile(&SourceId::from("pg"), &profile2, Utc::now());
        assert_eq!(n2, 1);
        assert_eq!(ir.column_profiles().len(), 1);
        assert_eq!(ir.column_profiles()[0].stats.distinct_count, 9);
    }

    #[test]
    fn ir_round_trips_column_profiles_through_serde_json() {
        use crate::ir::OntologyIR;

        let mut ir = OntologyIR::new(
            "ont-1".into(),
            "Test".into(),
            ox_core::i18n::LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        );
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "orders".into(),
                row_count: 7,
                column_stats: vec![ColumnStats {
                    column_name: "status".into(),
                    null_count: 0,
                    distinct_count: 3,
                    sample_values: vec!["new".into(), "paid".into(), "shipped".into()],
                    min_value: None,
                    max_value: None,
                    pii_redacted: None,
                }],
            }],
        };
        ir.ingest_source_profile(&SourceId::from("pg"), &profile, Utc::now());
        let json = serde_json::to_string(&ir).expect("serialize");
        let back: OntologyIR = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.column_profiles().len(), 1);
        assert_eq!(back.column_profiles()[0].stats.distinct_count, 3);
        assert_eq!(back.column_profiles()[0].relation, "orders");
    }

    #[test]
    fn profile_to_column_defs_emits_one_entry_per_column() {
        let profile = SourceProfile {
            table_profiles: vec![
                TableProfile {
                    table_name: "users".into(),
                    row_count: 100,
                    column_stats: vec![
                        ColumnStats {
                            column_name: "id".into(),
                            null_count: 0,
                            distinct_count: 100,
                            sample_values: vec![],
                            min_value: None,
                            max_value: None,
                            pii_redacted: None,
                        },
                        ColumnStats {
                            column_name: "email".into(),
                            null_count: 5,
                            distinct_count: 95,
                            sample_values: vec![],
                            min_value: None,
                            max_value: None,
                            pii_redacted: None,
                        },
                    ],
                },
                TableProfile {
                    table_name: "orders".into(),
                    row_count: 250,
                    column_stats: vec![ColumnStats {
                        column_name: "status".into(),
                        null_count: 0,
                        distinct_count: 4,
                        sample_values: vec!["new".into(), "paid".into()],
                        min_value: None,
                        max_value: None,
                        pii_redacted: None,
                    }],
                },
            ],
        };

        let now = Utc::now();
        let entries = profile_to_column_defs(&SourceId::from("pg"), &profile, now);
        assert_eq!(entries.len(), 3);
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"cp:pg:users:id"));
        assert!(ids.contains(&"cp:pg:users:email"));
        assert!(ids.contains(&"cp:pg:orders:status"));
        for e in &entries {
            assert_eq!(e.sampled_at, now);
        }
    }
}

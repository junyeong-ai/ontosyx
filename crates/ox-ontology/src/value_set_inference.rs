//! Automatic `ValueSet` proposals derived from sample-level column
//! statistics.
//!
//! The introspection kernel (`ox-source`) feeds a `SourceProfile` that
//! carries a `ColumnStats` entry per profiled column. When a column's
//! distinct-value count sits below a small bound with low null ratio,
//! it is almost always an enum-like status / kind / role column — the
//! sort of thing an operator ends up modelling as a `CodeSystem` +
//! `ValueSet` by hand.
//!
//! This module walks that signal and emits a `ValueSetProposal` for
//! every column that clears the policy gates. Each proposal bundles
//! three objects that must be applied atomically (applying only the
//! `ValueSet` without its backing `CodeSystem` creates a dangling
//! reference, refused by `OntologyIR::validate()`):
//!
//! 1. an anonymous `CodeSystemDef` seeded from the observed sample
//!    values,
//! 2. a `ValueSetDef` that includes every code in that system,
//! 3. the `ColumnRef` that identified the binding target so the
//!    caller can wire `PropertyDef.value_set_id` once the proposal
//!    is applied.
//!
//! Proposals are *returned*, never injected — confirmation happens
//! through the admin API. The module is therefore a pure function
//! over `(SourceSchema, SourceProfile, policy)` and has no I/O.

use ox_core::i18n::LocalizedText;
use ox_core::source_schema::{ColumnStats, SourceProfile, SourceSchema, TableProfile};

use crate::code_system::{CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId};
use crate::mapping::ColumnRef;
use crate::value_set::{
    IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
};

/// Policy knobs for `propose_value_sets`. Defaults land on the low-
/// recall / high-precision side so an operator reviewing proposals
/// does not have to discard dozens of obvious false positives.
#[derive(Debug, Clone, Copy)]
pub struct ValueSetInferencePolicy {
    /// Upper bound on `distinct_count`. Columns with more distinct
    /// values than this are considered open-set (free text, ids,
    /// timestamps, etc.) and skipped.
    pub distinct_threshold: usize,
    /// Upper bound on `null_count / row_count`. Columns that are
    /// mostly null are usually optional free-form fields, not enums.
    pub null_ratio_max: f32,
    /// Lower bound on `row_count`. Below this the signal is too thin
    /// to make a confident proposal.
    pub min_sample_rows: u64,
    /// Lower bound on `distinct_count`. A column with a single value
    /// is almost always a default flag or a fixed sentinel and is
    /// not enum-like.
    pub min_distinct_count: usize,
    /// Require `sample_values` to fully cover `distinct_count`. When
    /// the profiler captures fewer sample values than distinct
    /// values, the proposal would be incomplete; rather than guess,
    /// we skip it unless this is set to `false`.
    pub require_full_sample_coverage: bool,
}

impl Default for ValueSetInferencePolicy {
    fn default() -> Self {
        Self {
            distinct_threshold: 10,
            null_ratio_max: 0.5,
            min_sample_rows: 100,
            min_distinct_count: 2,
            require_full_sample_coverage: true,
        }
    }
}

/// Reason an enum candidate was rejected. Surfaces through the same
/// API that returns proposals so operators can see which columns
/// *almost* qualified and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSetRejection {
    TooManyDistinct { distinct: u64 },
    TooFewDistinct { distinct: u64 },
    TooSparse { row_count: u64 },
    NullRatioTooHigh { ratio_millis: u32 },
    SampleValuesMissing,
    SampleCoverageIncomplete { distinct: u64, sampled: u64 },
}

/// Numbers that led to a proposal — surfaced to the UI for
/// operator-facing confidence display.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueSetEvidence {
    pub row_count: u64,
    pub distinct_count: u64,
    pub null_count: u64,
    pub null_ratio: f32,
    pub observed_codes: Vec<String>,
}

/// A single column-level enum proposal. Apply all three payloads
/// atomically or discard the proposal — partial application leaves
/// dangling references.
#[derive(Debug, Clone)]
pub struct ValueSetProposal {
    pub column_ref: ColumnRef,
    pub code_system: CodeSystemDef,
    pub value_set: ValueSetDef,
    pub evidence: ValueSetEvidence,
    /// `0.0 ..= 1.0`. Combines null ratio and distinct ratio so the
    /// UI can order proposals by strength.
    pub confidence: f32,
}

/// Columns the policy *considered* but excluded, for UI display.
#[derive(Debug, Clone)]
pub struct ValueSetSkip {
    pub column_ref: ColumnRef,
    pub reason: ValueSetRejection,
}

/// Full result of one inference pass.
#[derive(Debug, Clone, Default)]
pub struct ValueSetInferenceReport {
    pub proposals: Vec<ValueSetProposal>,
    pub skipped: Vec<ValueSetSkip>,
}

/// Walk every profiled column and emit a proposal for every column
/// that clears `policy`. Pure function — no I/O, no randomness — so
/// the same inputs always yield the same proposals (ids included).
pub fn propose_value_sets(
    schema: &SourceSchema,
    profile: &SourceProfile,
    policy: ValueSetInferencePolicy,
) -> ValueSetInferenceReport {
    let mut report = ValueSetInferenceReport::default();

    // Index columns by (table, name) so we can surface a data-type
    // hint alongside the proposal. The hint is informational today;
    // a future slice uses it to refine the CodeSystem kind (e.g., a
    // small numeric enum could be flagged as `Internal` + integer
    // semantics).
    for table_profile in &profile.table_profiles {
        for stats in &table_profile.column_stats {
            let column_ref = ColumnRef::new(&table_profile.table_name, &stats.column_name);
            match evaluate_column(table_profile, stats, policy) {
                Ok(proposal_seed) => {
                    let (code_system, value_set, evidence, confidence) =
                        build_proposal(&schema.source_type, &column_ref, proposal_seed);
                    report.proposals.push(ValueSetProposal {
                        column_ref,
                        code_system,
                        value_set,
                        evidence,
                        confidence,
                    });
                }
                Err(reason) => {
                    // Noisy rejections (`TooManyDistinct` on a big
                    // table, `SampleValuesMissing` on a free-text
                    // column) still surface so the UI can explain why
                    // an expected column did not show up as a
                    // proposal — operators routinely ask "why did it
                    // skip this one?".
                    report.skipped.push(ValueSetSkip { column_ref, reason });
                }
            }
        }
    }

    report
}

struct ProposalSeed<'a> {
    stats: &'a ColumnStats,
    row_count: u64,
    null_ratio: f32,
    distinct_ratio: f32,
}

fn evaluate_column<'a>(
    table: &'a TableProfile,
    stats: &'a ColumnStats,
    policy: ValueSetInferencePolicy,
) -> Result<ProposalSeed<'a>, ValueSetRejection> {
    let row_count = table.row_count;
    if row_count < policy.min_sample_rows {
        return Err(ValueSetRejection::TooSparse { row_count });
    }
    let distinct = stats.distinct_count;
    if distinct as usize > policy.distinct_threshold {
        return Err(ValueSetRejection::TooManyDistinct { distinct });
    }
    if (distinct as usize) < policy.min_distinct_count {
        return Err(ValueSetRejection::TooFewDistinct { distinct });
    }
    let null_ratio = null_ratio(row_count, stats.null_count);
    if null_ratio > policy.null_ratio_max {
        // Scale to millis so `Eq` / `PartialEq` works without a
        // floating-point wrapper.
        let ratio_millis = (null_ratio * 1000.0) as u32;
        return Err(ValueSetRejection::NullRatioTooHigh { ratio_millis });
    }
    if stats.sample_values.is_empty() {
        return Err(ValueSetRejection::SampleValuesMissing);
    }
    if policy.require_full_sample_coverage
        && (stats.sample_values.len() as u64) < distinct
    {
        return Err(ValueSetRejection::SampleCoverageIncomplete {
            distinct,
            sampled: stats.sample_values.len() as u64,
        });
    }
    let distinct_ratio = distinct as f32 / row_count.max(1) as f32;
    Ok(ProposalSeed {
        stats,
        row_count,
        null_ratio,
        distinct_ratio,
    })
}

fn build_proposal(
    source_type: &str,
    column_ref: &ColumnRef,
    seed: ProposalSeed<'_>,
) -> (CodeSystemDef, ValueSetDef, ValueSetEvidence, f32) {
    // Deterministic id derivation: hash of (source_type, relation,
    // column, sorted codes). Same inputs always give the same ids so
    // re-running the inference produces stable proposals that can
    // be compared / merged client-side.
    let mut sorted_codes: Vec<String> = seed.stats.sample_values.clone();
    sorted_codes.sort();
    sorted_codes.dedup();

    let fingerprint = fingerprint(source_type, column_ref, &sorted_codes);
    let cs_id = CodeSystemId::new(format!("cs_auto_{fingerprint}"));
    let vs_id = ValueSetId::new(format!("vs_auto_{fingerprint}"));
    let auto_name = format!(
        "auto_{}_{}",
        slugify(&column_ref.relation),
        slugify(&column_ref.column)
    );

    let codes: Vec<CodedValue> = sorted_codes
        .iter()
        .map(|code| CodedValue {
            id: CodedValueId::new(format!(
                "cv_{fingerprint}_{}",
                slugify_short(code)
            )),
            code: code.clone(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: Vec::new(),
            broader_id: None,
            examples: Vec::new(),
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        })
        .collect();

    let code_system = CodeSystemDef {
        id: cs_id.clone(),
        name: auto_name.clone(),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        uri: None,
        version: "1".into(),
        kind: CodeSystemKind::Internal,
        hierarchical: false,
        codes,
        deprecated_at: None,
        replaced_by_id: None,
    };

    let value_set = ValueSetDef {
        id: vs_id,
        name: auto_name,
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        version: "1".into(),
        composition: vec![ValueSetIncludeRule {
            system_id: cs_id,
            selector: ValueSetSelector::All,
            mode: IncludeMode::Include,
        }],
    };

    let evidence = ValueSetEvidence {
        row_count: seed.row_count,
        distinct_count: seed.stats.distinct_count,
        null_count: seed.stats.null_count,
        null_ratio: seed.null_ratio,
        observed_codes: sorted_codes,
    };

    // Confidence score. Two factors:
    //  1. `1 - null_ratio` — fewer nulls = stronger enum signal.
    //  2. `1 - distinct_ratio` — enum columns should be sparse in
    //     distinct-values per row; a column with `distinct_count ≈
    //     row_count` is closer to an identifier than an enum.
    // Combine geometrically so both factors must be strong for a
    // high score.
    let null_factor = (1.0 - seed.null_ratio).clamp(0.0, 1.0);
    let density_factor = (1.0 - seed.distinct_ratio).clamp(0.0, 1.0);
    let confidence = (null_factor * density_factor).sqrt();

    (code_system, value_set, evidence, confidence)
}

fn null_ratio(row_count: u64, null_count: u64) -> f32 {
    if row_count == 0 {
        return 0.0;
    }
    (null_count as f32 / row_count as f32).clamp(0.0, 1.0)
}

fn fingerprint(source_type: &str, column_ref: &ColumnRef, codes: &[String]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    source_type.hash(&mut h);
    column_ref.relation.hash(&mut h);
    column_ref.column.hash(&mut h);
    for c in codes {
        c.hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn slugify_short(value: &str) -> String {
    let slug = slugify(value);
    if slug.len() > 24 { slug[..24].to_string() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::source_schema::{
        ColumnStats, ForeignKeyDef, SourceColumnDef, SourceSchema, SourceTableDef, TableProfile,
    };

    fn schema_profile(rows: u64, col: &str, distinct: u64, nulls: u64, samples: &[&str]) -> (SourceSchema, SourceProfile) {
        let schema = SourceSchema {
            source_type: "postgresql".into(),
            tables: vec![SourceTableDef {
                name: "orders".into(),
                columns: vec![SourceColumnDef {
                    name: col.into(),
                    data_type: "varchar".into(),
                    nullable: true,
                }],
                primary_key: vec!["id".into()],
            }],
            foreign_keys: Vec::<ForeignKeyDef>::new(),
        };
        let profile = SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: "orders".into(),
                row_count: rows,
                column_stats: vec![ColumnStats {
                    column_name: col.into(),
                    null_count: nulls,
                    distinct_count: distinct,
                    sample_values: samples.iter().map(|s| s.to_string()).collect(),
                    min_value: None,
                    max_value: None,
                    pii_redacted: None,
                }],
            }],
        };
        (schema, profile)
    }

    #[test]
    fn small_enum_column_produces_a_proposal() {
        let (schema, profile) =
            schema_profile(1000, "status", 3, 20, &["PENDING", "ACTIVE", "CLOSED"]);
        let report = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert_eq!(report.proposals.len(), 1);
        assert!(report.skipped.is_empty());
        let p = &report.proposals[0];
        assert_eq!(p.code_system.codes.len(), 3);
        assert_eq!(p.value_set.composition.len(), 1);
        assert_eq!(p.evidence.distinct_count, 3);
        // Sorted codes — ACTIVE / CLOSED / PENDING lexicographically.
        assert_eq!(p.evidence.observed_codes, vec!["ACTIVE", "CLOSED", "PENDING"]);
        assert!(p.confidence > 0.9, "confidence {} unexpectedly low", p.confidence);
    }

    #[test]
    fn high_cardinality_column_is_skipped() {
        let (schema, profile) =
            schema_profile(1000, "email", 950, 5, &["a@x.com", "b@x.com"]);
        let report = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert!(report.proposals.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert!(matches!(
            report.skipped[0].reason,
            ValueSetRejection::TooManyDistinct { .. }
        ));
    }

    #[test]
    fn tiny_table_is_skipped_as_too_sparse() {
        let (schema, profile) = schema_profile(10, "status", 3, 0, &["A", "B", "C"]);
        let report = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert!(report.proposals.is_empty());
        assert!(matches!(
            report.skipped[0].reason,
            ValueSetRejection::TooSparse { .. }
        ));
    }

    #[test]
    fn mostly_null_column_is_skipped() {
        let (schema, profile) = schema_profile(1000, "tier", 3, 900, &["A", "B", "C"]);
        let report = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert!(report.proposals.is_empty());
        assert!(matches!(
            report.skipped[0].reason,
            ValueSetRejection::NullRatioTooHigh { .. }
        ));
    }

    #[test]
    fn singleton_column_is_skipped_as_too_few_distinct() {
        let (schema, profile) = schema_profile(1000, "flag", 1, 0, &["YES"]);
        let report = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert!(report.proposals.is_empty());
        assert!(matches!(
            report.skipped[0].reason,
            ValueSetRejection::TooFewDistinct { .. }
        ));
    }

    #[test]
    fn incomplete_sample_coverage_is_skipped_by_default() {
        // 4 distinct but only 2 sample values — policy refuses to
        // guess the missing two.
        let (schema, profile) = schema_profile(1000, "status", 4, 0, &["A", "B"]);
        let report = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert!(report.proposals.is_empty());
        assert!(matches!(
            report.skipped[0].reason,
            ValueSetRejection::SampleCoverageIncomplete { .. }
        ));
    }

    #[test]
    fn proposal_ids_are_deterministic_across_calls() {
        let (schema, profile) =
            schema_profile(500, "status", 3, 0, &["PENDING", "ACTIVE", "CLOSED"]);
        let a = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        let b = propose_value_sets(&schema, &profile, ValueSetInferencePolicy::default());
        assert_eq!(a.proposals[0].code_system.id, b.proposals[0].code_system.id);
        assert_eq!(a.proposals[0].value_set.id, b.proposals[0].value_set.id);
    }

    #[test]
    fn lower_null_ratio_yields_higher_confidence_than_higher_null_ratio() {
        let (s1, p1) = schema_profile(1000, "status", 3, 10, &["A", "B", "C"]);
        let (s2, p2) = schema_profile(1000, "status", 3, 400, &["A", "B", "C"]);
        let r1 = propose_value_sets(&s1, &p1, ValueSetInferencePolicy::default());
        let r2 = propose_value_sets(&s2, &p2, ValueSetInferencePolicy::default());
        assert!(r1.proposals[0].confidence > r2.proposals[0].confidence);
    }
}

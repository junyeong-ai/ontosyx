//! Temporal AS-OF query rewriting.
//!
//! [`QueryIR.as_of`] carries a wall-clock pivot; the runtime resolves it
//! against an [`OntologyVersion`] snapshot before the compiler sees the
//! query. This module is that resolution boundary.
//!
//! # Design
//!
//! The rewriter is a pure function — it does not load ontologies, does
//! not touch a database. Callers are expected to pick the right
//! [`OntologyIR`] snapshot for the timestamp and hand it in. Keeping the
//! rewriter pure means:
//!
//! - The compile pipeline stays synchronous (the `GraphCompiler` trait
//!   is sync; adding an async resolver would force every caller into
//!   an async boundary they don't need).
//! - Testing doesn't need a mock store — a caller builds two
//!   `OntologyIR` fixtures and verifies each `as_of` pivot routes to
//!   the right one.
//! - The caller owns the resolution policy. A runtime that stores
//!   every committed version as JSONB can hand back the exact
//!   snapshot; a runtime that only stores the current schema can
//!   refuse non-`None` `as_of` with a clear error at the resolution
//!   boundary.
//!
//! # What this pass does today
//!
//! - Validates that `snapshot.version.valid_from <= as_of <
//!   snapshot.version.valid_to` (where `valid_to = None` means
//!   "current"). Mismatch → `OxError::Validation`.
//! - Clears `as_of` on the returned query so the compiler accepts it.
//!
//! # What it does not do yet (explicit non-goal, tracked for follow-up)
//!
//! - Label-rename rewriting. If the ontology at `as_of` had a node
//!   labelled "Customer" but the current saved PatternIR references
//!   "Client" (a later rename), the rewriter does not today walk the
//!   `OntologyCommand` log between versions to reverse the rename. The
//!   compiler will attempt to emit a `(:Client)` query against a
//!   snapshot where no such label existed — the resulting error
//!   surfaces at execution time. Wiring the command log into the
//!   rewrite pass is the next commit's territory.
//!
//! This is a deliberate foundation-first slice: interface + window
//! check + `as_of` clear, with semantic label rewriting split out so
//! the window-validation bug surface can be reviewed in isolation.

use chrono::{DateTime, Utc};

use ox_core::error::{OxError, OxResult};
use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::QueryIR;

/// Rewrite a temporal-pivoted query to evaluate against the given
/// ontology snapshot.
///
/// The caller is responsible for choosing the snapshot — typically by
/// consulting the store of committed [`OntologyVersion`] metadata for
/// the window containing `query.as_of`.
///
/// Returns the query unchanged when `as_of` is `None` (the common
/// path). Errors when the supplied snapshot's validity window does not
/// contain the requested timestamp.
pub fn rewrite_temporal(
    query: QueryIR,
    snapshot: &OntologyIR,
) -> OxResult<QueryIR> {
    let Some(as_of) = query.as_of else {
        // No-op fast path: non-temporal queries traverse the rewriter
        // without allocating. Keeping this branch cheap means we can
        // unconditionally pipe every query through the rewriter at the
        // runtime boundary without a `if query.as_of.is_some()` guard
        // at every call site.
        return Ok(query);
    };

    validate_window(&as_of, snapshot)?;

    let mut out = query;
    out.as_of = None;
    Ok(out)
}

/// Validate that the supplied snapshot's `OntologyVersion` window
/// contains the requested `as_of` timestamp. A mismatch is a caller
/// bug (wrong snapshot passed in) — we fail with a Validation error
/// rather than silently fall through.
fn validate_window(as_of: &DateTime<Utc>, snapshot: &OntologyIR) -> OxResult<()> {
    let v = &snapshot.version;

    // `None` on valid_from means "version 1, known since before the
    // system started tracking timestamps". We accept any as_of.
    if let Some(from) = v.valid_from
        && *as_of < from
    {
        return Err(OxError::Validation {
            field: "as_of".to_string(),
            message: format!(
                "timestamp {as_of} predates ontology version {} valid_from {from}",
                v.number,
            ),
        });
    }

    // `None` on valid_to means "current / still in force". An as_of
    // pointing into the future is accepted as "current snapshot".
    if let Some(to) = v.valid_to
        && *as_of >= to
    {
        return Err(OxError::Validation {
            field: "as_of".to_string(),
            message: format!(
                "timestamp {as_of} is at or past ontology version {} valid_to {to}",
                v.number,
            ),
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::ontology_ir::{NodeTypeDef, OntologyVersion};
    use ox_core::query_ir::{
        GraphPattern, QUERY_IR_SCHEMA_VERSION, QueryOp,
    };
    use ox_core::variable_name::VariableName;

    fn vn(s: &'static str) -> VariableName {
        VariableName::new(s).expect("test variable")
    }

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label")
    }

    fn ts(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0)
            .single()
            .expect("test timestamp")
    }

    fn snapshot_with_window(
        number: u32,
        valid_from: Option<DateTime<Utc>>,
        valid_to: Option<DateTime<Utc>>,
    ) -> OntologyIR {
        let version = OntologyVersion {
            number,
            valid_from,
            valid_to,
            committed_by: None,
            commit_message: None,
        };
        OntologyIR::new(
            "test".to_string(),
            "Test".to_string(),
            LocalizedText::default(),
            version,
            vec![NodeTypeDef {
                id: "nt1".into(),
                label: gl("Person"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
    }

    fn simple_query(as_of: Option<DateTime<Utc>>) -> QueryIR {
        QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p"),
                    label: Some(gl("Person")),
                    property_filters: vec![],
                }],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of,
        }
    }

    #[test]
    fn non_temporal_query_passes_through_unchanged() {
        let snap = snapshot_with_window(1, None, None);
        let q = simple_query(None);
        let out = rewrite_temporal(q.clone(), &snap).expect("no-op");
        assert!(out.as_of.is_none());
        // The rewrite is an identity on non-temporal queries.
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            serde_json::to_string(&q).unwrap(),
        );
    }

    #[test]
    fn temporal_within_window_clears_as_of() {
        // Snapshot valid [2026-01-01, 2026-06-01); as_of mid-window.
        let snap = snapshot_with_window(
            2,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
        );
        let q = simple_query(Some(ts(2026, 3, 15)));
        let out = rewrite_temporal(q, &snap).expect("in-window");
        assert!(
            out.as_of.is_none(),
            "rewriter must clear as_of so the compiler accepts the query"
        );
    }

    #[test]
    fn temporal_before_valid_from_fails() {
        let snap = snapshot_with_window(2, Some(ts(2026, 1, 1)), None);
        let q = simple_query(Some(ts(2025, 12, 31)));
        let err = rewrite_temporal(q, &snap).expect_err("before window");
        match err {
            OxError::Validation { field, message } => {
                assert_eq!(field, "as_of");
                assert!(
                    message.contains("predates") && message.contains("valid_from"),
                    "error should name the mismatched boundary: {message}"
                );
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn temporal_at_or_past_valid_to_fails() {
        let snap = snapshot_with_window(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
        );
        // `>= valid_to` should fail — the window is half-open.
        let q = simple_query(Some(ts(2026, 6, 1)));
        let err = rewrite_temporal(q, &snap).expect_err("at upper bound");
        match err {
            OxError::Validation { field, .. } => assert_eq!(field, "as_of"),
            other => panic!("expected Validation, got {other:?}"),
        }

        // Well past the window also fails.
        let q = simple_query(Some(ts(2027, 1, 1)));
        let err = rewrite_temporal(q, &snap).expect_err("past upper bound");
        assert!(matches!(err, OxError::Validation { .. }));
    }

    #[test]
    fn temporal_with_open_windows_is_permissive() {
        // valid_from=None / valid_to=None = "known since always, still in force".
        // Any timestamp should be accepted.
        let snap = snapshot_with_window(1, None, None);
        let past = simple_query(Some(ts(1970, 1, 1)));
        let future = simple_query(Some(ts(2099, 12, 31)));
        assert!(rewrite_temporal(past, &snap).is_ok());
        assert!(rewrite_temporal(future, &snap).is_ok());
    }

    #[test]
    fn temporal_respects_only_lower_bound() {
        // valid_to=None means "still in force"; any timestamp at or
        // after valid_from is accepted.
        let snap = snapshot_with_window(3, Some(ts(2026, 1, 1)), None);
        assert!(
            rewrite_temporal(simple_query(Some(ts(2026, 1, 1))), &snap).is_ok(),
            "inclusive lower bound"
        );
        assert!(
            rewrite_temporal(simple_query(Some(ts(2030, 1, 1))), &snap).is_ok(),
            "open upper bound accepts the far future"
        );
        assert!(
            rewrite_temporal(simple_query(Some(ts(2025, 12, 31))), &snap).is_err(),
            "below lower bound rejects"
        );
    }
}

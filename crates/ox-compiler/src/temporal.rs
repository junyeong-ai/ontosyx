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
//! # Label-rename rewriting
//!
//! [`rewrite_temporal_with_renames`] extends the base rewriter with a
//! label-substitution pass. A query authored today references
//! **current** labels (e.g. `(:Customer)`); evaluating it as of a past
//! timestamp when that node type was labelled `Client` requires
//! substituting every `Customer` literal with `Client` before the
//! compiler emits Cypher.
//!
//! The substitution is driven by diffing the two ontology snapshots on
//! stable type ids (`NodeTypeId`, `EdgeTypeId`): if the same id carries
//! different labels in `current` and `snapshot`, the rewriter records
//! a `current_label → snapshot_label` mapping and walks the query AST
//! to apply it. Labels that don't map (new types that didn't exist
//! yet, or unchanged types) pass through unchanged.
//!
//! Ambiguity at type-id granularity is impossible — a single id can
//! only have one label per snapshot — so the mapping is a plain
//! `HashMap`. Collisions at label granularity (two current labels
//! colliding to one snapshot label, for a later rename that reused a
//! name) are rejected with a clear error rather than silently picking
//! one — the semantics of `(:Client)` in the rewritten query would
//! depend on which rename you applied first, which is non-obvious.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use ox_core::error::{OxError, OxResult};
use ox_core::graph_label::GraphLabel;
use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::{
    AnalyticsSource, Expr, GraphPattern, MutateOp, PathElement, QueryIR, QueryOp,
};

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

/// Same as [`rewrite_temporal`] plus a label-rename pass: labels that
/// refer to a type renamed between `snapshot` and `current` are
/// rewritten to match the snapshot's label. See the module docs for
/// the diff semantics.
///
/// A single-snapshot caller (no interest in renames, or confident the
/// type labels haven't changed) should prefer [`rewrite_temporal`] and
/// skip the extra `current` argument.
pub fn rewrite_temporal_with_renames(
    query: QueryIR,
    snapshot: &OntologyIR,
    current: &OntologyIR,
) -> OxResult<QueryIR> {
    // No-op fast path. We still skip building the rename maps when
    // there is no temporal pivot — the maps would go unused.
    if query.as_of.is_none() {
        return Ok(query);
    }

    let mut query = rewrite_temporal(query, snapshot)?;

    let node_renames = diff_node_labels(current, snapshot)?;
    let edge_renames = diff_edge_labels(current, snapshot)?;

    if !node_renames.is_empty() || !edge_renames.is_empty() {
        apply_renames(&mut query.operation, &node_renames, &edge_renames)?;
    }

    Ok(query)
}

/// Build a `current_label → snapshot_label` map for node types whose
/// labels changed between the two ontologies.
///
/// Shared ids with identical labels contribute nothing. Ids unique to
/// either side (newly created types or types removed since) also
/// contribute nothing — the rewriter can't rewrite what isn't in the
/// current query's label set.
fn diff_node_labels(
    current: &OntologyIR,
    snapshot: &OntologyIR,
) -> OxResult<HashMap<String, GraphLabel>> {
    let snapshot_by_id: HashMap<&str, &GraphLabel> = snapshot
        .node_types()
        .iter()
        .map(|n| (n.id.as_ref(), &n.label))
        .collect();

    let mut map: HashMap<String, GraphLabel> = HashMap::new();
    for node in current.node_types() {
        if let Some(snap_label) = snapshot_by_id.get(node.id.as_ref())
            && *snap_label != &node.label
        {
            // A second current label collapsing onto an already-mapped
            // snapshot-label-key would be ambiguous — refuse rather
            // than pick a winner.
            if let Some(existing) = map.insert(node.label.to_string(), (*snap_label).clone())
                && &existing != *snap_label
            {
                return Err(OxError::Validation {
                    field: "node_rename".to_string(),
                    message: format!(
                        "ambiguous node rename: current label `{}` maps to both \
                         `{}` and `{}` in the requested snapshot",
                        node.label, existing, snap_label
                    ),
                });
            }
        }
    }
    Ok(map)
}

fn diff_edge_labels(
    current: &OntologyIR,
    snapshot: &OntologyIR,
) -> OxResult<HashMap<String, GraphLabel>> {
    let snapshot_by_id: HashMap<&str, &GraphLabel> = snapshot
        .edge_types()
        .iter()
        .map(|e| (e.id.as_ref(), &e.label))
        .collect();

    let mut map: HashMap<String, GraphLabel> = HashMap::new();
    for edge in current.edge_types() {
        if let Some(snap_label) = snapshot_by_id.get(edge.id.as_ref())
            && *snap_label != &edge.label
        {
            if let Some(existing) = map.insert(edge.label.to_string(), (*snap_label).clone())
                && &existing != *snap_label
            {
                return Err(OxError::Validation {
                    field: "edge_rename".to_string(),
                    message: format!(
                        "ambiguous edge rename: current label `{}` maps to both \
                         `{}` and `{}` in the requested snapshot",
                        edge.label, existing, snap_label
                    ),
                });
            }
        }
    }
    Ok(map)
}

/// Walk every label surface in a `QueryOp` tree and substitute any
/// rename hits. Additive on the [Expr / Pattern / Mutate / Analytics]
/// coverage — a new label-carrying variant needs an arm here or the
/// rewriter will silently miss its renames.
fn apply_renames(
    op: &mut QueryOp,
    nodes: &HashMap<String, GraphLabel>,
    edges: &HashMap<String, GraphLabel>,
) -> OxResult<()> {
    match op {
        QueryOp::Match {
            patterns, filter, ..
        } => {
            for p in patterns {
                rename_pattern(p, nodes, edges);
            }
            if let Some(expr) = filter {
                rename_expr(expr, nodes, edges)?;
            }
        }
        QueryOp::PathFind {
            start,
            end,
            edge_types,
            ..
        } => {
            if let Some(l) = start.label.as_mut() {
                swap_if_renamed(l, nodes);
            }
            if let Some(l) = end.label.as_mut() {
                swap_if_renamed(l, nodes);
            }
            for l in edge_types.iter_mut() {
                swap_if_renamed(l, edges);
            }
        }
        QueryOp::Aggregate {
            source, having, ..
        } => {
            apply_renames(&mut source.operation, nodes, edges)?;
            if let Some(expr) = having {
                rename_expr(expr, nodes, edges)?;
            }
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                apply_renames(&mut q.operation, nodes, edges)?;
            }
        }
        QueryOp::Chain { steps } => {
            for s in steps {
                apply_renames(&mut s.operation, nodes, edges)?;
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            apply_renames(&mut inner.operation, nodes, edges)?;
        }
        QueryOp::Mutate {
            context,
            operations,
            ..
        } => {
            if let Some(ctx) = context {
                apply_renames(ctx, nodes, edges)?;
            }
            for mut_op in operations {
                rename_mutate_op(mut_op, nodes, edges);
            }
        }
        QueryOp::Analytics { source, .. } => {
            if let AnalyticsSource::Labels {
                labels: src_labels, ..
            } = source
            {
                for l in src_labels.iter_mut() {
                    swap_if_renamed(l, nodes);
                }
            }
            if let AnalyticsSource::Subgraph { filter } = source {
                apply_renames(filter, nodes, edges)?;
            }
        }
    }
    Ok(())
}

fn rename_pattern(
    pattern: &mut GraphPattern,
    nodes: &HashMap<String, GraphLabel>,
    edges: &HashMap<String, GraphLabel>,
) {
    match pattern {
        GraphPattern::Node { label, .. } => {
            if let Some(l) = label.as_mut() {
                swap_if_renamed(l, nodes);
            }
        }
        GraphPattern::Relationship { label, .. } => {
            if let Some(l) = label.as_mut() {
                swap_if_renamed(l, edges);
            }
        }
        GraphPattern::Path { elements } => {
            for elem in elements {
                match elem {
                    PathElement::Node { label, .. } => {
                        if let Some(l) = label.as_mut() {
                            swap_if_renamed(l, nodes);
                        }
                    }
                    PathElement::Edge { label, .. } => {
                        if let Some(l) = label.as_mut() {
                            swap_if_renamed(l, edges);
                        }
                    }
                }
            }
        }
    }
}

fn rename_expr(
    expr: &mut Expr,
    nodes: &HashMap<String, GraphLabel>,
    edges: &HashMap<String, GraphLabel>,
) -> OxResult<()> {
    match expr {
        Expr::Comparison { left, right, .. } | Expr::StringOp { left, right, .. } => {
            rename_expr(left, nodes, edges)?;
            rename_expr(right, nodes, edges)?;
        }
        Expr::Logical { left, right, .. } => {
            rename_expr(left, nodes, edges)?;
            rename_expr(right, nodes, edges)?;
        }
        Expr::Not { inner } => rename_expr(inner, nodes, edges)?,
        Expr::In { expr, .. } => rename_expr(expr, nodes, edges)?,
        Expr::IsNull { expr, .. } => rename_expr(expr, nodes, edges)?,
        Expr::FunctionCall { args, .. } => {
            for a in args {
                rename_expr(a, nodes, edges)?;
            }
        }
        Expr::Exists { pattern } => rename_pattern(pattern, nodes, edges),
        Expr::Case {
            operand,
            when_clauses,
            else_result,
        } => {
            if let Some(op) = operand {
                rename_expr(op, nodes, edges)?;
            }
            for w in when_clauses {
                rename_expr(&mut w.condition, nodes, edges)?;
                rename_expr(&mut w.result, nodes, edges)?;
            }
            if let Some(e) = else_result {
                rename_expr(e, nodes, edges)?;
            }
        }
        Expr::Subquery { query, .. } => {
            apply_renames(&mut query.operation, nodes, edges)?;
        }
        // Leaves with no label surface.
        Expr::Literal { .. } | Expr::Param { .. } | Expr::Property { .. } => {}
    }
    Ok(())
}

fn rename_mutate_op(
    op: &mut MutateOp,
    nodes: &HashMap<String, GraphLabel>,
    edges: &HashMap<String, GraphLabel>,
) {
    match op {
        MutateOp::CreateNode { label, .. }
        | MutateOp::MergeNode { label, .. }
        | MutateOp::RemoveLabel { label, .. } => {
            swap_if_renamed(label, nodes);
        }
        MutateOp::CreateEdge { label, .. } | MutateOp::MergeEdge { label, .. } => {
            swap_if_renamed(label, edges);
        }
        MutateOp::SetProperty { .. }
        | MutateOp::RemoveProperty { .. }
        | MutateOp::Delete { .. } => {}
    }
}

fn swap_if_renamed(label: &mut GraphLabel, map: &HashMap<String, GraphLabel>) {
    if let Some(target) = map.get(label.as_str()) {
        *label = target.clone();
    }
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

    fn snapshot_with_label(
        number: u32,
        valid_from: Option<DateTime<Utc>>,
        valid_to: Option<DateTime<Utc>>,
        node_id: &'static str,
        label: &'static str,
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
                id: node_id.into(),
                label: gl(label),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
    }

    #[test]
    fn rename_rewrites_current_label_to_snapshot_label() {
        // At as_of the node was labelled "Client"; today it's "Customer".
        // Query written today references "Customer" → rewritten to "Client".
        let snap = snapshot_with_label(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Client",
        );
        let current = snapshot_with_label(2, Some(ts(2026, 6, 1)), None, "nt1", "Customer");
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
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
            as_of: Some(ts(2026, 3, 15)),
        };

        let out = rewrite_temporal_with_renames(query, &snap, &current)
            .expect("rewrite ok");
        assert!(out.as_of.is_none());
        let QueryOp::Match { patterns, .. } = &out.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node { label, .. } = &patterns[0] else {
            panic!("expected Node");
        };
        assert_eq!(
            label.as_ref().map(|l| l.as_str()),
            Some("Client"),
            "label must be rewritten to the snapshot-era name"
        );
    }

    #[test]
    fn rename_pass_no_op_when_as_of_absent() {
        // Even with a rename pair that would otherwise apply, a non-
        // temporal query must not be rewritten — the caller's intent
        // is "run against current" when as_of is None.
        let snap = snapshot_with_label(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Client",
        );
        let current = snapshot_with_label(2, Some(ts(2026, 6, 1)), None, "nt1", "Customer");
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
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
            as_of: None,
        };
        let out = rewrite_temporal_with_renames(query, &snap, &current)
            .expect("rewrite ok");
        let QueryOp::Match { patterns, .. } = &out.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node { label, .. } = &patterns[0] else {
            panic!("expected Node");
        };
        assert_eq!(
            label.as_ref().map(|l| l.as_str()),
            Some("Customer"),
            "no temporal pivot → label must stay current"
        );
    }

    #[test]
    fn rename_leaves_unchanged_labels_alone() {
        // Node id `nt1` — same label in both ontologies → no rewrite.
        let snap = snapshot_with_label(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
        );
        let current = snapshot_with_label(2, Some(ts(2026, 6, 1)), None, "nt1", "Customer");
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
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
            as_of: Some(ts(2026, 3, 15)),
        };
        let out = rewrite_temporal_with_renames(query, &snap, &current)
            .expect("rewrite ok");
        let QueryOp::Match { patterns, .. } = &out.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node { label, .. } = &patterns[0] else {
            panic!("expected Node");
        };
        assert_eq!(label.as_ref().map(|l| l.as_str()), Some("Customer"));
    }

    #[test]
    fn rename_diff_map_excludes_new_node_types() {
        // `nt2` exists in current but not in snapshot — rewriting a
        // query that references it against `snap` should still clear
        // as_of and leave the label alone (the compiler/runtime will
        // surface the "unknown label" error against the snapshot).
        let snap = snapshot_with_label(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
        );
        let current = {
            let mut ont = snapshot_with_label(
                2,
                Some(ts(2026, 6, 1)),
                None,
                "nt1",
                "Customer",
            );
            ont.add_node_type(NodeTypeDef {
                id: "nt2".into(),
                label: gl("Order"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            })
            .expect("add Order node");
            ont
        };
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("o"),
                    label: Some(gl("Order")),
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
            as_of: Some(ts(2026, 3, 15)),
        };
        let out = rewrite_temporal_with_renames(query, &snap, &current)
            .expect("rewrite ok");
        let QueryOp::Match { patterns, .. } = &out.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node { label, .. } = &patterns[0] else {
            panic!("expected Node");
        };
        assert_eq!(
            label.as_ref().map(|l| l.as_str()),
            Some("Order"),
            "labels for types that didn't exist at as_of must pass through \
             untouched — the compiler will surface the error against the snapshot"
        );
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

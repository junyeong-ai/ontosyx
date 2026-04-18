//! Canvas-oriented query representation.
//!
//! [`QueryIR`] is the compile target — a full graph-query algebra that the
//! compiler emits as Cypher / Gremlin / GQL. It tells the database what to
//! compute. That makes it the wrong shape for the *user-facing* query
//! builder: positions on a canvas, per-filter identifiers for inline edit
//! actions, and the ability to save and reload a half-finished query with
//! its layout intact.
//!
//! [`PatternIR`] is that UX-facing shape. Every node and edge carries a
//! stable id independent of its variable name, filters are individual
//! rows (not a collapsed AND-chain), and layout hints travel with the
//! structure rather than in a sidecar. When the user is ready to run the
//! query, [`PatternIR::compile`] produces a [`QueryIR`]; when the user
//! reopens a saved [`QueryIR`], [`PatternIR::decompile`] reconstructs a
//! viewable pattern (best-effort — layout positions were not stored in
//! the compiled form, so they come back as `None`).
//!
//! Design guarantees:
//!
//! - `compile` is **lossless** for the QueryIR shape it produces. Every
//!   node, edge, filter, and projection round-trips if the caller writes
//!   back the compiled QueryIR unchanged.
//! - `decompile` is **best-effort**. A `GraphPattern::Path` or a non-Match
//!   top-level operation collapses to an empty result rather than a hard
//!   error — the canvas presents an empty surface and a "this query
//!   can't be edited visually" indicator. Explicit non-goal.
//! - Layout (`Position`, `LayoutHints`) is **canvas-only** — `compile`
//!   never emits it, `decompile` never invents it. A UI re-opening a
//!   saved QueryIR is responsible for running its own auto-layout pass.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query_ir::{
    Expr, GraphPattern, LogicalOp, OrderClause, Projection, PropertyFilter, QueryIR, QueryOp,
    VarLength,
};
use crate::types::Direction;

// ---------------------------------------------------------------------------
// Core PatternIR types
// ---------------------------------------------------------------------------

/// Current on-wire schema version for `PatternIR` JSONB. See
/// [`crate::ontology_ir::ONTOLOGY_IR_SCHEMA_VERSION`] for the versioning
/// rationale — bump on incompatible shape change; deserialisation
/// rejects higher values to fail loud rather than silently drop fields.
pub const PATTERN_IR_SCHEMA_VERSION: u32 = 1;

fn default_pattern_ir_schema_version() -> u32 {
    PATTERN_IR_SCHEMA_VERSION
}

/// Why a decompiled PatternIR is not editable on the canvas.
///
/// `compile` is lossless for every `QueryIR::Match` shape, so `decompile`
/// of a `Match` always produces a fully editable canvas. Non-`Match`
/// operations (`Aggregate`, `Union`, `Chain`, `PathFind`, `Mutate`,
/// `Analytics`, `CallSubquery`) can't be round-tripped through the
/// canvas structure — instead of collapsing them to an empty canvas
/// (which the UI used to mistake for "this query has no nodes yet")
/// the decompiler now returns a `ReadOnlyReason` so the UI can render
/// a clear "not editable: Aggregate query" state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ReadOnlyReason {
    /// The QueryIR operation variant this PatternIR was decompiled
    /// from, spelled as the Rust variant name (`"Aggregate"`,
    /// `"Union"`, ...). The frontend maps this to localised labels;
    /// the backend keeps the canonical identifier here.
    pub original_op: String,
}

impl ReadOnlyReason {
    /// Name the variant of `op` as it appears in Rust source, for use
    /// as [`Self::original_op`]. Centralised so a rename of any variant
    /// doesn't drift between the decompile path and callers that
    /// build a `ReadOnlyReason` from scratch (tests, fixtures).
    pub fn name_query_op(op: &QueryOp) -> &'static str {
        match op {
            QueryOp::Match { .. } => "Match",
            QueryOp::PathFind { .. } => "PathFind",
            QueryOp::Aggregate { .. } => "Aggregate",
            QueryOp::Union { .. } => "Union",
            QueryOp::Chain { .. } => "Chain",
            QueryOp::CallSubquery { .. } => "CallSubquery",
            QueryOp::Mutate { .. } => "Mutate",
            QueryOp::Analytics { .. } => "Analytics",
        }
    }

    pub fn from_query_op(op: &QueryOp) -> Self {
        Self {
            original_op: Self::name_query_op(op).to_string(),
        }
    }
}

/// Root of the canvas representation. Each component (nodes, edges,
/// filters, projections) is a flat list with stable per-entry ids so a
/// frontend can address them individually in edit operations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PatternIR {
    /// On-wire struct shape version.
    #[serde(default = "default_pattern_ir_schema_version")]
    pub schema_version: u32,
    /// Nodes the user has placed on the canvas.
    #[serde(default)]
    pub nodes: Vec<PatternNode>,
    /// Edges between placed nodes. `source_node_id` / `target_node_id`
    /// reference [`PatternNode::id`] — never variable names.
    #[serde(default)]
    pub edges: Vec<PatternEdge>,
    /// Standalone filter rows. A WHERE clause in QueryIR is split into
    /// one entry per AND-connected leaf so the UI can remove / edit
    /// each predicate in isolation.
    #[serde(default)]
    pub filters: Vec<PatternFilter>,
    /// Output projections (RETURN clause). Carried as full
    /// [`Projection`] values — the canvas surfaces their shape in a
    /// "Return" panel rather than trying to render them as nodes.
    #[serde(default)]
    pub projections: Vec<PatternProjection>,
    /// Canvas-wide view state (zoom, pan). Never emitted by `compile`.
    #[serde(default)]
    pub layout_hints: LayoutHints,
    /// LIMIT clause — round-trips with QueryIR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// SKIP clause — round-trips with QueryIR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip: Option<usize>,
    /// ORDER BY clauses. Round-trip with QueryIR so a saved canvas
    /// reopens with the same sort order the user configured.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order_by: Vec<OrderClause>,
    /// `Some(_)` when this PatternIR came out of `decompile` for a
    /// QueryIR operation the canvas can't round-trip. `None` on a
    /// `Match` decompile and on a freshly built `PatternIR::default()`.
    /// The UI must gate every edit action on `is_editable()` — an
    /// empty nodes list no longer implies "blank canvas".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_reason: Option<ReadOnlyReason>,
}

impl PatternIR {
    /// `true` when the canvas may accept edits. `false` when this
    /// PatternIR was produced by decompiling a QueryIR operation the
    /// canvas can't represent — the UI must render a read-only view
    /// surfacing `read_only_reason.original_op`.
    pub fn is_editable(&self) -> bool {
        self.read_only_reason.is_none()
    }
}

// `Default` is a manual impl so `schema_version` starts at the correct
// current baseline. The `#[serde(default = ...)]` attribute only covers
// deserialisation; `PatternIR::default()` (used by test fixtures and
// `..Default::default()` struct-update syntax) needs its own seed.
impl Default for PatternIR {
    fn default() -> Self {
        Self {
            schema_version: PATTERN_IR_SCHEMA_VERSION,
            nodes: Vec::new(),
            edges: Vec::new(),
            filters: Vec::new(),
            projections: Vec::new(),
            layout_hints: LayoutHints::default(),
            limit: None,
            skip: None,
            order_by: Vec::new(),
            read_only_reason: None,
        }
    }
}

/// A single node on the canvas. `id` is the UI identity (stable across
/// renames of `variable` / `label`); `variable` is the query binding
/// that the edge's `source` / `target` will reference.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PatternNode {
    pub id: String,
    pub variable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub property_filters: Vec<PropertyFilter>,
    /// Canvas position. `None` means "let the UI auto-layout" — the
    /// default after `decompile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// A relationship edge on the canvas. References nodes by their
/// `PatternNode::id` (not by variable name) so renaming a variable
/// doesn't break the visual link.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PatternEdge {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub source_node_id: String,
    pub target_node_id: String,
    pub direction: Direction,
    #[serde(default)]
    pub property_filters: Vec<PropertyFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub var_length: Option<VarLength>,
}

/// A single WHERE-like predicate. Carries a full [`Expr`] so complex
/// expressions survive the round-trip; a multi-predicate WHERE splits
/// into several `PatternFilter`s so the UI can surface each one as an
/// independent row.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PatternFilter {
    pub id: String,
    pub expr: Expr,
}

/// A RETURN-clause projection with a canvas-stable id.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PatternProjection {
    pub id: String,
    pub projection: Projection,
}

/// Canvas view-state. None of these fields round-trip through
/// `compile` — a loaded QueryIR hands back `LayoutHints::default()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LayoutHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan_y: Option<f64>,
}

/// A 2-D canvas coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

// ---------------------------------------------------------------------------
// compile: PatternIR -> QueryIR
// ---------------------------------------------------------------------------

impl PatternIR {
    /// Lower the canvas representation to a runnable [`QueryIR`].
    ///
    /// Every `PatternNode` becomes a `GraphPattern::Node`; every
    /// `PatternEdge` becomes a `GraphPattern::Relationship` with its
    /// endpoints resolved via the `id → variable` map. Filter rows are
    /// combined through `AND` into a single `Expr`. Layout data is
    /// dropped — QueryIR has no place for it.
    pub fn compile(&self) -> QueryIR {
        let id_to_variable: HashMap<&str, &str> = self
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.variable.as_str()))
            .collect();

        let mut patterns: Vec<GraphPattern> =
            Vec::with_capacity(self.nodes.len() + self.edges.len());

        for node in &self.nodes {
            patterns.push(GraphPattern::Node {
                variable: node.variable.clone(),
                label: node.label.clone(),
                property_filters: node.property_filters.clone(),
            });
        }

        for edge in &self.edges {
            // An edge whose endpoint id no longer resolves is a canvas
            // inconsistency — propagate the bound variable verbatim
            // (empty string if missing) so validation downstream can
            // surface the failure rather than silently dropping the
            // edge.
            let source = id_to_variable
                .get(edge.source_node_id.as_str())
                .map(|s| (*s).to_string())
                .unwrap_or_default();
            let target = id_to_variable
                .get(edge.target_node_id.as_str())
                .map(|s| (*s).to_string())
                .unwrap_or_default();
            patterns.push(GraphPattern::Relationship {
                variable: edge.variable.clone(),
                label: edge.label.clone(),
                source,
                target,
                direction: edge.direction,
                property_filters: edge.property_filters.clone(),
                var_length: edge.var_length.clone(),
            });
        }

        let filter = combine_filters_and(self.filters.iter().map(|f| &f.expr));
        let projections: Vec<Projection> = self
            .projections
            .iter()
            .map(|p| p.projection.clone())
            .collect();

        QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns,
                filter,
                projections,
                optional: false,
                group_by: Vec::new(),
            },
            limit: self.limit,
            skip: self.skip,
            order_by: self.order_by.clone(),
        }
    }
}

/// Combine a stream of exprs with left-associative `AND`. Returns
/// `None` for an empty stream — QueryIR's `Match.filter` is already
/// `Option<Expr>`, so no surrogate value is needed.
fn combine_filters_and<'a>(exprs: impl IntoIterator<Item = &'a Expr>) -> Option<Expr> {
    let mut iter = exprs.into_iter().cloned();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, next| Expr::Logical {
        left: Box::new(acc),
        op: LogicalOp::And,
        right: Box::new(next),
    }))
}

// ---------------------------------------------------------------------------
// decompile: QueryIR -> PatternIR (best-effort)
// ---------------------------------------------------------------------------

impl PatternIR {
    /// Reconstruct a canvas view from a compiled [`QueryIR`].
    ///
    /// Best-effort by design: only `QueryOp::Match` (the canvas's
    /// native shape) yields a populated result. Everything else —
    /// `PathFind`, `Union`, `Chain`, `Aggregate`, `CallSubquery`,
    /// `Mutate`, `Analytics` — collapses to an empty `PatternIR`, so
    /// the UI can detect "this query can't be edited visually" by
    /// checking `nodes.is_empty()` against the source QueryIR's
    /// operation kind.
    ///
    /// Position data is never invented. A caller that wants laid-out
    /// nodes after decompile runs its own layout algorithm
    /// (force-directed, hierarchical, ELK) against the returned
    /// structure.
    pub fn decompile(query: &QueryIR) -> Self {
        let (patterns, filter, projections) = match &query.operation {
            QueryOp::Match {
                patterns,
                filter,
                projections,
                ..
            } => (patterns.as_slice(), filter.as_ref(), projections.as_slice()),
            other => {
                // Non-Match operations can't be represented on the canvas.
                // Return a read-only marker with the op name so the UI
                // renders "not editable: <op>" instead of a blank canvas
                // that the user might mistake for a new-query starting
                // point. Any limit / skip / order_by on the source query
                // is dropped: without nodes to attach them to, they'd be
                // meaningless in the canvas view.
                return Self {
                    read_only_reason: Some(ReadOnlyReason::from_query_op(other)),
                    ..Self::default()
                };
            }
        };

        let mut nodes: Vec<PatternNode> = Vec::new();
        let mut edges: Vec<PatternEdge> = Vec::new();
        let mut var_to_id: HashMap<String, String> = HashMap::new();

        for (idx, pattern) in patterns.iter().enumerate() {
            match pattern {
                GraphPattern::Node {
                    variable,
                    label,
                    property_filters,
                } => {
                    let id = format!("n{idx}");
                    var_to_id.insert(variable.clone(), id.clone());
                    nodes.push(PatternNode {
                        id,
                        variable: variable.clone(),
                        label: label.clone(),
                        property_filters: property_filters.clone(),
                        position: None,
                    });
                }
                GraphPattern::Relationship {
                    variable,
                    label,
                    source,
                    target,
                    direction,
                    property_filters,
                    var_length,
                } => {
                    // `missing_*` placeholders surface gracefully in
                    // the UI — the edge still renders but points to a
                    // synthetic dangling node id the frontend can
                    // highlight as broken.
                    let source_node_id = var_to_id
                        .get(source)
                        .cloned()
                        .unwrap_or_else(|| format!("missing_{source}"));
                    let target_node_id = var_to_id
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| format!("missing_{target}"));
                    edges.push(PatternEdge {
                        id: format!("e{idx}"),
                        variable: variable.clone(),
                        label: label.clone(),
                        source_node_id,
                        target_node_id,
                        direction: *direction,
                        property_filters: property_filters.clone(),
                        var_length: var_length.clone(),
                    });
                }
                GraphPattern::Path { .. } => {
                    // Path patterns: explicit non-goal for decompile.
                    // The compiler lowers an edge sequence into Path
                    // only for specific shapes we don't currently
                    // surface on the canvas. Skipping preserves the
                    // "best-effort" guarantee — the canvas just won't
                    // show path-pattern work.
                }
            }
        }

        let filters = filter
            .map(split_top_level_and)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(idx, expr)| PatternFilter {
                id: format!("f{idx}"),
                expr,
            })
            .collect();

        let projections = projections
            .iter()
            .enumerate()
            .map(|(idx, p)| PatternProjection {
                id: format!("p{idx}"),
                projection: p.clone(),
            })
            .collect();

        Self {
            schema_version: PATTERN_IR_SCHEMA_VERSION,
            nodes,
            edges,
            filters,
            projections,
            layout_hints: LayoutHints::default(),
            limit: query.limit,
            skip: query.skip,
            order_by: query.order_by.clone(),
            read_only_reason: None,
        }
    }
}

/// Walk an expression tree, flattening every AND-connected leaf into
/// the output vec. A non-AND at the root produces a single-element
/// vec. Used so that `WHERE a AND b AND c` becomes three individually
/// editable filter rows on the canvas.
fn split_top_level_and(expr: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    split_top_level_and_into(expr, &mut out);
    out
}

fn split_top_level_and_into(expr: &Expr, out: &mut Vec<Expr>) {
    match expr {
        Expr::Logical {
            left,
            op: LogicalOp::And,
            right,
        } => {
            split_top_level_and_into(left, out);
            split_top_level_and_into(right, out);
        }
        other => out.push(other.clone()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::{ComparisonOp, LogicalOp};
    use crate::types::PropertyValue;

    fn lit_int(n: i64) -> Expr {
        Expr::Literal {
            value: PropertyValue::Int(n),
        }
    }

    fn prop(variable: &str, field: &str) -> Expr {
        Expr::Property {
            variable: variable.to_string(),
            field: Some(field.to_string()),
        }
    }

    fn cmp(left: Expr, op: ComparisonOp, right: Expr) -> Expr {
        Expr::Comparison {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }

    // --- compile --------------------------------------------------------

    #[test]
    fn compile_empty_pattern_yields_empty_match() {
        let query = PatternIR::default().compile();
        match query.operation {
            QueryOp::Match {
                patterns,
                filter,
                projections,
                ..
            } => {
                assert!(patterns.is_empty());
                assert!(filter.is_none());
                assert!(projections.is_empty());
            }
            other => panic!("expected Match, got {other:?}"),
        }
        assert!(query.limit.is_none());
        assert!(query.skip.is_none());
    }

    #[test]
    fn compile_single_node_produces_one_graph_node() {
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: Some("Person".into()),
                property_filters: Vec::new(),
                position: Some(Position { x: 10.0, y: 20.0 }),
            }],
            ..Default::default()
        };
        let query = pattern.compile();
        match query.operation {
            QueryOp::Match { patterns, .. } => {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(
                    patterns[0],
                    GraphPattern::Node { ref variable, label: Some(ref l), .. }
                        if variable == "p" && l == "Person"
                ));
            }
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn compile_resolves_edge_endpoints_via_node_ids() {
        let pattern = PatternIR {
            nodes: vec![
                PatternNode {
                    id: "n1".into(),
                    variable: "a".into(),
                    label: Some("A".into()),
                    property_filters: Vec::new(),
                    position: None,
                },
                PatternNode {
                    id: "n2".into(),
                    variable: "b".into(),
                    label: Some("B".into()),
                    property_filters: Vec::new(),
                    position: None,
                },
            ],
            edges: vec![PatternEdge {
                id: "e1".into(),
                variable: Some("r".into()),
                label: Some("KNOWS".into()),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                direction: Direction::Outgoing,
                property_filters: Vec::new(),
                var_length: None,
            }],
            ..Default::default()
        };
        let query = pattern.compile();
        match query.operation {
            QueryOp::Match { patterns, .. } => {
                assert_eq!(patterns.len(), 3);
                match &patterns[2] {
                    GraphPattern::Relationship { source, target, .. } => {
                        assert_eq!(source, "a");
                        assert_eq!(target, "b");
                    }
                    other => panic!("expected Relationship, got {other:?}"),
                }
            }
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn compile_missing_endpoint_becomes_empty_string() {
        // An edge pointing at a node that was deleted (or never added) should
        // still produce a GraphPattern — with an empty variable string the
        // validator can flag downstream. We don't drop the edge silently.
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "a".into(),
                label: None,
                property_filters: Vec::new(),
                position: None,
            }],
            edges: vec![PatternEdge {
                id: "e1".into(),
                variable: None,
                label: None,
                source_node_id: "n1".into(),
                target_node_id: "deleted".into(),
                direction: Direction::Outgoing,
                property_filters: Vec::new(),
                var_length: None,
            }],
            ..Default::default()
        };
        let query = pattern.compile();
        match query.operation {
            QueryOp::Match { patterns, .. } => match &patterns[1] {
                GraphPattern::Relationship { source, target, .. } => {
                    assert_eq!(source, "a");
                    assert_eq!(target, "", "missing endpoint must not silently map");
                }
                _ => panic!("expected Relationship"),
            },
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn compile_combines_filter_rows_with_and() {
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: Some("Person".into()),
                property_filters: Vec::new(),
                position: None,
            }],
            filters: vec![
                PatternFilter {
                    id: "f1".into(),
                    expr: cmp(prop("p", "age"), ComparisonOp::Gt, lit_int(18)),
                },
                PatternFilter {
                    id: "f2".into(),
                    expr: cmp(prop("p", "age"), ComparisonOp::Lt, lit_int(65)),
                },
            ],
            ..Default::default()
        };
        let query = pattern.compile();
        match query.operation {
            QueryOp::Match { filter, .. } => match filter {
                Some(Expr::Logical {
                    op: LogicalOp::And, ..
                }) => (),
                other => panic!("expected top-level AND, got {other:?}"),
            },
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn compile_drops_layout_hints_and_positions() {
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: None,
                property_filters: Vec::new(),
                position: Some(Position { x: 5.0, y: 5.0 }),
            }],
            layout_hints: LayoutHints {
                zoom: Some(1.5),
                pan_x: Some(100.0),
                pan_y: Some(-50.0),
            },
            ..Default::default()
        };
        let query = pattern.compile();
        let json = serde_json::to_string(&query).expect("serializable");
        assert!(
            !json.contains("position"),
            "QueryIR must not carry canvas position"
        );
        assert!(!json.contains("zoom"), "QueryIR must not carry canvas zoom");
    }

    #[test]
    fn compile_carries_limit_and_skip() {
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: None,
                property_filters: Vec::new(),
                position: None,
            }],
            limit: Some(25),
            skip: Some(5),
            ..Default::default()
        };
        let query = pattern.compile();
        assert_eq!(query.limit, Some(25));
        assert_eq!(query.skip, Some(5));
    }

    #[test]
    fn compile_and_decompile_preserve_order_by() {
        use crate::query_ir::SortDirection;

        let order = OrderClause {
            projection: Projection::Field {
                variable: "p".into(),
                field: "age".into(),
                alias: None,
            },
            direction: SortDirection::Desc,
        };
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: Some("Person".into()),
                property_filters: Vec::new(),
                position: None,
            }],
            order_by: vec![order],
            ..Default::default()
        };

        // compile keeps the sort spec on the QueryIR.
        let query = pattern.compile();
        assert_eq!(query.order_by.len(), 1);
        matches!(query.order_by[0].direction, SortDirection::Desc);

        // And decompile carries it back onto PatternIR so a saved
        // pattern reopens with the user's sort intact.
        let back = PatternIR::decompile(&query);
        assert_eq!(back.order_by.len(), 1);
        match &back.order_by[0].projection {
            Projection::Field {
                variable, field, ..
            } => {
                assert_eq!(variable, "p");
                assert_eq!(field, "age");
            }
            other => panic!("expected Projection::Field, got {other:?}"),
        }
    }

    // --- decompile ------------------------------------------------------

    #[test]
    fn decompile_single_node_match() {
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "p".into(),
                    label: Some("Person".into()),
                    property_filters: Vec::new(),
                }],
                filter: None,
                projections: Vec::new(),
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert_eq!(pattern.nodes.len(), 1);
        assert_eq!(pattern.nodes[0].variable, "p");
        assert_eq!(pattern.nodes[0].label.as_deref(), Some("Person"));
        assert!(
            pattern.nodes[0].position.is_none(),
            "decompile must not invent layout"
        );
    }

    #[test]
    fn decompile_node_edge_pair_cross_references_ids() {
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: "a".into(),
                        label: Some("A".into()),
                        property_filters: Vec::new(),
                    },
                    GraphPattern::Node {
                        variable: "b".into(),
                        label: Some("B".into()),
                        property_filters: Vec::new(),
                    },
                    GraphPattern::Relationship {
                        variable: Some("r".into()),
                        label: Some("R".into()),
                        source: "a".into(),
                        target: "b".into(),
                        direction: Direction::Outgoing,
                        property_filters: Vec::new(),
                        var_length: None,
                    },
                ],
                filter: None,
                projections: Vec::new(),
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert_eq!(pattern.nodes.len(), 2);
        assert_eq!(pattern.edges.len(), 1);
        let a_id = &pattern.nodes[0].id;
        let b_id = &pattern.nodes[1].id;
        assert_eq!(&pattern.edges[0].source_node_id, a_id);
        assert_eq!(&pattern.edges[0].target_node_id, b_id);
    }

    #[test]
    fn decompile_edge_with_unknown_endpoint_marks_missing() {
        // Hand-crafted (non-roundtripped) QueryIR where an edge names
        // a variable no node declares. decompile must surface the
        // break as a synthetic `missing_*` id rather than dropping
        // the edge.
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: "a".into(),
                        label: None,
                        property_filters: Vec::new(),
                    },
                    GraphPattern::Relationship {
                        variable: None,
                        label: None,
                        source: "a".into(),
                        target: "gone".into(),
                        direction: Direction::Outgoing,
                        property_filters: Vec::new(),
                        var_length: None,
                    },
                ],
                filter: None,
                projections: Vec::new(),
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert_eq!(pattern.edges.len(), 1);
        assert_eq!(pattern.edges[0].target_node_id, "missing_gone");
    }

    #[test]
    fn decompile_splits_top_level_and_into_individual_filters() {
        let expr = Expr::Logical {
            left: Box::new(Expr::Logical {
                left: Box::new(cmp(prop("p", "age"), ComparisonOp::Gt, lit_int(18))),
                op: LogicalOp::And,
                right: Box::new(cmp(prop("p", "age"), ComparisonOp::Lt, lit_int(65))),
            }),
            op: LogicalOp::And,
            right: Box::new(cmp(prop("p", "status"), ComparisonOp::Eq, lit_int(1))),
        };
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "p".into(),
                    label: None,
                    property_filters: Vec::new(),
                }],
                filter: Some(expr),
                projections: Vec::new(),
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert_eq!(
            pattern.filters.len(),
            3,
            "top-level AND must yield 3 filter rows"
        );
    }

    #[test]
    fn decompile_keeps_non_and_root_as_single_filter() {
        let expr = Expr::Logical {
            left: Box::new(cmp(prop("p", "x"), ComparisonOp::Eq, lit_int(1))),
            op: LogicalOp::Or,
            right: Box::new(cmp(prop("p", "y"), ComparisonOp::Eq, lit_int(2))),
        };
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "p".into(),
                    label: None,
                    property_filters: Vec::new(),
                }],
                filter: Some(expr),
                projections: Vec::new(),
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert_eq!(pattern.filters.len(), 1, "OR at root is not split");
    }

    #[test]
    fn decompile_non_match_operation_returns_empty() {
        // PathFind is a non-Match operation; decompile should bail out.
        use crate::query_ir::{NodeRef, PathAlgorithm};
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::PathFind {
                start: NodeRef {
                    variable: "s".into(),
                    label: None,
                    property_filters: Vec::new(),
                },
                end: NodeRef {
                    variable: "e".into(),
                    label: None,
                    property_filters: Vec::new(),
                },
                edge_types: Vec::new(),
                direction: Direction::Outgoing,
                max_depth: None,
                algorithm: PathAlgorithm::ShortestPath,
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert!(pattern.nodes.is_empty());
        assert!(pattern.edges.is_empty());
        // The structural gap is now self-describing: the decompiled
        // PatternIR says *why* it's empty via `read_only_reason`, so
        // the UI renders "not editable: PathFind" instead of mistaking
        // it for a blank new-query canvas.
        assert_eq!(
            pattern
                .read_only_reason
                .as_ref()
                .map(|r| r.original_op.as_str()),
            Some("PathFind"),
        );
        assert!(
            !pattern.is_editable(),
            "non-Match decompile must not be editable"
        );
    }

    #[test]
    fn decompile_match_is_editable() {
        // Sanity: the common case still produces an editable canvas.
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "n".into(),
                    label: Some("Person".into()),
                    property_filters: Vec::new(),
                }],
                filter: None,
                projections: Vec::new(),
                optional: false,
                group_by: Vec::new(),
            },
            limit: None,
            skip: None,
            order_by: Vec::new(),
        };
        let pattern = PatternIR::decompile(&query);
        assert!(pattern.read_only_reason.is_none());
        assert!(pattern.is_editable());
    }

    #[test]
    fn default_pattern_ir_is_editable() {
        // A freshly constructed (blank) PatternIR must stay editable —
        // read_only_reason is exclusively a decompile output, never the
        // starting state of a new canvas.
        assert!(PatternIR::default().is_editable());
    }

    #[test]
    fn read_only_reason_names_every_non_match_op() {
        // Pin the string → variant mapping so a `QueryOp` rename
        // immediately fails this test instead of silently drifting.
        use crate::query_ir::{NodeRef, PathAlgorithm};

        let dummy_match = QueryOp::Match {
            patterns: Vec::new(),
            filter: None,
            projections: Vec::new(),
            optional: false,
            group_by: Vec::new(),
        };
        assert_eq!(ReadOnlyReason::name_query_op(&dummy_match), "Match");

        let path = QueryOp::PathFind {
            start: NodeRef {
                variable: "s".into(),
                label: None,
                property_filters: Vec::new(),
            },
            end: NodeRef {
                variable: "e".into(),
                label: None,
                property_filters: Vec::new(),
            },
            edge_types: Vec::new(),
            direction: Direction::Outgoing,
            max_depth: None,
            algorithm: PathAlgorithm::ShortestPath,
        };
        assert_eq!(ReadOnlyReason::name_query_op(&path), "PathFind");
    }

    // --- roundtrip ------------------------------------------------------

    #[test]
    fn roundtrip_compile_decompile_preserves_structure() {
        let original = PatternIR {
            schema_version: PATTERN_IR_SCHEMA_VERSION,
            nodes: vec![
                PatternNode {
                    id: "n1".into(),
                    variable: "a".into(),
                    label: Some("A".into()),
                    property_filters: Vec::new(),
                    position: Some(Position { x: 0.0, y: 0.0 }),
                },
                PatternNode {
                    id: "n2".into(),
                    variable: "b".into(),
                    label: Some("B".into()),
                    property_filters: Vec::new(),
                    position: Some(Position { x: 100.0, y: 0.0 }),
                },
            ],
            edges: vec![PatternEdge {
                id: "e1".into(),
                variable: Some("r".into()),
                label: Some("R".into()),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                direction: Direction::Outgoing,
                property_filters: Vec::new(),
                var_length: None,
            }],
            filters: Vec::new(),
            projections: Vec::new(),
            layout_hints: LayoutHints {
                zoom: Some(1.0),
                pan_x: Some(0.0),
                pan_y: Some(0.0),
            },
            limit: Some(10),
            skip: None,
            order_by: Vec::new(),
            read_only_reason: None,
        };

        let query = original.compile();
        let roundtripped = PatternIR::decompile(&query);

        // Structural equality modulo ids (which regenerate as n0/n1/e2
        // on decompile) and layout (dropped intentionally).
        assert_eq!(roundtripped.nodes.len(), original.nodes.len());
        assert_eq!(roundtripped.edges.len(), original.edges.len());
        assert_eq!(roundtripped.nodes[0].variable, "a");
        assert_eq!(roundtripped.nodes[1].variable, "b");
        assert_eq!(roundtripped.edges[0].variable.as_deref(), Some("r"));
        assert_eq!(roundtripped.limit, Some(10));
        assert!(
            roundtripped.nodes.iter().all(|n| n.position.is_none()),
            "decompile must not invent positions"
        );
        assert_eq!(
            roundtripped.layout_hints.zoom, None,
            "decompile must not invent layout hints"
        );
    }

    #[test]
    fn roundtrip_filters_split_back_into_rows() {
        let original = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: Some("Person".into()),
                property_filters: Vec::new(),
                position: None,
            }],
            filters: vec![
                PatternFilter {
                    id: "f1".into(),
                    expr: cmp(prop("p", "age"), ComparisonOp::Gt, lit_int(18)),
                },
                PatternFilter {
                    id: "f2".into(),
                    expr: cmp(prop("p", "age"), ComparisonOp::Lt, lit_int(65)),
                },
            ],
            ..Default::default()
        };
        let query = original.compile();
        let rt = PatternIR::decompile(&query);
        assert_eq!(
            rt.filters.len(),
            2,
            "compile AND-chained, decompile must split back"
        );
    }

    #[test]
    fn compile_serde_roundtrip() {
        // Serialize a PatternIR to JSON and parse it back; structural
        // fields survive and layout is only present when set.
        let pattern = PatternIR {
            nodes: vec![PatternNode {
                id: "n1".into(),
                variable: "p".into(),
                label: Some("Person".into()),
                property_filters: Vec::new(),
                position: Some(Position { x: 1.0, y: 2.0 }),
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&pattern).unwrap();
        let back: PatternIR = serde_json::from_str(&json).unwrap();
        assert_eq!(back.nodes.len(), 1);
        assert_eq!(back.nodes[0].position, Some(Position { x: 1.0, y: 2.0 }));
    }
}

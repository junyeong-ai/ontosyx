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
use ox_ontology::ir::{EdgeTypeDef, EdgeTypeId, NodeTypeId, OntologyIR};
use ox_core::property_key::PropertyKey;
use ox_query_ir::query::{
    AnalyticsSource, Expr, GraphPattern, MutateOp, NodeRef, PathElement, PropertyAssignment,
    PropertyFilter, QueryIR, QueryOp,
};
use ox_core::variable_name::VariableName;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// The owner-type of a bound variable — the piece of schema a variable
/// refers to in a pattern. Resolved from the current ontology because
/// type ids are stable across renames; the snapshot's type id for the
/// same semantic type is identical, so the owner ref is a reliable
/// bridge between "the query's authored labels" and "the snapshot's
/// property map".
#[derive(Debug, Clone)]
enum OwnerRef {
    Node(NodeTypeId),
    Edge(EdgeTypeId),
}

/// Label rename map: `current_label → snapshot_label`, keyed by the
/// strongly-typed `GraphLabel`. Using the newtype as the key (rather
/// than the raw `String` used in earlier iterations) rules out
/// accidental cross-type mixing — e.g. a variable name hashed as a
/// label.
type LabelRenames = HashMap<GraphLabel, GraphLabel>;

/// Property rename map keyed by `(owner_type_id, current_property_name)`.
/// Two types with same-named properties rename into different
/// snapshot names without colliding because the key includes the owner.
type NodePropRenames = HashMap<(NodeTypeId, PropertyKey), PropertyKey>;
type EdgePropRenames = HashMap<(EdgeTypeId, PropertyKey), PropertyKey>;

/// Bundled rename tables and lookup context. Grouping the four rename
/// maps plus the variable→owner map plus the `current` ontology into a
/// single struct keeps the recursive walkers (`apply_renames`,
/// `rename_expr`, `rename_mutate_op`, `rename_pattern`) at a manageable
/// argument count and makes it obvious when a caller forgot to propagate
/// context into a new sub-tree variant.
struct RenameCtx<'a> {
    /// The current (authored-against) ontology. Labels in the input
    /// query are authored against this, so label→type_id resolution
    /// happens here before any rename is applied.
    current: &'a OntologyIR,
    /// Label renames detected by diffing current vs snapshot.
    node_labels: &'a LabelRenames,
    edge_labels: &'a LabelRenames,
    /// Property renames, keyed by stable owner type id.
    node_props: &'a NodePropRenames,
    edge_props: &'a EdgePropRenames,
    /// Variable bindings collected by `collect_variable_owners`.
    /// `Expr::Property { variable, field }` and `MutateOp::SetProperty`
    /// use this to route a property reference to the right rename map.
    var_types: &'a HashMap<VariableName, OwnerRef>,
}

impl RenameCtx<'_> {
    /// Rename a property reference anchored to a variable. Returns
    /// silently when the variable has no owner binding (no pattern
    /// label, or a label the current ontology doesn't declare) —
    /// silently guessing a type could corrupt unrelated property
    /// references.
    fn rename_var_property(&self, variable: &VariableName, property: &mut PropertyKey) {
        let Some(owner) = self.var_types.get(variable) else {
            return;
        };
        match owner {
            OwnerRef::Node(tid) => {
                if let Some(snap) = self.node_props.get(&(tid.clone(), property.clone())) {
                    *property = snap.clone();
                }
            }
            OwnerRef::Edge(tid) => {
                if let Some(snap) = self.edge_props.get(&(tid.clone(), property.clone())) {
                    *property = snap.clone();
                }
            }
        }
    }
}

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

    let node_labels = diff_node_labels(current, snapshot)?;
    let edge_labels = diff_edge_labels(current, snapshot)?;
    let node_props = diff_node_property_renames(current, snapshot);
    let edge_props = diff_edge_property_renames(current, snapshot);

    // Early-out when nothing moved between versions.
    // `var_types` collection is still non-trivial (walks the whole AST)
    // so skip it in the common no-rename case.
    if node_labels.is_empty()
        && edge_labels.is_empty()
        && node_props.is_empty()
        && edge_props.is_empty()
    {
        return Ok(query);
    }

    // Build a variable → owner-type map so `Expr::Property` and
    // `MutateOp::SetProperty` references can resolve to the right
    // property rename map. Walk every pattern surface (Match / PathFind
    // / Mutate / Analytics / nested subqueries in filter expressions) —
    // a missed surface means silent no-op on that variable's property
    // references.
    let mut var_types: HashMap<VariableName, OwnerRef> = HashMap::new();
    collect_variable_owners(&query.operation, current, &mut var_types);

    let ctx = RenameCtx {
        current,
        node_labels: &node_labels,
        edge_labels: &edge_labels,
        node_props: &node_props,
        edge_props: &edge_props,
        var_types: &var_types,
    };

    apply_renames(&mut query.operation, &ctx)?;
    Ok(query)
}

/// Build a `current_label → snapshot_label` map for node types whose
/// labels changed between the two ontologies.
///
/// Shared ids with identical labels contribute nothing. Ids unique to
/// either side (newly created types or types removed since) also
/// contribute nothing — the rewriter can't rewrite what isn't in the
/// current query's label set.
/// Build a `current_label → snapshot_label` map for node types whose
/// labels changed between the two ontologies.
///
/// Shared ids with identical labels contribute nothing. Ids unique to
/// either side (newly created types or types removed since) also
/// contribute nothing — the rewriter can't rewrite what isn't in the
/// current query's label set.
///
/// A second current label collapsing onto an already-mapped
/// snapshot-label-key would be ambiguous — refuse rather
/// than pick a winner.
fn diff_node_labels(current: &OntologyIR, snapshot: &OntologyIR) -> OxResult<LabelRenames> {
    let snapshot_by_id: HashMap<&str, &GraphLabel> = snapshot
        .node_types()
        .iter()
        .map(|n| (n.id.as_ref(), &n.label))
        .collect();

    let mut map = LabelRenames::new();
    for node in current.node_types() {
        if let Some(snap_label) = snapshot_by_id.get(node.id.as_ref())
            && *snap_label != &node.label
            && let Some(existing) = map.insert(node.label.clone(), (*snap_label).clone())
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
    Ok(map)
}

/// Mirror of [`diff_node_labels`] for edge types. See that function's
/// docs for the diff semantics and collision rule.
fn diff_edge_labels(current: &OntologyIR, snapshot: &OntologyIR) -> OxResult<LabelRenames> {
    let snapshot_by_id: HashMap<&str, &GraphLabel> = snapshot
        .edge_types()
        .iter()
        .map(|e| (e.id.as_ref(), &e.label))
        .collect();

    let mut map = LabelRenames::new();
    for edge in current.edge_types() {
        if let Some(snap_label) = snapshot_by_id.get(edge.id.as_ref())
            && *snap_label != &edge.label
            && let Some(existing) = map.insert(edge.label.clone(), (*snap_label).clone())
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
    Ok(map)
}

/// Build a per-owner-type property rename map for node types. Property
/// ids are stable across renames, so we walk current's properties, look
/// each one up by id in the snapshot, and record `(current_name →
/// snapshot_name)` when the names differ.
///
/// Keyed by `(owner_type_id, current_property_name)` — a same-named
/// property on two different types can rename to different snapshot
/// names without colliding because the key includes the owner. A query
/// rewriter resolves the owner type via the variable's pattern label
/// before consulting this map.
///
/// Unlike label rename, property rename has no collision case: a
/// single property id can only have one name per snapshot, and the
/// `(type_id, current_name)` key uniquely identifies the slot.
fn diff_node_property_renames(current: &OntologyIR, snapshot: &OntologyIR) -> NodePropRenames {
    let mut map: NodePropRenames = HashMap::new();
    for cur_nt in current.node_types() {
        let Some(snap_nt) = snapshot.node_by_id(cur_nt.id.as_ref()) else {
            continue;
        };
        for cur_prop in &cur_nt.properties {
            if let Some(snap_prop) = snap_nt.properties.iter().find(|p| p.id == cur_prop.id)
                && snap_prop.name != cur_prop.name
            {
                map.insert(
                    (cur_nt.id.clone(), cur_prop.name.clone()),
                    snap_prop.name.clone(),
                );
            }
        }
    }
    map
}

/// Build a per-owner-type property rename map for edge types. Mirror of
/// [`diff_node_property_renames`] for edges.
fn diff_edge_property_renames(current: &OntologyIR, snapshot: &OntologyIR) -> EdgePropRenames {
    let mut map: EdgePropRenames = HashMap::new();
    for cur_et in current.edge_types() {
        let Some(snap_et) = snapshot.edge_by_id(cur_et.id.as_ref()) else {
            continue;
        };
        for cur_prop in &cur_et.properties {
            if let Some(snap_prop) = snap_et.properties.iter().find(|p| p.id == cur_prop.id)
                && snap_prop.name != cur_prop.name
            {
                map.insert(
                    (cur_et.id.clone(), cur_prop.name.clone()),
                    snap_prop.name.clone(),
                );
            }
        }
    }
    map
}

/// Walk a `QueryOp` tree and record every bound variable's owner type
/// id (resolved via the current ontology's label → type_id index).
///
/// Labels are taken from the query AST as authored (pre-rename), so we
/// consult `current` rather than `snapshot`. Variables whose pattern
/// carries no label, or whose label is unknown in `current`, stay
/// unbound in the map — `Expr::Property` against such a variable is
/// left as-is because we can't safely pick a property rename map.
///
/// Nested subqueries (both `QueryOp::CallSubquery` and `Expr::Subquery`
/// inside filter / having / case / function arguments) are walked into
/// recursively, so variables declared in subqueries can have their
/// own property references renamed. Scope leakage is a non-issue
/// because a HashMap's last-write-wins shadowing already matches
/// Cypher's "inner scope binding wins" semantics for name collisions —
/// in practice, query-builder UIs and LLM output rarely shadow names
/// across scopes, so the flat map is safe.
fn collect_variable_owners(
    op: &QueryOp,
    current: &OntologyIR,
    map: &mut HashMap<VariableName, OwnerRef>,
) {
    match op {
        QueryOp::Match {
            patterns, filter, ..
        } => {
            for p in patterns {
                collect_pattern_vars(p, current, map);
            }
            // Filter can host `Expr::Subquery { query }` with its own
            // pattern bindings; walk into it so we don't miss inner
            // property references.
            if let Some(expr) = filter {
                collect_expr_vars(expr, current, map);
            }
        }
        QueryOp::PathFind {
            start,
            end,
            edge_types: _,
            ..
        } => {
            bind_node_var(&start.variable, start.label.as_ref(), current, map);
            bind_node_var(&end.variable, end.label.as_ref(), current, map);
        }
        QueryOp::Aggregate {
            source, having, ..
        } => {
            collect_variable_owners(&source.operation, current, map);
            if let Some(expr) = having {
                collect_expr_vars(expr, current, map);
            }
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                collect_variable_owners(&q.operation, current, map);
            }
        }
        QueryOp::Chain { steps } => {
            for s in steps {
                collect_variable_owners(&s.operation, current, map);
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            collect_variable_owners(&inner.operation, current, map);
        }
        QueryOp::Mutate {
            context,
            operations,
            ..
        } => {
            if let Some(ctx) = context {
                collect_variable_owners(ctx, current, map);
            }
            for mut_op in operations {
                match mut_op {
                    MutateOp::CreateNode {
                        variable, label, ..
                    }
                    | MutateOp::MergeNode {
                        variable, label, ..
                    } => {
                        bind_node_var(variable, Some(label), current, map);
                    }
                    MutateOp::CreateEdge {
                        variable: Some(var),
                        label,
                        ..
                    }
                    | MutateOp::MergeEdge {
                        variable: Some(var),
                        label,
                        ..
                    } => {
                        bind_edge_var(var, label, current, map);
                    }
                    _ => {}
                }
            }
        }
        QueryOp::Analytics { source, .. } => {
            if let AnalyticsSource::Subgraph { filter } = source {
                collect_variable_owners(filter, current, map);
            }
        }
        // Hybrid retrieval doesn't introduce variable bindings
        // through pattern matching — its result is a ranked
        // node list, projected by the planner. The temporal
        // rewriter has nothing to attribute here.
        QueryOp::HybridSearch { .. } => {}
    }
}

/// Walk expression-level variable bindings (subqueries, case operands,
/// function arguments). Called from `collect_variable_owners` whenever
/// a filter/having/case can host an `Expr::Subquery` that declares new
/// pattern variables.
fn collect_expr_vars(
    expr: &Expr,
    current: &OntologyIR,
    map: &mut HashMap<VariableName, OwnerRef>,
) {
    match expr {
        Expr::Comparison { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::StringOp { left, right, .. } => {
            collect_expr_vars(left, current, map);
            collect_expr_vars(right, current, map);
        }
        Expr::Not { inner } => collect_expr_vars(inner, current, map),
        Expr::In { expr, .. } | Expr::IsNull { expr, .. } => {
            collect_expr_vars(expr, current, map);
        }
        Expr::FunctionCall { args, .. } => {
            for a in args {
                collect_expr_vars(a, current, map);
            }
        }
        Expr::Exists { pattern } => collect_pattern_vars(pattern, current, map),
        Expr::Case {
            operand,
            when_clauses,
            else_result,
        } => {
            if let Some(op) = operand {
                collect_expr_vars(op, current, map);
            }
            for w in when_clauses {
                collect_expr_vars(&w.condition, current, map);
                collect_expr_vars(&w.result, current, map);
            }
            if let Some(e) = else_result {
                collect_expr_vars(e, current, map);
            }
        }
        Expr::Subquery { query, .. } => {
            collect_variable_owners(&query.operation, current, map);
        }
        // Leaves carry no variable bindings.
        Expr::Literal { .. } | Expr::Param { .. } | Expr::Property { .. } => {}
    }
}

fn collect_pattern_vars(
    pattern: &GraphPattern,
    current: &OntologyIR,
    map: &mut HashMap<VariableName, OwnerRef>,
) {
    match pattern {
        GraphPattern::Node {
            variable, label, ..
        } => bind_node_var(variable, label.as_ref(), current, map),
        GraphPattern::Relationship {
            variable: Some(var),
            label: Some(l),
            ..
        } => bind_edge_var(var, l, current, map),
        GraphPattern::Relationship { .. } => {}
        GraphPattern::Path { elements } => {
            for elem in elements {
                match elem {
                    PathElement::Node {
                        variable, label, ..
                    } => bind_node_var(variable, label.as_ref(), current, map),
                    PathElement::Edge {
                        variable: Some(var),
                        label: Some(l),
                        ..
                    } => bind_edge_var(var, l, current, map),
                    PathElement::Edge { .. } => {}
                }
            }
        }
    }
}

/// Resolve a variable's owner type from a node label (when present) and
/// record it. Unresolvable labels (unknown in current) are a silent
/// no-op — missing-label errors are the compiler's job, not the
/// rewriter's.
fn bind_node_var(
    variable: &VariableName,
    label: Option<&GraphLabel>,
    current: &OntologyIR,
    map: &mut HashMap<VariableName, OwnerRef>,
) {
    if let Some(l) = label
        && let Some(nt) = current.node_by_label(l.as_str())
    {
        map.insert(variable.clone(), OwnerRef::Node(nt.id.clone()));
    }
}

/// Mirror of [`bind_node_var`] for edge variables.
fn bind_edge_var(
    variable: &VariableName,
    label: &GraphLabel,
    current: &OntologyIR,
    map: &mut HashMap<VariableName, OwnerRef>,
) {
    if let Some(et) = edge_by_label(current, label.as_str()) {
        map.insert(variable.clone(), OwnerRef::Edge(et.id.clone()));
    }
}

/// Linear-scan fallback — `OntologyIR` indexes node labels but not edge
/// labels, so a lookup here walks the edge type vec. Acceptable because
/// temporal rewriting is off the hot path and most ontologies have <100
/// edge types.
fn edge_by_label<'a>(ontology: &'a OntologyIR, label: &str) -> Option<&'a EdgeTypeDef> {
    ontology
        .edge_types()
        .iter()
        .find(|e| e.label.as_str() == label)
}

/// Walk every label and property surface in a `QueryOp` tree and
/// substitute any rename hits. Additive on the [Expr / Pattern /
/// Mutate / Analytics] coverage — a new label- or property-carrying
/// variant needs an arm here or the rewriter will silently miss its
/// renames.
///
/// Property renames require the variable → owner type map (bundled in
/// `ctx.var_types`) so an expression like `c.name` resolves `c`'s type
/// and consults the right property rename map. Property filters inside
/// a pattern read the owner directly from the pattern's own label,
/// bypassing the var map.
fn apply_renames(op: &mut QueryOp, ctx: &RenameCtx<'_>) -> OxResult<()> {
    match op {
        QueryOp::Match {
            patterns, filter, ..
        } => {
            for p in patterns {
                rename_pattern(p, ctx);
            }
            if let Some(expr) = filter {
                rename_expr(expr, ctx)?;
            }
        }
        QueryOp::PathFind {
            start,
            end,
            edge_types,
            ..
        } => {
            rename_node_ref(start, ctx);
            rename_node_ref(end, ctx);
            for l in edge_types.iter_mut() {
                swap_if_renamed(l, ctx.edge_labels);
            }
        }
        QueryOp::Aggregate {
            source, having, ..
        } => {
            apply_renames(&mut source.operation, ctx)?;
            if let Some(expr) = having {
                rename_expr(expr, ctx)?;
            }
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                apply_renames(&mut q.operation, ctx)?;
            }
        }
        QueryOp::Chain { steps } => {
            for s in steps {
                apply_renames(&mut s.operation, ctx)?;
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            apply_renames(&mut inner.operation, ctx)?;
        }
        QueryOp::Mutate {
            context,
            operations,
            ..
        } => {
            if let Some(inner) = context {
                apply_renames(inner, ctx)?;
            }
            for mut_op in operations {
                rename_mutate_op(mut_op, ctx)?;
            }
        }
        QueryOp::Analytics { source, .. } => match source {
            AnalyticsSource::Labels {
                labels: src_labels, ..
            } => {
                for l in src_labels.iter_mut() {
                    swap_if_renamed(l, ctx.node_labels);
                }
            }
            AnalyticsSource::Subgraph { filter } => {
                apply_renames(filter, ctx)?;
            }
            // WholeGraph carries no label or property surface — nothing
            // to rewrite, but an arm is required so a new variant
            // doesn't slip past this walker without review.
            AnalyticsSource::WholeGraph => {}
        },
        QueryOp::HybridSearch { request } => {
            // Rename labels embedded in the optional graph
            // constraint sub-pattern. The vector / fulltext
            // queries are opaque text — temporal rewrites don't
            // touch them.
            if let Some(constraint) = &mut request.graph_constraints {
                for node in &mut constraint.nodes {
                    if let Some(lbl) = &mut node.label {
                        swap_if_renamed(lbl, ctx.node_labels);
                    }
                }
                for edge in &mut constraint.edges {
                    if let Some(lbl) = &mut edge.label {
                        swap_if_renamed(lbl, ctx.edge_labels);
                    }
                }
            }
        }
    }
    Ok(())
}

fn rename_pattern(pattern: &mut GraphPattern, ctx: &RenameCtx<'_>) {
    match pattern {
        GraphPattern::Node {
            label,
            property_filters,
            ..
        } => {
            // Resolve the pattern's type via the authored (current)
            // label BEFORE applying the label rename, then swap the
            // label. Property filters are owned by the pattern's own
            // type regardless of whether the variable is bound, so we
            // don't go through `var_types`.
            if let Some(l) = label.as_ref()
                && let Some(nt) = ctx.current.node_by_label(l.as_str())
            {
                rename_property_filters_node(property_filters, &nt.id, ctx.node_props);
            }
            if let Some(l) = label.as_mut() {
                swap_if_renamed(l, ctx.node_labels);
            }
        }
        GraphPattern::Relationship {
            label,
            property_filters,
            ..
        } => {
            if let Some(l) = label.as_ref()
                && let Some(et) = edge_by_label(ctx.current, l.as_str())
            {
                rename_property_filters_edge(property_filters, &et.id, ctx.edge_props);
            }
            if let Some(l) = label.as_mut() {
                swap_if_renamed(l, ctx.edge_labels);
            }
        }
        GraphPattern::Path { elements } => {
            for elem in elements {
                match elem {
                    PathElement::Node { label, .. } => {
                        if let Some(l) = label.as_mut() {
                            swap_if_renamed(l, ctx.node_labels);
                        }
                    }
                    PathElement::Edge { label, .. } => {
                        if let Some(l) = label.as_mut() {
                            swap_if_renamed(l, ctx.edge_labels);
                        }
                    }
                }
            }
        }
    }
}

fn rename_property_filters_node(
    filters: &mut [PropertyFilter],
    owner: &NodeTypeId,
    node_props: &NodePropRenames,
) {
    for f in filters {
        if let Some(snap) = node_props.get(&(owner.clone(), f.property.clone())) {
            f.property = snap.clone();
        }
    }
}

/// Rename a PathFind start/end `NodeRef` in place — its inline property
/// filters reference the ref's own type (resolved via the authored
/// label), and the label itself then swaps through the rename map.
/// Symmetric to the Match-pattern `GraphPattern::Node` path.
fn rename_node_ref(node_ref: &mut NodeRef, ctx: &RenameCtx<'_>) {
    if let Some(l) = node_ref.label.as_ref()
        && let Some(nt) = ctx.current.node_by_label(l.as_str())
    {
        rename_property_filters_node(&mut node_ref.property_filters, &nt.id, ctx.node_props);
    }
    if let Some(l) = node_ref.label.as_mut() {
        swap_if_renamed(l, ctx.node_labels);
    }
}

fn rename_property_filters_edge(
    filters: &mut [PropertyFilter],
    owner: &EdgeTypeId,
    edge_props: &EdgePropRenames,
) {
    for f in filters {
        if let Some(snap) = edge_props.get(&(owner.clone(), f.property.clone())) {
            f.property = snap.clone();
        }
    }
}

/// Apply node-property renames to each assignment in a list of
/// `{property: value}` pairs that all belong to a single node type.
/// Used by `CreateNode.properties` and `MergeNode.{match_properties,
/// on_create, on_match}` — all three MergeNode lists share the same
/// owner type, so one call per list is sufficient.
fn rename_property_assignments_node(
    assignments: &mut [PropertyAssignment],
    owner: &NodeTypeId,
    node_props: &NodePropRenames,
    ctx: &RenameCtx<'_>,
) -> OxResult<()> {
    for a in assignments {
        if let Some(snap) = node_props.get(&(owner.clone(), a.property.clone())) {
            a.property = snap.clone();
        }
        rename_expr(&mut a.value, ctx)?;
    }
    Ok(())
}

/// Mirror of [`rename_property_assignments_node`] for edge types.
fn rename_property_assignments_edge(
    assignments: &mut [PropertyAssignment],
    owner: &EdgeTypeId,
    edge_props: &EdgePropRenames,
    ctx: &RenameCtx<'_>,
) -> OxResult<()> {
    for a in assignments {
        if let Some(snap) = edge_props.get(&(owner.clone(), a.property.clone())) {
            a.property = snap.clone();
        }
        rename_expr(&mut a.value, ctx)?;
    }
    Ok(())
}

fn rename_expr(expr: &mut Expr, ctx: &RenameCtx<'_>) -> OxResult<()> {
    match expr {
        Expr::Comparison { left, right, .. }
        | Expr::StringOp { left, right, .. }
        | Expr::Logical { left, right, .. } => {
            rename_expr(left, ctx)?;
            rename_expr(right, ctx)?;
        }
        Expr::Not { inner } => rename_expr(inner, ctx)?,
        Expr::In { expr, .. } | Expr::IsNull { expr, .. } => rename_expr(expr, ctx)?,
        Expr::FunctionCall { args, .. } => {
            for a in args {
                rename_expr(a, ctx)?;
            }
        }
        Expr::Exists { pattern } => rename_pattern(pattern, ctx),
        Expr::Case {
            operand,
            when_clauses,
            else_result,
        } => {
            if let Some(op) = operand {
                rename_expr(op, ctx)?;
            }
            for w in when_clauses {
                rename_expr(&mut w.condition, ctx)?;
                rename_expr(&mut w.result, ctx)?;
            }
            if let Some(e) = else_result {
                rename_expr(e, ctx)?;
            }
        }
        Expr::Subquery { query, .. } => {
            apply_renames(&mut query.operation, ctx)?;
        }
        // The central property-rename site: `c.name` where `c` was
        // bound to some node/edge type in an outer pattern.
        Expr::Property { variable, field } => {
            if let Some(field_key) = field {
                ctx.rename_var_property(variable, field_key);
            }
        }
        // Leaves with no label or property surface.
        Expr::Literal { .. } | Expr::Param { .. } => {}
    }
    Ok(())
}

fn rename_mutate_op(op: &mut MutateOp, ctx: &RenameCtx<'_>) -> OxResult<()> {
    match op {
        MutateOp::CreateNode {
            label, properties, ..
        } => {
            rename_node_label_and_assignments(label, properties, ctx)?;
        }
        MutateOp::MergeNode {
            label,
            match_properties,
            on_create,
            on_match,
            ..
        } => {
            // Resolve type once via current, then apply to all three
            // assignment lists (they share the same owner type_id).
            if let Some(nt) = ctx.current.node_by_label(label.as_str()) {
                let owner = nt.id.clone();
                rename_property_assignments_node(match_properties, &owner, ctx.node_props, ctx)?;
                rename_property_assignments_node(on_create, &owner, ctx.node_props, ctx)?;
                rename_property_assignments_node(on_match, &owner, ctx.node_props, ctx)?;
            }
            swap_if_renamed(label, ctx.node_labels);
        }
        MutateOp::CreateEdge {
            label, properties, ..
        } => {
            rename_edge_label_and_assignments(label, properties, ctx)?;
        }
        MutateOp::MergeEdge {
            label,
            match_properties,
            on_create,
            on_match,
            ..
        } => {
            if let Some(et) = edge_by_label(ctx.current, label.as_str()) {
                let owner = et.id.clone();
                rename_property_assignments_edge(match_properties, &owner, ctx.edge_props, ctx)?;
                rename_property_assignments_edge(on_create, &owner, ctx.edge_props, ctx)?;
                rename_property_assignments_edge(on_match, &owner, ctx.edge_props, ctx)?;
            }
            swap_if_renamed(label, ctx.edge_labels);
        }
        MutateOp::RemoveLabel { label, .. } => {
            swap_if_renamed(label, ctx.node_labels);
        }
        MutateOp::SetProperty {
            variable,
            property,
            value,
        } => {
            ctx.rename_var_property(variable, property);
            rename_expr(value, ctx)?;
        }
        MutateOp::RemoveProperty { variable, property } => {
            ctx.rename_var_property(variable, property);
        }
        MutateOp::Delete { .. } => {}
    }
    Ok(())
}

/// Resolve a CreateNode's type once via the authored label, rename all
/// property assignments, then rename the label itself.
fn rename_node_label_and_assignments(
    label: &mut GraphLabel,
    properties: &mut [PropertyAssignment],
    ctx: &RenameCtx<'_>,
) -> OxResult<()> {
    if let Some(nt) = ctx.current.node_by_label(label.as_str()) {
        let owner = nt.id.clone();
        rename_property_assignments_node(properties, &owner, ctx.node_props, ctx)?;
    }
    swap_if_renamed(label, ctx.node_labels);
    Ok(())
}

/// Mirror of [`rename_node_label_and_assignments`] for CreateEdge.
fn rename_edge_label_and_assignments(
    label: &mut GraphLabel,
    properties: &mut [PropertyAssignment],
    ctx: &RenameCtx<'_>,
) -> OxResult<()> {
    if let Some(et) = edge_by_label(ctx.current, label.as_str()) {
        let owner = et.id.clone();
        rename_property_assignments_edge(properties, &owner, ctx.edge_props, ctx)?;
    }
    swap_if_renamed(label, ctx.edge_labels);
    Ok(())
}

fn swap_if_renamed(label: &mut GraphLabel, map: &LabelRenames) {
    if let Some(target) = map.get(label) {
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
    use ox_ontology::ir::{NodeTypeDef, OntologyVersion};
    use ox_query_ir::query::{
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

    // ---------------------------------------------------------------
    // Property rename tests — snapshot has property P1 on NodeType NT1
    // under the name `email`, current renamed to `primary_email`. A
    // query written against current references `c.primary_email`
    // and must be rewritten to `c.email`.
    // ---------------------------------------------------------------

    use ox_ontology::ir::PropertyDef;
    use ox_core::property_key::PropertyKey;
    use ox_query_ir::query::{ComparisonOp, Expr, PropertyFilter};
    use ox_core::types::{PropertyType, PropertyValue};

    fn pk(s: &'static str) -> PropertyKey {
        PropertyKey::new(s).expect("property key")
    }

    fn prop(id: &'static str, name: &'static str) -> PropertyDef {
        // Struct-update from `Default::default()` so new fields on
        // `PropertyDef` (e.g. semantic links) do not break this
        // helper — the defaults are the empty shape a fixture
        // wants anyway.
        PropertyDef {
            id: id.into(),
            name: pk(name),
            property_type: PropertyType::String,
            nullable: false,
            ..Default::default()
        }
    }

    fn snapshot_with_node_property(
        number: u32,
        valid_from: Option<DateTime<Utc>>,
        valid_to: Option<DateTime<Utc>>,
        node_id: &'static str,
        label: &'static str,
        prop_id: &'static str,
        prop_name: &'static str,
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
                properties: vec![prop(prop_id, prop_name)],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
    }

    #[test]
    fn property_rename_in_expr_property() {
        // Snapshot: nt1.p1 was `email`. Current: nt1.p1 is `primary_email`.
        // Query WHERE c.primary_email = "x" → WHERE c.email = "x".
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: vn("c"),
                        field: Some(pk("primary_email")),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("x".into()),
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: Some(ts(2026, 3, 15)),
        };

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Match { filter, .. } = &out.operation else {
            panic!("expected Match");
        };
        let Some(Expr::Comparison { left, .. }) = filter else {
            panic!("expected Comparison filter");
        };
        let Expr::Property { field, .. } = left.as_ref() else {
            panic!("expected Property expr");
        };
        assert_eq!(
            field.as_ref().map(|f| f.as_str()),
            Some("email"),
            "property must be rewritten to the snapshot-era name"
        );
    }

    #[test]
    fn property_rename_in_pattern_filter() {
        // Inline pattern filter: (c:Customer {primary_email: "x"})
        // must rewrite to {email: "x"}.
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: vec![PropertyFilter {
                        property: pk("primary_email"),
                        value: Expr::Literal {
                            value: PropertyValue::String("x".into()),
                        },
                    }],
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

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Match { patterns, .. } = &out.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node {
            property_filters, ..
        } = &patterns[0]
        else {
            panic!("expected Node");
        };
        assert_eq!(
            property_filters[0].property.as_str(),
            "email",
            "inline pattern filter must rewrite to the snapshot-era property name"
        );
    }

    #[test]
    fn property_rename_no_op_when_unchanged() {
        // Same property id + same name across versions → no rewrite.
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "email",
        );

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: vec![PropertyFilter {
                        property: pk("email"),
                        value: Expr::Literal {
                            value: PropertyValue::String("x".into()),
                        },
                    }],
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

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Match { patterns, .. } = &out.operation else {
            panic!("expected Match");
        };
        let GraphPattern::Node {
            property_filters, ..
        } = &patterns[0]
        else {
            panic!("expected Node");
        };
        assert_eq!(property_filters[0].property.as_str(), "email");
    }

    #[test]
    fn property_rename_leaves_unbound_variable_alone() {
        // A query where the pattern has no label → variable is
        // unbound in the owner map → property reference stays as-is
        // rather than the rewriter silently guessing a type.
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: None, // no label — variable is unbound
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: vn("c"),
                        field: Some(pk("primary_email")),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("x".into()),
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: Some(ts(2026, 3, 15)),
        };

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Match { filter, .. } = &out.operation else {
            panic!("expected Match");
        };
        let Some(Expr::Comparison { left, .. }) = filter else {
            panic!("expected filter");
        };
        let Expr::Property { field, .. } = left.as_ref() else {
            panic!("expected Property");
        };
        assert_eq!(
            field.as_ref().map(|f| f.as_str()),
            Some("primary_email"),
            "variable unbound (no pattern label) → property left as-is"
        );
    }

    // ---------------------------------------------------------------
    // MutateOp property-assignment rename tests — the second gap that
    // the refactor closed: CreateNode/CreateEdge `.properties` and
    // MergeNode/MergeEdge `.match_properties` / `.on_create` /
    // `.on_match` lists were previously unwalked even when the rename
    // map was populated.
    // ---------------------------------------------------------------

    use ox_query_ir::query::{MutateOp, PropertyAssignment};

    fn mutate_with_one_op(op: MutateOp, as_of: Option<DateTime<Utc>>) -> QueryIR {
        QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Mutate {
                context: None,
                operations: vec![op],
                returning: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of,
        }
    }

    #[test]
    fn property_rename_in_create_node_assignments() {
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        let query = mutate_with_one_op(
            MutateOp::CreateNode {
                variable: vn("c"),
                label: gl("Customer"),
                properties: vec![PropertyAssignment {
                    property: pk("primary_email"),
                    value: Expr::Literal {
                        value: PropertyValue::String("x@y".into()),
                    },
                }],
            },
            Some(ts(2026, 3, 15)),
        );

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Mutate { operations, .. } = &out.operation else {
            panic!("expected Mutate");
        };
        let MutateOp::CreateNode { properties, .. } = &operations[0] else {
            panic!("expected CreateNode");
        };
        assert_eq!(
            properties[0].property.as_str(),
            "email",
            "CreateNode property assignments must be rewritten to snapshot-era names"
        );
    }

    #[test]
    fn property_rename_in_merge_node_all_three_lists() {
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        let mk_assignment = || PropertyAssignment {
            property: pk("primary_email"),
            value: Expr::Literal {
                value: PropertyValue::String("x@y".into()),
            },
        };

        let query = mutate_with_one_op(
            MutateOp::MergeNode {
                variable: vn("c"),
                label: gl("Customer"),
                match_properties: vec![mk_assignment()],
                on_create: vec![mk_assignment()],
                on_match: vec![mk_assignment()],
            },
            Some(ts(2026, 3, 15)),
        );

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Mutate { operations, .. } = &out.operation else {
            panic!("expected Mutate");
        };
        let MutateOp::MergeNode {
            match_properties,
            on_create,
            on_match,
            ..
        } = &operations[0]
        else {
            panic!("expected MergeNode");
        };
        for (list, name) in [
            (match_properties, "match_properties"),
            (on_create, "on_create"),
            (on_match, "on_match"),
        ] {
            assert_eq!(
                list[0].property.as_str(),
                "email",
                "MergeNode.{name} must rewrite property names"
            );
        }
    }

    // ---------------------------------------------------------------
    // Expr::Subquery variable-binding collection test — previously the
    // walker only recursed into top-level QueryOp::CallSubquery but
    // skipped filter-level Expr::Subquery, so inner subquery patterns'
    // variables never made it into var_types. A property reference
    // inside the subquery's own filter was silently un-renamed.
    // ---------------------------------------------------------------

    #[test]
    fn property_rename_in_pathfind_node_refs() {
        // `NodeRef` (PathFind start/end) has its own `property_filters`
        // field. The inline filter must be rewritten using the ref's
        // own label to resolve the owner type, mirroring the Match
        // pattern path.
        use ox_query_ir::query::{NodeRef, PathAlgorithm};
        use ox_core::types::Direction;

        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::PathFind {
                start: NodeRef {
                    variable: vn("a"),
                    label: Some(gl("Customer")),
                    property_filters: vec![PropertyFilter {
                        property: pk("primary_email"),
                        value: Expr::Literal {
                            value: PropertyValue::String("x@y".into()),
                        },
                    }],
                },
                end: NodeRef {
                    variable: vn("b"),
                    label: Some(gl("Customer")),
                    property_filters: vec![],
                },
                edge_types: vec![],
                direction: Direction::Outgoing,
                max_depth: None,
                algorithm: PathAlgorithm::ShortestPath,
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: Some(ts(2026, 3, 15)),
        };

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::PathFind { start, .. } = &out.operation else {
            panic!("expected PathFind");
        };
        assert_eq!(
            start.property_filters[0].property.as_str(),
            "email",
            "PathFind.start.property_filters must rewrite to snapshot-era names"
        );
    }

    #[test]
    fn property_rename_reaches_into_expr_subquery() {
        let snap = snapshot_with_node_property(
            1,
            Some(ts(2026, 1, 1)),
            Some(ts(2026, 6, 1)),
            "nt1",
            "Customer",
            "p1",
            "email",
        );
        let current = snapshot_with_node_property(
            2,
            Some(ts(2026, 6, 1)),
            None,
            "nt1",
            "Customer",
            "p1",
            "primary_email",
        );

        // Outer: MATCH (outer_anon) WHERE SUBQUERY { MATCH (c:Customer)
        //   WHERE c.primary_email = "x" RETURN 1 }
        //
        // `c` is declared inside the subquery; its property reference
        // against `c.primary_email` must still be rewritten to `email`.
        let inner_query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: vn("c"),
                        field: Some(pk("primary_email")),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("x".into()),
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };

        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                // Outer pattern carries no label (a wildcard) but
                // that's fine — only the subquery's `c` needs to bind.
                patterns: vec![GraphPattern::Node {
                    variable: vn("outer"),
                    label: None,
                    property_filters: vec![],
                }],
                filter: Some(Expr::Subquery {
                    query: Box::new(inner_query),
                    import_variables: vec![],
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: Some(ts(2026, 3, 15)),
        };

        let out = rewrite_temporal_with_renames(query, &snap, &current).expect("rewrite");
        let QueryOp::Match { filter, .. } = &out.operation else {
            panic!("expected outer Match");
        };
        let Some(Expr::Subquery { query: inner, .. }) = filter else {
            panic!("expected Subquery filter");
        };
        let QueryOp::Match {
            filter: inner_filter,
            ..
        } = &inner.operation
        else {
            panic!("expected inner Match");
        };
        let Some(Expr::Comparison { left, .. }) = inner_filter else {
            panic!("expected inner Comparison");
        };
        let Expr::Property { field, .. } = left.as_ref() else {
            panic!("expected Property");
        };
        assert_eq!(
            field.as_ref().map(|f| f.as_str()),
            Some("email"),
            "subquery-local variable's property must rewrite"
        );
    }
}

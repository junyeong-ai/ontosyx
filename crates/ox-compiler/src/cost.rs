use std::collections::{HashMap, HashSet};

use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::{ChainStep, GraphPattern, QueryIR, QueryOp};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Query cost estimation — analyses QueryIR before compilation
//
// DB-agnostic: works on QueryIR + OntologyIR, independent of target backend.
// Detects expensive patterns (Cartesian products, deep variable-length
// traversals, multiple OPTIONAL MATCHes) and assigns a risk level.
// ---------------------------------------------------------------------------

// Threshold constants — derived from Neo4j query planner heuristics.
// Variable-length paths with depth > 3 trigger intermediate expansion;
// depth > 6 risks combinatorial blowup on dense graphs.
const VAR_LENGTH_MEDIUM_THRESHOLD: usize = 3;
const VAR_LENGTH_HIGH_THRESHOLD: usize = 6;
const OPTIONAL_MATCH_MEDIUM_THRESHOLD: usize = 2;
const PATTERN_COUNT_MEDIUM_THRESHOLD: usize = 6;
/// Sentinel depth for unbounded variable-length paths (Cypher `*`).
/// Treated as HIGH risk since the DB will traverse the entire graph.
const UNBOUNDED_VAR_LENGTH_DEPTH: usize = 100;

/// Estimated cost characteristics of a QueryIR.
///
/// Two axes:
/// - **structural** (`pattern_count`, `has_cartesian`, `max_var_length_depth`,
///   `optional_match_count`, `uses_indexed_filter`, `has_high_fanout`)
///   — what the IR's shape implies regardless of the data the
///   ontology has cardinality stats for.
/// - **quantitative** (`estimated_pattern_expansions`, `estimated_rows`,
///   `estimated_wallclock_ms`) — extrapolations from per-edge
///   fan-out hints + per-label cardinality stats. `None` on the
///   row / wallclock axes means "no usable cardinality stats" —
///   the router's `CostBudget.max_rows` / `max_wallclock_ms`
///   gates fall back to risk-level rejection in that case.
#[derive(Debug, Clone, Serialize)]
pub struct QueryCost {
    /// Total number of graph patterns across all Match operations
    pub pattern_count: usize,
    /// True if disconnected patterns exist (Cartesian product risk)
    pub has_cartesian: bool,
    /// Deepest variable-length hop (0 if none; fixed-length paths excluded)
    pub max_var_length_depth: usize,
    /// Number of OPTIONAL MATCH operations
    pub optional_match_count: usize,
    /// True if filter properties are indexed (from OntologyIR)
    pub uses_indexed_filter: bool,
    /// True if many-to-many relationships dominate (high fan-out risk)
    pub has_high_fanout: bool,
    /// Overall risk classification
    pub risk_level: RiskLevel,
    /// Human-readable warnings for High/Medium risk queries
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Number of pattern expansions implied by per-relationship
    /// fan-out estimates. Calculated as the product of per-edge
    /// average fan-out across the Match's relationships, capped at
    /// `EXPANSION_CAP` to avoid overflow on `var_length` paths
    /// (`fanout^depth` blows up fast). `0` when the IR has no
    /// relationship patterns (pure node-label scan).
    pub estimated_pattern_expansions: u64,
    /// Estimated row-count of the IR's result. `Some(n)` when every
    /// touched label has a `cardinality_estimate` populated on its
    /// `ObjectMappingDef.cache_hint` chain; `None` otherwise. The
    /// router consults `CostBudget.max_rows` only when this is
    /// `Some`.
    pub estimated_rows: Option<u64>,
    /// Estimated wallclock in milliseconds. Calibration constant
    /// `WALLCLOCK_PER_EXPANSION_MS` × `estimated_pattern_expansions`.
    /// `None` when the IR has no relationship patterns or when
    /// expansions exceed the cap (the cap signals "explosive
    /// shape" which the structural risk axis already captures via
    /// `RiskLevel::High`).
    pub estimated_wallclock_ms: Option<u64>,
}

/// Cap on `estimated_pattern_expansions` — `var_length` paths over
/// even modest fan-out (e.g., 10^6 over depth 6) saturate u64 fast.
/// Past this cap the structural-risk axis (`RiskLevel::High` from
/// `max_var_length_depth > VAR_LENGTH_HIGH_THRESHOLD`) carries the
/// rejection signal so the quantitative side stops contributing.
const EXPANSION_CAP: u64 = 1_000_000_000;

/// Calibration constant used when extrapolating wallclock from
/// expansion count. Order-of-magnitude estimate from production
/// Neo4j 5.x measurements (1 µs per pattern expansion under cold
/// cache + indexed filter; the constant exists as a single
/// adjustable knob, not as a precise predictor).
const WALLCLOCK_PER_EXPANSION_MS: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Analyse a QueryIR and return cost characteristics.
///
/// Uses `ontology` to check index availability and relationship cardinality.
/// Call this between `translate_query` and `compile_query`.
pub fn estimate_cost(query: &QueryIR, ontology: &OntologyIR) -> QueryCost {
    let mut ctx = CostCtx::default();
    walk_op(&query.operation, &mut ctx);

    // Index availability: check if any filter label.property is indexed
    let uses_indexed_filter = check_indexed_filters(&ctx.filter_labels, ontology);

    // High fan-out: check if relationship labels are many-to-many
    let has_high_fanout = check_high_fanout(&ctx.relationship_labels, ontology);

    let mut warnings = Vec::new();

    if ctx.has_cartesian {
        warnings
            .push("Disconnected patterns detected — this may produce a Cartesian product".into());
    }
    if ctx.max_var_length_depth > VAR_LENGTH_MEDIUM_THRESHOLD {
        let depth_desc = if ctx.max_var_length_depth >= UNBOUNDED_VAR_LENGTH_DEPTH {
            "unbounded".to_string()
        } else {
            ctx.max_var_length_depth.to_string()
        };
        warnings.push(format!(
            "Variable-length traversal depth {depth_desc} may be slow on large graphs",
        ));
    }
    if ctx.optional_match_count > OPTIONAL_MATCH_MEDIUM_THRESHOLD {
        warnings.push(format!(
            "{} OPTIONAL MATCH clauses — consider splitting the query",
            ctx.optional_match_count,
        ));
    }
    if has_high_fanout && !uses_indexed_filter {
        warnings
            .push("Many-to-many relationships without indexed filters — high fan-out risk".into());
    }

    let missing_partition_filters = check_partition_filters(&ctx, ontology);
    for entry in &missing_partition_filters {
        warnings.push(format!(
            "Label '{}' is mapped to a partition-aware source ({}) but the query carries no \
             literal filter on any of its partition columns ({}). The source will reject the \
             scan or charge for a full-table read.",
            entry.label,
            entry.relation,
            entry.partition_columns.join(", "),
        ));
    }

    let estimated_pattern_expansions =
        estimate_pattern_expansions(&ctx, ontology);
    // Wallclock estimate is suppressed at the cap because the
    // structural axis already classifies these queries as
    // RiskLevel::High; emitting a saturated number here would
    // be misleading.
    let estimated_wallclock_ms = if estimated_pattern_expansions == 0
        || estimated_pattern_expansions >= EXPANSION_CAP
    {
        None
    } else {
        Some(((estimated_pattern_expansions as f64) * WALLCLOCK_PER_EXPANSION_MS).ceil() as u64)
    };
    // estimated_rows is left None until the cost path consumes
    // ColumnProfile.row_count via the materialised side. Today the
    // IR has no per-label cardinality stats; the structural risk
    // axis carries rejection until that wiring lands.
    let estimated_rows: Option<u64> = None;

    let risk_level = classify_risk(
        &ctx,
        uses_indexed_filter,
        has_high_fanout,
        !missing_partition_filters.is_empty(),
    );

    QueryCost {
        pattern_count: ctx.pattern_count,
        has_cartesian: ctx.has_cartesian,
        max_var_length_depth: ctx.max_var_length_depth,
        optional_match_count: ctx.optional_match_count,
        uses_indexed_filter,
        has_high_fanout,
        risk_level,
        warnings,
        estimated_pattern_expansions,
        estimated_rows,
        estimated_wallclock_ms,
    }
}

/// Default per-edge fan-out when the IR has only `Cardinality`
/// declarative info. Production systems would source this from
/// `ColumnProfile.row_count / FK uniqueness ratio`; until the cost
/// path consumes that, the heuristic is bounded by `Cardinality`.
fn fanout_for_cardinality(card: ox_ontology::ir::Cardinality) -> u64 {
    use ox_ontology::ir::Cardinality;
    match card {
        Cardinality::OneToOne | Cardinality::ManyToOne => 1,
        // 10 is order-of-magnitude — most production OneToMany
        // relationships sit between 2 and 100. The cap below
        // saturates well before this matters for `var_length`
        // explosion detection.
        Cardinality::OneToMany | Cardinality::ManyToMany => 10,
    }
}

/// Multiply per-relationship fan-out into a single bound on the
/// number of rows the planner would materialise. `var_length`
/// hops contribute `fanout^depth`; saturated at `EXPANSION_CAP`.
fn estimate_pattern_expansions(ctx: &CostCtx, ontology: &OntologyIR) -> u64 {
    if ctx.relationship_labels.is_empty() {
        return 0;
    }
    let mut total: u64 = 1;
    for label in &ctx.relationship_labels {
        let fanout = ontology
            .edge_types()
            .iter()
            .find(|e| e.label == *label)
            .map(|e| fanout_for_cardinality(e.cardinality))
            .unwrap_or(10);
        total = total.saturating_mul(fanout);
        if total >= EXPANSION_CAP {
            return EXPANSION_CAP;
        }
    }
    if ctx.max_var_length_depth > 0 {
        // Variable-length expansion: every depth-step multiplies
        // by the average per-edge fan-out across the touched
        // relationships. Use the max fanout observed (worst-case
        // under uniform var-length over heterogeneous edges).
        let max_fanout = ctx
            .relationship_labels
            .iter()
            .map(|label| {
                ontology
                    .edge_types()
                    .iter()
                    .find(|e| e.label == *label)
                    .map(|e| fanout_for_cardinality(e.cardinality))
                    .unwrap_or(10)
            })
            .max()
            .unwrap_or(10);
        for _ in 0..ctx.max_var_length_depth {
            total = total.saturating_mul(max_fanout);
            if total >= EXPANSION_CAP {
                return EXPANSION_CAP;
            }
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CostCtx {
    pattern_count: usize,
    has_cartesian: bool,
    max_var_length_depth: usize,
    optional_match_count: usize,
    /// (label, property) pairs used in filters — for index check
    filter_labels: Vec<(String, String)>,
    /// Relationship labels referenced — for cardinality check
    relationship_labels: Vec<String>,
    /// Every node label the query mentions, including labels that
    /// carry no property filter — partition-filter checks need to
    /// see "label appears at all" not just "label appears in a
    /// filter".
    referenced_labels: HashSet<String>,
}

fn classify_risk(
    ctx: &CostCtx,
    indexed: bool,
    high_fanout: bool,
    missing_partition_filter: bool,
) -> RiskLevel {
    if ctx.has_cartesian
        || ctx.max_var_length_depth > VAR_LENGTH_HIGH_THRESHOLD
        || missing_partition_filter
    {
        return RiskLevel::High;
    }
    if ctx.max_var_length_depth > VAR_LENGTH_MEDIUM_THRESHOLD
        || ctx.optional_match_count > OPTIONAL_MATCH_MEDIUM_THRESHOLD
        || ctx.pattern_count > PATTERN_COUNT_MEDIUM_THRESHOLD
        || (high_fanout && !indexed && ctx.max_var_length_depth > 1)
    {
        return RiskLevel::Medium;
    }
    RiskLevel::Low
}

/// Pair the labels referenced by the query with their object mapping
/// declarations, then flag any whose source declares
/// `partition_columns` but the query supplies no literal filter on
/// one of them. Returns one entry per offending label.
fn check_partition_filters(
    ctx: &CostCtx,
    ontology: &OntologyIR,
) -> Vec<MissingPartitionFilter> {
    let mut filtered_columns: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut all_labels: HashSet<&str> = HashSet::new();
    for label in &ctx.referenced_labels {
        all_labels.insert(label.as_str());
    }
    for (label, prop) in &ctx.filter_labels {
        all_labels.insert(label.as_str());
        filtered_columns
            .entry(label.as_str())
            .or_default()
            .insert(prop.as_str());
    }

    let mut out = Vec::new();
    for label in all_labels {
        let Some(node) = ontology
            .node_types()
            .iter()
            .find(|n| n.label.as_str() == label)
        else {
            continue;
        };
        for mapping in ontology
            .object_mappings()
            .iter()
            .filter(|m| m.node_type_id == node.id)
        {
            if mapping.partition_columns.is_empty() {
                continue;
            }
            let partition_names: HashSet<&str> = mapping
                .partition_columns
                .iter()
                .map(|c| c.column.as_str())
                .collect();
            let filtered = filtered_columns.get(label).cloned().unwrap_or_default();
            if partition_names.is_disjoint(&filtered) {
                out.push(MissingPartitionFilter {
                    label: label.to_string(),
                    relation: mapping.relation.clone(),
                    partition_columns: mapping
                        .partition_columns
                        .iter()
                        .map(|c| c.column.clone())
                        .collect(),
                });
            }
        }
    }
    out
}

#[derive(Debug)]
struct MissingPartitionFilter {
    label: String,
    relation: String,
    partition_columns: Vec<String>,
}

/// Recursively walk a QueryOp tree, accumulating cost signals.
fn walk_op(op: &QueryOp, ctx: &mut CostCtx) {
    match op {
        QueryOp::Match {
            patterns, optional, ..
        } => {
            ctx.pattern_count += patterns.len();
            if *optional {
                ctx.optional_match_count += 1;
            }

            if patterns.len() > 1 && has_cartesian_product(patterns) {
                ctx.has_cartesian = true;
            }

            for p in patterns {
                collect_pattern_signals(p, ctx);
            }
        }

        QueryOp::PathFind { max_depth, .. } => {
            ctx.pattern_count += 1;
            if let Some(d) = max_depth
                && *d > ctx.max_var_length_depth
            {
                ctx.max_var_length_depth = *d;
            }
        }

        QueryOp::Aggregate { source, .. } => {
            walk_op(&source.operation, ctx);
        }

        QueryOp::Union { queries, .. } => {
            for q in queries {
                walk_op(&q.operation, ctx);
            }
        }

        QueryOp::Chain { steps } => {
            for ChainStep { operation, .. } in steps {
                walk_op(operation, ctx);
            }
        }

        QueryOp::CallSubquery { inner, .. } => {
            walk_op(&inner.operation, ctx);
        }

        QueryOp::Mutate { context, .. } => {
            if let Some(c) = context {
                walk_op(c, ctx);
            }
        }

        QueryOp::Analytics { source, .. } => {
            if let ox_query_ir::query::AnalyticsSource::Subgraph { filter } = source {
                walk_op(filter, ctx);
            }
        }

        QueryOp::HybridSearch { request } => {
            // Hybrid retrieval is a top-K operation against
            // pre-computed indexes — its cost is bounded by
            // top_k regardless of the corpus size. Treat it as
            // a single bounded read with no pattern signals, so
            // the cost dispatcher routes it through the cheap
            // index-procedure path rather than triggering
            // Cartesian / var-length warnings.
            ctx.pattern_count += 1;
            if let Some(constraint) = &request.graph_constraints {
                for node in &constraint.nodes {
                    if let Some(lbl) = &node.label {
                        ctx.referenced_labels.insert(lbl.to_string());
                    }
                }
            }
        }
    }
}

/// Extract cost signals from a single pattern.
fn collect_pattern_signals(pattern: &GraphPattern, ctx: &mut CostCtx) {
    match pattern {
        GraphPattern::Node {
            label,
            property_filters,
            ..
        } => {
            if let Some(lbl) = label {
                ctx.referenced_labels.insert(lbl.to_string());
                for pf in property_filters {
                    ctx.filter_labels
                        .push((lbl.to_string(), pf.property.to_string()));
                }
            }
        }

        GraphPattern::Relationship {
            label,
            var_length,
            property_filters,
            ..
        } => {
            if let Some(lbl) = label {
                ctx.relationship_labels.push(lbl.to_string());
                for pf in property_filters {
                    ctx.filter_labels
                        .push((lbl.to_string(), pf.property.to_string()));
                }
            }

            // Only count actual variable-length patterns (not fixed paths)
            if let Some(vl) = var_length {
                // Unbounded `*` (min=None, max=None) → sentinel HIGH depth
                let depth = match (vl.min, vl.max) {
                    (_, Some(max)) => max,
                    (Some(min), None) => min.max(UNBOUNDED_VAR_LENGTH_DEPTH),
                    (None, None) => UNBOUNDED_VAR_LENGTH_DEPTH,
                };
                if depth > ctx.max_var_length_depth {
                    ctx.max_var_length_depth = depth;
                }
            }
        }

        GraphPattern::Path { elements } => {
            // Path elements are fixed-length hops — no variable-length depth.
            // Only collect relationship labels for cardinality check.
            for e in elements {
                if let ox_query_ir::query::PathElement::Edge {
                    label: Some(lbl), ..
                } = e
                {
                    ctx.relationship_labels.push(lbl.to_string());
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ontology-aware checks
// ---------------------------------------------------------------------------

/// Check if any filter's (label, property) pair is covered by an ontology index.
fn check_indexed_filters(filters: &[(String, String)], ontology: &OntologyIR) -> bool {
    if filters.is_empty() {
        return false;
    }

    // Build a set of indexed (node_label, property_name) pairs
    let mut indexed: HashSet<(&str, &str)> = HashSet::new();
    for idx in ontology.indexes() {
        let (node_id, prop_ids) = match idx {
            ox_ontology::ir::IndexDef::Single {
                node_id,
                property_id,
                ..
            } => (node_id, vec![property_id]),
            ox_ontology::ir::IndexDef::Composite {
                node_id,
                property_ids,
                ..
            } => (node_id, property_ids.iter().collect()),
            ox_ontology::ir::IndexDef::FullText {
                node_id,
                property_ids,
                ..
            } => (node_id, property_ids.iter().collect()),
            ox_ontology::ir::IndexDef::Vector {
                node_id,
                property_id,
                ..
            } => (node_id, vec![property_id]),
        };

        if let Some(node) = ontology.node_types().iter().find(|n| &n.id == node_id) {
            for pid in prop_ids {
                if let Some(prop) = node.properties.iter().find(|p| &p.id == pid) {
                    indexed.insert((&node.label, &prop.name));
                }
            }
        }
    }

    // Also count unique constraints as implicit indexes
    for node in ontology.node_types() {
        for cdef in node.constraints.iter() {
            let prop_ids: Vec<&str> = match &cdef.constraint {
                ox_ontology::ir::NodeConstraint::Unique { property_ids } => {
                    property_ids.iter().map(|id| id.as_ref()).collect()
                }
                ox_ontology::ir::NodeConstraint::NodeKey { property_ids } => {
                    property_ids.iter().map(|id| id.as_ref()).collect()
                }
                ox_ontology::ir::NodeConstraint::Exists { .. } => continue,
            };
            for pid in prop_ids {
                if let Some(prop) = node.properties.iter().find(|p| p.id == pid) {
                    indexed.insert((&node.label, &prop.name));
                }
            }
        }
    }

    filters
        .iter()
        .any(|(label, prop)| indexed.contains(&(label.as_str(), prop.as_str())))
}

/// Check if any referenced relationship label has many-to-many cardinality.
fn check_high_fanout(rel_labels: &[String], ontology: &OntologyIR) -> bool {
    rel_labels.iter().any(|label| {
        ontology.edge_types().iter().any(|e| {
            e.label == *label
                && matches!(e.cardinality, ox_ontology::ir::Cardinality::ManyToMany)
        })
    })
}

// ---------------------------------------------------------------------------
// Cartesian product detection via Union-Find
// ---------------------------------------------------------------------------

/// Returns true if the patterns contain disconnected components
/// (i.e., node variables that share no relationship or path).
fn has_cartesian_product(patterns: &[GraphPattern]) -> bool {
    let mut var_index: HashMap<String, usize> = HashMap::new();
    let mut connections: Vec<(usize, usize)> = Vec::new();

    let get_or_insert = |var: &str, map: &mut HashMap<String, usize>| -> usize {
        let next = map.len();
        *map.entry(var.to_string()).or_insert(next)
    };

    for p in patterns {
        match p {
            GraphPattern::Node { variable, .. } => {
                get_or_insert(variable, &mut var_index);
            }
            GraphPattern::Relationship { source, target, .. } => {
                let si = get_or_insert(source, &mut var_index);
                let ti = get_or_insert(target, &mut var_index);
                connections.push((si, ti));
            }
            GraphPattern::Path { elements } => {
                let path_indices: Vec<usize> = elements
                    .iter()
                    .filter_map(|e| match e {
                        ox_query_ir::query::PathElement::Node { variable, .. } => {
                            Some(get_or_insert(variable, &mut var_index))
                        }
                        _ => None,
                    })
                    .collect();
                for pair in path_indices.windows(2) {
                    connections.push((pair[0], pair[1]));
                }
            }
        }
    }

    let n = var_index.len();
    if n <= 1 {
        return false;
    }

    // Union-Find with path compression
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    for (a, b) in &connections {
        let ra = find(&mut parent, *a);
        let rb = find(&mut parent, *b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    let roots: HashSet<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    roots.len() > 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::GraphLabel;
    use ox_core::LocalizedText;
    use ox_core::PropertyKey;
    use ox_ontology::ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, PropertyDef,
    };
    use ox_query_ir::query::*;
    use ox_core::types::{Direction, PropertyType};

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label literal must be valid")
    }

    fn vn(s: &'static str) -> ox_core::VariableName {
        ox_core::VariableName::new(s).expect("test variable name literal must be valid")
    }

    fn pk(s: &'static str) -> PropertyKey {
        PropertyKey::new(s).expect("test property name literal must be valid")
    }
    fn empty_ontology() -> OntologyIR {
        OntologyIR::new(
            "test".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        )
    }

    fn ontology_with_index() -> OntologyIR {
        OntologyIR::new(
            "test".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![NodeTypeDef {
                id: "nt1".into(),
                label: gl("Person"),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    id: "p1".into(),
                    name: pk("name"),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    ..Default::default()
                }],
                constraints: vec![ConstraintDef {
                    id: "c1".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p1".into()],
                    },
                }],
                ..Default::default()
            }],
            vec![EdgeTypeDef {
                id: "et1".into(),
                label: gl("KNOWS"),
                description: LocalizedText::default(),
                source_node_id: "nt1".into(),
                target_node_id: "nt1".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
                ..Default::default()
            }],
            vec![],
        )
    }

    fn simple_match(patterns: Vec<GraphPattern>) -> QueryIR {
        QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns,
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        }
    }

    #[test]
    fn single_node_is_low_risk() {
        let ir = simple_match(vec![GraphPattern::Node {
            variable: vn("n"),
            label: Some(gl("Person")),
            property_filters: vec![],
        }]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.risk_level, RiskLevel::Low);
        assert!(!cost.has_cartesian);
        assert_eq!(cost.pattern_count, 1);
    }

    #[test]
    fn connected_patterns_no_cartesian() {
        let ir = simple_match(vec![
            GraphPattern::Node {
                variable: vn("a"),
                label: Some(gl("Person")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("KNOWS")),
                source: vn("a"),
                target: vn("b"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
        ]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert!(!cost.has_cartesian);
    }

    #[test]
    fn disconnected_patterns_cartesian() {
        let ir = simple_match(vec![
            GraphPattern::Node {
                variable: vn("a"),
                label: Some(gl("Person")),
                property_filters: vec![],
            },
            GraphPattern::Node {
                variable: vn("b"),
                label: Some(gl("Company")),
                property_filters: vec![],
            },
        ]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert!(cost.has_cartesian);
        assert_eq!(cost.risk_level, RiskLevel::High);
    }

    #[test]
    fn deep_var_length_is_high_risk() {
        let ir = simple_match(vec![GraphPattern::Relationship {
            variable: None,
            label: Some(gl("FOLLOWS")),
            source: vn("a"),
            target: vn("b"),
            direction: Direction::Outgoing,
            property_filters: vec![],
            var_length: Some(VarLength {
                min: Some(1),
                max: Some(8),
            }),
        }]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.max_var_length_depth, 8);
        assert_eq!(cost.risk_level, RiskLevel::High);
    }

    #[test]
    fn moderate_var_length_is_medium() {
        let ir = simple_match(vec![GraphPattern::Relationship {
            variable: None,
            label: Some(gl("FOLLOWS")),
            source: vn("a"),
            target: vn("b"),
            direction: Direction::Outgoing,
            property_filters: vec![],
            var_length: Some(VarLength {
                min: Some(1),
                max: Some(4),
            }),
        }]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.risk_level, RiskLevel::Medium);
    }

    #[test]
    fn fixed_length_path_not_counted_as_var_length() {
        let ir = simple_match(vec![GraphPattern::Path {
            elements: vec![
                PathElement::Node {
                    variable: vn("a"),
                    label: Some(gl("Person")),
                },
                PathElement::Edge {
                    variable: None,
                    label: Some(gl("KNOWS")),
                    direction: Direction::Outgoing,
                },
                PathElement::Node {
                    variable: vn("b"),
                    label: None,
                },
                PathElement::Edge {
                    variable: None,
                    label: Some(gl("WORKS_AT")),
                    direction: Direction::Outgoing,
                },
                PathElement::Node {
                    variable: vn("c"),
                    label: Some(gl("Company")),
                },
            ],
        }]);
        let cost = estimate_cost(&ir, &empty_ontology());
        // Fixed-length path: 2 edges = 2 hops, but this is NOT variable-length
        assert_eq!(cost.max_var_length_depth, 0);
    }

    #[test]
    fn optional_matches_tracked() {
        let ir = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Chain {
                steps: vec![
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("n"),
                                label: None,
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![],
                            optional: true,
                            group_by: vec![],
                        },
                    },
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("m"),
                                label: None,
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![],
                            optional: true,
                            group_by: vec![],
                        },
                    },
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("x"),
                                label: None,
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![],
                            optional: true,
                            group_by: vec![],
                        },
                    },
                ],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.optional_match_count, 3);
        assert_eq!(cost.risk_level, RiskLevel::Medium);
    }

    #[test]
    fn aggregate_walks_into_source() {
        let inner = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: vn("x"),
                        label: None,
                        property_filters: vec![],
                    },
                    GraphPattern::Node {
                        variable: vn("y"),
                        label: None,
                        property_filters: vec![],
                    },
                ],
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
        let ir = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Aggregate {
                source: Box::new(inner),
                group_by: vec![],
                aggregations: vec![],
                having: None,
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        let cost = estimate_cost(&ir, &empty_ontology());
        assert!(cost.has_cartesian);
    }

    #[test]
    fn indexed_filter_detected() {
        let ir = simple_match(vec![GraphPattern::Node {
            variable: vn("p"),
            label: Some(gl("Person")),
            property_filters: vec![PropertyFilter {
                property: pk("name"),
                value: Expr::Literal {
                    value: ox_core::types::PropertyValue::String("Alice".into()),
                },
            }],
        }]);
        let ont = ontology_with_index();
        let cost = estimate_cost(&ir, &ont);
        assert!(cost.uses_indexed_filter);
    }

    #[test]
    fn unbounded_var_length_is_high_risk() {
        // Cypher `*` → VarLength { min: None, max: None } = unlimited traversal
        let ir = simple_match(vec![GraphPattern::Relationship {
            variable: None,
            label: Some(gl("FOLLOWS")),
            source: vn("a"),
            target: vn("b"),
            direction: Direction::Outgoing,
            property_filters: vec![],
            var_length: Some(VarLength {
                min: None,
                max: None,
            }),
        }]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.risk_level, RiskLevel::High);
        assert!(cost.max_var_length_depth > VAR_LENGTH_HIGH_THRESHOLD);
    }

    #[test]
    fn min_only_var_length_is_high_risk() {
        // `*3..` → min=3, max=None = unbounded upper
        let ir = simple_match(vec![GraphPattern::Relationship {
            variable: None,
            label: Some(gl("FOLLOWS")),
            source: vn("a"),
            target: vn("b"),
            direction: Direction::Outgoing,
            property_filters: vec![],
            var_length: Some(VarLength {
                min: Some(3),
                max: None,
            }),
        }]);
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.risk_level, RiskLevel::High);
    }

    #[test]
    fn many_to_many_detected_as_high_fanout() {
        let ir = simple_match(vec![GraphPattern::Relationship {
            variable: None,
            label: Some(gl("KNOWS")),
            source: vn("a"),
            target: vn("b"),
            direction: Direction::Outgoing,
            property_filters: vec![],
            var_length: None,
        }]);
        let ont = ontology_with_index();
        let cost = estimate_cost(&ir, &ont);
        assert!(cost.has_high_fanout);
    }

    fn ontology_with_partition_aware_mapping() -> OntologyIR {
        use ox_ontology::mapping::{ColumnRef, ObjectMappingDef};
        let mut ont = ontology_with_index();
        let nt = ont.node_types()[0].id.clone();
        let mut mapping = ObjectMappingDef::new(
            "om-person",
            nt,
            "bigquery:warehouse",
            "fact.persons",
        );
        mapping.partition_columns = vec![ColumnRef::new("fact.persons", "stdrd_ymd")];
        ont.add_object_mapping(mapping)
            .expect("partition-aware fixture mapping must register");
        ont
    }

    fn ir_match_node(label: &'static str, var: &'static str, filters: Vec<PropertyFilter>) -> QueryIR {
        QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn(var),
                    label: Some(gl(label)),
                    property_filters: filters,
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
        }
    }

    #[test]
    fn missing_partition_filter_promotes_to_high_risk() {
        let ir = ir_match_node("Person", "p", vec![]);
        let ont = ontology_with_partition_aware_mapping();
        let cost = estimate_cost(&ir, &ont);
        assert_eq!(cost.risk_level, RiskLevel::High);
        assert!(
            cost.warnings.iter().any(|w| w.contains("partition")),
            "expected a partition warning, got {:?}",
            cost.warnings
        );
    }

    #[test]
    fn partition_column_in_filter_clears_warning() {
        let filters = vec![PropertyFilter {
            property: pk("stdrd_ymd"),
            value: Expr::Literal {
                value: ox_core::types::PropertyValue::String("2026-01-01".into()),
            },
        }];
        let ir = ir_match_node("Person", "p", filters);
        let ont = ontology_with_partition_aware_mapping();
        let cost = estimate_cost(&ir, &ont);
        assert!(
            !cost.warnings.iter().any(|w| w.contains("partition")),
            "expected no partition warning, got {:?}",
            cost.warnings
        );
    }
}

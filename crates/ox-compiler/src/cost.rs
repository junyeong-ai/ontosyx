use std::collections::{HashMap, HashSet};

use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::{ChainStep, GraphPattern, QueryIR, QueryOp};
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
}

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

    let risk_level = classify_risk(&ctx, uses_indexed_filter, has_high_fanout);

    QueryCost {
        pattern_count: ctx.pattern_count,
        has_cartesian: ctx.has_cartesian,
        max_var_length_depth: ctx.max_var_length_depth,
        optional_match_count: ctx.optional_match_count,
        uses_indexed_filter,
        has_high_fanout,
        risk_level,
        warnings,
    }
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
}

fn classify_risk(ctx: &CostCtx, indexed: bool, high_fanout: bool) -> RiskLevel {
    if ctx.has_cartesian || ctx.max_var_length_depth > VAR_LENGTH_HIGH_THRESHOLD {
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
            if let ox_core::query_ir::AnalyticsSource::Subgraph { filter } = source {
                walk_op(filter, ctx);
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
            // Track filtered label.property pairs for index check
            if let Some(lbl) = label {
                for pf in property_filters {
                    ctx.filter_labels.push((lbl.clone(), pf.property.clone()));
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
                ctx.relationship_labels.push(lbl.clone());
                for pf in property_filters {
                    ctx.filter_labels.push((lbl.clone(), pf.property.clone()));
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
                if let ox_core::query_ir::PathElement::Edge {
                    label: Some(lbl), ..
                } = e
                {
                    ctx.relationship_labels.push(lbl.clone());
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
    for idx in &ontology.indexes {
        let (node_id, prop_ids) = match idx {
            ox_core::ontology_ir::IndexDef::Single {
                node_id,
                property_id,
                ..
            } => (node_id, vec![property_id]),
            ox_core::ontology_ir::IndexDef::Composite {
                node_id,
                property_ids,
                ..
            } => (node_id, property_ids.iter().collect()),
            ox_core::ontology_ir::IndexDef::FullText {
                node_id,
                property_ids,
                ..
            } => (node_id, property_ids.iter().collect()),
            ox_core::ontology_ir::IndexDef::Vector {
                node_id,
                property_id,
                ..
            } => (node_id, vec![property_id]),
        };

        if let Some(node) = ontology.node_types.iter().find(|n| &n.id == node_id) {
            for pid in prop_ids {
                if let Some(prop) = node.properties.iter().find(|p| &p.id == pid) {
                    indexed.insert((&node.label, &prop.name));
                }
            }
        }
    }

    // Also count unique constraints as implicit indexes
    for node in &ontology.node_types {
        for cdef in node.constraints.iter() {
            let prop_ids: Vec<&str> = match &cdef.constraint {
                ox_core::ontology_ir::NodeConstraint::Unique { property_ids } => {
                    property_ids.iter().map(|id| id.as_ref()).collect()
                }
                ox_core::ontology_ir::NodeConstraint::NodeKey { property_ids } => {
                    property_ids.iter().map(|id| id.as_ref()).collect()
                }
                ox_core::ontology_ir::NodeConstraint::Exists { .. } => continue,
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
        ontology.edge_types.iter().any(|e| {
            e.label == *label
                && matches!(e.cardinality, ox_core::ontology_ir::Cardinality::ManyToMany)
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
                        ox_core::query_ir::PathElement::Node { variable, .. } => {
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
    use ox_core::ontology_ir::{
        Cardinality, ConstraintDef, EdgeTypeDef, NodeConstraint, NodeTypeDef, PropertyDef,
    };
    use ox_core::query_ir::*;
    use ox_core::types::{Direction, PropertyType};

    fn empty_ontology() -> OntologyIR {
        OntologyIR::new(
            "test".into(),
            "Test".into(),
            None,
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
            None,
            1,
            vec![NodeTypeDef {
                id: "nt1".into(),
                label: "Person".into(),
                description: None,
                source_table: None,
                properties: vec![PropertyDef {
                    id: "p1".into(),
                    name: "name".into(),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: None,
                    classification: None,
                }],
                constraints: vec![ConstraintDef {
                    id: "c1".into(),
                    constraint: NodeConstraint::Unique {
                        property_ids: vec!["p1".into()],
                    },
                }],
            }],
            vec![EdgeTypeDef {
                id: "et1".into(),
                label: "KNOWS".into(),
                description: None,
                source_node_id: "nt1".into(),
                target_node_id: "nt1".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToMany,
            }],
            vec![],
        )
    }

    fn simple_match(patterns: Vec<GraphPattern>) -> QueryIR {
        QueryIR {
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
        }
    }

    #[test]
    fn single_node_is_low_risk() {
        let ir = simple_match(vec![GraphPattern::Node {
            variable: "n".into(),
            label: Some("Person".into()),
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
                variable: "a".into(),
                label: Some("Person".into()),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some("KNOWS".into()),
                source: "a".into(),
                target: "b".into(),
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
                variable: "a".into(),
                label: Some("Person".into()),
                property_filters: vec![],
            },
            GraphPattern::Node {
                variable: "b".into(),
                label: Some("Company".into()),
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
            label: Some("FOLLOWS".into()),
            source: "a".into(),
            target: "b".into(),
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
            label: Some("FOLLOWS".into()),
            source: "a".into(),
            target: "b".into(),
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
                    variable: "a".into(),
                    label: Some("Person".into()),
                },
                PathElement::Edge {
                    variable: None,
                    label: Some("KNOWS".into()),
                    direction: Direction::Outgoing,
                },
                PathElement::Node {
                    variable: "b".into(),
                    label: None,
                },
                PathElement::Edge {
                    variable: None,
                    label: Some("WORKS_AT".into()),
                    direction: Direction::Outgoing,
                },
                PathElement::Node {
                    variable: "c".into(),
                    label: Some("Company".into()),
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
            operation: QueryOp::Chain {
                steps: vec![
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: "n".into(),
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
                                variable: "m".into(),
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
                                variable: "x".into(),
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
        };
        let cost = estimate_cost(&ir, &empty_ontology());
        assert_eq!(cost.optional_match_count, 3);
        assert_eq!(cost.risk_level, RiskLevel::Medium);
    }

    #[test]
    fn aggregate_walks_into_source() {
        let inner = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: "x".into(),
                        label: None,
                        property_filters: vec![],
                    },
                    GraphPattern::Node {
                        variable: "y".into(),
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
        };
        let ir = QueryIR {
            operation: QueryOp::Aggregate {
                source: Box::new(inner),
                group_by: vec![],
                aggregations: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };
        let cost = estimate_cost(&ir, &empty_ontology());
        assert!(cost.has_cartesian);
    }

    #[test]
    fn indexed_filter_detected() {
        let ir = simple_match(vec![GraphPattern::Node {
            variable: "p".into(),
            label: Some("Person".into()),
            property_filters: vec![PropertyFilter {
                property: "name".into(),
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
            label: Some("FOLLOWS".into()),
            source: "a".into(),
            target: "b".into(),
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
            label: Some("FOLLOWS".into()),
            source: "a".into(),
            target: "b".into(),
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
            label: Some("KNOWS".into()),
            source: "a".into(),
            target: "b".into(),
            direction: Direction::Outgoing,
            property_filters: vec![],
            var_length: None,
        }]);
        let ont = ontology_with_index();
        let cost = estimate_cost(&ir, &ont);
        assert!(cost.has_high_fanout);
    }
}

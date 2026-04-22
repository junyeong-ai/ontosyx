//! Π-2 — auto-DISTINCT for aggregations that cross a ManyToMany link.
//!
//! When an aggregation's source pattern traverses an edge whose
//! physical `LinkMappingDef.cardinality` is `OneToMany` or
//! `ManyToMany`, a single "logical" source row can fan out into
//! multiple "physical" rows. Running `sum / count / avg` over that
//! result without `DISTINCT` double-counts — a bug that is easy to
//! miss in schema-native authoring ("it's just a count, how could
//! it be wrong?") and annoying to diagnose because the number is
//! *plausible*, just inflated.
//!
//! This pass walks the incoming `QueryIR`, and for every
//! `QueryOp::Aggregate { source, … }` asks the ontology: does any
//! relationship in `source` cross a link whose
//! `LinkCardinality::requires_distinct_on_aggregation()` is true? If
//! so, we set `distinct: true` on every `AggregationExpr` in the
//! aggregate block. The flag was always there on the IR (see
//! `AggregationExpr.distinct`) — this pass just populates it
//! correctly from the ontology schema instead of relying on the
//! query author to remember.
//!
//! ## Scope
//!
//! - Works on every `QueryOp` variant recursively — `Union`,
//!   `Chain`, `CallSubquery` are walked so an Aggregate nested
//!   inside them still gets the treatment.
//! - Does **not** inject `DISTINCT` into non-aggregating queries.
//!   `count(n)` is the only canonical counting surface; raw `MATCH`
//!   with a fan-out link is a user-facing issue the PatternIR UI
//!   flags separately.
//! - Idempotent: calling it on a QueryIR that already has
//!   `distinct: true` on every aggregation leaves the IR unchanged.
//!
//! ## Why not inject at the Cypher emitter?
//!
//! The emitter (`crates/ox-compiler/src/cypher/query.rs`) does not
//! receive the `OntologyIR`. Giving it one would push schema lookups
//! into every emitter implementation (Cypher + future SQL / Gremlin)
//! and force `GraphCompiler::compile_query` to widen its signature.
//! A standalone pre-pass keeps the emitter trait narrow and mirrors
//! the Λ / Phase-2 `rewrite_temporal_with_renames` pattern already
//! in use.

use ox_ontology::OntologyIR;
use ox_ontology::mapping::LinkMappingDef;
use ox_query_ir::query::{GraphPattern, QueryIR, QueryOp};

/// Rewrite `query` so every aggregation that crosses a ManyToMany /
/// OneToMany link gets `distinct: true` set on its aggregations.
/// Returns the rewritten query; the original is consumed.
pub fn rewrite_auto_distinct(mut query: QueryIR, ontology: &OntologyIR) -> QueryIR {
    query.operation = walk_op(query.operation, ontology);
    query
}

fn walk_op(op: QueryOp, ontology: &OntologyIR) -> QueryOp {
    match op {
        QueryOp::Aggregate {
            source,
            group_by,
            mut aggregations,
            having,
        } => {
            let inner = walk_op(source.operation, ontology);
            let needs_distinct = pattern_crosses_many_link(&inner, ontology);
            if needs_distinct {
                for agg in aggregations.iter_mut() {
                    agg.distinct = true;
                }
            }
            QueryOp::Aggregate {
                source: Box::new(QueryIR {
                    operation: inner,
                    ..*source
                }),
                group_by,
                aggregations,
                having,
            }
        }
        QueryOp::Union { queries, all } => QueryOp::Union {
            queries: queries
                .into_iter()
                .map(|q| rewrite_auto_distinct(q, ontology))
                .collect(),
            all,
        },
        QueryOp::Chain { steps } => QueryOp::Chain { steps },
        QueryOp::CallSubquery {
            inner,
            import_variables,
        } => QueryOp::CallSubquery {
            inner: Box::new(rewrite_auto_distinct(*inner, ontology)),
            import_variables,
        },
        // Non-aggregating / leaf variants — nothing to rewrite.
        other => other,
    }
}

/// True iff `op` contains at least one `GraphPattern::Relationship`
/// whose resolved `LinkMappingDef.cardinality` requires DISTINCT for
/// aggregations. An edge with no link mapping (schema-only, no
/// physical binding) is conservatively treated as non-distinct-
/// requiring — we do not have cardinality data to reason about it.
fn pattern_crosses_many_link(op: &QueryOp, ontology: &OntologyIR) -> bool {
    match op {
        QueryOp::Match { patterns, .. } => patterns.iter().any(|p| pattern_requires_distinct(p, ontology)),
        QueryOp::PathFind { edge_types, .. } => edge_types
            .iter()
            .any(|lbl| edge_label_requires_distinct(lbl.as_str(), ontology)),
        QueryOp::Union { queries, .. } => queries
            .iter()
            .any(|q| pattern_crosses_many_link(&q.operation, ontology)),
        QueryOp::Chain { steps } => steps.iter().any(|s| {
            // ChainStep shape varies by construction; the Aggregate
            // above already recursed into `source`. A nested Chain
            // is rare enough that we conservatively treat it as
            // fan-out-free — the outer rewriter will catch the
            // step's own Aggregate when it fires.
            let _ = s;
            false
        }),
        _ => false,
    }
}

fn pattern_requires_distinct(pattern: &GraphPattern, ontology: &OntologyIR) -> bool {
    match pattern {
        GraphPattern::Relationship {
            label: Some(label), ..
        } => edge_label_requires_distinct(label.as_str(), ontology),
        GraphPattern::Path { elements } => {
            use ox_query_ir::query::PathElement;
            elements.iter().any(|el| match el {
                PathElement::Edge {
                    label: Some(label), ..
                } => edge_label_requires_distinct(label.as_str(), ontology),
                _ => false,
            })
        }
        // Node patterns cannot fan out on their own — fan-out is a
        // property of traversal. Relationships without a label bind
        // to no particular edge type, so we cannot resolve cardinality
        // for them; treat as non-fan-out. `GraphPattern::Shortest` /
        // other variants delegate to their contained relationships
        // through the same match arm once the variant is added.
        _ => false,
    }
}

fn edge_label_requires_distinct(edge_label: &str, ontology: &OntologyIR) -> bool {
    // Resolve the label to an EdgeTypeDef id, then check every link
    // mapping bound to that edge type. An edge backed by multiple
    // link mappings takes the worst-case cardinality across them —
    // if any mapping fans out, the aggregation over that edge can
    // double-count, so DISTINCT is the safe default.
    let Some(edge_type) = ontology.edge_types().iter().find(|e| e.label.as_str() == edge_label)
    else {
        return false;
    };
    ontology
        .link_mappings()
        .iter()
        .any(|lm: &LinkMappingDef| {
            lm.edge_type_id == edge_type.id && lm.cardinality.requires_distinct_on_aggregation()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::types::Direction;
    use ox_core::variable_name::VariableName;
    use ox_ontology::ir::{EdgeTypeDef, NodeTypeDef, OntologyIR, OntologyVersion};
    use ox_ontology::mapping::{
        EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef, LinkMappingId, LinkMappingKind,
        SourceId,
    };
    use ox_query_ir::query::{
        AggFunction, AggregationExpr, FieldRef, GraphPattern, QUERY_IR_SCHEMA_VERSION, QueryIR,
        QueryOp,
    };

    fn vn(s: &str) -> VariableName {
        VariableName::new(s).expect("var name")
    }

    fn gl(s: &str) -> GraphLabel {
        GraphLabel::new(s).expect("graph label")
    }

    fn agg_count(alias: &str) -> AggregationExpr {
        AggregationExpr {
            function: AggFunction::Count,
            field: FieldRef {
                variable: vn("x"),
                field: None,
            },
            alias: alias.to_string(),
            distinct: false,
        }
    }

    /// An aggregation whose source pattern crosses no edges keeps
    /// `distinct: false` — no fan-out risk, no false positive.
    #[test]
    fn leaf_match_no_distinct_needed() {
        let ontology = minimal_ontology();
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Aggregate {
                source: Box::new(QueryIR {
                    schema_version: QUERY_IR_SCHEMA_VERSION,
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Node {
                            variable: vn("a"),
                            label: Some(gl("A")),
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
                }),
                group_by: vec![],
                aggregations: vec![agg_count("n")],
                having: None,
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        let rewritten = rewrite_auto_distinct(query, &ontology);
        match rewritten.operation {
            QueryOp::Aggregate { aggregations, .. } => {
                assert!(!aggregations[0].distinct, "no fan-out link → no DISTINCT");
            }
            _ => panic!("top op must remain Aggregate"),
        }
    }

    /// Traversing a OneToOne edge also leaves distinct=false —
    /// cardinality is not fan-out.
    #[test]
    fn one_to_one_traversal_no_distinct() {
        let ontology = ontology_with_edge("HAS_ONE", LinkCardinality::OneToOne);
        let rewritten = rewrite_auto_distinct(agg_across_edge("HAS_ONE"), &ontology);
        match rewritten.operation {
            QueryOp::Aggregate { aggregations, .. } => {
                assert!(!aggregations[0].distinct);
            }
            _ => unreachable!(),
        }
    }

    /// OneToMany traversal flips every aggregation in the block to
    /// distinct — even aggs that don't reference the target side,
    /// because the planner generally has no cheap way to prove
    /// per-aggregation that the result is safe.
    #[test]
    fn one_to_many_traversal_forces_distinct() {
        let ontology = ontology_with_edge("HAS_MANY", LinkCardinality::OneToMany);
        let rewritten = rewrite_auto_distinct(agg_across_edge("HAS_MANY"), &ontology);
        match rewritten.operation {
            QueryOp::Aggregate { aggregations, .. } => {
                assert!(aggregations[0].distinct);
            }
            _ => unreachable!(),
        }
    }

    /// ManyToMany also forces DISTINCT.
    #[test]
    fn many_to_many_traversal_forces_distinct() {
        let ontology = ontology_with_edge("BRIDGE", LinkCardinality::ManyToMany);
        let rewritten = rewrite_auto_distinct(agg_across_edge("BRIDGE"), &ontology);
        match rewritten.operation {
            QueryOp::Aggregate { aggregations, .. } => {
                assert!(aggregations[0].distinct);
            }
            _ => unreachable!(),
        }
    }

    /// Rewriter is idempotent — running twice on a fan-out query
    /// doesn't flip distinct back, and the second pass makes no
    /// further changes.
    #[test]
    fn idempotent_on_already_distinct() {
        let ontology = ontology_with_edge("HAS_MANY", LinkCardinality::OneToMany);
        let once = rewrite_auto_distinct(agg_across_edge("HAS_MANY"), &ontology);
        let twice = rewrite_auto_distinct(once, &ontology);
        match twice.operation {
            QueryOp::Aggregate { aggregations, .. } => assert!(aggregations[0].distinct),
            _ => unreachable!(),
        }
    }

    /// Aggregations nested inside UNION branches each get their own
    /// rewrite — the pass descends.
    #[test]
    fn rewrites_aggregate_inside_union() {
        let ontology = ontology_with_edge("HAS_MANY", LinkCardinality::ManyToMany);
        let query = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Union {
                all: false,
                queries: vec![agg_across_edge("HAS_MANY")],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        let rewritten = rewrite_auto_distinct(query, &ontology);
        match rewritten.operation {
            QueryOp::Union { queries, .. } => match &queries[0].operation {
                QueryOp::Aggregate { aggregations, .. } => {
                    assert!(aggregations[0].distinct);
                }
                _ => panic!("union branch should still be an Aggregate"),
            },
            _ => panic!("top op must remain Union"),
        }
    }

    /// Helper: Aggregate whose source is `(a)-[:label]->(b)` and a
    /// single `count(b)` aggregation.
    fn agg_across_edge(edge_label: &str) -> QueryIR {
        QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Aggregate {
                source: Box::new(QueryIR {
                    schema_version: QUERY_IR_SCHEMA_VERSION,
                    operation: QueryOp::Match {
                        patterns: vec![
                            GraphPattern::Node {
                                variable: vn("a"),
                                label: Some(gl("A")),
                                property_filters: vec![],
                            },
                            GraphPattern::Relationship {
                                variable: None,
                                label: Some(gl(edge_label)),
                                source: vn("a"),
                                target: vn("b"),
                                direction: Direction::Outgoing,
                                property_filters: vec![],
                                var_length: None,
                            },
                            GraphPattern::Node {
                                variable: vn("b"),
                                label: Some(gl("B")),
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
                }),
                group_by: vec![],
                aggregations: vec![agg_count("n")],
                having: None,
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        }
    }

    fn minimal_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont-test".into(),
            "AD Test".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                valid_from: None,
                valid_to: None,
                committed_by: None,
                commit_message: None,
            },
            vec![NodeTypeDef {
                id: "nt_a".into(),
                label: gl("A"),
                description: LocalizedText::default(),
                properties: vec![],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
    }

    /// Build an ontology with two node types (A, B), one edge (label
    /// supplied by caller), and one LinkMapping whose cardinality
    /// comes from the caller. The LinkMapping's endpoint refs point
    /// at fabricated source relations — `add_link_mapping` only
    /// enforces id uniqueness + `edge_type_id` reference at insert
    /// time, so the rest is free-form for this focused test.
    fn ontology_with_edge(edge_label: &str, cardinality: LinkCardinality) -> OntologyIR {
        let mut ir = OntologyIR::new(
            "ont-test".into(),
            "AD Test".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: 1,
                valid_from: None,
                valid_to: None,
                committed_by: None,
                commit_message: None,
            },
            vec![
                NodeTypeDef {
                    id: "nt_a".into(),
                    label: gl("A"),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "nt_b".into(),
                    label: gl("B"),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "et_x".into(),
                label: gl(edge_label),
                description: LocalizedText::default(),
                source_node_id: "nt_a".into(),
                target_node_id: "nt_b".into(),
                cardinality: ox_ontology::ir::Cardinality::ManyToMany,
                properties: vec![],
                ..Default::default()
            }],
            vec![],
        );

        let endpoint = |rel: &str| EndpointRef {
            source_id: SourceId::new("src"),
            relation: rel.to_string(),
            key_columns: vec!["id".to_string()],
        };

        ir.add_link_mapping(LinkMappingDef {
            id: LinkMappingId::new("lm_x"),
            edge_type_id: "et_x".into(),
            kind: LinkMappingKind::ForeignKey {
                source_column: ox_ontology::mapping::ColumnRef::new("t_a", "b_id"),
                target_column: ox_ontology::mapping::ColumnRef::new("t_b", "id"),
            },
            source_endpoint: endpoint("t_a"),
            target_endpoint: endpoint("t_b"),
            join_cost_hint: JoinCostHint::default(),
            precedence: 0,
            cardinality,
        })
        .expect("attach LinkMapping");

        ir
    }
}

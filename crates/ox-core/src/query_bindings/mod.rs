//! Resolve QueryIR variable bindings against an OntologyIR.
//!
//! Walks the QueryIR AST and produces [`ResolvedQueryBindings`] — a structured
//! provenance record of which ontology entities (nodes, edges, properties) each
//! variable/pattern references. This powers "Show on graph" highlighting.
//!
//! **Scope-aware**: UNION branches and EXISTS sub-queries each get isolated
//! variable scopes. Variables defined inside a scope don't leak into siblings
//! or the outer scope. Property bindings track scope paths and allow duplicates
//! for the same property used in different contexts (WHERE + ORDER BY etc).
//!
//! Module layout:
//! - [`ctx`]      — `ResolverCtx` mutable scope state + low-level binders
//! - [`ops`]      — top-level `QueryOp` dispatch
//! - [`patterns`]  — graph patterns (Node/Relationship/Path)
//! - [`mutations`] — write-side operations (CREATE/MERGE/SET/REMOVE/DELETE)
//! - [`exprs`]    — expressions (WHERE) and projections (RETURN)

mod ctx;
mod exprs;
mod mutations;
mod ops;
mod patterns;

use serde::{Deserialize, Serialize};

use crate::ontology_ir::OntologyIR;
use crate::query_ir::QueryIR;

use ctx::ResolverCtx;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResolvedQueryBindings {
    pub node_bindings: Vec<NodeBinding>,
    pub edge_bindings: Vec<EdgeBinding>,
    pub property_bindings: Vec<PropertyBinding>,
}

/// Which kind of query operation produced this binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Match,
    PathFind,
    Chain,
    Exists,
    Mutation,
}

/// Scope path segment for nested query constructs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ScopeSegment {
    Root,
    UnionBranch { index: usize },
    ExistsSubquery { depth: usize },
    ChainStep { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeBinding {
    pub variable: String,
    pub node_id: String,
    pub label: String,
    pub binding_kind: BindingKind,
    pub pattern_index: usize,
    pub scope_path: Vec<ScopeSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeBinding {
    pub variable: Option<String>,
    pub edge_id: String,
    pub label: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub binding_kind: BindingKind,
    pub pattern_index: usize,
    pub scope_path: Vec<ScopeSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyBinding {
    pub owner_variable: Option<String>,
    pub property_name: String,
    pub property_id: String,
    pub owner_id: String,
    pub binding_kind: BindingKind,
    pub scope_path: Vec<ScopeSegment>,
    /// AST location hint for UI disambiguation (e.g. "filter", "projection", "order_by").
    pub usage_hint: PropertyUsageHint,
}

/// Where in the AST a property reference was encountered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyUsageHint {
    PatternFilter,
    WhereFilter,
    Projection,
    OrderBy,
    GroupBy,
    Aggregation,
    Mutation,
    General,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Resolve all variable bindings in a QueryIR against an OntologyIR.
///
/// Walks patterns, filters, projections, and sub-queries to extract which
/// ontology nodes/edges/properties are referenced. Scopes are isolated for
/// UNION branches and EXISTS sub-queries to prevent variable leakage.
pub fn resolve_query_bindings(query: &QueryIR, ontology: &OntologyIR) -> ResolvedQueryBindings {
    let mut ctx = ResolverCtx::new(ontology);
    ctx.resolve_op(&query.operation);

    // Also resolve ORDER BY projections at the top level
    let prev_hint = ctx.usage_hint;
    ctx.usage_hint = PropertyUsageHint::OrderBy;
    for clause in &query.order_by {
        ctx.resolve_projection(&clause.projection);
    }
    ctx.usage_hint = prev_hint;

    ctx.into_bindings()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_label::GraphLabel;
    use crate::i18n::LocalizedText;
    use crate::ontology_ir::*;
    use crate::property_key::PropertyKey;
    use crate::query_ir::*;
    use crate::types::{Direction, PropertyType, PropertyValue};
    use crate::variable_name::VariableName;

    fn vn(s: &'static str) -> VariableName {
        VariableName::new(s).expect("test variable name literal must be valid")
    }

    fn test_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont1".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: GraphLabel::new("Person").expect("Person is a valid label"),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p1".into(),
                        name: PropertyKey::new("name").expect("name is valid"),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: GraphLabel::new("Company").expect("Company is a valid label"),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p2".into(),
                        name: PropertyKey::new("title").expect("title is valid"),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: GraphLabel::new("WORKS_AT").expect("WORKS_AT is valid"),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
                ..Default::default()
            }],
            vec![],
        )
    }

    #[test]
    fn test_match_pattern_bindings() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: vn("p"),
                        label: Some("Person".into()),
                        property_filters: vec![],
                    },
                    GraphPattern::Relationship {
                        variable: Some(vn("r")),
                        label: Some("WORKS_AT".into()),
                        source: vn("p"),
                        target: vn("c"),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: None,
                    },
                    GraphPattern::Node {
                        variable: vn("c"),
                        label: Some("Company".into()),
                        property_filters: vec![],
                    },
                ],
                filter: None,
                projections: vec![Projection::Field {
                    variable: vn("p"),
                    field: "name".into(),
                    alias: None,
                }],
                optional: false,
                group_by: vec![],
            },
            limit: Some(10),
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        assert_eq!(bindings.node_bindings.len(), 2);
        let p_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "p")
            .unwrap();
        assert_eq!(p_bind.node_id, "n1");
        assert_eq!(p_bind.binding_kind, BindingKind::Match);
        assert_eq!(p_bind.pattern_index, 0);
        assert_eq!(p_bind.scope_path, vec![ScopeSegment::Root]);

        let c_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "c")
            .unwrap();
        assert_eq!(c_bind.node_id, "n2");
        assert_eq!(c_bind.binding_kind, BindingKind::Match);
        assert_eq!(c_bind.pattern_index, 2);

        assert_eq!(bindings.edge_bindings.len(), 1);
        let eb = &bindings.edge_bindings[0];
        assert_eq!(eb.variable.as_deref(), Some("r"));
        assert_eq!(eb.edge_id, "e1");
        assert_eq!(eb.binding_kind, BindingKind::Match);
        assert_eq!(eb.pattern_index, 1);

        assert_eq!(bindings.property_bindings.len(), 1);
        assert_eq!(bindings.property_bindings[0].property_name, "name");
        assert_eq!(bindings.property_bindings[0].property_id, "p1");
        assert_eq!(
            bindings.property_bindings[0].binding_kind,
            BindingKind::Match
        );
        assert_eq!(
            bindings.property_bindings[0].usage_hint,
            PropertyUsageHint::Projection
        );
    }

    #[test]
    fn test_filter_property_bindings() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p"),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: vn("p"),
                        field: Some("name".into()),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("Alice".into()),
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);
        assert_eq!(bindings.property_bindings.len(), 1);
        assert_eq!(bindings.property_bindings[0].property_name, "name");
        assert_eq!(
            bindings.property_bindings[0].usage_hint,
            PropertyUsageHint::WhereFilter
        );
    }

    #[test]
    fn test_unknown_label_ignored() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("x"),
                    label: Some("UnknownType".into()),
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
        };

        let bindings = resolve_query_bindings(&query, &ontology);
        assert!(bindings.node_bindings.is_empty());
    }

    #[test]
    fn test_exists_subquery_scope_isolation() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p"),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Exists {
                    pattern: Box::new(GraphPattern::Relationship {
                        variable: Some(vn("r")),
                        label: Some("WORKS_AT".into()),
                        source: vn("p"),
                        target: vn("c"),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: None,
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        let p_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "p")
            .unwrap();
        assert_eq!(p_bind.binding_kind, BindingKind::Match);
        assert_eq!(p_bind.pattern_index, 0);
        assert_eq!(p_bind.scope_path, vec![ScopeSegment::Root]);

        assert_eq!(bindings.edge_bindings.len(), 1);
        let eb = &bindings.edge_bindings[0];
        assert_eq!(eb.binding_kind, BindingKind::Exists);
        assert_eq!(eb.pattern_index, 0);
        assert_eq!(
            eb.scope_path,
            vec![
                ScopeSegment::Root,
                ScopeSegment::ExistsSubquery { depth: 1 },
            ]
        );
    }

    #[test]
    fn test_union_branch_scope_isolation() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Union {
                queries: vec![
                    QueryIR {
                        schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("x"),
                                label: Some("Person".into()),
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![Projection::Field {
                                variable: vn("x"),
                                field: "name".into(),
                                alias: None,
                            }],
                            optional: false,
                            group_by: vec![],
                        },
                        limit: None,
                        skip: None,
                        order_by: vec![],
                    },
                    QueryIR {
                        schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("x"),
                                label: Some("Company".into()),
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![Projection::Field {
                                variable: vn("x"),
                                field: "title".into(),
                                alias: None,
                            }],
                            optional: false,
                            group_by: vec![],
                        },
                        limit: None,
                        skip: None,
                        order_by: vec![],
                    },
                ],
                all: false,
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        assert_eq!(bindings.node_bindings.len(), 2);

        let branch0 = bindings
            .node_bindings
            .iter()
            .find(|b| b.node_id == "n1")
            .unwrap();
        assert_eq!(branch0.variable, "x");
        assert!(
            branch0
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 0 })
        );

        let branch1 = bindings
            .node_bindings
            .iter()
            .find(|b| b.node_id == "n2")
            .unwrap();
        assert_eq!(branch1.variable, "x");
        assert!(
            branch1
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 1 })
        );

        assert_eq!(bindings.property_bindings.len(), 2);

        let name_bind = bindings
            .property_bindings
            .iter()
            .find(|b| b.property_name == "name")
            .unwrap();
        assert_eq!(name_bind.property_id, "p1");
        assert!(
            name_bind
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 0 })
        );

        let title_bind = bindings
            .property_bindings
            .iter()
            .find(|b| b.property_name == "title")
            .unwrap();
        assert_eq!(title_bind.property_id, "p2");
        assert!(
            title_bind
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 1 })
        );
    }

    #[test]
    fn test_property_multi_use_not_deduped() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p"),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: vn("p"),
                        field: Some("name".into()),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("Alice".into()),
                    }),
                }),
                projections: vec![Projection::Field {
                    variable: vn("p"),
                    field: "name".into(),
                    alias: None,
                }],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        assert_eq!(bindings.property_bindings.len(), 2);
        let hints: Vec<_> = bindings
            .property_bindings
            .iter()
            .map(|b| b.usage_hint)
            .collect();
        assert!(hints.contains(&PropertyUsageHint::WhereFilter));
        assert!(hints.contains(&PropertyUsageHint::Projection));
    }

    #[test]
    fn test_pathfind_binding_kind() {
        let ontology = test_ontology();
        let query = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::PathFind {
                start: NodeRef {
                    variable: vn("s"),
                    label: Some("Person".into()),
                    property_filters: vec![],
                },
                end: NodeRef {
                    variable: vn("e"),
                    label: Some("Company".into()),
                    property_filters: vec![],
                },
                edge_types: vec!["WORKS_AT".into()],
                direction: Direction::Outgoing,
                max_depth: Some(3),
                algorithm: PathAlgorithm::ShortestPath,
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        assert_eq!(bindings.node_bindings.len(), 2);
        for nb in &bindings.node_bindings {
            assert_eq!(nb.binding_kind, BindingKind::PathFind);
        }

        assert_eq!(bindings.edge_bindings.len(), 1);
        assert_eq!(
            bindings.edge_bindings[0].binding_kind,
            BindingKind::PathFind
        );
        assert_eq!(bindings.edge_bindings[0].edge_id, "e1");
    }

    /// Nested EXISTS regression test: a CallSubquery containing an EXISTS
    /// must produce two distinct depths (1 and 2). The pre-fix bug
    /// collapsed both onto `depth: 1` because CallSubquery did not bump
    /// `exists_depth`.
    #[test]
    fn nested_subquery_scope_paths_have_distinct_depths() {
        let ontology = test_ontology();
        let inner = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p2"),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Exists {
                    pattern: Box::new(GraphPattern::Relationship {
                        variable: Some(vn("r")),
                        label: Some("WORKS_AT".into()),
                        source: vn("p2"),
                        target: vn("c"),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: None,
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };
        let outer = QueryIR {
            schema_version: crate::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::CallSubquery {
                inner: Box::new(inner),
                import_variables: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&outer, &ontology);

        // p2 is bound inside the CallSubquery (depth 1).
        let p2_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "p2")
            .unwrap();
        assert!(
            p2_bind
                .scope_path
                .contains(&ScopeSegment::ExistsSubquery { depth: 1 })
        );

        // The relationship is inside the EXISTS *inside* the CallSubquery.
        // Its scope path must include depth 2 — proving the counter bumped.
        assert_eq!(bindings.edge_bindings.len(), 1);
        let edge = &bindings.edge_bindings[0];
        assert!(
            edge.scope_path
                .contains(&ScopeSegment::ExistsSubquery { depth: 2 }),
            "expected depth: 2 inside nested EXISTS, got {:?}",
            edge.scope_path
        );
    }
}

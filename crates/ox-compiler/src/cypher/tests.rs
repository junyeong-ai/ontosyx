use super::CypherCompiler;
use crate::GraphCompiler;

use ox_core::load_plan::PropertyMapping;
use ox_core::load_plan::{ConflictStrategy, LoadOp, LoadPlan, LoadStep};
use ox_core::ontology_ir::*;
use ox_core::query_ir::*;
use ox_core::types::*;

#[test]
fn test_compile_simple_match() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Product".to_string()),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: "n".to_string(),
                    field: Some("price".to_string()),
                }),
                op: ComparisonOp::Gt,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Int(1000),
                }),
            }),
            projections: vec![
                Projection::Field {
                    variable: "n".to_string(),
                    field: "name".to_string(),
                    alias: None,
                },
                Projection::Field {
                    variable: "n".to_string(),
                    field: "price".to_string(),
                    alias: None,
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: Some(10),
        skip: None,
        order_by: vec![OrderClause {
            projection: Projection::Field {
                variable: "n".to_string(),
                field: "price".to_string(),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let result = &compiled.statement;
    assert!(result.contains("MATCH (n:`Product`)"));
    // Value 1000 should be parameterized
    assert!(result.contains("WHERE n.`price` > $p0"), "got: {result}");
    assert_eq!(compiled.params.get("p0"), Some(&PropertyValue::Int(1000)));
    assert!(result.contains("RETURN n.`name`, n.`price`"));
    assert!(result.contains("ORDER BY n.`price` DESC"));
    assert!(result.contains("LIMIT 10"));
}

#[test]
fn test_compile_relationship_pattern() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Relationship {
                variable: Some("r".to_string()),
                label: Some("PURCHASED".to_string()),
                source: "c".to_string(),
                target: "p".to_string(),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            }],
            filter: None,
            projections: vec![
                Projection::Variable {
                    variable: "c".to_string(),
                    alias: None,
                },
                Projection::Variable {
                    variable: "p".to_string(),
                    alias: None,
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    assert!(compiled.statement.contains("(c)-[r:`PURCHASED`]->(p)"));
    assert!(compiled.params.is_empty());
}

#[test]
fn test_compile_schema_constraints() {
    let compiler = CypherCompiler;
    let ontology = OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        None,
        1,
        vec![NodeTypeDef {
            id: "node-product".into(),
            label: "Product".to_string(),
            description: None,
            source_table: None,
            properties: vec![
                PropertyDef {
                    id: "prop-sku".into(),
                    name: "sku".to_string(),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: None,
                },
                PropertyDef {
                    id: "prop-name".into(),
                    name: "name".to_string(),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: None,
                },
            ],
            constraints: vec![ConstraintDef {
                id: "cst-1".into(),
                constraint: NodeConstraint::Unique {
                    property_ids: vec!["prop-sku".into()],
                },
            }],
        }],
        vec![],
        vec![],
    );

    let result = compiler.compile_schema(&ontology).unwrap();
    assert!(
        result
            .iter()
            .any(|s| s.contains("REQUIRE (n.`sku`) IS UNIQUE"))
    );
}

#[test]
fn test_compile_merge_node() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Mutate {
            context: None,
            operations: vec![MutateOp::MergeNode {
                variable: "p".to_string(),
                label: "Product".to_string(),
                match_properties: vec![PropertyAssignment {
                    property: "sku".to_string(),
                    value: Expr::Literal {
                        value: PropertyValue::String("ABC123".to_string()),
                    },
                }],
                on_create: vec![PropertyAssignment {
                    property: "name".to_string(),
                    value: Expr::Literal {
                        value: PropertyValue::String("Widget".to_string()),
                    },
                }],
                on_match: vec![],
            }],
            returning: vec![Projection::Variable {
                variable: "p".to_string(),
                alias: None,
            }],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let result = &compiled.statement;
    // String values should be parameterized
    assert!(
        result.contains("MERGE (p:`Product` {`sku`: $p0})"),
        "got: {result}"
    );
    assert!(
        result.contains("ON CREATE SET p.`name` = $p1"),
        "got: {result}"
    );
    assert_eq!(
        compiled.params.get("p0"),
        Some(&PropertyValue::String("ABC123".to_string()))
    );
    assert_eq!(
        compiled.params.get("p1"),
        Some(&PropertyValue::String("Widget".to_string()))
    );
    assert!(result.contains("RETURN p"));
}

#[test]
fn test_compile_load_plan() {
    let compiler = CypherCompiler;
    let plan = LoadPlan {
        id: "test-load".to_string(),
        ontology_id: "test".to_string(),
        ontology_version: 1,
        source: ox_core::load_plan::DataSourceSpec::Csv {
            delimiter: ',',
            has_header: true,
            columns: vec![],
        },
        steps: vec![LoadStep {
            order: 0,
            depends_on: vec![],
            operation: LoadOp::UpsertNode {
                target_label: "Product".to_string(),
                match_fields: vec![PropertyMapping {
                    source_column: "sku".to_string(),
                    graph_property: "sku".to_string(),
                    transform: None,
                }],
                set_fields: vec![PropertyMapping {
                    source_column: "name".to_string(),
                    graph_property: "name".to_string(),
                    transform: None,
                }],
                on_conflict: ConflictStrategy::Update,
            },
            description: "Load products".to_string(),
        }],
        batch_config: ox_core::load_plan::BatchConfig::default(),
    };

    let result = compiler.compile_load(&plan).unwrap();
    assert_eq!(result.len(), 1);
    assert!(
        !result[0].contains("UNWIND"),
        "should not use UNWIND for per-record execution: {}",
        result[0]
    );
    assert!(
        result[0].contains("MERGE (n:`Product` {`sku`: $row_sku})"),
        "got: {}",
        result[0]
    );
}

#[test]
fn test_parameterization_string_values() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Person".to_string()),
                property_filters: vec![PropertyFilter {
                    property: "name".to_string(),
                    value: Expr::Literal {
                        value: PropertyValue::String("Alice".to_string()),
                    },
                }],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: "n".to_string(),
                    field: Some("city".to_string()),
                }),
                op: ox_core::query_ir::ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::String("Seoul".to_string()),
                }),
            }),
            projections: vec![Projection::Variable {
                variable: "n".to_string(),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    // String values must become $pN params, never inline quotes
    assert!(
        compiled.statement.contains("$p0"),
        "inline property filter should be parameterized: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("$p1"),
        "WHERE filter string should be parameterized: {}",
        compiled.statement
    );
    assert!(
        !compiled.statement.contains("'Alice'") && !compiled.statement.contains("\"Alice\""),
        "string literal must not appear inline: {}",
        compiled.statement
    );
    assert_eq!(
        compiled.params.get("p0"),
        Some(&PropertyValue::String("Alice".to_string()))
    );
    assert_eq!(
        compiled.params.get("p1"),
        Some(&PropertyValue::String("Seoul".to_string()))
    );
}

#[test]
fn test_parameterization_in_clause() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Product".to_string()),
                property_filters: vec![],
            }],
            filter: Some(Expr::In {
                expr: Box::new(Expr::Property {
                    variable: "n".to_string(),
                    field: Some("status".to_string()),
                }),
                values: vec![
                    PropertyValue::String("active".to_string()),
                    PropertyValue::String("pending".to_string()),
                    PropertyValue::Int(42),
                ],
            }),
            projections: vec![Projection::Variable {
                variable: "n".to_string(),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    // All three IN-clause values must be parameterized
    assert!(
        compiled.statement.contains("$p0"),
        "got: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("$p1"),
        "got: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("$p2"),
        "got: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("IN [$p0, $p1, $p2]"),
        "got: {}",
        compiled.statement
    );
    assert_eq!(compiled.params.len(), 3);
    assert_eq!(
        compiled.params.get("p0"),
        Some(&PropertyValue::String("active".to_string()))
    );
    assert_eq!(
        compiled.params.get("p1"),
        Some(&PropertyValue::String("pending".to_string()))
    );
    assert_eq!(compiled.params.get("p2"), Some(&PropertyValue::Int(42)));
}

#[test]
fn test_parameterization_null_stays_inline() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Product".to_string()),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: "n".to_string(),
                    field: Some("status".to_string()),
                }),
                op: ox_core::query_ir::ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Null,
                }),
            }),
            projections: vec![Projection::Variable {
                variable: "n".to_string(),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    // Null must stay inline as the `null` keyword, not parameterized
    assert!(
        compiled.statement.contains("null"),
        "null should appear inline: {}",
        compiled.statement
    );
    assert!(compiled.params.is_empty(), "null must not be in params");
}

#[test]
fn test_parameterization_date_values() {
    let compiler = CypherCompiler;
    let date_val = chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Event".to_string()),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: "n".to_string(),
                    field: Some("date".to_string()),
                }),
                op: ox_core::query_ir::ComparisonOp::Gte,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Date(date_val),
                }),
            }),
            projections: vec![Projection::Variable {
                variable: "n".to_string(),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    // Date values must be inline Cypher function calls (not parameterized)
    assert!(
        compiled.statement.contains("date('2025-06-15')"),
        "date should be inline: {}",
        compiled.statement
    );
    assert!(compiled.params.is_empty(), "date must not be in params");
}

#[test]
fn test_compile_aggregate_query() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "o".to_string(),
                label: Some("Order".to_string()),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![
                Projection::Field {
                    variable: "o".to_string(),
                    field: "status".to_string(),
                    alias: Some("status".to_string()),
                },
                Projection::Aggregation {
                    function: AggFunction::Count,
                    argument: Box::new(Projection::Variable {
                        variable: "o".to_string(),
                        alias: None,
                    }),
                    alias: "total".to_string(),
                    distinct: false,
                },
                Projection::Aggregation {
                    function: AggFunction::Sum,
                    argument: Box::new(Projection::Field {
                        variable: "o".to_string(),
                        field: "amount".to_string(),
                        alias: None,
                    }),
                    alias: "total_amount".to_string(),
                    distinct: false,
                },
            ],
            optional: false,
            group_by: vec![Projection::Field {
                variable: "o".to_string(),
                field: "status".to_string(),
                alias: None,
            }],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("MATCH (o:`Order`)"), "got: {stmt}");
    assert!(stmt.contains("count(o) AS total"), "got: {stmt}");
    assert!(
        stmt.contains("sum(o.`amount`) AS total_amount"),
        "got: {stmt}"
    );
}

#[test]
fn test_compile_union_query() {
    let compiler = CypherCompiler;
    let q1 = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Person".to_string()),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Field {
                variable: "n".to_string(),
                field: "name".to_string(),
                alias: Some("name".to_string()),
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };
    let q2 = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Company".to_string()),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Field {
                variable: "n".to_string(),
                field: "name".to_string(),
                alias: Some("name".to_string()),
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let union_query = QueryIR {
        operation: QueryOp::Union {
            queries: vec![q1, q2],
            all: true,
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&union_query).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("UNION ALL"), "got: {stmt}");
    assert!(stmt.contains("MATCH (n:`Person`)"), "got: {stmt}");
    assert!(stmt.contains("MATCH (n:`Company`)"), "got: {stmt}");
}

#[test]
fn test_compile_chain_with_pass_through() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Chain {
            steps: vec![
                ChainStep {
                    pass_through: vec![],
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Node {
                            variable: "c".to_string(),
                            label: Some("Customer".to_string()),
                            property_filters: vec![],
                        }],
                        filter: None,
                        projections: vec![Projection::Variable {
                            variable: "c".to_string(),
                            alias: None,
                        }],
                        optional: false,
                        group_by: vec![],
                    },
                },
                ChainStep {
                    pass_through: vec![Projection::Variable {
                        variable: "c".to_string(),
                        alias: None,
                    }],
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Relationship {
                            variable: Some("r".to_string()),
                            label: Some("PURCHASED".to_string()),
                            source: "c".to_string(),
                            target: "p".to_string(),
                            direction: Direction::Outgoing,
                            property_filters: vec![],
                            var_length: None,
                        }],
                        filter: None,
                        projections: vec![
                            Projection::Variable {
                                variable: "c".to_string(),
                                alias: None,
                            },
                            Projection::Variable {
                                variable: "p".to_string(),
                                alias: None,
                            },
                        ],
                        optional: false,
                        group_by: vec![],
                    },
                },
            ],
        },
        limit: Some(20),
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("WITH c"), "WITH clause expected: {stmt}");
    assert!(stmt.contains("MATCH (c:`Customer`)"), "got: {stmt}");
    assert!(stmt.contains("(c)-[r:`PURCHASED`]->(p)"), "got: {stmt}");
    assert!(stmt.contains("LIMIT 20"), "got: {stmt}");
}

#[test]
fn test_compile_load_edge_upsert() {
    use ox_core::load_plan::{BatchConfig, DataSourceSpec, NodeMatch, PropertyMapping};

    let compiler = CypherCompiler;
    let plan = LoadPlan {
        id: "test-edge-load".to_string(),
        ontology_id: "test".to_string(),
        ontology_version: 1,
        source: DataSourceSpec::Csv {
            delimiter: ',',
            has_header: true,
            columns: vec![],
        },
        steps: vec![LoadStep {
            order: 0,
            depends_on: vec![],
            operation: LoadOp::UpsertEdge {
                target_label: "PURCHASED".to_string(),
                source_match: NodeMatch {
                    label: "Customer".to_string(),
                    match_property: "id".to_string(),
                    source_field: "customer_id".to_string(),
                },
                target_match: NodeMatch {
                    label: "Product".to_string(),
                    match_property: "sku".to_string(),
                    source_field: "product_sku".to_string(),
                },
                set_fields: vec![PropertyMapping {
                    source_column: "quantity".to_string(),
                    graph_property: "quantity".to_string(),
                    transform: None,
                }],
                on_conflict: ConflictStrategy::Update,
            },
            description: "Load purchases".to_string(),
        }],
        batch_config: BatchConfig::default(),
    };

    let result = compiler.compile_load(&plan).unwrap();
    assert_eq!(result.len(), 1);
    let stmt = &result[0];
    assert!(!stmt.contains("UNWIND"), "should not use UNWIND: {stmt}");
    assert!(
        stmt.contains("MATCH (a:`Customer` {`id`: $row_customer_id})"),
        "got: {stmt}"
    );
    assert!(
        stmt.contains("MATCH (b:`Product` {`sku`: $row_product_sku})"),
        "got: {stmt}"
    );
    assert!(
        stmt.contains("MERGE (a)-[r:`PURCHASED`]->(b)"),
        "got: {stmt}"
    );
    assert!(stmt.contains("ON CREATE SET"), "got: {stmt}");
    assert!(stmt.contains("ON MATCH SET"), "got: {stmt}");
    assert!(stmt.contains("r.`quantity` = $row_quantity"), "got: {stmt}");
}

#[test]
fn test_compile_load_merge_non_null() {
    use ox_core::load_plan::{BatchConfig, DataSourceSpec, PropertyMapping};

    let compiler = CypherCompiler;
    let plan = LoadPlan {
        id: "test-merge-nonnull".to_string(),
        ontology_id: "test".to_string(),
        ontology_version: 1,
        source: DataSourceSpec::Csv {
            delimiter: ',',
            has_header: true,
            columns: vec![],
        },
        steps: vec![LoadStep {
            order: 0,
            depends_on: vec![],
            operation: LoadOp::UpsertNode {
                target_label: "Customer".to_string(),
                match_fields: vec![PropertyMapping {
                    source_column: "id".to_string(),
                    graph_property: "id".to_string(),
                    transform: None,
                }],
                set_fields: vec![
                    PropertyMapping {
                        source_column: "name".to_string(),
                        graph_property: "name".to_string(),
                        transform: None,
                    },
                    PropertyMapping {
                        source_column: "email".to_string(),
                        graph_property: "email".to_string(),
                        transform: None,
                    },
                ],
                on_conflict: ConflictStrategy::MergeNonNull,
            },
            description: "Merge customers".to_string(),
        }],
        batch_config: BatchConfig::default(),
    };

    let result = compiler.compile_load(&plan).unwrap();
    assert_eq!(result.len(), 1);
    let stmt = &result[0];
    assert!(!stmt.contains("UNWIND"), "should not use UNWIND: {stmt}");
    assert!(
        stmt.contains("MERGE (n:`Customer` {`id`: $row_id})"),
        "got: {stmt}"
    );
    // ON CREATE should use direct assignment
    assert!(stmt.contains("ON CREATE SET"), "got: {stmt}");
    // ON MATCH should use COALESCE for non-null merge
    assert!(
        stmt.contains("COALESCE($row_name, n.`name`)"),
        "got: {stmt}"
    );
    assert!(
        stmt.contains("COALESCE($row_email, n.`email`)"),
        "got: {stmt}"
    );
}

// ---------------------------------------------------------------------------
// Subquery tests
// ---------------------------------------------------------------------------

#[test]
fn test_call_subquery_compilation() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Chain {
            steps: vec![
                ChainStep {
                    pass_through: vec![],
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Node {
                            variable: "n".to_string(),
                            label: Some("Person".to_string()),
                            property_filters: vec![],
                        }],
                        filter: None,
                        projections: vec![Projection::Variable {
                            variable: "n".to_string(),
                            alias: None,
                        }],
                        optional: false,
                        group_by: vec![],
                    },
                },
                ChainStep {
                    pass_through: vec![Projection::Variable {
                        variable: "n".to_string(),
                        alias: None,
                    }],
                    operation: QueryOp::CallSubquery {
                        inner: Box::new(QueryIR {
                            operation: QueryOp::Match {
                                patterns: vec![GraphPattern::Relationship {
                                    variable: None,
                                    label: None,
                                    source: "n".to_string(),
                                    target: "m".to_string(),
                                    direction: Direction::Outgoing,
                                    property_filters: vec![],
                                    var_length: None,
                                }],
                                filter: None,
                                projections: vec![Projection::Aggregation {
                                    function: AggFunction::Count,
                                    argument: Box::new(Projection::Variable {
                                        variable: "m".to_string(),
                                        alias: None,
                                    }),
                                    alias: "neighbor_count".to_string(),
                                    distinct: false,
                                }],
                                optional: false,
                                group_by: vec![],
                            },
                            limit: None,
                            skip: None,
                            order_by: vec![],
                        }),
                        import_variables: vec!["n".to_string()],
                    },
                },
            ],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("CALL {"), "should contain CALL block: {stmt}");
    assert!(stmt.contains("WITH n"), "should import n: {stmt}");
    assert!(
        stmt.contains("(n)-[]->(m)"),
        "should match neighbors: {stmt}"
    );
    assert!(
        stmt.contains("count(m) AS neighbor_count"),
        "should count neighbors: {stmt}"
    );
}

#[test]
fn test_subquery_expr_count() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Person".to_string()),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![
                Projection::Variable {
                    variable: "n".to_string(),
                    alias: None,
                },
                Projection::Expression {
                    expr: Expr::Subquery {
                        query: Box::new(QueryIR {
                            operation: QueryOp::Match {
                                patterns: vec![GraphPattern::Relationship {
                                    variable: None,
                                    label: Some("KNOWS".to_string()),
                                    source: "n".to_string(),
                                    target: "friend".to_string(),
                                    direction: Direction::Outgoing,
                                    property_filters: vec![],
                                    var_length: None,
                                }],
                                filter: None,
                                projections: vec![Projection::Variable {
                                    variable: "friend".to_string(),
                                    alias: None,
                                }],
                                optional: false,
                                group_by: vec![],
                            },
                            limit: None,
                            skip: None,
                            order_by: vec![],
                        }),
                        import_variables: vec!["n".to_string()],
                    },
                    alias: "friend_count".to_string(),
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let stmt = &compiled.statement;
    assert!(
        stmt.contains("COUNT {"),
        "should use COUNT subquery: {stmt}"
    );
    assert!(
        stmt.contains("WITH n"),
        "should import variables: {stmt}"
    );
    assert!(
        stmt.contains("AS friend_count"),
        "should alias result: {stmt}"
    );
}

#[test]
fn test_call_subquery_standalone() {
    // Test CallSubquery as a top-level operation
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::CallSubquery {
            inner: Box::new(QueryIR {
                operation: QueryOp::Match {
                    patterns: vec![GraphPattern::Node {
                        variable: "x".to_string(),
                        label: Some("Task".to_string()),
                        property_filters: vec![],
                    }],
                    filter: None,
                    projections: vec![Projection::Aggregation {
                        function: AggFunction::Count,
                        argument: Box::new(Projection::Variable {
                            variable: "x".to_string(),
                            alias: None,
                        }),
                        alias: "task_count".to_string(),
                        distinct: false,
                    }],
                    optional: false,
                    group_by: vec![],
                },
                limit: None,
                skip: None,
                order_by: vec![],
            }),
            import_variables: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("CALL {"), "should contain CALL block: {stmt}");
    assert!(
        stmt.contains("MATCH (x:`Task`)"),
        "should match tasks: {stmt}"
    );
    assert!(
        stmt.contains("count(x) AS task_count"),
        "should count: {stmt}"
    );
    // No WITH since import_variables is empty
    assert!(
        !stmt.contains("WITH"),
        "should not have WITH when no imports: {stmt}"
    );
}

#[test]
fn test_collect_list_aggregation() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Relationship {
                variable: None,
                label: Some("TAGGED".to_string()),
                source: "p".to_string(),
                target: "t".to_string(),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            }],
            filter: None,
            projections: vec![
                Projection::Variable {
                    variable: "p".to_string(),
                    alias: None,
                },
                Projection::Aggregation {
                    function: AggFunction::CollectList,
                    argument: Box::new(Projection::Field {
                        variable: "t".to_string(),
                        field: "name".to_string(),
                        alias: None,
                    }),
                    alias: "tags".to_string(),
                    distinct: false,
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    let stmt = &compiled.statement;
    assert!(
        stmt.contains("collect(t.`name`) AS tags"),
        "should use collect() for CollectList: {stmt}"
    );
}

// ---------------------------------------------------------------------------
// PathFind tests
// ---------------------------------------------------------------------------

#[test]
fn test_compile_shortest_path() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::PathFind {
            start: NodeRef {
                variable: "a".to_string(),
                label: Some("Person".to_string()),
                property_filters: vec![PropertyFilter {
                    property: "name".to_string(),
                    value: Expr::Literal {
                        value: PropertyValue::String("Alice".to_string()),
                    },
                }],
            },
            end: NodeRef {
                variable: "b".to_string(),
                label: Some("Person".to_string()),
                property_filters: vec![PropertyFilter {
                    property: "name".to_string(),
                    value: Expr::Literal {
                        value: PropertyValue::String("Bob".to_string()),
                    },
                }],
            },
            edge_types: vec![],
            direction: Direction::Outgoing,
            max_depth: None,
            algorithm: PathAlgorithm::ShortestPath,
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    assert!(
        compiled.statement.contains("shortestPath("),
        "got: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("RETURN p"),
        "got: {}",
        compiled.statement
    );
    // Property filters should be parameterized
    assert!(compiled.params.len() >= 2);
}

#[test]
fn test_compile_all_shortest_paths() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::PathFind {
            start: NodeRef {
                variable: "a".to_string(),
                label: Some("City".to_string()),
                property_filters: vec![],
            },
            end: NodeRef {
                variable: "b".to_string(),
                label: Some("City".to_string()),
                property_filters: vec![],
            },
            edge_types: vec!["ROAD".to_string()],
            direction: Direction::Both,
            max_depth: Some(10),
            algorithm: PathAlgorithm::AllShortestPaths,
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    assert!(
        compiled.statement.contains("allShortestPaths("),
        "got: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("`ROAD`"),
        "edge type should be escaped: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("*..10"),
        "max_depth should appear: {}",
        compiled.statement
    );
}

#[test]
fn test_compile_all_paths_variable_length() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::PathFind {
            start: NodeRef {
                variable: "a".to_string(),
                label: Some("Node".to_string()),
                property_filters: vec![],
            },
            end: NodeRef {
                variable: "b".to_string(),
                label: Some("Node".to_string()),
                property_filters: vec![],
            },
            edge_types: vec!["CONNECTS".to_string(), "LINKS".to_string()],
            direction: Direction::Outgoing,
            max_depth: Some(5),
            algorithm: PathAlgorithm::AllPaths,
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    // AllPaths should NOT use shortestPath/allShortestPaths functions
    assert!(
        !compiled.statement.contains("shortestPath"),
        "AllPaths should not use shortestPath function: {}",
        compiled.statement
    );
    // Should use variable-length pattern
    assert!(
        compiled.statement.contains("*..5"),
        "should have depth limit: {}",
        compiled.statement
    );
    // Should have piped edge types
    assert!(
        compiled.statement.contains("`CONNECTS`|`LINKS`"),
        "edge types should be piped: {}",
        compiled.statement
    );
    assert!(
        compiled.statement.contains("RETURN p"),
        "got: {}",
        compiled.statement
    );
}

#[test]
fn test_compile_case_expression() {
    let compiler = CypherCompiler;
    let query = QueryIR {
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "n".to_string(),
                label: Some("Product".to_string()),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Expression {
                expr: Expr::Case {
                    operand: None,
                    when_clauses: vec![WhenClause {
                        condition: Expr::Comparison {
                            left: Box::new(Expr::Property {
                                variable: "n".to_string(),
                                field: Some("price".to_string()),
                            }),
                            op: ComparisonOp::Gt,
                            right: Box::new(Expr::Literal {
                                value: PropertyValue::Int(100),
                            }),
                        },
                        result: Expr::Literal {
                            value: PropertyValue::String("expensive".to_string()),
                        },
                    }],
                    else_result: Some(Box::new(Expr::Literal {
                        value: PropertyValue::String("cheap".to_string()),
                    })),
                },
                alias: "category".to_string(),
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
    };

    let compiled = compiler.compile_query(&query).unwrap();
    assert!(compiled.statement.contains("CASE"));
    assert!(compiled.statement.contains("WHEN"));
    assert!(compiled.statement.contains("THEN"));
    assert!(compiled.statement.contains("ELSE"));
    assert!(compiled.statement.contains("END"));
    assert!(compiled.statement.contains("AS category"));
    // Values should be parameterized
    assert!(!compiled.params.is_empty());
}

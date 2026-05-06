use super::CypherCompiler;
use crate::GraphCompiler;

use ox_core::GraphLabel;
use ox_core::LocalizedText;
use ox_core::PropertyKey;
use ox_ontology::load_plan::PropertyMapping;
use ox_ontology::load_plan::{ConflictStrategy, LoadMode, LoadOp, LoadPlan, LoadStep};
use ox_ontology::ir::*;
use ox_query_ir::query::*;
use ox_core::types::*;

fn gl(s: &'static str) -> GraphLabel {
    GraphLabel::new(s).expect("test label literal must be valid")
}

fn vn(s: &'static str) -> ox_core::VariableName {
    ox_core::VariableName::new(s).expect("test variable name literal must be valid")
}

fn pk(s: &'static str) -> PropertyKey {
    PropertyKey::new(s).expect("test property name literal must be valid")
}
#[test]
fn test_compile_simple_match() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Product")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("n"),
                    field: Some(pk("price")),
                }),
                op: ComparisonOp::Gt,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Int(1000),
                }),
            }),
            projections: vec![
                Projection::Field {
                    variable: vn("n"),
                    field: pk("name"),
                    alias: None,
                },
                Projection::Field {
                    variable: vn("n"),
                    field: pk("price"),
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
                variable: vn("n"),
                field: pk("price"),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Relationship {
                variable: Some(vn("r")),
                label: Some(gl("PURCHASED")),
                source: vn("c"),
                target: vn("p"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            }],
            filter: None,
            projections: vec![
                Projection::Variable {
                    variable: vn("c"),
                    alias: None,
                },
                Projection::Variable {
                    variable: vn("p"),
                    alias: None,
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    assert!(compiled.statement.contains("(c)-[r:`PURCHASED`]->(p)"));
    assert!(compiled.params.is_empty());
}

#[test]
fn test_compile_schema_constraints() {
    let compiler = CypherCompiler::neo4j();
    let ontology = OntologyIR::new(
        "test".to_string(),
        "Test".to_string(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-product".into(),
            label: gl("Product"),
            description: LocalizedText::default(),
            properties: vec![
                PropertyDef {
                    id: "prop-sku".into(),
                    name: pk("sku"),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    ..Default::default()
                },
                PropertyDef {
                    id: "prop-name".into(),
                    name: pk("name"),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    ..Default::default()
                },
            ],
            constraints: vec![ConstraintDef {
                id: "cst-1".into(),
                constraint: NodeConstraint::Unique {
                    property_ids: vec!["prop-sku".into()],
                },
            }],
            ..Default::default()
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

// ---------------------------------------------------------------------------
// Korean label round-trip
//
// Verifies that every Cypher emission path (constraints, indexes, match
// patterns, edge types) correctly backtick-escapes Korean identifiers.
// These tests are the MVP gate: if any of them fail, the whole Korean
// domain story is broken at the compiler layer.
// ---------------------------------------------------------------------------

#[test]
fn test_korean_ontology_compiles_schema() {
    let compiler = CypherCompiler::neo4j();
    let ontology = ox_ontology::test_fixtures::korean_ecommerce_ontology();

    let statements = compiler.compile_schema(&ontology).unwrap();
    let joined = statements.join("\n");

    // Every Korean label must be backtick-wrapped, never raw
    for label in [
        "고객",
        "주문",
        "상품",
        "카테고리",
        "리뷰",
        "배송",
        "결제수단",
    ] {
        let raw_colon = format!(":{label}");
        let escaped = format!(":`{label}`");
        // Raw `:Korean` (without backticks) must not appear
        assert!(
            !joined.contains(&raw_colon) || joined.contains(&escaped),
            "unescaped Korean label `{label}` leaked into Cypher: {joined}"
        );
    }

    // Primary-key unique constraints must be emitted for each Korean node
    for (label, pk) in [
        ("고객", "고객번호"),
        ("주문", "주문번호"),
        ("상품", "상품번호"),
        ("카테고리", "카테고리번호"),
        ("리뷰", "리뷰번호"),
        ("배송", "배송번호"),
        ("결제수단", "결제번호"),
    ] {
        let expected = format!(
            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:`{label}`) REQUIRE (n.`{pk}`) IS UNIQUE"
        );
        assert!(
            statements.contains(&expected),
            "missing constraint for {label}.{pk}; got:\n{joined}"
        );
    }

    // Explicit single index on 상품.상품명
    assert!(
        statements
            .iter()
            .any(|s| s == "CREATE INDEX IF NOT EXISTS FOR (n:`상품`) ON (n.`상품명`)"),
        "missing explicit index on 상품.상품명; got:\n{joined}"
    );
}

#[test]
fn test_korean_ontology_match_query_escapes_labels() {
    use ox_query_ir::query::*;

    let compiler = CypherCompiler::neo4j();
    // MATCH (c:고객)-[r:주문함]->(o:주문) WHERE c.이름 = '홍길동' RETURN o.주문번호
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![
                GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("고객")),
                    property_filters: vec![],
                },
                GraphPattern::Relationship {
                    variable: Some(vn("r")),
                    label: Some(gl("주문함")),
                    source: vn("c"),
                    target: vn("o"),
                    direction: Direction::Outgoing,
                    property_filters: vec![],
                    var_length: None,
                },
                GraphPattern::Node {
                    variable: vn("o"),
                    label: Some(gl("주문")),
                    property_filters: vec![],
                },
            ],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("c"),
                    field: Some(pk("이름")),
                }),
                op: ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::String("홍길동".to_string()),
                }),
            }),
            projections: vec![Projection::Field {
                variable: vn("o"),
                field: pk("주문번호"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: Some(10),
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    let stmt = &compiled.statement;

    assert!(stmt.contains("(c:`고객`)"), "got: {stmt}");
    assert!(stmt.contains("[r:`주문함`]"), "got: {stmt}");
    assert!(stmt.contains("(o:`주문`)"), "got: {stmt}");
    assert!(stmt.contains("c.`이름`"), "got: {stmt}");
    assert!(stmt.contains("o.`주문번호`"), "got: {stmt}");
    // Value should be parameterized, not inlined
    assert!(
        !stmt.contains("홍길동"),
        "Korean value leaked into query: {stmt}"
    );
    assert_eq!(
        compiled.params.get("p0"),
        Some(&PropertyValue::String("홍길동".to_string()))
    );
}

#[test]
fn test_korean_backtick_in_label_is_doubled() {
    // Neo4j escaping convention: backtick inside an identifier is doubled.
    // This ensures that even malicious labels with ` cannot inject.
    use ox_core::types::escape_cypher_identifier;
    assert_eq!(escape_cypher_identifier("고객"), "`고객`");
    assert_eq!(escape_cypher_identifier("has`tick"), "`has``tick`");
    assert_eq!(escape_cypher_identifier("한국`어"), "`한국``어`");
}

#[test]
fn test_korean_ontology_json_round_trip() {
    // An OntologyIR with Korean labels must serialize to JSON and
    // deserialize back to an equivalent structure (lookup indices rebuild).
    let original = ox_ontology::test_fixtures::korean_ecommerce_ontology();
    let json = serde_json::to_string(&original).expect("serialize");
    let round: ox_ontology::ir::OntologyIR = serde_json::from_str(&json).expect("deserialize");

    // Lookup indices must be functional post-deserialize
    assert_eq!(
        round.node_by_label("고객").map(|n| n.label.as_str()),
        Some("고객")
    );
    assert_eq!(
        round.node_by_label("주문").map(|n| n.label.as_str()),
        Some("주문")
    );
    assert!(
        round.neighbor_labels("고객").contains(&"주문"),
        "고객 should connect to 주문"
    );
    assert!(
        round.neighbor_labels("주문").contains(&"상품"),
        "주문 should connect to 상품 via 포함"
    );
}

#[test]
fn test_compile_merge_node() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Mutate {
            context: None,
            operations: vec![MutateOp::MergeNode {
                variable: vn("p"),
                label: gl("Product"),
                match_properties: vec![PropertyAssignment {
                    property: pk("sku"),
                    value: Expr::Literal {
                        value: PropertyValue::String("ABC123".to_string()),
                    },
                }],
                on_create: vec![PropertyAssignment {
                    property: pk("name"),
                    value: Expr::Literal {
                        value: PropertyValue::String("Widget".to_string()),
                    },
                }],
                on_match: vec![],
            }],
            returning: vec![Projection::Variable {
                variable: vn("p"),
                alias: None,
            }],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let plan = LoadPlan {
        id: "test-load".to_string(),
        ontology_lineage_id: "test".to_string(),
        ontology_version: 1,
        mode: LoadMode::Full,
        source: ox_ontology::load_plan::DataSourceSpec::Csv {
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
        batch_config: ox_ontology::load_plan::BatchConfig::default(),
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Person")),
                property_filters: vec![PropertyFilter {
                    property: PropertyKey::new("name").unwrap(),
                    value: Expr::Literal {
                        value: PropertyValue::String("Alice".to_string()),
                    },
                }],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("n"),
                    field: Some(pk("city")),
                }),
                op: ox_query_ir::query::ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::String("Seoul".to_string()),
                }),
            }),
            projections: vec![Projection::Variable {
                variable: vn("n"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Product")),
                property_filters: vec![],
            }],
            filter: Some(Expr::In {
                expr: Box::new(Expr::Property {
                    variable: vn("n"),
                    field: Some(pk("status")),
                }),
                values: vec![
                    PropertyValue::String("active".to_string()),
                    PropertyValue::String("pending".to_string()),
                    PropertyValue::Int(42),
                ],
            }),
            projections: vec![Projection::Variable {
                variable: vn("n"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Product")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("n"),
                    field: Some(pk("status")),
                }),
                op: ox_query_ir::query::ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Null,
                }),
            }),
            projections: vec![Projection::Variable {
                variable: vn("n"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let date_val = chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Event")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("n"),
                    field: Some(pk("date")),
                }),
                op: ox_query_ir::query::ComparisonOp::Gte,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Date(date_val),
                }),
            }),
            projections: vec![Projection::Variable {
                variable: vn("n"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![
                Projection::Field {
                    variable: vn("o"),
                    field: pk("status"),
                    alias: Some("status".to_string()),
                },
                Projection::Aggregation {
                    function: AggFunction::Count,
                    argument: Some(Box::new(Projection::Variable {
                        variable: vn("o"),
                        alias: None,
                    })),
                    alias: "total".to_string(),
                    distinct: false,
                },
                Projection::Aggregation {
                    function: AggFunction::Sum,
                    argument: Some(Box::new(Projection::Field {
                        variable: vn("o"),
                        field: pk("amount"),
                        alias: None,
                    })),
                    alias: "total_amount".to_string(),
                    distinct: false,
                },
            ],
            optional: false,
            group_by: vec![Projection::Field {
                variable: vn("o"),
                field: pk("status"),
                alias: None,
            }],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let q1 = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Person")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Field {
                variable: vn("n"),
                field: pk("name"),
                alias: Some("name".to_string()),
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };
    let q2 = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Company")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Field {
                variable: vn("n"),
                field: pk("name"),
                alias: Some("name".to_string()),
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let union_query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Union {
            queries: vec![q1, q2],
            all: true,
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&union_query, None).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("UNION ALL"), "got: {stmt}");
    assert!(stmt.contains("MATCH (n:`Person`)"), "got: {stmt}");
    assert!(stmt.contains("MATCH (n:`Company`)"), "got: {stmt}");
}

#[test]
fn test_compile_chain_with_pass_through() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Chain {
            steps: vec![
                ChainStep {
                    pass_through: vec![],
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Node {
                            variable: vn("c"),
                            label: Some(gl("Customer")),
                            property_filters: vec![],
                        }],
                        filter: None,
                        projections: vec![Projection::Variable {
                            variable: vn("c"),
                            alias: None,
                        }],
                        optional: false,
                        group_by: vec![],
                    },
                },
                ChainStep {
                    pass_through: vec![Projection::Variable {
                        variable: vn("c"),
                        alias: None,
                    }],
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Relationship {
                            variable: Some(vn("r")),
                            label: Some(gl("PURCHASED")),
                            source: vn("c"),
                            target: vn("p"),
                            direction: Direction::Outgoing,
                            property_filters: vec![],
                            var_length: None,
                        }],
                        filter: None,
                        projections: vec![
                            Projection::Variable {
                                variable: vn("c"),
                                alias: None,
                            },
                            Projection::Variable {
                                variable: vn("p"),
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
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    let stmt = &compiled.statement;
    assert!(stmt.contains("WITH c"), "WITH clause expected: {stmt}");
    assert!(stmt.contains("MATCH (c:`Customer`)"), "got: {stmt}");
    assert!(stmt.contains("(c)-[r:`PURCHASED`]->(p)"), "got: {stmt}");
    assert!(stmt.contains("LIMIT 20"), "got: {stmt}");
}

#[test]
fn test_compile_load_edge_upsert() {
    use ox_ontology::load_plan::{BatchConfig, DataSourceSpec, NodeMatch, PropertyMapping};

    let compiler = CypherCompiler::neo4j();
    let plan = LoadPlan {
        id: "test-edge-load".to_string(),
        ontology_lineage_id: "test".to_string(),
        ontology_version: 1,
        mode: LoadMode::Full,
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
    use ox_ontology::load_plan::{BatchConfig, DataSourceSpec, PropertyMapping};

    let compiler = CypherCompiler::neo4j();
    let plan = LoadPlan {
        id: "test-merge-nonnull".to_string(),
        ontology_lineage_id: "test".to_string(),
        ontology_version: 1,
        mode: LoadMode::Full,
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Chain {
            steps: vec![
                ChainStep {
                    pass_through: vec![],
                    operation: QueryOp::Match {
                        patterns: vec![GraphPattern::Node {
                            variable: vn("n"),
                            label: Some(gl("Person")),
                            property_filters: vec![],
                        }],
                        filter: None,
                        projections: vec![Projection::Variable {
                            variable: vn("n"),
                            alias: None,
                        }],
                        optional: false,
                        group_by: vec![],
                    },
                },
                ChainStep {
                    pass_through: vec![Projection::Variable {
                        variable: vn("n"),
                        alias: None,
                    }],
                    operation: QueryOp::CallSubquery {
                        inner: Box::new(QueryIR {
                            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
                            operation: QueryOp::Match {
                                patterns: vec![GraphPattern::Relationship {
                                    variable: None,
                                    label: None,
                                    source: vn("n"),
                                    target: vn("m"),
                                    direction: Direction::Outgoing,
                                    property_filters: vec![],
                                    var_length: None,
                                }],
                                filter: None,
                                projections: vec![Projection::Aggregation {
                                    function: AggFunction::Count,
                                    argument: Some(Box::new(Projection::Variable {
                                        variable: vn("m"),
                                        alias: None,
                                    })),
                                    alias: "neighbor_count".to_string(),
                                    distinct: false,
                                }],
                                optional: false,
                                group_by: vec![],
                            },
                            limit: None,
                            skip: None,
                            order_by: vec![],
                            as_of: None,
                        }),
                        import_variables: vec!["n".to_string()],
                    },
                },
            ],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Person")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![
                Projection::Variable {
                    variable: vn("n"),
                    alias: None,
                },
                Projection::Expression {
                    expr: Expr::Subquery {
                        query: Box::new(QueryIR {
                            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
                            operation: QueryOp::Match {
                                patterns: vec![GraphPattern::Relationship {
                                    variable: None,
                                    label: Some(gl("KNOWS")),
                                    source: vn("n"),
                                    target: vn("friend"),
                                    direction: Direction::Outgoing,
                                    property_filters: vec![],
                                    var_length: None,
                                }],
                                filter: None,
                                projections: vec![Projection::Variable {
                                    variable: vn("friend"),
                                    alias: None,
                                }],
                                optional: false,
                                group_by: vec![],
                            },
                            limit: None,
                            skip: None,
                            order_by: vec![],
                            as_of: None,
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
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    let stmt = &compiled.statement;
    assert!(
        stmt.contains("COUNT {"),
        "should use COUNT subquery: {stmt}"
    );
    assert!(stmt.contains("WITH n"), "should import variables: {stmt}");
    assert!(
        stmt.contains("AS friend_count"),
        "should alias result: {stmt}"
    );
}

#[test]
fn test_call_subquery_standalone() {
    // Test CallSubquery as a top-level operation
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::CallSubquery {
            inner: Box::new(QueryIR {
                schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
                operation: QueryOp::Match {
                    patterns: vec![GraphPattern::Node {
                        variable: vn("x"),
                        label: Some(gl("Task")),
                        property_filters: vec![],
                    }],
                    filter: None,
                    projections: vec![Projection::Aggregation {
                        function: AggFunction::Count,
                        argument: Some(Box::new(Projection::Variable {
                            variable: vn("x"),
                            alias: None,
                        })),
                        alias: "task_count".to_string(),
                        distinct: false,
                    }],
                    optional: false,
                    group_by: vec![],
                },
                limit: None,
                skip: None,
                order_by: vec![],
                as_of: None,
            }),
            import_variables: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Relationship {
                variable: None,
                label: Some(gl("TAGGED")),
                source: vn("p"),
                target: vn("t"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            }],
            filter: None,
            projections: vec![
                Projection::Variable {
                    variable: vn("p"),
                    alias: None,
                },
                Projection::Aggregation {
                    function: AggFunction::CollectList,
                    argument: Some(Box::new(Projection::Field {
                        variable: vn("t"),
                        field: pk("name"),
                        alias: None,
                    })),
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
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::PathFind {
            start: NodeRef {
                variable: vn("a"),
                label: Some(gl("Person")),
                property_filters: vec![PropertyFilter {
                    property: PropertyKey::new("name").unwrap(),
                    value: Expr::Literal {
                        value: PropertyValue::String("Alice".to_string()),
                    },
                }],
            },
            end: NodeRef {
                variable: vn("b"),
                label: Some(gl("Person")),
                property_filters: vec![PropertyFilter {
                    property: PropertyKey::new("name").unwrap(),
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
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::PathFind {
            start: NodeRef {
                variable: vn("a"),
                label: Some(gl("City")),
                property_filters: vec![],
            },
            end: NodeRef {
                variable: vn("b"),
                label: Some(gl("City")),
                property_filters: vec![],
            },
            edge_types: vec![gl("ROAD")],
            direction: Direction::Both,
            max_depth: Some(10),
            algorithm: PathAlgorithm::AllShortestPaths,
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::PathFind {
            start: NodeRef {
                variable: vn("a"),
                label: Some(gl("Node")),
                property_filters: vec![],
            },
            end: NodeRef {
                variable: vn("b"),
                label: Some(gl("Node")),
                property_filters: vec![],
            },
            edge_types: vec![gl("CONNECTS"), gl("LINKS")],
            direction: Direction::Outgoing,
            max_depth: Some(5),
            algorithm: PathAlgorithm::AllPaths,
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
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
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("n"),
                label: Some(gl("Product")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Expression {
                expr: Expr::Case {
                    operand: None,
                    when_clauses: vec![WhenClause {
                        condition: Expr::Comparison {
                            left: Box::new(Expr::Property {
                                variable: vn("n"),
                                field: Some(pk("price")),
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
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    assert!(compiled.statement.contains("CASE"));
    assert!(compiled.statement.contains("WHEN"));
    assert!(compiled.statement.contains("THEN"));
    assert!(compiled.statement.contains("ELSE"));
    assert!(compiled.statement.contains("END"));
    assert!(compiled.statement.contains("AS category"));
    // Values should be parameterized
    assert!(!compiled.params.is_empty());
}

// ---------------------------------------------------------------------------
// Memgraph dialect — compile-time DDL shape
//
// The MemGraphRuntime no longer post-processes schema statements; the
// compiler emits Memgraph-native DDL directly. These tests lock the
// shape of that output in so a regression in the compiler shows up
// here instead of as a silent Memgraph syntax error at runtime.
// ---------------------------------------------------------------------------

#[test]
fn memgraph_dialect_emits_4x_unique_constraint() {
    let compiler = CypherCompiler::memgraph();
    let ontology = OntologyIR::new(
        "test".into(),
        "Test".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: gl("User"),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: "prop-email".into(),
                name: pk("email"),
                property_type: PropertyType::String,
                nullable: false,
                default_value: None,
                description: LocalizedText::default(),
                classification: None,
                ..Default::default()
            }],
            constraints: vec![ConstraintDef {
                id: "cst-email".into(),
                constraint: NodeConstraint::Unique {
                    property_ids: vec!["prop-email".into()],
                },
            }],
            ..Default::default()
        }],
        vec![],
        vec![],
    );

    let stmts = compiler.compile_schema(&ontology).unwrap();
    // Memgraph 4.x: `CREATE CONSTRAINT ON (n:Label) ASSERT n.prop IS UNIQUE`
    assert!(
        stmts.iter().any(|s| s.contains("ASSERT n.`email` IS UNIQUE")
            && s.contains("CREATE CONSTRAINT ON")),
        "expected Memgraph 4.x unique constraint, got: {stmts:?}"
    );
    // Must NOT contain the Neo4j 5.x `REQUIRE` form — that's what the
    // old runtime rewriter existed to fix.
    assert!(
        stmts.iter().all(|s| !s.contains("REQUIRE")),
        "Memgraph dialect must not emit Neo4j 5.x REQUIRE syntax: {stmts:?}"
    );
}

#[test]
fn memgraph_dialect_emits_4x_exists_constraint() {
    let compiler = CypherCompiler::memgraph();
    let ontology = OntologyIR::new(
        "test".into(),
        "Test".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: gl("User"),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: "prop-name".into(),
                name: pk("name"),
                property_type: PropertyType::String,
                nullable: false,
                default_value: None,
                description: LocalizedText::default(),
                classification: None,
                ..Default::default()
            }],
            constraints: vec![ConstraintDef {
                id: "cst-exists".into(),
                constraint: NodeConstraint::Exists {
                    property_id: "prop-name".into(),
                },
            }],
            ..Default::default()
        }],
        vec![],
        vec![],
    );

    let stmts = compiler.compile_schema(&ontology).unwrap();
    assert!(
        stmts.iter().any(|s| s.contains("ASSERT EXISTS (n.`name`)")),
        "expected Memgraph 4.x EXISTS, got: {stmts:?}"
    );
}

#[test]
fn memgraph_dialect_skips_node_key_constraint() {
    let compiler = CypherCompiler::memgraph();
    let ontology = OntologyIR::new(
        "test".into(),
        "Test".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: gl("User"),
            description: LocalizedText::default(),
            properties: vec![
                PropertyDef {
                    id: "prop-first".into(),
                    name: pk("first"),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    ..Default::default()
                },
                PropertyDef {
                    id: "prop-last".into(),
                    name: pk("last"),
                    property_type: PropertyType::String,
                    nullable: false,
                    default_value: None,
                    description: LocalizedText::default(),
                    classification: None,
                    ..Default::default()
                },
            ],
            constraints: vec![ConstraintDef {
                id: "cst-nk".into(),
                constraint: NodeConstraint::NodeKey {
                    property_ids: vec!["prop-first".into(), "prop-last".into()],
                },
            }],
            ..Default::default()
        }],
        vec![],
        vec![],
    );

    let stmts = compiler.compile_schema(&ontology).unwrap();
    assert!(
        stmts.iter().all(|s| !s.contains("NODE KEY")),
        "Memgraph dialect has no NODE KEY; compiler must skip it: {stmts:?}"
    );
}

#[test]
fn memgraph_dialect_uses_short_index_syntax() {
    // Auto-generated index for a non-nullable property must use
    // Memgraph's short form: `CREATE INDEX ON :Label(prop)`.
    let compiler = CypherCompiler::memgraph();
    let ontology = OntologyIR::new(
        "test".into(),
        "Test".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "node-user".into(),
            label: gl("User"),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: "prop-name".into(),
                name: pk("name"),
                property_type: PropertyType::String,
                nullable: false,
                default_value: None,
                description: LocalizedText::default(),
                classification: None,
                ..Default::default()
            }],
            constraints: vec![],
            ..Default::default()
        }],
        vec![],
        vec![],
    );

    let stmts = compiler.compile_schema(&ontology).unwrap();
    assert!(
        stmts.iter().any(|s| s == "CREATE INDEX ON :`User`(`name`)"),
        "expected Memgraph short index syntax, got: {stmts:?}"
    );
    // Neo4j 5.x `CREATE INDEX IF NOT EXISTS FOR (n:...) ON (n.x)`
    // syntax must not appear.
    assert!(
        stmts.iter().all(|s| !s.contains("IF NOT EXISTS FOR")),
        "Memgraph dialect must not emit Neo4j 5.x index form: {stmts:?}"
    );
}

// ---------------------------------------------------------------------------
// Temporal AS-OF rejection
// ---------------------------------------------------------------------------

#[test]
fn temporal_as_of_is_rejected_with_compilation_error() {
    use chrono::TimeZone;

    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
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
        as_of: Some(
            chrono::Utc
                .with_ymd_and_hms(2026, 3, 5, 12, 0, 0)
                .single()
                .expect("fixture timestamp"),
        ),
    };

    let err = compiler.compile_query(&query, None).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("temporal") && msg.contains("not yet supported"),
        "expected temporal-rejection error, got: {msg}"
    );
}

#[test]
fn temporal_as_of_none_compiles_normally() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Person")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Variable {
                variable: vn("p"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };
    // Absence of as_of is the existing hot path; this test pins the
    // additive field's backwards compatibility.
    let compiled = compiler.compile_query(&query, None).expect("compile");
    assert!(compiled.statement.contains("MATCH (p:`Person`)"));
}

// ---------------------------------------------------------------------------
// HAVING filter on Aggregate
// ---------------------------------------------------------------------------

#[test]
fn aggregate_with_having_emits_with_where_return() {
    let compiler = CypherCompiler::neo4j();
    let inner = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
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
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Aggregate {
            source: Box::new(inner),
            group_by: vec![FieldRef {
                variable: vn("c"),
                field: Some(pk("country")),
            }],
            aggregations: vec![AggregationExpr {
                function: AggFunction::Count,
                field: FieldRef {
                    variable: vn("c"),
                    field: None,
                },
                distinct: false,
                alias: "customer_count".into(),
            }],
            // HAVING customer_count > 10
            having: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("customer_count"),
                    field: None,
                }),
                op: ox_query_ir::query::ComparisonOp::Gt,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Int(10),
                }),
            }),
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    let stmt = &compiled.statement;
    // Verify the HAVING idiom: WITH alias, agg_alias / WHERE agg_alias > $p0 / RETURN alias, agg_alias
    assert!(
        stmt.contains("WITH c.`country` AS `country`, count(c) AS customer_count"),
        "pre-HAVING WITH should project group-by + aggregation aliases: {stmt}"
    );
    assert!(
        stmt.contains("WHERE customer_count > $p0"),
        "HAVING compiles as a WHERE on aggregation aliases: {stmt}"
    );
    assert!(
        stmt.contains("RETURN `country`, customer_count"),
        "RETURN should reference the projected names only: {stmt}"
    );
    // Autogen param captures the numeric threshold.
    assert_eq!(compiled.params.get("p0"), Some(&PropertyValue::Int(10)));
}

#[test]
fn aggregate_without_having_preserves_existing_shape() {
    let compiler = CypherCompiler::neo4j();
    let inner = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
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
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Aggregate {
            source: Box::new(inner),
            group_by: vec![FieldRef {
                variable: vn("c"),
                field: Some(pk("country")),
            }],
            aggregations: vec![AggregationExpr {
                function: AggFunction::Count,
                field: FieldRef {
                    variable: vn("c"),
                    field: None,
                },
                distinct: false,
                alias: "customer_count".into(),
            }],
            having: None,
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };
    let stmt = compiler.compile_query(&query, None).unwrap().statement;
    // Without HAVING there's no intermediate WITH/WHERE — the compiler
    // emits a single RETURN with the projections inline.
    assert!(
        stmt.contains("RETURN c.`country` AS `country`, count(c) AS customer_count"),
        "plain aggregate should inline the RETURN, got: {stmt}"
    );
    assert!(
        !stmt.contains("WHERE customer_count"),
        "no HAVING means no post-aggregation WHERE, got: {stmt}"
    );
}

// ---------------------------------------------------------------------------
// Named parameter placeholders (Expr::Param)
// ---------------------------------------------------------------------------

#[test]
fn named_param_compiles_to_dollar_name() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Person")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("p"),
                    field: Some(pk("name")),
                }),
                op: ox_query_ir::query::ComparisonOp::Eq,
                right: Box::new(Expr::Param {
                    name: "target_name".to_string(),
                }),
            }),
            projections: vec![Projection::Variable {
                variable: vn("p"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler.compile_query(&query, None).unwrap();
    assert!(
        compiled.statement.contains("$target_name"),
        "named param should emit as $name, got: {}",
        compiled.statement
    );
    // Autogen params ($p0, $p1, ...) should not appear since the only
    // value in the query is a caller-bound parameter, not a literal.
    assert!(
        !compiled.statement.contains("$p0"),
        "no autogen params expected: {}",
        compiled.statement
    );
    // Named param is NOT in the compiled params map — caller binds it.
    assert!(
        !compiled.params.contains_key("target_name"),
        "named params must not enter the autogen params map"
    );
}

#[test]
fn named_param_rejects_injection_shaped_name() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Person")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("p"),
                    field: Some(pk("name")),
                }),
                op: ox_query_ir::query::ComparisonOp::Eq,
                right: Box::new(Expr::Param {
                    name: "bad name } RETURN p //".to_string(),
                }),
            }),
            projections: vec![Projection::Variable {
                variable: vn("p"),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let err = compiler.compile_query(&query, None).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("is_valid_graph_identifier"),
        "compile error should mention validator: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Backend capability gates
// ---------------------------------------------------------------------------

#[test]
fn memgraph_refuses_graph_analytics_at_compile_time() {
    let compiler = CypherCompiler::memgraph();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Analytics {
            algorithm: GraphAlgorithm::PageRank,
            source: AnalyticsSource::WholeGraph,
            params: Default::default(),
            projections: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let err = compiler
        .compile_query(&query, None)
        .expect_err("memgraph must not lower GDS procedures");
    let msg = format!("{err}");
    assert!(
        msg.contains("Memgraph") && msg.contains("MAGE"),
        "remediation must name Memgraph + alternative path, got: {msg}"
    );
}

#[test]
fn neo4j_still_lowers_graph_analytics_unchanged() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Analytics {
            algorithm: GraphAlgorithm::PageRank,
            source: AnalyticsSource::WholeGraph,
            params: Default::default(),
            projections: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler
        .compile_query(&query, None)
        .expect("neo4j retains GDS lowering");
    assert!(compiled.statement.contains("gds.pageRank.stream"));
}

#[test]
fn memgraph_refuses_percentile_aggregation_at_compile_time() {
    let compiler = CypherCompiler::memgraph();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Aggregate {
            source: Box::new(QueryIR {
                schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
                operation: QueryOp::Match {
                    patterns: vec![GraphPattern::Node {
                        variable: vn("n"),
                        label: Some(gl("Order")),
                        property_filters: vec![],
                    }],
                    filter: None,
                    projections: vec![Projection::Variable {
                        variable: vn("n"),
                        alias: None,
                    }],
                    optional: false,
                    group_by: vec![],
                },
                limit: None,
                skip: None,
                order_by: vec![],
                as_of: None,
            }),
            group_by: vec![],
            aggregations: vec![AggregationExpr {
                function: AggFunction::Percentile,
                field: FieldRef {
                    variable: vn("n"),
                    field: Some(pk("amount")),
                },
                alias: "p95".to_string(),
                distinct: false,
            }],
            having: None,
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let err = compiler
        .compile_query(&query, None)
        .expect_err("memgraph must not lower percentileCont");
    let msg = format!("{err}");
    assert!(
        msg.contains("Memgraph") && msg.contains("percentileCont"),
        "remediation must name function + backend, got: {msg}"
    );
}

#[test]
fn neo4j_lowers_percentile_aggregation_unchanged() {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Aggregate {
            source: Box::new(QueryIR {
                schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
                operation: QueryOp::Match {
                    patterns: vec![GraphPattern::Node {
                        variable: vn("n"),
                        label: Some(gl("Order")),
                        property_filters: vec![],
                    }],
                    filter: None,
                    projections: vec![Projection::Variable {
                        variable: vn("n"),
                        alias: None,
                    }],
                    optional: false,
                    group_by: vec![],
                },
                limit: None,
                skip: None,
                order_by: vec![],
                as_of: None,
            }),
            group_by: vec![],
            aggregations: vec![AggregationExpr {
                function: AggFunction::Percentile,
                field: FieldRef {
                    variable: vn("n"),
                    field: Some(pk("amount")),
                },
                alias: "p95".to_string(),
                distinct: false,
            }],
            having: None,
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler
        .compile_query(&query, None)
        .expect("neo4j retains percentile lowering");
    assert!(compiled.statement.contains("percentileCont"));
}

#[test]
fn hybrid_search_vector_only_compiles_to_neo4j_index_call() {
    use ox_query_ir::hybrid_retrieval::{
        Embedding, FusionStrategy, HybridSearchRequest,
    };

    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::HybridSearch {
            request: HybridSearchRequest {
                vector_query: Embedding::new(
                    vec![0.1, 0.2, 0.3, 0.4],
                    "test-model",
                ),
                fulltext_query: None,
                graph_constraints: None,
                fuse: FusionStrategy::default(),
                top_k: 25,
            },
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let compiled = compiler
        .compile_query(&query, None)
        .expect("vector-only HybridSearch compiles");
    // Calls the Neo4j 5 vector procedure with parameter
    // bindings — the index name, top_k, and vector all ride
    // through the ParamCollector.
    assert!(
        compiled.statement.contains("CALL db.index.vector.queryNodes("),
        "missing vector procedure call:\n{}",
        compiled.statement,
    );
    assert!(
        compiled.statement.contains("YIELD node, score"),
        "missing YIELD on hybrid search:\n{}",
        compiled.statement,
    );
    assert!(
        compiled.statement.contains("ORDER BY score DESC"),
        "missing score-desc ordering:\n{}",
        compiled.statement,
    );
    // The vector + index_name + top_k all land in `params` —
    // nothing inlined as literal so the operator is shielded
    // from injection / quoting issues. 3 params exactly: index
    // name, top_k, vector.
    assert_eq!(
        compiled.params.len(),
        3,
        "expected 3 params (index, top_k, vector) — got {}: {:?}",
        compiled.params.len(),
        compiled.params,
    );
}

#[test]
fn hybrid_search_with_fulltext_query_returns_unsupported_until_rrf_lands() {
    use ox_query_ir::hybrid_retrieval::{
        Embedding, FusionStrategy, HybridSearchRequest,
    };

    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::HybridSearch {
            request: HybridSearchRequest {
                vector_query: Embedding::new(vec![0.1, 0.2], "test"),
                // Fulltext present — RRF fusion path not yet
                // emitted; the compiler fails fast with a
                // typed UnsupportedOperation rather than
                // silently dropping the fulltext side.
                fulltext_query: Some("customer churn".into()),
                graph_constraints: None,
                fuse: FusionStrategy::default(),
                top_k: 10,
            },
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };

    let err = compiler
        .compile_query(&query, None)
        .expect_err("fulltext path should be deferred");
    let msg = format!("{err}");
    assert!(
        msg.contains("RRF") || msg.contains("fulltext_query"),
        "unexpected error: {msg}",
    );
}

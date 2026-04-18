// =============================================================================
// Phase 6.5 — Criterion bench for IR → Cypher compilation.
//
// Benchmarks three representative QueryIR shapes through the CypherCompiler:
//   1. Simple MATCH with a scalar filter and projection.
//   2. MATCH with relationship traversal and ordering.
//   3. Aggregation with GROUP BY.
//
// Output can be compared to `bench/baseline.json` via
// `scripts/check-bench-regression.sh`.
// =============================================================================

// Benches are not `cfg(test)`, so the workspace-level panic lints apply. We
// intentionally `unwrap()` inside criterion iteration closures to fail fast
// and keep the hot loop lean.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use ox_compiler::GraphCompiler;
use ox_compiler::cypher::CypherCompiler;
use ox_core::query_ir::{
    AggFunction, AggregationExpr, ComparisonOp, Expr, FieldRef, GraphPattern, OrderClause,
    Projection, QueryIR, QueryOp, SortDirection,
};
use ox_core::types::{Direction, PropertyValue};

fn bench_simple_match(c: &mut Criterion) {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
            schema_version: ox_core::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "p".into(),
                label: Some("Product".into()),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: "p".into(),
                    field: Some("price".into()),
                }),
                op: ComparisonOp::Gt,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Int(1000),
                }),
            }),
            projections: vec![
                Projection::Field {
                    variable: "p".into(),
                    field: "name".into(),
                    alias: None,
                },
                Projection::Field {
                    variable: "p".into(),
                    field: "price".into(),
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
                variable: "p".into(),
                field: "price".into(),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
    };

    c.bench_function("simple_match", |b| {
        b.iter(|| {
            let compiled = compiler.compile_query(black_box(&query)).unwrap();
            black_box(compiled);
        });
    });
}

fn bench_relationship_traversal(c: &mut Criterion) {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
            schema_version: ox_core::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
            patterns: vec![
                GraphPattern::Node {
                    variable: "c".into(),
                    label: Some("Customer".into()),
                    property_filters: vec![],
                },
                GraphPattern::Node {
                    variable: "o".into(),
                    label: Some("Order".into()),
                    property_filters: vec![],
                },
                GraphPattern::Relationship {
                    variable: Some("r".into()),
                    source: "c".into(),
                    target: "o".into(),
                    label: Some("PLACED".into()),
                    direction: Direction::Outgoing,
                    property_filters: vec![],
                    var_length: None,
                },
            ],
            filter: None,
            projections: vec![
                Projection::Field {
                    variable: "c".into(),
                    field: "name".into(),
                    alias: None,
                },
                Projection::Field {
                    variable: "o".into(),
                    field: "total".into(),
                    alias: Some("order_total".into()),
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: Some(100),
        skip: None,
        order_by: vec![OrderClause {
            projection: Projection::Field {
                variable: "o".into(),
                field: "total".into(),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
    };

    c.bench_function("relationship_traversal", |b| {
        b.iter(|| {
            let compiled = compiler.compile_query(black_box(&query)).unwrap();
            black_box(compiled);
        });
    });
}

fn bench_aggregation(c: &mut Criterion) {
    let compiler = CypherCompiler::neo4j();
    let inner = QueryIR {
            schema_version: ox_core::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: "o".into(),
                label: Some("Order".into()),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: "o".into(),
                    field: Some("status".into()),
                }),
                op: ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::String("completed".into()),
                }),
            }),
            projections: vec![
                Projection::Field {
                    variable: "o".into(),
                    field: "category".into(),
                    alias: None,
                },
                Projection::Field {
                    variable: "o".into(),
                    field: "total".into(),
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

    let query = QueryIR {
            schema_version: ox_core::query_ir::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Aggregate {
            source: Box::new(inner),
            group_by: vec![FieldRef {
                variable: "o".into(),
                field: Some("category".into()),
            }],
            aggregations: vec![AggregationExpr {
                function: AggFunction::Sum,
                field: FieldRef {
                    variable: "o".into(),
                    field: Some("total".into()),
                },
                distinct: false,
                alias: "total_revenue".into(),
            }],
        },
        limit: Some(20),
        skip: None,
        order_by: vec![],
    };

    c.bench_function("aggregation_group_by", |b| {
        b.iter(|| {
            let compiled = compiler.compile_query(black_box(&query)).unwrap();
            black_box(compiled);
        });
    });
}

criterion_group!(
    benches,
    bench_simple_match,
    bench_relationship_traversal,
    bench_aggregation,
);
criterion_main!(benches);

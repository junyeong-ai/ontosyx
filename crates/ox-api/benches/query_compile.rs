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
use ox_core::{GraphLabel, PropertyKey, VariableName};
use ox_query_ir::query::{
    AggFunction, AggregationExpr, ComparisonOp, Expr, FieldRef, GraphPattern, OrderClause,
    Projection, QueryIR, QueryOp, SortDirection,
};
use ox_core::types::{Direction, PropertyValue};

fn vn(s: &'static str) -> VariableName {
    VariableName::new(s).expect("bench variable literal must be valid")
}

fn gl(s: &'static str) -> GraphLabel {
    GraphLabel::new(s).expect("bench graph label literal must be valid")
}

fn pk(s: &'static str) -> PropertyKey {
    PropertyKey::new(s).expect("bench property key literal must be valid")
}

fn bench_simple_match(c: &mut Criterion) {
    let compiler = CypherCompiler::neo4j();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Product")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("p"),
                    field: Some(pk("price")),
                }),
                op: ComparisonOp::Gt,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::Int(1000),
                }),
            }),
            projections: vec![
                Projection::Field {
                    variable: vn("p"),
                    field: pk("name"),
                    alias: None,
                },
                Projection::Field {
                    variable: vn("p"),
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
                variable: vn("p"),
                field: pk("price"),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
        as_of: None,
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
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![
                GraphPattern::Node {
                    variable: vn("c"),
                    label: Some(gl("Customer")),
                    property_filters: vec![],
                },
                GraphPattern::Node {
                    variable: vn("o"),
                    label: Some(gl("Order")),
                    property_filters: vec![],
                },
                GraphPattern::Relationship {
                    variable: Some(vn("r")),
                    source: vn("c"),
                    target: vn("o"),
                    label: Some(gl("PLACED")),
                    direction: Direction::Outgoing,
                    property_filters: vec![],
                    var_length: None,
                },
            ],
            filter: None,
            projections: vec![
                Projection::Field {
                    variable: vn("c"),
                    field: pk("name"),
                    alias: None,
                },
                Projection::Field {
                    variable: vn("o"),
                    field: pk("total"),
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
                variable: vn("o"),
                field: pk("total"),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
        as_of: None,
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
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("o"),
                    field: Some(pk("status")),
                }),
                op: ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::String("completed".into()),
                }),
            }),
            projections: vec![
                Projection::Field {
                    variable: vn("o"),
                    field: pk("category"),
                    alias: None,
                },
                Projection::Field {
                    variable: vn("o"),
                    field: pk("total"),
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

    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Aggregate {
            source: Box::new(inner),
            group_by: vec![FieldRef {
                variable: vn("o"),
                field: Some(pk("category")),
            }],
            aggregations: vec![AggregationExpr {
                function: AggFunction::Sum,
                field: FieldRef {
                    variable: vn("o"),
                    field: Some(pk("total")),
                },
                distinct: false,
                alias: "total_revenue".into(),
            }],
            having: None,
        },
        limit: Some(20),
        skip: None,
        order_by: vec![],
        as_of: None,
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

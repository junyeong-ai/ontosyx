//! End-to-end federation test: `QueryIR` → `MatchPlanner` →
//! `build_match_plan` → `FederationContext::execute_plan` → Arrow
//! `RecordBatch`.
//!
//! Walks the full lowering pipeline. The `MATCH (n:User)` pattern
//! lives in the query layer and has never been seen by DataFusion
//! directly; by the time `execute_plan` returns, every planner stage
//! has done its job and the CSV adapter has handed rows back through
//! the Arrow RecordBatch surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;
use ox_core::types::{Direction, PropertyValue};
use ox_core::variable_name::VariableName;
use ox_federation::{
    FederationContext, InMemoryAdapterResolver, MatchPlanner, build_match_op, build_match_plan,
    build_query_ir, build_query_ir_scoped, context::WorkspaceRef,
};
use ox_ontology::OntologyIR;
use ox_ontology::ir::{EdgeTypeDef, NodeTypeDef};
use ox_ontology::mapping::{
    ColumnRef, EndpointRef, JoinCostHint, LinkMappingDef, LinkMappingId, LinkMappingKind,
    ObjectMappingDef, SourceId, SourceRelationKind, SourceRelationRef,
};
use ox_query_ir::query::{
    ComparisonOp, Expr, GraphPattern, OrderClause, Projection, PropertyFilter, QueryIR, QueryOp,
    SortDirection,
};
use ox_source::DataSourceAdapter;
use ox_source::sample::{CsvAdapter, JsonAdapter};

fn gl(s: &str) -> GraphLabel {
    GraphLabel::new(s).expect("valid graph label")
}

fn vn(s: &str) -> VariableName {
    VariableName::new(s).expect("valid variable name")
}

fn build_customer_ontology() -> (OntologyIR, InMemoryAdapterResolver) {
    // Single NodeType `Customer` mapped to a CSV relation called
    // `records`. The adapter is registered in the resolver under
    // the mapping's `source_id` so the planner can find it.
    let mut ont = OntologyIR::new(
        "ont-customers".into(),
        "customers".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-customer".into(),
            label: gl("Customer"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-customer",
        "nt-customer",
        "csv-crm",
        "records",
    ))
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let csv = "id,name,amount\n1,Alice,100\n2,Bob,250\n3,Charlie,42\n";
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(csv).expect("csv adapter"));
    resolver.register("csv-crm", adapter);

    (ont, resolver)
}

fn match_customer() -> QueryOp {
    QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("c"),
            label: Some(gl("Customer")),
            property_filters: vec![],
        }],
        filter: None,
        projections: vec![],
        optional: false,
        group_by: vec![],
    }
}

#[tokio::test]
async fn match_single_node_executes_end_to_end_against_csv_adapter() {
    let (ont, resolver) = build_customer_ontology();

    // Stage 1 — MatchPlanner lowers the QueryIR op to a MatchPlanSpec.
    let spec = MatchPlanner::new(&ont)
        .plan(&match_customer())
        .expect("match planner accepts a single-node match");

    // Stage 2 — build_match_plan turns the spec into a DataFusion
    // LogicalPlan. The plan embeds the SourceTableProvider directly,
    // so we do *not* have to register_table on the context.
    let plan = build_match_plan(&spec, &resolver)
        .await
        .expect("logical plan builder accepts a single-scan spec");

    // Stage 3 — FederationContext executes the plan and collects
    // batches. The CSV fixture has 3 rows; with no projection /
    // filter the scan must return exactly 3.
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("execute_plan drives the logical plan to completion");

    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 3,
        "expected every CSV row to surface through the federation execute path"
    );
    // The CSV fixture has three columns (id / name / amount). The
    // scan is un-projected, so every batch exposes all three.
    assert_eq!(batches[0].num_columns(), 3);
}

#[tokio::test]
async fn match_with_field_projection_narrows_output_to_one_column() {
    // `MATCH (c:Customer) RETURN c.name AS customer_name` — the
    // executed plan must return one column, three rows: projections
    // pass from QueryIR through to the DataFusion plan.
    let (ont, resolver) = build_customer_ontology();
    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("c"),
            label: Some(gl("Customer")),
            property_filters: vec![],
        }],
        filter: None,
        projections: vec![Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("name").expect("valid"),
            alias: Some("customer_name".into()),
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();

    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 3, "one row per CSV customer");
    assert_eq!(
        batches[0].num_columns(),
        1,
        "projection narrows to one column"
    );
    assert_eq!(batches[0].schema().field(0).name(), "customer_name");
}

#[tokio::test]
async fn match_with_variable_only_projection_is_equivalent_to_no_projection() {
    // `MATCH (c:Customer) RETURN c` — a Variable projection with no
    // alias keeps every column. The planner's short-circuit skips
    // the projection node; the result is the same row count as the
    // un-projected version.
    let (ont, resolver) = build_customer_ontology();
    let op = QueryOp::Match {
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
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 3);
    assert_eq!(batches[0].num_columns(), 3);
}

#[tokio::test]
async fn match_with_where_comparison_filters_rows() {
    // `MATCH (c:Customer) WHERE c.amount > 100 RETURN c.name` →
    // CSV fixture has {100, 250, 42}; 250 is the only match above 100.
    let (ont, resolver) = build_customer_ontology();
    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("c"),
            label: Some(gl("Customer")),
            property_filters: vec![],
        }],
        filter: Some(Expr::Comparison {
            left: Box::new(Expr::Property {
                variable: vn("c"),
                field: Some(PropertyKey::new("amount").unwrap()),
            }),
            op: ComparisonOp::Gt,
            right: Box::new(Expr::Literal {
                value: PropertyValue::Int(100),
            }),
        }),
        projections: vec![Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("name").unwrap(),
            alias: None,
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 1, "only Bob (amount=250) should pass > 100");
}

#[tokio::test]
async fn match_with_inline_property_filter_narrows_scan() {
    // `MATCH (c:Customer {name: "Alice"}) RETURN c.amount` — inline
    // property filter is a shortcut that the planner folds into a
    // WHERE. One match in the fixture.
    let (ont, resolver) = build_customer_ontology();
    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("c"),
            label: Some(gl("Customer")),
            property_filters: vec![PropertyFilter {
                property: PropertyKey::new("name").unwrap(),
                value: Expr::Literal {
                    value: PropertyValue::String("Alice".into()),
                },
            }],
        }],
        filter: None,
        projections: vec![Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("amount").unwrap(),
            alias: None,
        }],
        optional: false,
        group_by: vec![],
    };
    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 1, "only the Alice row should match");
}

#[tokio::test]
async fn query_ir_order_by_desc_plus_limit_yields_top_n() {
    // `MATCH (c:Customer) RETURN c.name, c.amount ORDER BY c.amount
    // DESC LIMIT 2` — fixture has {Alice: 100, Bob: 250, Charlie: 42}.
    // Top-2 by amount DESC = Bob, Alice.
    let (ont, resolver) = build_customer_ontology();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("c"),
                label: Some(gl("Customer")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![
                Projection::Field {
                    variable: vn("c"),
                    field: PropertyKey::new("name").unwrap(),
                    alias: None,
                },
                Projection::Field {
                    variable: vn("c"),
                    field: PropertyKey::new("amount").unwrap(),
                    alias: None,
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: Some(2),
        skip: None,
        order_by: vec![OrderClause {
            projection: Projection::Field {
                variable: vn("c"),
                field: PropertyKey::new("amount").unwrap(),
                alias: None,
            },
            direction: SortDirection::Desc,
        }],
        as_of: None,
    };

    let plan = build_query_ir(&ont, &query, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 2, "LIMIT 2 caps result");
}

#[tokio::test]
async fn query_ir_skip_offsets_results() {
    // SKIP 1 LIMIT 1 → one row, the middle element after ordering
    // by amount ASC = Alice (100). Smoke test that SKIP threads
    // through `limit(skip, fetch)`.
    let (ont, resolver) = build_customer_ontology();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("c"),
                label: Some(gl("Customer")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Field {
                variable: vn("c"),
                field: PropertyKey::new("name").unwrap(),
                alias: None,
            }],
            optional: false,
            group_by: vec![],
        },
        limit: Some(1),
        skip: Some(1),
        order_by: vec![OrderClause {
            projection: Projection::Field {
                variable: vn("c"),
                field: PropertyKey::new("amount").unwrap(),
                alias: None,
            },
            direction: SortDirection::Asc,
        }],
        as_of: None,
    };
    let plan = build_query_ir(&ont, &query, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 1, "SKIP 1 LIMIT 1 → exactly one row");
}

#[tokio::test]
async fn workspace_scope_filters_rows_to_the_requested_tenant() {
    // Build a CSV fixture that interleaves rows from two workspaces
    // in the same relation. The mapping declares
    // `workspace_scope = Some(_workspace_id)`, so `build_query_ir_scoped`
    // must inject a `_workspace_id = $ws` filter on top of the scan.
    // Result: only the rows belonging to the requested workspace
    // come back.
    let csv = "\
id,name,amount,_workspace_id
1,Alice,100,ws-a
2,Bob,250,ws-b
3,Charlie,42,ws-a
4,Dana,75,ws-b
";
    let mut ont = OntologyIR::new(
        "ont-tenant".into(),
        "tenant".into(),
        ox_core::i18n::LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-customer".into(),
            label: gl("Customer"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    let mut mapping = ObjectMappingDef::new("om-1", "nt-customer", "csv-multi", "records");
    // Tell the planner which column carries the workspace id on the
    // scan side. The relation name here is descriptive only — the
    // planner references `workspace_scope.column` on the single
    // scan table.
    mapping.workspace_scope = Some(ColumnRef::new("records", "_workspace_id"));
    ont.add_object_mapping(mapping).unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(csv).unwrap());
    resolver.register("csv-multi", adapter);

    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("c"),
                label: Some(gl("Customer")),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![Projection::Field {
                variable: vn("c"),
                field: PropertyKey::new("name").unwrap(),
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

    let ctx = FederationContext::new(WorkspaceRef::new("ws-a"));

    // ws-a → 2 rows (Alice + Charlie)
    let plan_a = build_query_ir_scoped(&ont, &query, "ws-a", &resolver)
        .await
        .unwrap();
    let rows_a: usize = ctx
        .execute_plan(plan_a)
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(rows_a, 2, "ws-a scope must isolate Alice and Charlie");

    // ws-b → 2 rows (Bob + Dana)
    let plan_b = build_query_ir_scoped(&ont, &query, "ws-b", &resolver)
        .await
        .unwrap();
    let rows_b: usize = ctx
        .execute_plan(plan_b)
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(rows_b, 2, "ws-b scope must isolate Bob and Dana");

    // Unscoped (system-bypass path) sees every row.
    let plan_all = build_query_ir(&ont, &query, &resolver).await.unwrap();
    let rows_all: usize = ctx
        .execute_plan(plan_all)
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(rows_all, 4, "unscoped build_query_ir must see every row");
}

#[tokio::test]
async fn mapping_without_workspace_scope_is_shared_across_workspaces() {
    // A mapping without `workspace_scope` represents a shared
    // relation. `build_query_ir_scoped` must *not* inject a filter
    // on that mapping — otherwise a shared reference table would
    // look empty to every workspace. The fixture from
    // `build_customer_ontology` has no workspace_scope.
    let (ont, resolver) = build_customer_ontology();
    let query = QueryIR {
        schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
        operation: match_customer(),
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    };
    let plan = build_query_ir_scoped(&ont, &query, "ws-anything", &resolver)
        .await
        .unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-anything"));
    let rows: usize = ctx
        .execute_plan(plan)
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(
        rows, 3,
        "shared mapping returns every row regardless of workspace"
    );
}

#[tokio::test]
async fn multi_mapping_union_all_doubles_row_count() {
    // Two object mappings, both pointing at the same CSV relation.
    // After UNION ALL the execute path must return each row twice —
    // this is the visible signal that the builder is threading two
    // scan plans through one logical plan.
    let (mut ont, resolver) = build_customer_ontology();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-customer-2",
        "nt-customer",
        "csv-crm",
        "records",
    ))
    .unwrap();

    let spec = MatchPlanner::new(&ont)
        .plan(&match_customer())
        .expect("match planner accepts the doubled mapping");
    assert_eq!(spec.scans[0].mappings.len(), 2);

    let plan = build_match_plan(&spec, &resolver)
        .await
        .expect("union plan builds");

    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.expect("execute_plan");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 6,
        "3 rows × 2 mappings → 6 rows through UNION ALL"
    );
}

#[tokio::test]
async fn match_relationship_foreign_key_hop_inner_joins_endpoints() {
    // ForeignKey link-mapping hop, end-to-end:
    //   MATCH (u:User)-[:PLACED]->(o:Order) RETURN u.name, o.id
    // Two CSV adapters back User and Order. The PLACED link mapping is
    // a ForeignKey `orders.user_id = users.id`. INNER JOIN drops the
    // orphan order (user_id=99) and doubles Alice (two orders). Final
    // expected row count: 3.
    let users_csv = "id,name\n1,Alice\n2,Bob\n3,Charlie\n";
    let orders_csv = "id,user_id,amount\n\
                      100,1,50\n\
                      101,1,75\n\
                      102,2,120\n\
                      103,99,10\n";

    let mut ont = OntologyIR::new(
        "ont-commerce".into(),
        "commerce".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-placed".into(),
            label: gl("PLACED"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-order".into(),
            ..Default::default()
        }],
        vec![],
    );
    // Both adapters expose a single built-in `records` table — that's
    // the CSV adapter's only addressable relation. The link mapping
    // uses author-declared endpoint relations ("users" / "orders")
    // purely as disambiguators for the FK column refs; scan-time
    // routing reads through the ObjectMappingDef's `relation` field.
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-order",
        "nt-order",
        "csv-orders",
        "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-placed"),
        edge_type_id: "e-placed".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "user_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let users_adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(CsvAdapter::new(users_csv).expect("users csv"));
    let orders_adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(CsvAdapter::new(orders_csv).expect("orders csv"));
    resolver.register("csv-users", users_adapter);
    resolver.register("csv-orders", orders_adapter);

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("user_name".into()),
            },
            Projection::Field {
                variable: vn("o"),
                field: PropertyKey::new("id").unwrap(),
                alias: Some("order_id".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("join plan");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.expect("execute plan");

    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 3,
        "INNER JOIN keeps the 3 orders with a matching user, drops the orphan"
    );
    assert_eq!(batches[0].num_columns(), 2, "two projected columns");
    assert_eq!(batches[0].schema().field(0).name(), "user_name");
    assert_eq!(batches[0].schema().field(1).name(), "order_id");
}

#[tokio::test]
async fn match_relationship_with_where_filter_pushes_on_left_side() {
    // Smoke-test: an additional WHERE clause on the user side must
    // survive the join. MATCH (u:User)-[:PLACED]->(o:Order) WHERE
    // u.name = "Alice" RETURN o.id → Alice has two orders, so the
    // result is two rows.
    let users_csv = "id,name\n1,Alice\n2,Bob\n";
    let orders_csv = "id,user_id,amount\n10,1,5\n11,1,6\n12,2,7\n";

    let mut ont = OntologyIR::new(
        "ont".into(),
        "c".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-placed".into(),
            label: gl("PLACED"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-order".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-order",
        "nt-order",
        "csv-orders",
        "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-placed"),
        edge_type_id: "e-placed".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "user_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-orders",
        Arc::new(CsvAdapter::new(orders_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
        ],
        filter: Some(Expr::Comparison {
            left: Box::new(Expr::Property {
                variable: vn("u"),
                field: Some(PropertyKey::new("name").unwrap()),
            }),
            op: ComparisonOp::Eq,
            right: Box::new(Expr::Literal {
                value: PropertyValue::String("Alice".into()),
            }),
        }),
        projections: vec![Projection::Field {
            variable: vn("o"),
            field: PropertyKey::new("id").unwrap(),
            alias: None,
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 2,
        "Alice has two orders — the WHERE filter keeps both"
    );
}

#[tokio::test]
async fn match_two_hop_chain_inner_joins_every_endpoint() {
    // A two-hop chain threads three scans through two INNER JOINs:
    //
    //   MATCH (u:User)-[:PLACED]->(o:Order)-[:CONTAINS]->(p:Product)
    //   RETURN u.name, o.id, p.name
    //
    // User 1 (Alice) placed orders 100 (product A) and 101 (product B).
    // User 2 (Bob) placed order 102 (product B).
    // Order 103 references a missing user (user_id=99) → dropped by the
    // u↔o join.
    // Order 101 also references a missing product (product_id=Z) →
    // dropped by the o↔p join.
    //
    // Expected surviving rows:
    //   (Alice, 100, Apple), (Bob, 102, Banana) → 2 rows.
    let users_csv = "id,name\n1,Alice\n2,Bob\n";
    let orders_csv = "id,user_id,product_id\n\
                      100,1,A\n\
                      101,1,Z\n\
                      102,2,B\n\
                      103,99,A\n";
    let products_csv = "id,name\nA,Apple\nB,Banana\n";

    let mut ont = OntologyIR::new(
        "ont-commerce".into(),
        "commerce".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-product".into(),
                label: gl("Product"),
                ..Default::default()
            },
        ],
        vec![
            EdgeTypeDef {
                id: "e-placed".into(),
                label: gl("PLACED"),
                source_node_id: "nt-user".into(),
                target_node_id: "nt-order".into(),
                ..Default::default()
            },
            EdgeTypeDef {
                id: "e-contains".into(),
                label: gl("CONTAINS"),
                source_node_id: "nt-order".into(),
                target_node_id: "nt-product".into(),
                ..Default::default()
            },
        ],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-order",
        "nt-order",
        "csv-orders",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-product",
        "nt-product",
        "csv-products",
        "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-placed"),
        edge_type_id: "e-placed".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "user_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-contains"),
        edge_type_id: "e-contains".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "product_id"),
            target_column: ColumnRef::new("products", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-products"),
            relation: "products".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-orders",
        Arc::new(CsvAdapter::new(orders_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-products",
        Arc::new(CsvAdapter::new(products_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("CONTAINS")),
                source: vn("o"),
                target: vn("p"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Product")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("user_name".into()),
            },
            Projection::Field {
                variable: vn("o"),
                field: PropertyKey::new("id").unwrap(),
                alias: Some("order_id".into()),
            },
            Projection::Field {
                variable: vn("p"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("product_name".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("two-hop chain builds");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.expect("execute two-hop plan");

    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 2,
        "chain keeps only orders with both a valid user and a valid product"
    );
    assert_eq!(batches[0].num_columns(), 3);
    let schema = batches[0].schema();
    let fields: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    assert_eq!(fields, vec!["user_name", "order_id", "product_name"]);
}

#[tokio::test]
async fn match_disconnected_components_are_explicitly_rejected() {
    // Two independent hops that do not share a variable:
    //   MATCH (a:User)-[:PLACED]->(b:Order), (c:User)-[:PLACED]->(d:Order)
    // The planner refuses rather than silently emitting an implicit
    // cross-product — the author should split the MATCH or add a
    // connecting pattern.
    let (ont, resolver) = build_two_entity_ontology();

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("a"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("a"),
                target: vn("b"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("b"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
            GraphPattern::Node {
                variable: vn("c"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("c"),
                target: vn("d"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("d"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![],
        optional: false,
        group_by: vec![],
    };

    let err = build_match_op(&ont, &op, &resolver)
        .await
        .expect_err("disconnected components must refuse");
    match err {
        ox_federation::FederationError::Unsupported(msg) => {
            assert!(
                msg.contains("disconnected"),
                "error must name the disconnected-component case: {msg}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[tokio::test]
async fn match_bridge_link_mapping_joins_via_intermediate_relation() {
    // A many-to-many hop threads an intermediate bridge relation
    // into the join chain.
    //
    //   MATCH (p:Post)-[:TAGGED]->(t:Tag) RETURN p.title, t.name
    //
    // Fixture:
    //   posts: (1, "Hello"), (2, "World"), (3, "Orphan")
    //   tags:  ("rust", "Rust"), ("sql", "SQL"), ("web", "Web")
    //   post_tags: (1, rust), (1, sql), (2, rust), (99, web), (2, zzz)
    //
    // The bridge rows (99, web) and (2, zzz) each reference a
    // missing endpoint, so they drop from the INNER join chain.
    // Expected surviving (post, tag) pairs:
    //   (Hello, Rust), (Hello, SQL), (World, Rust) → 3 rows.
    let posts_csv = "id,title\n1,Hello\n2,World\n3,Orphan\n";
    let tags_csv = "id,name\nrust,Rust\nsql,SQL\nweb,Web\n";
    let post_tags_csv = "post_id,tag_id\n\
                         1,rust\n\
                         1,sql\n\
                         2,rust\n\
                         99,web\n\
                         2,zzz\n";

    let mut ont = OntologyIR::new(
        "ont-cms".into(),
        "cms".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-post".into(),
                label: gl("Post"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-tag".into(),
                label: gl("Tag"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-tagged".into(),
            label: gl("TAGGED"),
            source_node_id: "nt-post".into(),
            target_node_id: "nt-tag".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-post",
        "nt-post",
        "csv-posts",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-tag", "nt-tag", "csv-tags", "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-tagged"),
        edge_type_id: "e-tagged".into(),
        kind: LinkMappingKind::Bridge {
            bridge_relation: SourceRelationRef {
                source_id: SourceId::new("csv-post-tags"),
                relation: "records".into(),
                kind: SourceRelationKind::Table,
            },
            source_join: vec![ColumnRef::new("post_tags", "post_id")],
            target_join: vec![ColumnRef::new("post_tags", "tag_id")],
            bridge_workspace_scope: None,
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-posts"),
            relation: "posts".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-tags"),
            relation: "tags".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-posts",
        Arc::new(CsvAdapter::new(posts_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-tags",
        Arc::new(CsvAdapter::new(tags_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-post-tags",
        Arc::new(CsvAdapter::new(post_tags_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Post")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("TAGGED")),
                source: vn("p"),
                target: vn("t"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("t"),
                label: Some(gl("Tag")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("p"),
                field: PropertyKey::new("title").unwrap(),
                alias: Some("post_title".into()),
            },
            Projection::Field {
                variable: vn("t"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("tag_name".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("bridge hop builds a plan");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.expect("bridge plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 3,
        "only (post, tag) pairs with both endpoints present survive the chain"
    );
    assert_eq!(batches[0].num_columns(), 2);
}

#[tokio::test]
async fn match_federated_link_mapping_joins_across_sources() {
    // A `Federated` link mapping with endpoints in *different*
    // sources lowers to the same INNER JOIN shape as ForeignKey.
    // DataFusion materialises each side into Arrow and performs the
    // equi-join engine-side, so the cross-source case works through
    // the generic execute path with no source-native join plumbing.
    //
    //   MATCH (u:User)-[:OWNS]->(a:Account) RETURN u.email, a.alias
    //
    // Users live in csv-users; accounts live in a *separate*
    // csv-accounts source. The "match column" on each side is
    // `email` / `owner_email` — a value-based correspondence, not a
    // DB-level FK.
    let users_csv = "id,email\n1,alice@example.com\n2,bob@example.com\n3,dana@example.com\n";
    let accounts_csv = "alias,owner_email\n\
                        acct-a,alice@example.com\n\
                        acct-b,bob@example.com\n\
                        acct-orphan,stranger@example.com\n";

    let mut ont = OntologyIR::new(
        "ont-idm".into(),
        "idm".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-account".into(),
                label: gl("Account"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-owns".into(),
            label: gl("OWNS"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-account".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-account",
        "nt-account",
        "csv-accounts",
        "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-owns"),
        edge_type_id: "e-owns".into(),
        kind: LinkMappingKind::Federated {
            source_match_column: ColumnRef::new("users", "email"),
            target_match_column: ColumnRef::new("accounts", "owner_email"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-accounts"),
            relation: "accounts".into(),
            key_columns: vec!["alias".into()],
        },
        join_cost_hint: JoinCostHint::Scan,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-accounts",
        Arc::new(CsvAdapter::new(accounts_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("OWNS")),
                source: vn("u"),
                target: vn("a"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("a"),
                label: Some(gl("Account")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("email").unwrap(),
                alias: Some("user_email".into()),
            },
            Projection::Field {
                variable: vn("a"),
                field: PropertyKey::new("alias").unwrap(),
                alias: Some("account_alias".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("federated hop builds a plan");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("federated plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 2,
        "only Alice+acct-a and Bob+acct-b match; Dana has no account, acct-orphan \
         references no user"
    );
    assert_eq!(batches[0].num_columns(), 2);
}

/// Shared fixture for tests that reuse the two-entity PLACED graph.
fn build_two_entity_ontology() -> (OntologyIR, InMemoryAdapterResolver) {
    let users_csv = "id,name\n1,Alice\n2,Bob\n";
    let orders_csv = "id,user_id\n10,1\n11,2\n";
    let mut ont = OntologyIR::new(
        "ont".into(),
        "c".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-placed".into(),
            label: gl("PLACED"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-order".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-order",
        "nt-order",
        "csv-orders",
        "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-placed"),
        edge_type_id: "e-placed".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "user_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-orders",
        Arc::new(CsvAdapter::new(orders_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    (ont, resolver)
}

#[tokio::test]
async fn match_single_node_executes_end_to_end_against_nested_json_table() {
    // Nested-table support: a JSON object with two array-of-object
    // fields becomes two separate scannable tables (`users` and
    // `orders`). MATCH over the `users` relation — registered under
    // the `users` table name on the JSON adapter — materialises the
    // nested array into Arrow rows and flows through the federation
    // planner exactly like a top-level-array JSON source would.
    let json = r#"{
        "users": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ],
        "orders": [
            {"id": 100, "user_id": 1}
        ]
    }"#;

    let mut ont = OntologyIR::new(
        "ont-nested-json".into(),
        "nested-json".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-user".into(),
            label: gl("User"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    // The mapping's `relation` field uses `analyze_json`'s namespaced
    // child-table name: nested arrays live under `records_<field>`,
    // not the bare field name.
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "json-nested",
        "records_users",
    ))
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(JsonAdapter::new(json).expect("nested json adapter"));
    resolver.register("json-nested", adapter);

    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("u"),
            label: Some(gl("User")),
            property_filters: vec![],
        }],
        filter: None,
        projections: vec![Projection::Field {
            variable: vn("u"),
            field: PropertyKey::new("name").unwrap(),
            alias: Some("user_name".into()),
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 2, "two users in the nested array");
    assert_eq!(batches[0].num_columns(), 1);
    assert_eq!(batches[0].schema().field(0).name(), "user_name");
}

#[tokio::test]
async fn match_single_node_executes_end_to_end_against_two_level_nested_json() {
    // A two-level nested JSON fixture: each user owns a list of
    // addresses, and the MATCH targets `records_users_addresses`.
    // `analyze_json` emits the nested relation; JsonAdapter::scan
    // walks `schema.tables` to build the parent chain
    // (`records → users → addresses`), then flattens the JSON
    // top-down to produce one row per address.
    let json = r#"{
        "users": [
            {
                "id": 1,
                "name": "Alice",
                "addresses": [
                    {"city": "Seoul", "country": "KR"},
                    {"city": "NYC", "country": "US"}
                ]
            },
            {
                "id": 2,
                "name": "Bob",
                "addresses": [
                    {"city": "London", "country": "UK"}
                ]
            },
            {
                "id": 3,
                "name": "Carol (no addresses)",
                "addresses": []
            }
        ]
    }"#;

    let mut ont = OntologyIR::new(
        "ont-two-level".into(),
        "two-level".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-address".into(),
            label: gl("Address"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-address",
        "nt-address",
        "json-two-level",
        "records_users_addresses",
    ))
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(JsonAdapter::new(json).expect("two-level json adapter"));
    resolver.register("json-two-level", adapter);

    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("a"),
            label: Some(gl("Address")),
            property_filters: vec![],
        }],
        filter: None,
        projections: vec![Projection::Field {
            variable: vn("a"),
            field: PropertyKey::new("city").unwrap(),
            alias: Some("city".into()),
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 3,
        "two addresses for Alice + one for Bob + zero for Carol = 3 rows"
    );
    assert_eq!(batches[0].num_columns(), 1);
    assert_eq!(batches[0].schema().field(0).name(), "city");
}

#[tokio::test]
async fn match_single_node_executes_end_to_end_against_three_level_nested_json() {
    // Pushes the multi-level walker past what the 2-level test
    // covers: three object-array hops from root to leaf. The
    // profiler emits intermediate tables at every level
    // (`records_users`, `records_users_addresses`, and the target
    // `records_users_addresses_phones`), and `parent_chain_fields`
    // walks the longest-known-prefix chain at each step.
    //
    // Fixture — two users, each with one address, each address has
    // zero / one / two phones respectively for variety:
    let json = r#"{
        "users": [
            {
                "id": 1,
                "addresses": [
                    {
                        "city": "Seoul",
                        "phones": [
                            {"number": "010-1111"},
                            {"number": "010-2222"}
                        ]
                    }
                ]
            },
            {
                "id": 2,
                "addresses": [
                    {
                        "city": "NYC",
                        "phones": [
                            {"number": "212-3333"}
                        ]
                    }
                ]
            }
        ]
    }"#;

    let mut ont = OntologyIR::new(
        "ont-three-level".into(),
        "three-level".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-phone".into(),
            label: gl("Phone"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-phone",
        "nt-phone",
        "json-three-level",
        "records_users_addresses_phones",
    ))
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(JsonAdapter::new(json).expect("three-level json adapter"));
    resolver.register("json-three-level", adapter);

    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("p"),
            label: Some(gl("Phone")),
            property_filters: vec![],
        }],
        filter: None,
        projections: vec![Projection::Field {
            variable: vn("p"),
            field: PropertyKey::new("number").unwrap(),
            alias: Some("number".into()),
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 3,
        "two phones for Alice's only address + one for Bob's = 3 rows"
    );
    assert_eq!(batches[0].num_columns(), 1);
    assert_eq!(batches[0].schema().field(0).name(), "number");
}

#[tokio::test]
async fn match_single_node_executes_end_to_end_against_json_adapter() {
    // JsonAdapter gained its own `scan()` implementation in the same
    // slice that added this test. Before the change a federation
    // query against a JSON source silently returned
    // `UnsupportedOperation`. This test pins the happy path: a
    // top-level JSON array of objects is materialised into a
    // RecordBatch, the federation planner scans it, and every row
    // surfaces through `execute_plan`.
    let json = r#"[
        {"id": 1, "name": "Alice", "amount": 100},
        {"id": 2, "name": "Bob", "amount": 250},
        {"id": 3, "name": "Charlie", "amount": 42}
    ]"#;

    let mut ont = OntologyIR::new(
        "ont-json".into(),
        "json-customers".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-customer".into(),
            label: gl("Customer"),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-customer",
        "nt-customer",
        "json-crm",
        "records",
    ))
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(JsonAdapter::new(json).expect("json adapter"));
    resolver.register("json-crm", adapter);

    let op = QueryOp::Match {
        patterns: vec![GraphPattern::Node {
            variable: vn("c"),
            label: Some(gl("Customer")),
            property_filters: vec![],
        }],
        filter: None,
        projections: vec![Projection::Field {
            variable: vn("c"),
            field: PropertyKey::new("name").unwrap(),
            alias: Some("customer_name".into()),
        }],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 3, "every JSON row must surface through scan");
    assert_eq!(batches[0].num_columns(), 1);
    assert_eq!(batches[0].schema().field(0).name(), "customer_name");
}

#[tokio::test]
async fn match_multi_mapping_hop_unions_per_link_mapping() {
    // Two FK paths cover the same edge type — e.g., orders might be
    // tied to users either as the *placer* (orders.user_id → users.id)
    // or as the *recipient* (orders.recipient_id → users.id). The
    // federation planner should union the two FK join results so
    // "any row reachable by either path" shows up.
    //
    // Fixture:
    //   users:  (1, Alice), (2, Bob)
    //   orders:
    //     (10, placer=1, recipient=2)  // Alice placed, Bob received
    //     (11, placer=2, recipient=1)  // Bob placed, Alice received
    //     (12, placer=1, recipient=1)  // Alice both placed & received
    //
    // For MATCH (u:User)-[:RELATED]->(o:Order):
    //   placer path → (Alice,10), (Bob,11), (Alice,12)     = 3 rows
    //   recipient path → (Bob,10), (Alice,11), (Alice,12)  = 3 rows
    // UNION ALL → 6 rows. (DISTINCT across both sides would
    // collapse to 5 — we assert 6 because the lowering explicitly
    // uses UNION ALL for now.)
    let users_csv = "id,name\n1,Alice\n2,Bob\n";
    let orders_csv = "id,placer_id,recipient_id\n\
                      10,1,2\n\
                      11,2,1\n\
                      12,1,1\n";

    let mut ont = OntologyIR::new(
        "ont-multi".into(),
        "multi".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-related".into(),
            label: gl("RELATED"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-order".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-order",
        "nt-order",
        "csv-orders",
        "records",
    ))
    .unwrap();
    // Placer FK mapping.
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-related-placer"),
        edge_type_id: "e-related".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "placer_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();
    // Recipient FK mapping — same edge type, different FK column.
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-related-recipient"),
        edge_type_id: "e-related".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "recipient_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-orders",
        Arc::new(CsvAdapter::new(orders_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("RELATED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
        ],
        filter: None,
        // Projections survive a multi-mapping hop because the
        // planner now pushes them into each UNION branch *before*
        // the union (the push-down avoids DataFusion's UNION node
        // stripping variable-level qualifiers from the merged
        // schema). Aliases therefore reach the final output.
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("user_name".into()),
            },
            Projection::Field {
                variable: vn("o"),
                field: PropertyKey::new("id").unwrap(),
                alias: Some("order_id".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("multi-mapping hop plan builds");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("multi-mapping plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 6,
        "UNION ALL of placer + recipient mappings emits 3 rows per mapping"
    );
    assert_eq!(
        batches[0].num_columns(),
        2,
        "projection narrows to two cols"
    );
    let schema = batches[0].schema();
    let field_names: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    assert_eq!(field_names, vec!["user_name", "order_id"]);
}

#[tokio::test]
async fn match_multi_mapping_seed_supports_bridge_alongside_fk() {
    // Two link mappings on the same edge type `INTERACTS`:
    //   (a) ForeignKey — direct FK `messages.sender_id` → `users.id`
    //   (b) Bridge      — `interactions(user_id, other_user_id)` table
    //                     (per-branch scan aliased `__br<hop>_<branch>`)
    //
    // Fixture:
    //   users:         (1, Alice), (2, Bob)
    //   messages:      (10, sender=1), (11, sender=2)           → FK path
    //   conversations: (100, sender=2) — different shape than messages,
    //     but the Bridge has its own endpoint so it doesn't matter what
    //     the target column name is in the target source.
    //   interactions:  (user_id=1, other=100), (user_id=2, other=11)
    //                                                          → bridge path
    //
    // Both link mappings feed into the same edge → UNION ALL.
    // Expected row count: FK(2) + Bridge(2) = 4.
    let users_csv = "id,name\n1,Alice\n2,Bob\n";
    let msgs_csv = "id,sender_id\n10,1\n11,2\n";
    let interactions_csv = "user_id,other_id\n1,11\n2,10\n";

    let mut ont = OntologyIR::new(
        "ont-multi-bridge".into(),
        "mb".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-msg".into(),
                label: gl("Message"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-interacts".into(),
            label: gl("INTERACTS"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-msg".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-msg", "nt-msg", "csv-msgs", "records",
    ))
    .unwrap();
    // Link mapping A — direct FK.
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-direct"),
        edge_type_id: "e-interacts".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("messages", "sender_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-msgs"),
            relation: "messages".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();
    // Link mapping B — Bridge through an interactions table.
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-bridge"),
        edge_type_id: "e-interacts".into(),
        kind: LinkMappingKind::Bridge {
            bridge_relation: SourceRelationRef {
                source_id: SourceId::new("csv-interactions"),
                relation: "records".into(),
                kind: SourceRelationKind::Table,
            },
            source_join: vec![ColumnRef::new("interactions", "user_id")],
            target_join: vec![ColumnRef::new("interactions", "other_id")],
            bridge_workspace_scope: None,
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-msgs"),
            relation: "messages".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-msgs",
        Arc::new(CsvAdapter::new(msgs_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-interactions",
        Arc::new(CsvAdapter::new(interactions_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("INTERACTS")),
                source: vn("u"),
                target: vn("m"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("m"),
                label: Some(gl("Message")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("user_name".into()),
            },
            Projection::Field {
                variable: vn("m"),
                field: PropertyKey::new("id").unwrap(),
                alias: Some("msg_id".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("multi-mapping FK+Bridge plan builds");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("multi-mapping FK+Bridge plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 4,
        "FK branch (Alice→10, Bob→11) + Bridge branch (Alice→11, Bob→10) = 4"
    );
    assert_eq!(batches[0].num_columns(), 2);
    let schema = batches[0].schema();
    let fields: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    assert_eq!(fields, vec!["user_name", "msg_id"]);
}

#[tokio::test]
async fn match_multi_mapping_hop_at_extend_position_unions_branches() {
    // Two-hop chain: seed (User—PLACED→Order) is single-mapping; the
    // second hop (Order—RELATED→Item) carries two link mappings. The
    // planner clones the seed join tree per branch, joins Items into
    // each clone, and UNION ALL-s the results. Filter + projection
    // push-down is still load-bearing because DataFusion's UNION
    // strips qualifiers.
    //
    // Fixture: one user, one order, one item. Seed yields 1 row;
    // extend with two identical mappings yields 2 rows (UNION ALL
    // does not deduplicate).
    let users_csv = "id,name\n1,Alice\n";
    let orders_csv = "id,placer_id,recipient_id\n10,1,1\n";
    let items_csv = "id,order_id\n100,10\n";

    let mut ont = OntologyIR::new(
        "ont-multi-nonseed".into(),
        "multi".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-item".into(),
                label: gl("Item"),
                ..Default::default()
            },
        ],
        vec![
            EdgeTypeDef {
                id: "e-placed".into(),
                label: gl("PLACED"),
                source_node_id: "nt-user".into(),
                target_node_id: "nt-order".into(),
                ..Default::default()
            },
            EdgeTypeDef {
                id: "e-related".into(),
                label: gl("RELATED"),
                source_node_id: "nt-order".into(),
                target_node_id: "nt-item".into(),
                ..Default::default()
            },
        ],
        vec![],
    );
    for om in [
        ObjectMappingDef::new("om-user", "nt-user", "csv-users", "records"),
        ObjectMappingDef::new("om-order", "nt-order", "csv-orders", "records"),
        ObjectMappingDef::new("om-item", "nt-item", "csv-items", "records"),
    ] {
        ont.add_object_mapping(om).unwrap();
    }
    // Seed hop: single mapping (User-PLACED->Order).
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-placed"),
        edge_type_id: "e-placed".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "placer_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();
    // Second hop: multi-mapping (two ways to tie orders to items).
    for (id, src) in [("lm-related-a", "order_id"), ("lm-related-b", "order_id")] {
        ont.add_link_mapping(LinkMappingDef {
            id: LinkMappingId::new(id),
            edge_type_id: "e-related".into(),
            kind: LinkMappingKind::ForeignKey {
                source_column: ColumnRef::new("items", src),
                target_column: ColumnRef::new("orders", "id"),
            },
            source_endpoint: EndpointRef {
                source_id: SourceId::new("csv-orders"),
                relation: "orders".into(),
                key_columns: vec!["id".into()],
            },
            target_endpoint: EndpointRef {
                source_id: SourceId::new("csv-items"),
                relation: "items".into(),
                key_columns: vec!["id".into()],
            },
            join_cost_hint: JoinCostHint::Indexed,
            precedence: 100,
            cardinality: ox_ontology::LinkCardinality::ManyToMany,
        })
        .unwrap();
    }

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-orders",
        Arc::new(CsvAdapter::new(orders_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-items",
        Arc::new(CsvAdapter::new(items_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("RELATED")),
                source: vn("o"),
                target: vn("i"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("i"),
                label: Some(gl("Item")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("user_name".into()),
            },
            Projection::Field {
                variable: vn("i"),
                field: PropertyKey::new("id").unwrap(),
                alias: Some("item_id".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("extend-position multi-mapping plan builds");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("extend-position multi-mapping plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 2,
        "two identical branches of the extend hop each yield one row; UNION ALL = 2"
    );
    assert_eq!(batches[0].num_columns(), 2);
    let schema = batches[0].schema();
    let fields: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    assert_eq!(fields, vec!["user_name", "item_id"]);
}

#[tokio::test]
async fn match_multi_mapping_hop_at_close_cycle_unions_predicates() {
    // Close-cycle multi-mapping: both endpoints of the second hop
    // are already in the tree after the seed hop.
    //   MATCH (u:User)-[:PLACED]->(o:Order), (u)-[:RELATED]->(o)
    // RELATED carries two link mappings — each branch clones the
    // seed tree and adds its own predicate as a filter. UNION ALL
    // over the branches produces rows for every way the cycle can
    // close.
    let users_csv = "id,name\n1,Alice\n";
    let orders_csv = "id,placer_id,related_id_a,related_id_b\n10,1,1,1\n";
    let mut ont = OntologyIR::new(
        "ont-close-cycle".into(),
        "cc".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-order".into(),
                label: gl("Order"),
                ..Default::default()
            },
        ],
        vec![
            EdgeTypeDef {
                id: "e-placed".into(),
                label: gl("PLACED"),
                source_node_id: "nt-user".into(),
                target_node_id: "nt-order".into(),
                ..Default::default()
            },
            EdgeTypeDef {
                id: "e-related".into(),
                label: gl("RELATED"),
                source_node_id: "nt-user".into(),
                target_node_id: "nt-order".into(),
                ..Default::default()
            },
        ],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-order",
        "nt-order",
        "csv-orders",
        "records",
    ))
    .unwrap();
    // Seed hop: single mapping.
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-placed"),
        edge_type_id: "e-placed".into(),
        kind: LinkMappingKind::ForeignKey {
            source_column: ColumnRef::new("orders", "placer_id"),
            target_column: ColumnRef::new("users", "id"),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-orders"),
            relation: "orders".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();
    // Close-cycle hop: two link mappings on the same edge between
    // (u, o) where both are already in the tree.
    for (id, src) in [
        ("lm-related-a", "related_id_a"),
        ("lm-related-b", "related_id_b"),
    ] {
        ont.add_link_mapping(LinkMappingDef {
            id: LinkMappingId::new(id),
            edge_type_id: "e-related".into(),
            kind: LinkMappingKind::ForeignKey {
                source_column: ColumnRef::new("orders", src),
                target_column: ColumnRef::new("users", "id"),
            },
            source_endpoint: EndpointRef {
                source_id: SourceId::new("csv-users"),
                relation: "users".into(),
                key_columns: vec!["id".into()],
            },
            target_endpoint: EndpointRef {
                source_id: SourceId::new("csv-orders"),
                relation: "orders".into(),
                key_columns: vec!["id".into()],
            },
            join_cost_hint: JoinCostHint::Indexed,
            precedence: 100,
            cardinality: ox_ontology::LinkCardinality::ManyToMany,
        })
        .unwrap();
    }
    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-orders",
        Arc::new(CsvAdapter::new(orders_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("PLACED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Order")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("RELATED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("u"),
                field: PropertyKey::new("name").unwrap(),
                alias: Some("user_name".into()),
            },
            Projection::Field {
                variable: vn("o"),
                field: PropertyKey::new("id").unwrap(),
                alias: Some("order_id".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("close-cycle multi-mapping plan builds");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("close-cycle multi-mapping plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    // Seed (User-PLACED-Order) yields 1 row (Alice, order 10). Each
    // RELATED branch filters on related_id_{a,b} = users.id; both
    // succeed in this fixture (Alice.id = related_id_a = related_id_b
    // = 1), so each branch emits 1 row and the UNION ALL produces 2.
    assert_eq!(total_rows, 2, "two link mappings each close the cycle once");
    assert_eq!(batches[0].num_columns(), 2);
    let schema = batches[0].schema();
    let fields: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    assert_eq!(fields, vec!["user_name", "order_id"]);
}

#[tokio::test]
async fn match_bridge_with_composite_keys_ands_predicates_per_side() {
    // Composite PK on the target endpoint: a Warehouse is keyed by
    // `(region, code)`. The bridge `inventory(region, code, sku)`
    // links products to warehouses using both key columns. The
    // planner must AND two equi-predicates on the target side —
    // `w.region = inv.region AND w.code = inv.code` — plus the
    // single-column source-side predicate on products.
    //
    // Fixture:
    //   products:  (sku=A, ...), (sku=B)
    //   warehouses: (region=us, code=1), (region=us, code=2),
    //               (region=eu, code=1)
    //   inventory:
    //     (sku=A, region=us, code=1)  -- valid
    //     (sku=A, region=eu, code=1)  -- valid
    //     (sku=B, region=us, code=2)  -- valid
    //     (sku=B, region=us, code=99) -- warehouse (us,99) does not
    //                                   exist, dropped by AND-ed
    //                                   predicate
    //
    // Expected: 3 rows (A×us-1, A×eu-1, B×us-2). The orphan
    // (B,us,99) is the signal that the AND really combined both
    // columns — with just region matching, it would pass.
    let products_csv = "sku,name\nA,Alpha\nB,Bravo\n";
    let warehouses_csv = "region,code\nus,1\nus,2\neu,1\n";
    let inventory_csv = "sku,region,code\nA,us,1\nA,eu,1\nB,us,2\nB,us,99\n";

    let mut ont = OntologyIR::new(
        "ont-inv".into(),
        "inv".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-product".into(),
                label: gl("Product"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-warehouse".into(),
                label: gl("Warehouse"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-stocked".into(),
            label: gl("STOCKED_AT"),
            source_node_id: "nt-product".into(),
            target_node_id: "nt-warehouse".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-product",
        "nt-product",
        "csv-products",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-warehouse",
        "nt-warehouse",
        "csv-warehouses",
        "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-stocked"),
        edge_type_id: "e-stocked".into(),
        kind: LinkMappingKind::Bridge {
            bridge_relation: SourceRelationRef {
                source_id: SourceId::new("csv-inventory"),
                relation: "records".into(),
                kind: SourceRelationKind::Table,
            },
            // Single-column source endpoint (products keyed by sku).
            source_join: vec![ColumnRef::new("inventory", "sku")],
            // Composite target endpoint (warehouses keyed by
            // region + code). Bridge carries both columns; the
            // planner zips them with warehouse.key_columns.
            target_join: vec![
                ColumnRef::new("inventory", "region"),
                ColumnRef::new("inventory", "code"),
            ],
            bridge_workspace_scope: None,
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-products"),
            relation: "products".into(),
            key_columns: vec!["sku".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-warehouses"),
            relation: "warehouses".into(),
            key_columns: vec!["region".into(), "code".into()],
        },
        join_cost_hint: JoinCostHint::Indexed,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-products",
        Arc::new(CsvAdapter::new(products_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-warehouses",
        Arc::new(CsvAdapter::new(warehouses_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-inventory",
        Arc::new(CsvAdapter::new(inventory_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("p"),
                label: Some(gl("Product")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("STOCKED_AT")),
                source: vn("p"),
                target: vn("w"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("w"),
                label: Some(gl("Warehouse")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![
            Projection::Field {
                variable: vn("p"),
                field: PropertyKey::new("sku").unwrap(),
                alias: Some("product_sku".into()),
            },
            Projection::Field {
                variable: vn("w"),
                field: PropertyKey::new("code").unwrap(),
                alias: Some("warehouse_code".into()),
            },
        ],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver)
        .await
        .expect("composite-key bridge plan builds");
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("composite-key bridge plan executes");
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(
        total_rows, 3,
        "the AND-ed (region, code) predicate drops the (B, us, 99) row \
         because warehouse (us, 99) does not exist"
    );
    assert_eq!(batches[0].num_columns(), 2);
}

#[tokio::test]
async fn match_computed_link_mapping_filters_via_arbitrary_predicate() {
    // `Computed` link mapping: the edge is defined by an arbitrary
    // SQL predicate. The federation planner CROSS JOINs the two
    // endpoint scans, parses the predicate via DataFusion's SQL
    // expression parser, and applies it as a filter. DataFusion's
    // optimiser lifts filter-after-cross-join back into a proper
    // join at execute time.
    //
    // Fixture: match users to messages whose `topic` starts with
    // the user's `name_prefix` — the non-FK edge Computed is meant
    // to express.
    let users_csv = "id,name_prefix\n1,ali\n2,bob\n";
    let messages_csv = "id,topic\n10,ali-question\n11,bob-question\n12,ali-update\n13,carol-note\n";

    let mut ont = OntologyIR::new(
        "ont-computed".into(),
        "computed".into(),
        LocalizedText::default(),
        1,
        vec![
            NodeTypeDef {
                id: "nt-user".into(),
                label: gl("User"),
                ..Default::default()
            },
            NodeTypeDef {
                id: "nt-msg".into(),
                label: gl("Message"),
                ..Default::default()
            },
        ],
        vec![EdgeTypeDef {
            id: "e-authored".into(),
            label: gl("AUTHORED"),
            source_node_id: "nt-user".into(),
            target_node_id: "nt-msg".into(),
            ..Default::default()
        }],
        vec![],
    );
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-user",
        "nt-user",
        "csv-users",
        "records",
    ))
    .unwrap();
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-msg", "nt-msg", "csv-msgs", "records",
    ))
    .unwrap();
    ont.add_link_mapping(LinkMappingDef {
        id: LinkMappingId::new("lm-authored"),
        edge_type_id: "e-authored".into(),
        kind: LinkMappingKind::Computed {
            predicate: "starts_with(o.topic, u.name_prefix)".into(),
        },
        source_endpoint: EndpointRef {
            source_id: SourceId::new("csv-users"),
            relation: "users".into(),
            key_columns: vec!["id".into()],
        },
        target_endpoint: EndpointRef {
            source_id: SourceId::new("csv-msgs"),
            relation: "messages".into(),
            key_columns: vec!["id".into()],
        },
        join_cost_hint: JoinCostHint::Scan,
        precedence: 100,
        cardinality: ox_ontology::LinkCardinality::ManyToMany,
    })
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    resolver.register(
        "csv-users",
        Arc::new(CsvAdapter::new(users_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );
    resolver.register(
        "csv-msgs",
        Arc::new(CsvAdapter::new(messages_csv).unwrap()) as Arc<dyn DataSourceAdapter>,
    );

    let op = QueryOp::Match {
        patterns: vec![
            GraphPattern::Node {
                variable: vn("u"),
                label: Some(gl("User")),
                property_filters: vec![],
            },
            GraphPattern::Relationship {
                variable: None,
                label: Some(gl("AUTHORED")),
                source: vn("u"),
                target: vn("o"),
                direction: Direction::Outgoing,
                property_filters: vec![],
                var_length: None,
            },
            GraphPattern::Node {
                variable: vn("o"),
                label: Some(gl("Message")),
                property_filters: vec![],
            },
        ],
        filter: None,
        projections: vec![],
        optional: false,
        group_by: vec![],
    };

    let plan = build_match_op(&ont, &op, &resolver).await.unwrap();
    let ctx = FederationContext::new(WorkspaceRef::new("ws-test"));
    let batches = ctx.execute_plan(plan).await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    // Expected matches (by `starts_with(topic, prefix)`):
    //   Alice "ali" → "ali-question", "ali-update"  = 2
    //   Bob   "bob" → "bob-question"                = 1
    //   "carol-note" matches neither prefix
    assert_eq!(
        total_rows, 3,
        "Computed predicate matches Alice × 2 + Bob × 1"
    );
}

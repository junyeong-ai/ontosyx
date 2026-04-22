//! Slice W1 + W2 wire-up test.
//!
//! ox-api does not have an end-to-end axum test harness yet (building
//! a full `AppState` requires a live store + compiler + runtime), so
//! this test exercises the same pipeline the `execute_from_ir_federation`
//! handler runs *under* the axum layer:
//!
//!   InMemoryAdapterResolver (what slice W2's admin endpoint writes
//!       into state.federation_resolver)
//!        ↓
//!   build_query_ir_scoped (what the slice W1 handler calls)
//!        ↓
//!   FederationContext::execute_plan
//!        ↓
//!   ox_api::arrow_conversion::record_batches_to_query_result
//!        ↓
//!   ox_query_ir::query::QueryResult
//!
//! If the handler code itself drifts (different argument order,
//! wrong arg plumbing) a future axum-level test will catch that. For
//! now this test pins the middle of the pipeline — everything between
//! "admin registered a CSV" and "client received JSON rows".
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ox_api::arrow_conversion::record_batches_to_query_result;
use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyValue;
use ox_core::variable_name::VariableName;
use ox_federation::{
    FederationContext, InMemoryAdapterResolver, build_query_ir_scoped, context::WorkspaceRef,
};
use ox_ontology::OntologyIR;
use ox_ontology::ir::NodeTypeDef;
use ox_ontology::mapping::ObjectMappingDef;
use ox_query_ir::query::{
    GraphPattern, Projection, QueryIR, QueryOp, QUERY_IR_SCHEMA_VERSION,
};
use ox_source::DataSourceAdapter;
use ox_source::sample::CsvAdapter;

#[tokio::test]
async fn register_csv_adapter_then_execute_federation_query() {
    // 1. Ontology: one NodeType `Customer` mapped to source `csv-crm`.
    let mut ontology = OntologyIR::new(
        "ont".into(),
        "wire-test".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-customer".into(),
            label: GraphLabel::new("Customer").unwrap(),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    ontology
        .add_object_mapping(ObjectMappingDef::new(
            "om-customer",
            "nt-customer",
            "csv-crm",
            "records",
        ))
        .unwrap();

    // 2. Register an adapter into the resolver — this stands in for
    //    slice W2's admin endpoint writing to `AppState::federation_resolver`.
    let mut resolver = InMemoryAdapterResolver::new();
    let csv = "id,name,amount\n1,Alice,100\n2,Bob,250\n3,Charlie,42\n";
    let adapter: Arc<dyn DataSourceAdapter> = Arc::new(CsvAdapter::new(csv).unwrap());
    resolver.register("csv-crm", adapter);
    assert_eq!(resolver.len(), 1);

    // 3. Build the QueryIR the client would submit:
    //    MATCH (c:Customer) RETURN c.name AS customer_name, c.amount AS amount
    let query = QueryIR {
        schema_version: QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: VariableName::new("c").unwrap(),
                label: Some(GraphLabel::new("Customer").unwrap()),
                property_filters: vec![],
            }],
            filter: None,
            projections: vec![
                Projection::Field {
                    variable: VariableName::new("c").unwrap(),
                    field: PropertyKey::new("name").unwrap(),
                    alias: Some("customer_name".into()),
                },
                Projection::Field {
                    variable: VariableName::new("c").unwrap(),
                    field: PropertyKey::new("amount").unwrap(),
                    alias: Some("amount".into()),
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

    // 4. Run the same call `execute_from_ir_federation` makes.
    let plan = build_query_ir_scoped(&ontology, &query, "ws-wire-test", &resolver)
        .await
        .expect("scoped plan builds");

    let ctx = FederationContext::new(WorkspaceRef::new("ws-wire-test"));
    let batches = ctx.execute_plan(plan).await.expect("plan executes");

    // 5. Convert through the helper the handler uses.
    let result = record_batches_to_query_result(&batches, 42).expect("conversion succeeds");

    // 6. Assert the shape an HTTP client would see.
    assert_eq!(result.columns, vec!["customer_name", "amount"]);
    assert_eq!(result.metadata.rows_returned, 3);
    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.metadata.execution_time_ms, 42);

    // Row values (order preserved from the CSV).
    assert_eq!(result.rows[0][0], PropertyValue::String("Alice".into()));
    assert_eq!(result.rows[0][1], PropertyValue::Int(100));
    assert_eq!(result.rows[1][0], PropertyValue::String("Bob".into()));
    assert_eq!(result.rows[1][1], PropertyValue::Int(250));
    assert_eq!(result.rows[2][0], PropertyValue::String("Charlie".into()));
    assert_eq!(result.rows[2][1], PropertyValue::Int(42));
}

#[tokio::test]
async fn query_fails_cleanly_when_source_id_has_no_registered_adapter() {
    // Emulates a client submitting a query that names a source the
    // admin never registered. The handler surfaces this as
    // `FederationError::Unsupported` → HTTP 422. Here we just assert
    // the resolver layer produces a descriptive error.
    let mut ontology = OntologyIR::new(
        "ont".into(),
        "wire-test".into(),
        LocalizedText::default(),
        1,
        vec![NodeTypeDef {
            id: "nt-customer".into(),
            label: GraphLabel::new("Customer").unwrap(),
            ..Default::default()
        }],
        vec![],
        vec![],
    );
    ontology
        .add_object_mapping(ObjectMappingDef::new(
            "om-customer",
            "nt-customer",
            "csv-missing",
            "records",
        ))
        .unwrap();

    let resolver = InMemoryAdapterResolver::new(); // no adapters
    let query = QueryIR {
        schema_version: QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: VariableName::new("c").unwrap(),
                label: Some(GraphLabel::new("Customer").unwrap()),
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

    let err = build_query_ir_scoped(&ontology, &query, "ws-wire-test", &resolver)
        .await
        .expect_err("unregistered source must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("no adapter registered"),
        "error must name the missing source: got {msg}"
    );
    assert!(
        msg.contains("csv-missing"),
        "error must name the source_id: got {msg}"
    );
}

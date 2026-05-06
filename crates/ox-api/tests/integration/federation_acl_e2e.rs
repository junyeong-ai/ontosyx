//! End-to-end golden: `QueryIR` → federation planner → DataFusion
//! execute → Arrow `RecordBatch` → `QueryResult` →
//! `enforce_acl_on_result`.
//!
//! The federation crate's existing `tests/match_end_to_end.rs`
//! covers the planner + execute path. The ACL post-process step
//! lives in `ox-api` because the snapshot is loaded by the
//! workspace middleware. This test pins the *integration* —
//! the column the policy names is the column that gets masked
//! / dropped *after* the federation engine has handed Arrow
//! batches back.
//!
//! Three scenarios cover the policy actions the loader emits:
//!
//! - **Empty snapshot** — passthrough: every projected column
//!   reaches the response unchanged.
//! - **Mask** — the named property in the snapshot is replaced
//!   with the policy's `mask_pattern` for every row.
//! - **Deny** — the named property is dropped entirely from
//!   `result.columns` and from every `result.rows` cell.
//!
//! When the federation path grows its own pre-execute ACL
//! visitor (see ROADMAP "Out of scope: DataFusion-side ACL
//! pre-execute"), this test stays valid: post-process becomes a
//! no-op fallback rather than the only enforcement.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

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
    GraphPattern, Projection, QueryIR, QueryOp,
};
use ox_graph_runtime::cypher::{AclAction, AclPolicySpec, AclSnapshot};
use ox_source::DataSourceAdapter;
use ox_source::sample::CsvAdapter;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn gl(s: &str) -> GraphLabel {
    GraphLabel::new(s).expect("valid graph label")
}

fn vn(s: &str) -> VariableName {
    VariableName::new(s).expect("valid variable name")
}

fn pk(s: &str) -> PropertyKey {
    PropertyKey::new(s).expect("valid property key")
}

/// CSV with a name column and a sensitive `ssn` column. The Mask
/// scenario asserts `ssn` is rewritten while `name` survives;
/// the Deny scenario asserts `ssn` disappears while `name` is
/// untouched.
const SENSITIVE_CSV: &str = "id,name,ssn\n\
                              1,Alice,123-45-6789\n\
                              2,Bob,987-65-4321\n";

/// Single-NodeType ontology mapped to an in-memory CSV adapter.
/// `Customer` carries explicit `name` + `ssn` properties so the
/// planner accepts the projection's property keys; the adapter
/// returns the same column names so the projection alias and the
/// schema field name coincide.
fn build_customer_ontology() -> (OntologyIR, InMemoryAdapterResolver) {
    let ont = OntologyIR::new(
        "ont-acl".into(),
        "acl-fixture".into(),
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
    let mut ont = ont;
    ont.add_object_mapping(ObjectMappingDef::new(
        "om-customer",
        "nt-customer",
        "csv-crm",
        "records",
    ))
    .unwrap();

    let mut resolver = InMemoryAdapterResolver::new();
    let adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(CsvAdapter::new(SENSITIVE_CSV).expect("csv adapter"));
    resolver.register("csv-crm", adapter);

    (ont, resolver)
}

/// `MATCH (c:Customer) RETURN c.name AS name, c.ssn AS ssn` —
/// projection aliases match the property names so the resulting
/// schema field name is exactly what the ACL snapshot's
/// `properties` whitelist names.
fn select_name_and_ssn() -> QueryIR {
    QueryIR {
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
                    field: pk("name"),
                    alias: Some("name".into()),
                },
                Projection::Field {
                    variable: vn("c"),
                    field: pk("ssn"),
                    alias: Some("ssn".into()),
                },
            ],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: None,
    }
}

async fn execute_against_fixture(query: &QueryIR) -> ox_query_ir::query::QueryResult {
    let (ont, resolver) = build_customer_ontology();
    let workspace_id = "ws-acl-e2e";
    let plan = build_query_ir_scoped(&ont, query, workspace_id, &resolver)
        .await
        .expect("federation planner accepts the fixture query");
    let ctx = FederationContext::new(WorkspaceRef::new(workspace_id));
    let batches = ctx
        .execute_plan(plan)
        .await
        .expect("execute_plan drives the plan to completion");
    ox_api::arrow_conversion::record_batches_to_query_result(&batches, 0)
        .expect("record_batches_to_query_result accepts the batches")
}

fn mask_policy_on_ssn() -> AclSnapshot {
    AclSnapshot {
        policies: vec![AclPolicySpec {
            action: AclAction::Mask,
            resource_type: "label".into(),
            resource_value: Some("Customer".into()),
            properties: Some(vec!["ssn".into()]),
            mask_pattern: Some("***".into()),
            priority: 100,
        }],
    }
}

fn deny_policy_on_ssn() -> AclSnapshot {
    AclSnapshot {
        policies: vec![AclPolicySpec {
            action: AclAction::Deny,
            resource_type: "label".into(),
            resource_value: Some("Customer".into()),
            properties: Some(vec!["ssn".into()]),
            mask_pattern: None,
            priority: 100,
        }],
    }
}

fn string_at(result: &ox_query_ir::query::QueryResult, row: usize, col: usize) -> &str {
    match result.rows.get(row).and_then(|r| r.get(col)) {
        Some(PropertyValue::String(s)) => s.as_str(),
        other => panic!("expected string at row={row} col={col}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_snapshot_passes_every_projected_column_through() {
    let mut result = execute_against_fixture(&select_name_and_ssn()).await;
    let snapshot = AclSnapshot::empty();

    ox_api::acl_enforcement::enforce_acl_on_result(&mut result, &snapshot);

    assert_eq!(result.columns, vec!["name".to_string(), "ssn".to_string()]);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(string_at(&result, 0, 0), "Alice");
    assert_eq!(string_at(&result, 0, 1), "123-45-6789");
    assert_eq!(string_at(&result, 1, 0), "Bob");
    assert_eq!(string_at(&result, 1, 1), "987-65-4321");
}

#[tokio::test]
async fn mask_policy_replaces_named_property_with_mask_pattern() {
    let mut result = execute_against_fixture(&select_name_and_ssn()).await;

    ox_api::acl_enforcement::enforce_acl_on_result(&mut result, &mask_policy_on_ssn());

    assert_eq!(
        result.columns,
        vec!["name".to_string(), "ssn".to_string()],
        "Mask preserves the column structure — only cell values change",
    );
    assert_eq!(string_at(&result, 0, 0), "Alice", "name column untouched");
    assert_eq!(
        string_at(&result, 0, 1),
        "***",
        "ssn replaced with the policy's mask pattern",
    );
    assert_eq!(string_at(&result, 1, 0), "Bob");
    assert_eq!(string_at(&result, 1, 1), "***");
}

#[tokio::test]
async fn deny_policy_drops_named_property_from_result_entirely() {
    let mut result = execute_against_fixture(&select_name_and_ssn()).await;

    ox_api::acl_enforcement::enforce_acl_on_result(&mut result, &deny_policy_on_ssn());

    assert_eq!(
        result.columns,
        vec!["name".to_string()],
        "Deny removes the column from the wire shape — clients can't \
         reconstruct it from the response",
    );
    for row in &result.rows {
        assert_eq!(row.len(), 1, "every row matches the truncated columns");
    }
    assert_eq!(string_at(&result, 0, 0), "Alice");
    assert_eq!(string_at(&result, 1, 0), "Bob");
}

#[tokio::test]
async fn deny_supersedes_mask_when_both_target_the_same_property() {
    // The verified semantic of `enforce_acl_on_result`: Deny lands
    // first in the column-removal pass and Mask is only applied to
    // properties that aren't denied. Same-column Mask + Deny resolve
    // to "column dropped, mask never applied" — Deny wins by being
    // absolute, regardless of which policy comes first in the
    // snapshot. This pin guards against a future refactor that
    // reorders the deny/mask passes and silently lets a sensitive
    // column ship as the literal mask pattern.
    let mut result = execute_against_fixture(&select_name_and_ssn()).await;

    let combined = AclSnapshot {
        policies: vec![
            AclPolicySpec {
                action: AclAction::Mask,
                resource_type: "label".into(),
                resource_value: Some("Customer".into()),
                properties: Some(vec!["ssn".into()]),
                mask_pattern: Some("MASKED".into()),
                priority: 100,
            },
            AclPolicySpec {
                action: AclAction::Deny,
                resource_type: "label".into(),
                resource_value: Some("Customer".into()),
                properties: Some(vec!["ssn".into()]),
                mask_pattern: None,
                priority: 50,
            },
        ],
    };

    ox_api::acl_enforcement::enforce_acl_on_result(&mut result, &combined);

    assert_eq!(
        result.columns,
        vec!["name".to_string()],
        "Deny is absolute — column removed, mask pattern never reaches the wire",
    );
    for row in &result.rows {
        assert!(
            row.iter().all(|v| !matches!(
                v,
                PropertyValue::String(s) if s == "MASKED"
            )),
            "the mask pattern must never surface for a denied column",
        );
    }
}

#[tokio::test]
async fn mask_and_deny_on_distinct_columns_apply_additively() {
    // Independent properties — both policies apply because the deny
    // set and the mask set don't intersect. The non-denied column
    // gets its mask pattern; the denied column disappears.
    let csv = "id,name,ssn\n1,Alice,123-45-6789\n2,Bob,987-65-4321\n";
    let (ont, resolver) = build_customer_ontology_with_csv(csv);
    let workspace_id = "ws-acl-e2e";
    let plan = build_query_ir_scoped(&ont, &select_name_and_ssn(), workspace_id, &resolver)
        .await
        .expect("planner accepts the fixture query");
    let ctx = FederationContext::new(WorkspaceRef::new(workspace_id));
    let batches = ctx.execute_plan(plan).await.expect("execute_plan");
    let mut result =
        ox_api::arrow_conversion::record_batches_to_query_result(&batches, 0).expect("convert");

    let multi = AclSnapshot {
        policies: vec![
            AclPolicySpec {
                action: AclAction::Mask,
                resource_type: "label".into(),
                resource_value: Some("Customer".into()),
                properties: Some(vec!["name".into()]),
                mask_pattern: Some("[redacted]".into()),
                priority: 100,
            },
            AclPolicySpec {
                action: AclAction::Deny,
                resource_type: "label".into(),
                resource_value: Some("Customer".into()),
                properties: Some(vec!["ssn".into()]),
                mask_pattern: None,
                priority: 100,
            },
        ],
    };

    ox_api::acl_enforcement::enforce_acl_on_result(&mut result, &multi);

    assert_eq!(
        result.columns,
        vec!["name".to_string()],
        "ssn dropped by Deny; name kept (masked) — distinct columns merge the two actions",
    );
    assert_eq!(string_at(&result, 0, 0), "[redacted]");
    assert_eq!(string_at(&result, 1, 0), "[redacted]");
}

#[tokio::test]
async fn policy_targeting_a_different_label_is_a_noop() {
    // Sanity: an `Order`-targeted policy must not affect a Customer
    // query. The post-process layer compares column names against
    // the snapshot's `properties` whitelist, but the production
    // load-acl-snapshot path filters by `resource_value` upstream.
    // This test pins the post-process behaviour: an in-snapshot
    // policy whose `properties` don't intersect the result's
    // columns is a no-op.
    let mut result = execute_against_fixture(&select_name_and_ssn()).await;

    let unrelated = AclSnapshot {
        policies: vec![AclPolicySpec {
            action: AclAction::Mask,
            resource_type: "label".into(),
            resource_value: Some("Order".into()),
            properties: Some(vec!["total".into()]),
            mask_pattern: Some("***".into()),
            priority: 100,
        }],
    };

    ox_api::acl_enforcement::enforce_acl_on_result(&mut result, &unrelated);

    assert_eq!(result.columns, vec!["name".to_string(), "ssn".to_string()]);
    assert_eq!(string_at(&result, 0, 0), "Alice");
    assert_eq!(string_at(&result, 0, 1), "123-45-6789");
}

// Helper for `mask_and_deny_on_distinct_columns_apply_additively` — the
// other tests use the module-level `execute_against_fixture` which
// hard-codes the SENSITIVE_CSV constant. Letting this scenario specify
// its own CSV keeps the rebuild minimal.
fn build_customer_ontology_with_csv(csv: &str) -> (OntologyIR, InMemoryAdapterResolver) {
    let mut ont = OntologyIR::new(
        "ont-acl".into(),
        "acl-fixture".into(),
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
    let adapter: Arc<dyn DataSourceAdapter> =
        Arc::new(CsvAdapter::new(csv).expect("csv adapter"));
    resolver.register("csv-crm", adapter);

    (ont, resolver)
}

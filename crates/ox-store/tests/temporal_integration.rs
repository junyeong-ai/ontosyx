//! Phase 2-2 end-to-end Temporal AS-OF integration.
//!
//! Validates that the full pipeline — `get_ontology_version_at` →
//! `rewrite_temporal_with_renames` — resolves a point-in-time snapshot
//! and rewrites the query's labels into the snapshot's label space.
//!
//! Fixture: one lineage id, two versions.
//!   v1 committed 2026-01-01 — node labelled `Client`,
//!      window [2026-01-01, 2026-06-01).
//!   v2 committed 2026-06-01 — same node id, renamed to `Customer`,
//!      window [2026-06-01, open).
//!
//! A request written today against `Customer` with `as_of = 2026-03-15`
//! must resolve to v1 and be rewritten to `Client` before compilation.
//!
//! Ignored by default — same requirement as `rls_enforcement.rs`:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test temporal_integration -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use chrono::{DateTime, TimeZone, Utc};
use ox_compiler::rewrite_temporal_with_renames;
use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::ontology_ir::{NodeTypeDef, OntologyIR, OntologyVersion, PropertyDef};
use ox_core::property_key::PropertyKey;
use ox_core::query_ir::{
    ComparisonOp, Expr, GraphPattern, PropertyFilter, QUERY_IR_SCHEMA_VERSION, QueryIR, QueryOp,
};
use ox_core::types::{PropertyType, PropertyValue};
use ox_core::variable_name::VariableName;
use ox_store::{OntologyStore, PostgresStore};
use uuid::Uuid;

fn resolve_test_db_url() -> Option<String> {
    for key in ["OX_TEST_DATABASE_URL", "OX_DATABASE_URL", "DATABASE_URL"] {
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
        {
            return Some(v);
        }
    }
    None
}

async fn connect_store() -> Option<PostgresStore> {
    let url = resolve_test_db_url()?;
    let store = PostgresStore::connect(&url, 4).await.ok()?;
    store.migrate().await.ok()?;
    Some(store)
}

fn ts(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0)
        .single()
        .expect("test timestamp")
}

fn gl(s: &str) -> GraphLabel {
    GraphLabel::new(s).expect("graph label")
}

fn vn(s: &str) -> VariableName {
    VariableName::new(s).expect("variable name")
}

/// Build an OntologyIR with a single node type. The lineage id is the
/// same across versions; only the label and version window change.
fn build_ontology(
    lineage_id: &str,
    version_number: u32,
    valid_from: DateTime<Utc>,
    valid_to: Option<DateTime<Utc>>,
    node_label: &str,
) -> OntologyIR {
    OntologyIR::new(
        lineage_id.to_string(),
        "Temporal Test".to_string(),
        LocalizedText::default(),
        OntologyVersion {
            number: version_number,
            valid_from: Some(valid_from),
            valid_to,
            committed_by: None,
            commit_message: None,
        },
        vec![NodeTypeDef {
            id: "nt_party".into(),
            label: gl(node_label),
            description: LocalizedText::default(),
            properties: vec![],
            constraints: vec![],
            ..Default::default()
        }],
        vec![],
        vec![],
    )
}

struct TemporalFixture {
    user_id: Uuid,
    workspace_id: Uuid,
    lineage_id: String,
}

async fn seed_fixture(store: &PostgresStore) -> TemporalFixture {
    let suffix = Uuid::new_v4().simple().to_string();
    let short = suffix[..8].to_string();
    let lineage_id = format!("temporal-lineage-{short}");
    let v1_created_at = ts(2026, 1, 1);
    let v2_created_at = ts(2026, 6, 1);

    let v1 = build_ontology(
        &lineage_id,
        1,
        v1_created_at,
        Some(v2_created_at),
        "Client",
    );
    let v2 = build_ontology(&lineage_id, 2, v2_created_at, None, "Customer");

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let user_email = format!("temporal-test-{short}@example.com");
        let provider_sub = format!("temporal-test-sub-{short}");
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'Temporal Test User', 'test', $2, 'designer') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        let ws_slug = format!("temporal-ws-{short}");
        let workspace_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('Temporal Workspace', $1, $2) \
             RETURNING id",
        )
        .bind(&ws_slug)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace");

        let v1_ir = serde_json::to_value(&v1).expect("serialize v1");
        let v2_ir = serde_json::to_value(&v2).expect("serialize v2");
        let v1_name = format!("temporal-ontology-{short}");
        let v2_name = v1_name.clone();

        sqlx::query(
            "INSERT INTO saved_ontologies \
             (id, workspace_id, name, version, ontology_ir, created_by, created_at) \
             VALUES ($1, $2, $3, 1, $4, 'temporal-test', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(&v1_name)
        .bind(&v1_ir)
        .bind(v1_created_at)
        .execute(pool)
        .await
        .expect("insert v1");

        sqlx::query(
            "INSERT INTO saved_ontologies \
             (id, workspace_id, name, version, ontology_ir, created_by, created_at) \
             VALUES ($1, $2, $3, 2, $4, 'temporal-test', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(&v2_name)
        .bind(&v2_ir)
        .bind(v2_created_at)
        .execute(pool)
        .await
        .expect("insert v2");

        TemporalFixture {
            user_id,
            workspace_id,
            lineage_id,
        }
    })
    .await
}

async fn cleanup(store: &PostgresStore, fx: &TemporalFixture) {
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let _ = sqlx::query("DELETE FROM saved_ontologies WHERE workspace_id = $1")
            .bind(fx.workspace_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(fx.workspace_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(fx.user_id)
            .execute(pool)
            .await;
    })
    .await;
}

fn current_query_referencing_customer() -> QueryIR {
    // A query authored against the current (v2) ontology — it knows the
    // node type as `Customer`. The `as_of` will pivot it to v1 where
    // the same node type id was labelled `Client`.
    QueryIR {
        schema_version: QUERY_IR_SCHEMA_VERSION,
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
        as_of: Some(ts(2026, 3, 15)),
    }
}

// ---------------------------------------------------------------------------
// 1. `get_ontology_version_at` picks the newest row whose created_at
//    predates the requested timestamp.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn get_ontology_version_at_picks_v1_mid_window() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let mid_v1 = ts(2026, 3, 15);
    let snapshot = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_ontology_version_at(&fx.lineage_id, mid_v1)
            .await
            .expect("store call")
    })
    .await
    .expect("v1 must be live at mid-window");

    assert_eq!(snapshot.version, 1, "mid-window should resolve to v1");
    let ir: OntologyIR = serde_json::from_value(snapshot.ontology_ir).expect("decode");
    assert_eq!(
        ir.node_types().iter().next().map(|n| n.label.as_str()),
        Some("Client"),
        "v1's label was Client",
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 2. Same resolver at / after v2.created_at should pick v2.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn get_ontology_version_at_picks_v2_after_cutover() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let after_cutover = ts(2026, 8, 1);
    let snapshot = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_ontology_version_at(&fx.lineage_id, after_cutover)
            .await
            .expect("store call")
    })
    .await
    .expect("v2 must be live after cutover");

    assert_eq!(snapshot.version, 2, "post-cutover should resolve to v2");
    let ir: OntologyIR = serde_json::from_value(snapshot.ontology_ir).expect("decode");
    assert_eq!(
        ir.node_types().iter().next().map(|n| n.label.as_str()),
        Some("Customer"),
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 3. Timestamp before the lineage's first commit — resolver returns None.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn get_ontology_version_at_returns_none_before_lineage() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let before_v1 = ts(2025, 12, 1);
    let snapshot = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_ontology_version_at(&fx.lineage_id, before_v1)
            .await
            .expect("store call")
    })
    .await;

    assert!(
        snapshot.is_none(),
        "no version predates {before_v1}; resolver must return None"
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 4. End-to-end: store resolver → rewriter. A query written today against
//    `Customer` with as_of=mid-v1 must be rewritten to `Client`.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn rewriter_rewrites_current_label_to_snapshot_label_end_to_end() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    // Current ontology = v2 (latest).
    let current_saved = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_latest_ontology_by_lineage(&fx.lineage_id)
            .await
            .expect("store call")
    })
    .await
    .expect("latest lineage row exists");
    assert_eq!(current_saved.version, 2);
    let current: OntologyIR =
        serde_json::from_value(current_saved.ontology_ir).expect("decode current");

    // Snapshot at as_of = 2026-03-15 → v1.
    let query = current_query_referencing_customer();
    let as_of = query.as_of.expect("query carries as_of");
    let snapshot_saved = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_ontology_version_at(&fx.lineage_id, as_of)
            .await
            .expect("store call")
    })
    .await
    .expect("v1 resolvable");
    assert_eq!(snapshot_saved.version, 1);
    let snapshot: OntologyIR =
        serde_json::from_value(snapshot_saved.ontology_ir).expect("decode snapshot");

    let rewritten = rewrite_temporal_with_renames(query, &snapshot, &current)
        .expect("rewrite ok");

    assert!(
        rewritten.as_of.is_none(),
        "rewriter must clear as_of after resolving the snapshot"
    );
    let QueryOp::Match { patterns, .. } = &rewritten.operation else {
        panic!("expected Match");
    };
    let GraphPattern::Node { label, .. } = &patterns[0] else {
        panic!("expected Node pattern");
    };
    assert_eq!(
        label.as_ref().map(|l| l.as_str()),
        Some("Client"),
        "label must be rewritten to the snapshot-era name"
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 5. as_of inside v2's window is a no-op for label rewriting — the
//    query's `Customer` is already the snapshot-era label.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn rewriter_no_op_when_as_of_matches_current_label() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let current_saved = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_latest_ontology_by_lineage(&fx.lineage_id)
            .await
            .expect("store call")
    })
    .await
    .expect("latest lineage row exists");
    let current: OntologyIR =
        serde_json::from_value(current_saved.ontology_ir).expect("decode current");

    let as_of_in_v2 = ts(2026, 8, 1);
    let snapshot_saved = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_ontology_version_at(&fx.lineage_id, as_of_in_v2)
            .await
            .expect("store call")
    })
    .await
    .expect("v2 resolvable");
    let snapshot: OntologyIR =
        serde_json::from_value(snapshot_saved.ontology_ir).expect("decode snapshot");

    let mut query = current_query_referencing_customer();
    query.as_of = Some(as_of_in_v2);

    let rewritten = rewrite_temporal_with_renames(query, &snapshot, &current)
        .expect("rewrite ok");
    let QueryOp::Match { patterns, .. } = &rewritten.operation else {
        panic!("expected Match");
    };
    let GraphPattern::Node { label, .. } = &patterns[0] else {
        panic!("expected Node pattern");
    };
    assert_eq!(
        label.as_ref().map(|l| l.as_str()),
        Some("Customer"),
        "snapshot label == current label → no rewrite"
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 6. Property rename end-to-end. Same Customer node type across v1 and v2,
//    but `property[p1]` renamed from `email` (v1) to `primary_email` (v2).
//    Query authored today uses `c.primary_email`; rewriter resolves to v1
//    and substitutes to `c.email` — both in `Expr::Property` (WHERE clause)
//    and in an inline `PropertyFilter` (pattern {primary_email: …}).
// ---------------------------------------------------------------------------

fn pk(s: &str) -> PropertyKey {
    PropertyKey::new(s).expect("property key")
}

fn make_prop(id: &str, name: &str) -> PropertyDef {
    PropertyDef {
        id: id.into(),
        name: pk(name),
        display_name: LocalizedText::default(),
        property_type: PropertyType::String,
        nullable: false,
        default_value: None,
        description: LocalizedText::default(),
        min_count: None,
        max_count: None,
        is_localized: false,
        classification: None,
        semantic_type: None,
        unit: None,
        pii_kind: None,
        source_column: None,
        transform: None,
        deprecated_at: None,
        replaced_by_id: None,
    }
}

fn build_ontology_with_property(
    lineage_id: &str,
    version_number: u32,
    valid_from: DateTime<Utc>,
    valid_to: Option<DateTime<Utc>>,
    node_label: &str,
    prop_name: &str,
) -> OntologyIR {
    OntologyIR::new(
        lineage_id.to_string(),
        "Temporal Property Test".to_string(),
        LocalizedText::default(),
        OntologyVersion {
            number: version_number,
            valid_from: Some(valid_from),
            valid_to,
            committed_by: None,
            commit_message: None,
        },
        vec![NodeTypeDef {
            id: "nt_party".into(),
            label: gl(node_label),
            description: LocalizedText::default(),
            // Stable property id `p_email` across versions; only the
            // name changes. The rewriter diffs on id.
            properties: vec![make_prop("p_email", prop_name)],
            constraints: vec![],
            ..Default::default()
        }],
        vec![],
        vec![],
    )
}

async fn seed_property_fixture(store: &PostgresStore) -> TemporalFixture {
    let suffix = Uuid::new_v4().simple().to_string();
    let short = suffix[..8].to_string();
    let lineage_id = format!("temporal-prop-lineage-{short}");
    let v1_created_at = ts(2026, 1, 1);
    let v2_created_at = ts(2026, 6, 1);

    let v1 = build_ontology_with_property(
        &lineage_id,
        1,
        v1_created_at,
        Some(v2_created_at),
        "Customer",
        "email",
    );
    let v2 = build_ontology_with_property(
        &lineage_id,
        2,
        v2_created_at,
        None,
        "Customer",
        "primary_email",
    );

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let user_email = format!("temporal-prop-test-{short}@example.com");
        let provider_sub = format!("temporal-prop-test-sub-{short}");
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'Temporal Prop Test User', 'test', $2, 'designer') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        let ws_slug = format!("temporal-prop-ws-{short}");
        let workspace_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('Temporal Prop Workspace', $1, $2) \
             RETURNING id",
        )
        .bind(&ws_slug)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace");

        let v1_ir = serde_json::to_value(&v1).expect("serialize v1");
        let v2_ir = serde_json::to_value(&v2).expect("serialize v2");
        let name_stem = format!("temporal-prop-ontology-{short}");

        sqlx::query(
            "INSERT INTO saved_ontologies \
             (id, workspace_id, name, version, ontology_ir, created_by, created_at) \
             VALUES ($1, $2, $3, 1, $4, 'temporal-prop-test', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(&name_stem)
        .bind(&v1_ir)
        .bind(v1_created_at)
        .execute(pool)
        .await
        .expect("insert v1");

        sqlx::query(
            "INSERT INTO saved_ontologies \
             (id, workspace_id, name, version, ontology_ir, created_by, created_at) \
             VALUES ($1, $2, $3, 2, $4, 'temporal-prop-test', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(&name_stem)
        .bind(&v2_ir)
        .bind(v2_created_at)
        .execute(pool)
        .await
        .expect("insert v2");

        TemporalFixture {
            user_id,
            workspace_id,
            lineage_id,
        }
    })
    .await
}

#[tokio::test]
#[ignore]
async fn rewriter_renames_property_in_expr_end_to_end() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_property_fixture(&store).await;

    // Current ontology = v2 (latest); snapshot at as_of → v1.
    let current_saved = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_latest_ontology_by_lineage(&fx.lineage_id)
            .await
            .expect("store call")
    })
    .await
    .expect("latest");
    let current: OntologyIR =
        serde_json::from_value(current_saved.ontology_ir).expect("decode current");
    assert_eq!(
        current
            .node_types()
            .iter()
            .next()
            .and_then(|n| n.properties.first())
            .map(|p| p.name.as_str()),
        Some("primary_email"),
        "v2 carries the renamed property name"
    );

    let as_of = ts(2026, 3, 15);
    let snapshot_saved = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .get_ontology_version_at(&fx.lineage_id, as_of)
            .await
            .expect("store call")
    })
    .await
    .expect("v1 resolvable");
    let snapshot: OntologyIR =
        serde_json::from_value(snapshot_saved.ontology_ir).expect("decode snapshot");
    assert_eq!(
        snapshot
            .node_types()
            .iter()
            .next()
            .and_then(|n| n.properties.first())
            .map(|p| p.name.as_str()),
        Some("email"),
        "v1 carries the original property name"
    );

    // Query authored today uses `c.primary_email`. After temporal
    // rewrite it should reference `c.email`.
    let query = QueryIR {
        schema_version: QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("c"),
                label: Some(gl("Customer")),
                property_filters: vec![PropertyFilter {
                    property: pk("primary_email"),
                    value: Expr::Literal {
                        value: PropertyValue::String("x@y".into()),
                    },
                }],
            }],
            filter: Some(Expr::Comparison {
                left: Box::new(Expr::Property {
                    variable: vn("c"),
                    field: Some(pk("primary_email")),
                }),
                op: ComparisonOp::Eq,
                right: Box::new(Expr::Literal {
                    value: PropertyValue::String("x@y".into()),
                }),
            }),
            projections: vec![],
            optional: false,
            group_by: vec![],
        },
        limit: None,
        skip: None,
        order_by: vec![],
        as_of: Some(as_of),
    };

    let rewritten =
        rewrite_temporal_with_renames(query, &snapshot, &current).expect("rewrite ok");

    assert!(rewritten.as_of.is_none(), "as_of must be cleared");

    let QueryOp::Match {
        patterns, filter, ..
    } = &rewritten.operation
    else {
        panic!("expected Match");
    };

    // Inline pattern filter renamed.
    let GraphPattern::Node {
        property_filters, ..
    } = &patterns[0]
    else {
        panic!("expected Node");
    };
    assert_eq!(
        property_filters[0].property.as_str(),
        "email",
        "inline pattern filter must rewrite to snapshot-era property name"
    );

    // WHERE Expr::Property renamed.
    let Some(Expr::Comparison { left, .. }) = filter else {
        panic!("expected Comparison filter");
    };
    let Expr::Property { field, .. } = left.as_ref() else {
        panic!("expected Property expr");
    };
    assert_eq!(
        field.as_ref().map(|f| f.as_str()),
        Some("email"),
        "WHERE expression must rewrite to snapshot-era property name"
    );

    cleanup(&store, &fx).await;
}

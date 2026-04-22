//! Phase 2-2 end-to-end Temporal AS-OF integration.
//!
//! Validates that the full pipeline
//!
//!   `OntologyVersionStore::resolve_version_at` →
//!   `OntologyVersionStore::load_version` →
//!   `rewrite_temporal_with_renames`
//!
//! resolves a point-in-time snapshot and rewrites the query's labels into
//! the snapshot's label space.
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
use ox_core::property_key::PropertyKey;
use ox_core::types::{PropertyType, PropertyValue};
use ox_core::variable_name::VariableName;
use ox_ontology::ir::{NodeTypeDef, OntologyIR, OntologyVersion, PropertyDef};
use ox_query_ir::query::{
    Expr, GraphPattern, PropertyFilter, QUERY_IR_SCHEMA_VERSION, QueryIR, QueryOp,
};
use ox_store::{OntologyVersionStore, PostgresStore};
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

/// Build an OntologyIR with a single node type. The `lineage_id` is stored on
/// `OntologyIR.id`; only the label and window change across versions.
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
    ontology_id: Uuid,
    v1_version_id: Uuid,
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

    let (user_id, workspace_id) = PostgresStore::with_system_bypass(|| async {
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

        (user_id, workspace_id)
    })
    .await;

    // Now commit versions through the new content-addressed store. Each
    // commit runs under the workspace's RLS scope. The `valid_from` /
    // `valid_to` bitemporal columns default to NOW in the Rust commit
    // path, so we backdate them with a direct UPDATE under the bypass.
    let (ontology_id, v1_version_id, v2_version_id) =
        PostgresStore::with_workspace(workspace_id, || async {
            let identity = store
                .create_ontology(
                    &format!("temporal-ontology-{short}"),
                    &serde_json::json!({"default": "Temporal Test", "translations": {}}),
                    Some(&lineage_id),
                )
                .await
                .expect("create identity");
            let v1_snap = store
                .commit_version(identity.id, &v1, "1", None, "temporal-test", "seed v1")
                .await
                .expect("commit v1");
            let v2_snap = store
                .commit_version(
                    identity.id,
                    &v2,
                    "2",
                    Some(v1_snap.id),
                    "temporal-test",
                    "seed v2",
                )
                .await
                .expect("commit v2");
            (identity.id, v1_snap.id, v2_snap.id)
        })
        .await;

    // Backdate bitemporal windows for deterministic resolve_version_at
    // probes. Both UPDATEs run under system bypass since RLS would
    // otherwise refuse cross-policy edits from an arbitrary test session.
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        sqlx::query(
            "UPDATE ontology_version_snapshots \
             SET valid_from = $1, valid_to = $2 WHERE id = $3",
        )
        .bind(v1_created_at)
        .bind(v2_created_at)
        .bind(v1_version_id)
        .execute(pool)
        .await
        .expect("backdate v1");
        sqlx::query(
            "UPDATE ontology_version_snapshots \
             SET valid_from = $1, valid_to = NULL WHERE id = $2",
        )
        .bind(v2_created_at)
        .bind(v2_version_id)
        .execute(pool)
        .await
        .expect("backdate v2");
    })
    .await;

    TemporalFixture {
        user_id,
        workspace_id,
        ontology_id,
        v1_version_id,
    }
}

async fn cleanup(store: &PostgresStore, fx: &TemporalFixture) {
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        // Ontology + snapshot + pointer rows cascade via FK ON DELETE
        // CASCADE from `ontologies`; workspace delete handles the rest.
        let _ = sqlx::query("DELETE FROM ontologies WHERE id = $1")
            .bind(fx.ontology_id)
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
// 1. `resolve_version_at` picks the bitemporal window containing `as_of`.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn resolve_version_at_picks_v1_mid_window() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let mid_v1 = ts(2026, 3, 15);
    let snapshot = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .resolve_version_at(fx.ontology_id, mid_v1)
            .await
            .expect("store call")
    })
    .await
    .expect("v1 must be live at mid-window");

    assert_eq!(snapshot.id, fx.v1_version_id, "mid-window resolves to v1");
    assert_eq!(snapshot.version, "1", "v1 tag");

    let ir = PostgresStore::with_workspace(fx.workspace_id, || async {
        store.load_version(snapshot.id).await.expect("hydrate v1")
    })
    .await;
    assert_eq!(
        ir.node_types().iter().next().map(|n| n.label.as_str()),
        Some("Client"),
        "v1's label was Client",
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 2. Same resolver at / after v2.valid_from should pick v2.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn resolve_version_at_picks_v2_after_cutover() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let after_cutover = ts(2026, 8, 1);
    let snapshot = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .resolve_version_at(fx.ontology_id, after_cutover)
            .await
            .expect("store call")
    })
    .await
    .expect("v2 must be live after cutover");

    assert_eq!(snapshot.version, "2", "post-cutover resolves to v2");
    let ir = PostgresStore::with_workspace(fx.workspace_id, || async {
        store.load_version(snapshot.id).await.expect("hydrate v2")
    })
    .await;
    assert_eq!(
        ir.node_types().iter().next().map(|n| n.label.as_str()),
        Some("Customer"),
        "v2's label was Customer",
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 3. Before the first version's valid_from → `None`.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn resolve_version_at_returns_none_before_lineage() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let before_v1 = ts(2025, 1, 1);
    let snapshot = PostgresStore::with_workspace(fx.workspace_id, || async {
        store
            .resolve_version_at(fx.ontology_id, before_v1)
            .await
            .expect("store call")
    })
    .await;
    assert!(snapshot.is_none(), "no version is live before v1");

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 4. End-to-end — resolver + rewriter against v1 during its window.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn temporal_rewrite_pivots_customer_to_client() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let query = current_query_referencing_customer();
    let as_of = query.as_of.expect("test query has as_of");

    let (snapshot_ir, current_ir) = PostgresStore::with_workspace(fx.workspace_id, || async {
        let snapshot_row = store
            .resolve_version_at(fx.ontology_id, as_of)
            .await
            .expect("resolve snapshot")
            .expect("v1 live at 2026-03-15");
        let current_row = store
            .get_current_version(fx.ontology_id)
            .await
            .expect("current version lookup")
            .expect("current version exists");
        let snapshot = store
            .load_version(snapshot_row.id)
            .await
            .expect("hydrate snapshot");
        let current = store
            .load_version(current_row.id)
            .await
            .expect("hydrate current");
        (snapshot, current)
    })
    .await;

    let rewritten = rewrite_temporal_with_renames(query, &snapshot_ir, &current_ir)
        .expect("rewrite succeeds");

    // as_of should be consumed by the rewriter so the compiler sees a
    // history-resolved query.
    assert!(rewritten.as_of.is_none(), "rewriter clears as_of");

    match &rewritten.operation {
        QueryOp::Match { patterns, .. } => match &patterns[0] {
            GraphPattern::Node { label, .. } => {
                assert_eq!(
                    label.as_ref().map(GraphLabel::as_str),
                    Some("Client"),
                    "Customer must have been rewritten into Client for the v1 window",
                );
            }
            other => panic!("expected node pattern, got {other:?}"),
        },
        other => panic!("expected Match operation, got {other:?}"),
    }

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 5. After cutover the rewriter becomes a no-op for Customer.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn temporal_rewrite_is_noop_inside_v2_window() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixture(&store).await;

    let mut query = current_query_referencing_customer();
    let as_of_in_v2 = ts(2026, 9, 1);
    query.as_of = Some(as_of_in_v2);

    let (snapshot_ir, current_ir) = PostgresStore::with_workspace(fx.workspace_id, || async {
        let snapshot_row = store
            .resolve_version_at(fx.ontology_id, as_of_in_v2)
            .await
            .expect("resolve snapshot")
            .expect("v2 live in its window");
        let current_row = store
            .get_current_version(fx.ontology_id)
            .await
            .expect("current version lookup")
            .expect("current version exists");
        let snapshot = store
            .load_version(snapshot_row.id)
            .await
            .expect("hydrate snapshot");
        let current = store
            .load_version(current_row.id)
            .await
            .expect("hydrate current");
        (snapshot, current)
    })
    .await;

    let rewritten = rewrite_temporal_with_renames(query, &snapshot_ir, &current_ir)
        .expect("rewrite succeeds");

    match &rewritten.operation {
        QueryOp::Match { patterns, .. } => match &patterns[0] {
            GraphPattern::Node { label, .. } => {
                assert_eq!(
                    label.as_ref().map(GraphLabel::as_str),
                    Some("Customer"),
                    "Customer must stay Customer when the snapshot is v2",
                );
            }
            other => panic!("expected node pattern, got {other:?}"),
        },
        other => panic!("expected Match operation, got {other:?}"),
    }

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 6. Property filters on the rewritten node carry through unchanged when
//    the property key is not renamed across versions.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn temporal_rewrite_preserves_property_filters() {
    let Some(store) = connect_store().await else {
        return;
    };

    // Adjust the fixture: both versions carry a `region` property so a
    // filter on `c.region = 'APAC'` should survive the rewrite.
    let suffix = Uuid::new_v4().simple().to_string();
    let short = suffix[..8].to_string();
    let lineage_id = format!("temporal-lineage-prop-{short}");
    let v1_created_at = ts(2026, 1, 1);
    let v2_created_at = ts(2026, 6, 1);

    let build = |version: u32,
                 valid_from: DateTime<Utc>,
                 valid_to: Option<DateTime<Utc>>,
                 label: &str|
     -> OntologyIR {
        OntologyIR::new(
            lineage_id.clone(),
            "Temporal Prop Test".into(),
            LocalizedText::default(),
            OntologyVersion {
                number: version,
                valid_from: Some(valid_from),
                valid_to,
                committed_by: None,
                commit_message: None,
            },
            vec![NodeTypeDef {
                id: "nt_party".into(),
                label: gl(label),
                description: LocalizedText::default(),
                properties: vec![PropertyDef {
                    name: PropertyKey::new("region").expect("property key"),
                    property_type: PropertyType::String,
                    description: LocalizedText::default(),
                    nullable: true,
                    ..Default::default()
                }],
                constraints: vec![],
                ..Default::default()
            }],
            vec![],
            vec![],
        )
    };

    let v1 = build(1, v1_created_at, Some(v2_created_at), "Client");
    let v2 = build(2, v2_created_at, None, "Customer");

    let (user_id, workspace_id) = PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'Prop Temporal Test', 'test', $2, 'designer') RETURNING id",
        )
        .bind(format!("prop-{short}@example.com"))
        .bind(format!("prop-sub-{short}"))
        .fetch_one(pool)
        .await
        .expect("insert user");
        let workspace_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('Prop Ws', $1, $2) RETURNING id",
        )
        .bind(format!("prop-ws-{short}"))
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert ws");
        (user_id, workspace_id)
    })
    .await;

    let (ontology_id, v1_id, v2_id) =
        PostgresStore::with_workspace(workspace_id, || async {
            let identity = store
                .create_ontology(
                    &format!("prop-ontology-{short}"),
                    &serde_json::json!({"default": "Prop test", "translations": {}}),
                    Some(&lineage_id),
                )
                .await
                .expect("create identity");
            let v1 = store
                .commit_version(identity.id, &v1, "1", None, "test", "v1")
                .await
                .expect("commit v1");
            let v2 = store
                .commit_version(identity.id, &v2, "2", Some(v1.id), "test", "v2")
                .await
                .expect("commit v2");
            (identity.id, v1.id, v2.id)
        })
        .await;

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        sqlx::query(
            "UPDATE ontology_version_snapshots SET valid_from = $1, valid_to = $2 WHERE id = $3",
        )
        .bind(v1_created_at)
        .bind(v2_created_at)
        .bind(v1_id)
        .execute(pool)
        .await
        .expect("backdate v1");
        sqlx::query(
            "UPDATE ontology_version_snapshots SET valid_from = $1, valid_to = NULL WHERE id = $2",
        )
        .bind(v2_created_at)
        .bind(v2_id)
        .execute(pool)
        .await
        .expect("backdate v2");
    })
    .await;

    let query = QueryIR {
        schema_version: QUERY_IR_SCHEMA_VERSION,
        operation: QueryOp::Match {
            patterns: vec![GraphPattern::Node {
                variable: vn("c"),
                label: Some(gl("Customer")),
                property_filters: vec![PropertyFilter {
                    property: PropertyKey::new("region").expect("prop"),
                    value: Expr::Literal {
                        value: PropertyValue::String("APAC".into()),
                    },
                }],
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
    };

    let (snapshot_ir, current_ir) = PostgresStore::with_workspace(workspace_id, || async {
        let snap_row = store
            .resolve_version_at(ontology_id, query.as_of.expect("as_of"))
            .await
            .expect("resolve")
            .expect("v1 window");
        let cur_row = store
            .get_current_version(ontology_id)
            .await
            .expect("current")
            .expect("exists");
        (
            store.load_version(snap_row.id).await.expect("snap"),
            store.load_version(cur_row.id).await.expect("cur"),
        )
    })
    .await;

    let rewritten = rewrite_temporal_with_renames(query, &snapshot_ir, &current_ir)
        .expect("rewrite");

    match &rewritten.operation {
        QueryOp::Match { patterns, .. } => match &patterns[0] {
            GraphPattern::Node {
                label,
                property_filters,
                ..
            } => {
                assert_eq!(label.as_ref().map(GraphLabel::as_str), Some("Client"));
                assert_eq!(
                    property_filters.len(),
                    1,
                    "region filter survives the rewrite"
                );
                assert_eq!(property_filters[0].property.as_str(), "region");
            }
            other => panic!("expected node, got {other:?}"),
        },
        other => panic!("expected Match, got {other:?}"),
    }

    // Teardown
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let _ = sqlx::query("DELETE FROM ontologies WHERE id = $1")
            .bind(ontology_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    })
    .await;
}

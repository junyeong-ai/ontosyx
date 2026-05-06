//! Slice W3a — persistence for federation adapter configurations.
//!
//! Exercises the new `DataSourceStore` trait end-to-end against a
//! real Postgres instance, including the RLS isolation declared in
//! migration `0011_data_sources.sql`.
//!
//! Ignored by default; run with a live database:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test data_sources_integration -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use ox_store::{DataSourceStore, PostgresStore};
use serde_json::json;
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
    // env-var unset is the only legitimate "skip" condition. Any
    // other failure (connect / migrate) is a real defect — surface
    // it via .expect() instead of silent-skipping into a vacuous
    // "ok" report. See `feedback_test_silent_skip_pattern.md`.
    let url = resolve_test_db_url()?;
    let store = PostgresStore::connect(&url, 4)
        .await
        .expect("connect to test DB");
    store.migrate().await.expect("apply migrations");
    Some(store)
}

/// Seed a fresh user + two workspaces so the RLS assertions can show
/// data_sources rows in one workspace are invisible from the other.
async fn seed_workspaces(store: &PostgresStore) -> (Uuid, Uuid) {
    let suffix = Uuid::new_v4().simple().to_string();
    let user_email = format!("ds-test-{}@example.com", &suffix[..8]);
    let slug_a = format!("ds-ws-a-{}", &suffix[..8]);
    let slug_b = format!("ds-ws-b-{}", &suffix[..8]);

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let provider_sub = format!("ds-test-sub-{}", &suffix[..8]);
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'DataSource Test User', 'test', $2, 'admin') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        let ws_a: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('DataSource Workspace A', $1, $2) \
             RETURNING id",
        )
        .bind(&slug_a)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace A");

        let ws_b: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('DataSource Workspace B', $1, $2) \
             RETURNING id",
        )
        .bind(&slug_b)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace B");

        (ws_a, ws_b)
    })
    .await
}

#[tokio::test]
#[ignore]
async fn upsert_roundtrips_insert_and_replace() {
    let Some(store) = connect_store().await else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let (ws_a, _ws_b) = seed_workspaces(&store).await;

    let first = PostgresStore::with_workspace(ws_a, || async {
        store
            .upsert_data_source_by_source_id(
                "csv-orders",
                "csv",
                &json!({"data": "id\n1\n"}),
            )
            .await
            .expect("initial upsert")
    })
    .await;
    assert_eq!(first.source_id, "csv-orders");
    assert_eq!(first.kind, "csv");

    // Replace with new config — the `id` stays the same so callers
    // can treat `upsert_data_source_by_source_id` as an idempotent
    // registration by natural key.
    let second = PostgresStore::with_workspace(ws_a, || async {
        store
            .upsert_data_source_by_source_id(
                "csv-orders",
                "csv",
                &json!({"data": "id,name\n1,Alice\n"}),
            )
            .await
            .expect("replacing upsert")
    })
    .await;
    assert_eq!(second.id, first.id, "upsert must keep the row's PK stable");
    assert_eq!(
        second.config["data"],
        json!("id,name\n1,Alice\n"),
        "config must be replaced on repeat upsert"
    );
    assert!(
        second.updated_at >= first.updated_at,
        "updated_at must advance on replace"
    );
}

#[tokio::test]
#[ignore]
async fn list_get_find_and_delete_round_trip() {
    let Some(store) = connect_store().await else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let (ws_a, _ws_b) = seed_workspaces(&store).await;

    let row = PostgresStore::with_workspace(ws_a, || async {
        store
            .upsert_data_source_by_source_id("csv-a", "csv", &json!({"data": "a\n1\n"}))
            .await
            .expect("upsert csv-a")
    })
    .await;
    let _row_b = PostgresStore::with_workspace(ws_a, || async {
        store
            .upsert_data_source_by_source_id("csv-b", "csv", &json!({"data": "b\n2\n"}))
            .await
            .expect("upsert csv-b")
    })
    .await;

    // list — both rows visible.
    let listed = PostgresStore::with_workspace(ws_a, || async {
        store.list_data_sources().await.expect("list")
    })
    .await;
    assert_eq!(listed.len(), 2);
    // Ordered by source_id ASC per the SQL.
    assert_eq!(listed[0].source_id, "csv-a");
    assert_eq!(listed[1].source_id, "csv-b");

    // get by PK.
    let fetched = PostgresStore::with_workspace(ws_a, || async {
        store.get_data_source(row.id).await.expect("get")
    })
    .await;
    assert_eq!(fetched.map(|r| r.source_id), Some("csv-a".to_string()));

    // find by source_id.
    let by_sid = PostgresStore::with_workspace(ws_a, || async {
        store
            .find_data_source_by_source_id("csv-a")
            .await
            .expect("find")
    })
    .await;
    assert_eq!(by_sid.map(|r| r.id), Some(row.id));

    // delete by source_id — returns true the first time, false the second.
    let first_delete = PostgresStore::with_workspace(ws_a, || async {
        store
            .delete_data_source_by_source_id("csv-a")
            .await
            .expect("delete")
    })
    .await;
    assert!(first_delete);
    let second_delete = PostgresStore::with_workspace(ws_a, || async {
        store
            .delete_data_source_by_source_id("csv-a")
            .await
            .expect("delete idempotent")
    })
    .await;
    assert!(!second_delete, "delete on missing row must report false");
}

#[tokio::test]
#[ignore]
async fn workspace_rls_isolates_data_sources_between_tenants() {
    let Some(store) = connect_store().await else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let (ws_a, ws_b) = seed_workspaces(&store).await;

    PostgresStore::with_workspace(ws_a, || async {
        store
            .upsert_data_source_by_source_id(
                "csv-shared-label",
                "csv",
                &json!({"data": "x\n1\n"}),
            )
            .await
            .expect("insert for ws_a");
    })
    .await;

    PostgresStore::with_workspace(ws_b, || async {
        store
            .upsert_data_source_by_source_id(
                "csv-shared-label",
                "csv",
                &json!({"data": "x\n2\n"}),
            )
            .await
            .expect("insert for ws_b — same source_id allowed because scoped by ws");
    })
    .await;

    // Each workspace sees exactly its own row — the `(workspace_id,
    // source_id)` unique constraint lets two tenants reuse the same
    // `source_id` label without collision, and RLS filters the list
    // to the current workspace.
    let listed_a = PostgresStore::with_workspace(ws_a, || async {
        store.list_data_sources().await.expect("list a")
    })
    .await;
    let listed_b = PostgresStore::with_workspace(ws_b, || async {
        store.list_data_sources().await.expect("list b")
    })
    .await;

    let a_has_shared = listed_a
        .iter()
        .any(|r| r.source_id == "csv-shared-label");
    let b_has_shared = listed_b
        .iter()
        .any(|r| r.source_id == "csv-shared-label");
    assert!(a_has_shared, "ws_a must see its own csv-shared-label row");
    assert!(b_has_shared, "ws_b must see its own csv-shared-label row");

    // The two rows have different `id`s (they are distinct physical
    // rows living in different workspaces).
    let a_id = listed_a
        .iter()
        .find(|r| r.source_id == "csv-shared-label")
        .unwrap()
        .id;
    let b_id = listed_b
        .iter()
        .find(|r| r.source_id == "csv-shared-label")
        .unwrap()
        .id;
    assert_ne!(a_id, b_id, "shared source_id maps to distinct rows per ws");

    // ws_a cannot read ws_b's row by PK — RLS denies.
    let cross_read = PostgresStore::with_workspace(ws_a, || async {
        store.get_data_source(b_id).await.expect("cross read")
    })
    .await;
    assert!(
        cross_read.is_none(),
        "RLS must hide ws_b's row from ws_a's scope"
    );
}

/// Stamp the cached analysis snapshot + per-table fingerprint map
/// back onto the source row. Verifies the round-trip plus the
/// `last_analyzed_at` server-stamping behaviour.
#[tokio::test]
#[ignore]
async fn update_data_source_analysis_round_trip() {
    let Some(store) = connect_store().await else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };
    let (ws_a, _) = seed_workspaces(&store).await;

    let initial = PostgresStore::with_workspace(ws_a, || async {
        store
            .upsert_data_source_by_source_id(
                "csv-analysis",
                "csv",
                &json!({"data": "id,label\n1,A\n"}),
            )
            .await
            .expect("upsert")
    })
    .await;

    // Pre-update: every analysis field is the empty default.
    assert!(initial.last_analysis_snapshot.is_none());
    assert_eq!(initial.schema_fingerprints, json!({}));
    assert!(initial.last_analyzed_at.is_none());

    let snapshot = json!({
        "schema": {"source_type": "csv", "tables": [{"name": "records", "columns": []}], "foreign_keys": []},
        "profile": {"table_profiles": []},
        "warnings": []
    });
    let fingerprints = json!({
        "records": {"hash": "deadbeef", "computed_at": "2026-04-26T00:00:00Z"}
    });

    let updated = PostgresStore::with_workspace(ws_a, || async {
        store
            .update_data_source_analysis("csv-analysis", &snapshot, &fingerprints)
            .await
            .expect("update analysis")
    })
    .await;

    assert_eq!(updated.id, initial.id, "PK is stable across analysis updates");
    assert_eq!(updated.last_analysis_snapshot.as_ref(), Some(&snapshot));
    assert_eq!(updated.schema_fingerprints, fingerprints);
    let stamped = updated.last_analyzed_at.expect("server stamps last_analyzed_at");
    assert!(
        stamped >= initial.created_at,
        "last_analyzed_at must be at or after the source's creation time"
    );

    // Re-fetch via the standard get path to confirm SELECTs surface
    // the new columns.
    let refetched = PostgresStore::with_workspace(ws_a, || async {
        store
            .get_data_source(initial.id)
            .await
            .expect("get")
            .expect("row exists")
    })
    .await;
    assert_eq!(refetched.last_analysis_snapshot.as_ref(), Some(&snapshot));
}

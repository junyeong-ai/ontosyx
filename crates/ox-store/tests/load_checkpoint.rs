//! Integration coverage for `LoadCheckpointStore`.
//!
//! Validates the four contract properties the load pipeline
//! depends on:
//!
//! 1. `upsert_load_checkpoint` rejects a call outside any
//!    workspace scope (`MissingContext`).
//! 2. Round-trip: upsert + get returns the same row, with `id` /
//!    `workspace_id` populated by the store.
//! 3. Upsert is idempotent on the natural key
//!    `(workspace_id, project_id, source_table, graph_label)` and
//!    accumulates `record_count` (the production contract — every
//!    incremental load adds to the running total).
//! 4. RLS enforces workspace isolation on reads — a checkpoint
//!    written under workspace A is invisible from workspace B.
//!
//! Ignored by default — run against a live PostgreSQL:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test load_checkpoint -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use ox_core::error::OxError;
use ox_store::{LoadCheckpoint, LoadCheckpointStore, PostgresStore, SYSTEM_BYPASS};
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
    let store = PostgresStore::connect(&url, 4)
        .await
        .expect("connect to test DB");
    store.migrate().await.expect("apply migrations");
    Some(store)
}

fn fresh_checkpoint(
    project_id: Uuid,
    source_table: &str,
    graph_label: &str,
    watermark_value: &str,
    record_count: i64,
) -> LoadCheckpoint {
    LoadCheckpoint::draft(
        project_id,
        source_table.to_string(),
        graph_label.to_string(),
        "updated_at".to_string(),
        watermark_value.to_string(),
        record_count,
    )
}

async fn cleanup_workspace(store: &PostgresStore, workspace_id: Uuid) {
    SYSTEM_BYPASS
        .scope(true, async {
            let _ = sqlx::query("DELETE FROM load_checkpoints WHERE workspace_id = $1")
                .bind(workspace_id)
                .execute(store.pool())
                .await;
        })
        .await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let cp = fresh_checkpoint(Uuid::new_v4(), "orders", "Order", "0", 0);
    match store.upsert_load_checkpoint(&cp).await {
        Err(OxError::MissingContext { kind, .. }) => assert_eq!(kind, "workspace"),
        Err(other) => panic!("expected MissingContext, got {other:?}"),
        Ok(()) => panic!("upsert succeeded outside any scope — guard missing"),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn round_trip_upsert_and_get() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let table = format!("orders_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let cp = fresh_checkpoint(project_id, &table, "Order", "100", 50);

    PostgresStore::with_workspace(workspace_id, || async {
        store.upsert_load_checkpoint(&cp).await.expect("upsert");
        let found = store
            .get_load_checkpoint(project_id, &table, "Order")
            .await
            .expect("get")
            .expect("checkpoint must round-trip");
        assert_eq!(found.workspace_id, Some(workspace_id));
        assert!(found.id.is_some(), "store must populate id");
        assert_eq!(found.project_id, project_id);
        assert_eq!(found.source_table, table);
        assert_eq!(found.graph_label, "Order");
        assert_eq!(found.watermark_value, "100");
        assert_eq!(found.record_count, 50);
    })
    .await;
    cleanup_workspace(&store, workspace_id).await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_accumulates_record_count_on_natural_key_collision() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let table = format!("orders_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let first = fresh_checkpoint(project_id, &table, "Order", "100", 50);
    let second = fresh_checkpoint(project_id, &table, "Order", "200", 30);

    PostgresStore::with_workspace(workspace_id, || async {
        store.upsert_load_checkpoint(&first).await.expect("first upsert");
        store.upsert_load_checkpoint(&second).await.expect("second upsert");
        let found = store
            .get_load_checkpoint(project_id, &table, "Order")
            .await
            .expect("get")
            .expect("checkpoint must exist");
        // Watermark replaced with the latest value.
        assert_eq!(found.watermark_value, "200");
        // Production contract: record_count *accumulates* on
        // collision (every incremental load adds to the running
        // total, never overwrites).
        assert_eq!(found.record_count, 80);
    })
    .await;
    cleanup_workspace(&store, workspace_id).await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn rls_isolates_workspaces() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_a = Uuid::new_v4();
    let workspace_b = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let table = format!("orders_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let cp = fresh_checkpoint(project_id, &table, "Order", "100", 50);

    PostgresStore::with_workspace(workspace_a, || async {
        store.upsert_load_checkpoint(&cp).await.expect("upsert under A");
        let visible = store
            .get_load_checkpoint(project_id, &table, "Order")
            .await
            .expect("get under A");
        assert!(visible.is_some(), "workspace A must see its own row");
    })
    .await;

    PostgresStore::with_workspace(workspace_b, || async {
        let visible = store
            .get_load_checkpoint(project_id, &table, "Order")
            .await
            .expect("get under B");
        assert!(
            visible.is_none(),
            "workspace B must NOT see workspace A's checkpoint"
        );
    })
    .await;

    cleanup_workspace(&store, workspace_a).await;
}

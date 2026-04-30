//! Integration coverage for `DraftClusterCheckpointStore` — ADR-0027.
//!
//! Validates the four surface-area properties the streaming pipeline
//! depends on:
//!
//! 1. Workspace-context guards reject unscoped mutating calls
//!    (upsert / find / list / per-project delete).
//! 2. Round-trip: upsert + find_by_signature returns the same row.
//! 3. Upsert is idempotent on the natural key
//!    `(workspace_id, project_id, source_id, signature)` — second
//!    insert with the same key replaces output, never duplicates.
//! 4. `delete_expired_draft_cluster_checkpoints` runs under
//!    `SYSTEM_BYPASS::scope` and only removes rows past `expires_at`.
//!
//! Ignored by default — run against a live PostgreSQL:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test draft_cluster_checkpoint -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use chrono::{Duration as ChronoDuration, Utc};
use ox_core::error::OxError;
use ox_store::{
    DraftClusterCheckpointRow, DraftClusterCheckpointStore, PostgresStore, SYSTEM_BYPASS,
};
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
    let url = resolve_test_db_url()?;
    // `.expect()` matches the silent-skip-pattern memory: setup
    // helpers must hard-fail on migration errors so a broken
    // schema doesn't masquerade as a passing test by skipping.
    let store = PostgresStore::connect(&url, 4)
        .await
        .expect("connect to test DB");
    store.migrate().await.expect("apply migrations");
    Some(store)
}

fn fresh_row(
    workspace_id: Uuid,
    project_id: Uuid,
    source_id: &str,
    signature: &str,
    cluster_id: i32,
    output_marker: &str,
) -> DraftClusterCheckpointRow {
    DraftClusterCheckpointRow {
        id: Uuid::new_v4(),
        workspace_id,
        project_id,
        source_id: source_id.to_string(),
        signature: signature.to_string(),
        cluster_id,
        output: json!({ "marker": output_marker, "node_types": [] }),
        created_at: Utc::now(),
        expires_at: Utc::now() + ChronoDuration::hours(24),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let row = fresh_row(Uuid::nil(), Uuid::new_v4(), "src", "sig", 0, "x");
    match store.upsert_draft_cluster_checkpoint(&row).await {
        Err(OxError::MissingContext { kind, .. }) => {
            assert_eq!(kind, "workspace");
        }
        Err(other) => panic!("expected MissingContext, got {other:?}"),
        Ok(()) => panic!("upsert succeeded outside any scope — guard missing"),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn find_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    match store
        .find_draft_cluster_checkpoint_by_signature(Uuid::new_v4(), "src", "sig")
        .await
    {
        Err(OxError::MissingContext { kind, .. }) => assert_eq!(kind, "workspace"),
        Err(other) => panic!("expected MissingContext, got {other:?}"),
        Ok(found) => panic!("find succeeded outside any scope: {found:?}"),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_for_project_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    match store
        .delete_draft_cluster_checkpoints_for_project(Uuid::new_v4())
        .await
    {
        Err(OxError::MissingContext { kind, .. }) => assert_eq!(kind, "workspace"),
        Err(other) => panic!("expected MissingContext, got {other:?}"),
        Ok(_) => panic!("delete-for-project succeeded outside any scope"),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn round_trip_upsert_and_find() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let source_id = "src-rt";
    let signature = format!("sig-{}", Uuid::new_v4());
    let row = fresh_row(workspace_id, project_id, source_id, &signature, 0, "first");

    PostgresStore::with_workspace(workspace_id, || async {
            store
                .upsert_draft_cluster_checkpoint(&row)
                .await
                .expect("upsert");
            let found = store
                .find_draft_cluster_checkpoint_by_signature(project_id, source_id, &signature)
                .await
                .expect("find");
            let row = found.expect("checkpoint must round-trip");
            assert_eq!(row.signature, signature);
            assert_eq!(row.workspace_id, workspace_id);
            assert_eq!(row.project_id, project_id);
            assert_eq!(row.cluster_id, 0);
        })
    .await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_replaces_on_natural_key_collision() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let source_id = "src-collide";
    let signature = format!("sig-{}", Uuid::new_v4());
    let first = fresh_row(workspace_id, project_id, source_id, &signature, 0, "first");
    let second = {
        let mut r = fresh_row(workspace_id, project_id, source_id, &signature, 1, "second");
        r.id = Uuid::new_v4();
        r
    };

    PostgresStore::with_workspace(workspace_id, || async {
            store.upsert_draft_cluster_checkpoint(&first).await.expect("first upsert");
            store
                .upsert_draft_cluster_checkpoint(&second)
                .await
                .expect("second upsert");
            let listed = store
                .list_draft_cluster_checkpoints_for_project(project_id)
                .await
                .expect("list");
            // Natural key UNIQUE constraint = exactly one row for
            // this (workspace, project, source, signature).
            assert_eq!(listed.len(), 1);
            // Output replaced — the marker is the second one.
            let marker = listed[0].output.get("marker").and_then(|v| v.as_str());
            assert_eq!(marker, Some("second"));
            assert_eq!(listed[0].cluster_id, 1);
        })
    .await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_for_project_scoped_to_that_project() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_a = Uuid::new_v4();
    let project_b = Uuid::new_v4();
    let row_a = fresh_row(workspace_id, project_a, "src", "sig-a", 0, "a");
    let row_b = fresh_row(workspace_id, project_b, "src", "sig-b", 0, "b");

    PostgresStore::with_workspace(workspace_id, || async {
            store.upsert_draft_cluster_checkpoint(&row_a).await.expect("upsert a");
            store.upsert_draft_cluster_checkpoint(&row_b).await.expect("upsert b");

            let removed = store
                .delete_draft_cluster_checkpoints_for_project(project_a)
                .await
                .expect("delete a");
            assert_eq!(removed, 1);

            let still_b = store
                .find_draft_cluster_checkpoint_by_signature(project_b, "src", "sig-b")
                .await
                .expect("find b");
            assert!(still_b.is_some(), "project B's checkpoint must survive");

            let gone_a = store
                .find_draft_cluster_checkpoint_by_signature(project_a, "src", "sig-a")
                .await
                .expect("find a");
            assert!(gone_a.is_none(), "project A's checkpoint must be gone");
        })
    .await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_expired_under_system_bypass() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let signature_fresh = format!("sig-fresh-{}", Uuid::new_v4());
    let signature_expired = format!("sig-expired-{}", Uuid::new_v4());

    let mut fresh = fresh_row(workspace_id, project_id, "src", &signature_fresh, 0, "fresh");
    fresh.expires_at = Utc::now() + ChronoDuration::hours(24);

    let mut expired = fresh_row(
        workspace_id,
        project_id,
        "src",
        &signature_expired,
        1,
        "expired",
    );
    expired.expires_at = Utc::now() - ChronoDuration::hours(1);

    PostgresStore::with_workspace(workspace_id, || async {
            store.upsert_draft_cluster_checkpoint(&fresh).await.expect("upsert fresh");
            store.upsert_draft_cluster_checkpoint(&expired).await.expect("upsert expired");
        })
    .await;

    // Cron path runs under SYSTEM_BYPASS — no workspace scope.
    let removed = SYSTEM_BYPASS
        .scope(true, async {
            store
                .delete_expired_draft_cluster_checkpoints()
                .await
                .expect("sweep")
        })
    .await;
    assert!(
        removed >= 1,
        "sweep must remove at least the one expired row, got {removed}"
    );

    // Fresh row survives — verify under workspace scope.
    PostgresStore::with_workspace(workspace_id, || async {
            let still_fresh = store
                .find_draft_cluster_checkpoint_by_signature(
                    project_id,
                    "src",
                    &signature_fresh,
                )
                .await
                .expect("find fresh");
            assert!(still_fresh.is_some(), "fresh row must survive sweep");

            let gone = store
                .find_draft_cluster_checkpoint_by_signature(
                    project_id,
                    "src",
                    &signature_expired,
                )
                .await
                .expect("find expired");
            assert!(gone.is_none(), "expired row must be gone after sweep");
        })
    .await;
}

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
//! 4. `sweep_expired_draft_cluster_checkpoints` runs under
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
use ox_core::i18n::LocalizedText;
use ox_ontology::cluster_checkpoint::{ClusterSignature, DraftClusterCheckpoint};
use ox_ontology::input::InputOntologyDef;
use ox_store::{DraftClusterCheckpointStore, PostgresStore, SYSTEM_BYPASS};
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

fn fresh_checkpoint(
    project_id: Uuid,
    source_id: &str,
    signature: &str,
    cluster_id: usize,
) -> DraftClusterCheckpoint {
    DraftClusterCheckpoint::draft(
        project_id,
        source_id.to_string(),
        ClusterSignature::from_hex(signature.to_string()),
        cluster_id,
        empty_input_ontology(),
        ChronoDuration::hours(24),
    )
}

fn empty_input_ontology() -> InputOntologyDef {
    InputOntologyDef {
        format_version: 1,
        id: None,
        name: "test".to_string(),
        description: LocalizedText::default(),
        version: 1,
        node_types: Vec::new(),
        edge_types: Vec::new(),
        indexes: Vec::new(),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let checkpoint = fresh_checkpoint(Uuid::new_v4(), "src", "sig", 0);
    match store.upsert_draft_cluster_checkpoint(&checkpoint).await {
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
        .delete_draft_cluster_checkpoints_by_project(Uuid::new_v4())
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
    let checkpoint = fresh_checkpoint(project_id, source_id, &signature, 0);

    PostgresStore::with_workspace(workspace_id, || async {
        store
            .upsert_draft_cluster_checkpoint(&checkpoint)
            .await
            .expect("upsert");
        let found = store
            .find_draft_cluster_checkpoint_by_signature(project_id, source_id, &signature)
            .await
            .expect("find")
            .expect("checkpoint must round-trip");
        assert_eq!(found.signature.as_str(), signature);
        assert_eq!(found.workspace_id, workspace_id);
        assert_eq!(found.project_id, project_id);
        assert_eq!(found.cluster_id, 0);
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
    let first = fresh_checkpoint(project_id, source_id, &signature, 0);
    let second = fresh_checkpoint(project_id, source_id, &signature, 1);

    PostgresStore::with_workspace(workspace_id, || async {
        store
            .upsert_draft_cluster_checkpoint(&first)
            .await
            .expect("first upsert");
        store
            .upsert_draft_cluster_checkpoint(&second)
            .await
            .expect("second upsert");
        let listed = store
            .list_draft_cluster_checkpoints_by_project(project_id)
            .await
            .expect("list");
        // Natural key UNIQUE constraint = exactly one row for
        // this (workspace, project, source, signature).
        assert_eq!(listed.len(), 1);
        // cluster_id replaced — the second one wins.
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
    let cp_a = fresh_checkpoint(project_a, "src", "sig-a", 0);
    let cp_b = fresh_checkpoint(project_b, "src", "sig-b", 0);

    PostgresStore::with_workspace(workspace_id, || async {
        store
            .upsert_draft_cluster_checkpoint(&cp_a)
            .await
            .expect("upsert a");
        store
            .upsert_draft_cluster_checkpoint(&cp_b)
            .await
            .expect("upsert b");

        let removed = store
            .delete_draft_cluster_checkpoints_by_project(project_a)
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
async fn sweep_expired_under_system_bypass() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let signature_fresh = format!("sig-fresh-{}", Uuid::new_v4());
    let signature_expired = format!("sig-expired-{}", Uuid::new_v4());

    let fresh = fresh_checkpoint(project_id, "src", &signature_fresh, 0);
    let mut expired = fresh_checkpoint(project_id, "src", &signature_expired, 1);
    expired.expires_at = Utc::now() - ChronoDuration::hours(1);

    PostgresStore::with_workspace(workspace_id, || async {
        store
            .upsert_draft_cluster_checkpoint(&fresh)
            .await
            .expect("upsert fresh");
        store
            .upsert_draft_cluster_checkpoint(&expired)
            .await
            .expect("upsert expired");
    })
    .await;

    // Cron path runs under SYSTEM_BYPASS — no workspace scope.
    let removed = SYSTEM_BYPASS
        .scope(true, async {
            store
                .sweep_expired_draft_cluster_checkpoints()
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
            .find_draft_cluster_checkpoint_by_signature(project_id, "src", &signature_fresh)
            .await
            .expect("find fresh");
        assert!(still_fresh.is_some(), "fresh row must survive sweep");

        let gone = store
            .find_draft_cluster_checkpoint_by_signature(project_id, "src", &signature_expired)
            .await
            .expect("find expired");
        assert!(gone.is_none(), "expired row must be gone after sweep");
    })
    .await;
}

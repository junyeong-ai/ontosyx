//! Integration coverage for `DraftClusterCheckpointStore` — ADR-0027.
//!
//! Validates the four surface-area properties the streaming pipeline
//! depends on:
//!
//! 1. Workspace-context guards reject unscoped mutating calls
//!    (upsert / find / list / per-project delete).
//! 2. Round-trip: upsert + find_by_signature returns the same row.
//! 3. Upsert is idempotent on the natural key
//!    `(workspace_id, ontology_draft_id, source_id, signature)` — second
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

/// Drop every row this test wrote so the table doesn't accumulate
/// garbage across CI runs. Random workspace ids make per-test rows
/// unreachable to anything but a SYSTEM_BYPASS sweep, so this also
/// runs under bypass.
async fn cleanup_workspace(store: &PostgresStore, workspace_id: Uuid) {
    SYSTEM_BYPASS
        .scope(true, async {
            let _ = sqlx::query(
                "DELETE FROM draft_cluster_checkpoints WHERE workspace_id = $1",
            )
            .bind(workspace_id)
            .execute(store.pool())
            .await;
        })
        .await;
}

fn fresh_checkpoint(
    ontology_draft_id: Uuid,
    source_id: &str,
    signature: ClusterSignature,
    cluster_id: usize,
) -> DraftClusterCheckpoint {
    DraftClusterCheckpoint::draft(
        ontology_draft_id,
        source_id.to_string(),
        signature,
        cluster_id,
        empty_input_ontology(),
    )
}

/// Tests need stable-but-distinct signatures without spinning up a
/// real `TableCluster`. SHA-256 the seed string and lift through
/// `from_hex`'s validation so the helper exercises the same shape
/// the production path produces.
fn signature_from_seed(seed: &str) -> ClusterSignature {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(seed.as_bytes());
    ClusterSignature::from_hex(format!("{digest:x}")).expect("valid hex digest")
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
    let checkpoint = fresh_checkpoint(
        Uuid::new_v4(),
        "src",
        signature_from_seed("guard-test"),
        0,
    );
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
    let ontology_draft_id = Uuid::new_v4();
    let source_id = "src-rt";
    let signature = signature_from_seed(&format!("rt-{}", Uuid::new_v4()));
    let checkpoint = fresh_checkpoint(ontology_draft_id, source_id, signature.clone(), 0);

    PostgresStore::with_workspace(workspace_id, || async {
        store
            .upsert_draft_cluster_checkpoint(&checkpoint)
            .await
            .expect("upsert");
        let found = store
            .find_draft_cluster_checkpoint_by_signature(ontology_draft_id, source_id, signature.as_str())
            .await
            .expect("find")
            .expect("checkpoint must round-trip");
        assert_eq!(found.signature, signature);
        assert_eq!(found.workspace_id, Some(workspace_id));
        assert_eq!(found.ontology_draft_id, ontology_draft_id);
        assert_eq!(found.cluster_id, 0);
        assert!(found.id.is_some(), "store must populate id on insert");
    })
    .await;
    cleanup_workspace(&store, workspace_id).await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_replaces_on_natural_key_collision() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let ontology_draft_id = Uuid::new_v4();
    let source_id = "src-collide";
    let signature = signature_from_seed(&format!("collide-{}", Uuid::new_v4()));
    let first = fresh_checkpoint(ontology_draft_id, source_id, signature.clone(), 0);
    let second = fresh_checkpoint(ontology_draft_id, source_id, signature.clone(), 1);

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
            .list_draft_cluster_checkpoints_by_project(ontology_draft_id)
            .await
            .expect("list");
        // Natural key UNIQUE constraint = exactly one row for
        // this (workspace, project, source, signature).
        assert_eq!(listed.len(), 1);
        // cluster_id replaced — the second one wins.
        assert_eq!(listed[0].cluster_id, 1);
    })
    .await;
    cleanup_workspace(&store, workspace_id).await;
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
    let sig_a = signature_from_seed(&format!("project-a-{}", Uuid::new_v4()));
    let sig_b = signature_from_seed(&format!("project-b-{}", Uuid::new_v4()));
    let cp_a = fresh_checkpoint(project_a, "src", sig_a.clone(), 0);
    let cp_b = fresh_checkpoint(project_b, "src", sig_b.clone(), 0);

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
            .find_draft_cluster_checkpoint_by_signature(project_b, "src", sig_b.as_str())
            .await
            .expect("find b");
        assert!(still_b.is_some(), "project B's checkpoint must survive");

        let gone_a = store
            .find_draft_cluster_checkpoint_by_signature(project_a, "src", sig_a.as_str())
            .await
            .expect("find a");
        assert!(gone_a.is_none(), "project A's checkpoint must be gone");
    })
    .await;
    cleanup_workspace(&store, workspace_id).await;
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn sweep_expired_under_system_bypass() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();
    let ontology_draft_id = Uuid::new_v4();
    let sig_fresh = signature_from_seed(&format!("fresh-{}", Uuid::new_v4()));
    let sig_expired = signature_from_seed(&format!("expired-{}", Uuid::new_v4()));

    let fresh = fresh_checkpoint(ontology_draft_id, "src", sig_fresh.clone(), 0);
    let mut expired = fresh_checkpoint(ontology_draft_id, "src", sig_expired.clone(), 1);
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
            .find_draft_cluster_checkpoint_by_signature(ontology_draft_id, "src", sig_fresh.as_str())
            .await
            .expect("find fresh");
        assert!(still_fresh.is_some(), "fresh row must survive sweep");

        let gone = store
            .find_draft_cluster_checkpoint_by_signature(ontology_draft_id, "src", sig_expired.as_str())
            .await
            .expect("find expired");
        assert!(gone.is_none(), "expired row must be gone after sweep");
    })
    .await;
    cleanup_workspace(&store, workspace_id).await;
}

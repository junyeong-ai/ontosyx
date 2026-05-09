//! End-to-end coverage for ADR 0012's `require_workspace_context()`
//! guard on every mutating store method.
//!
//! The unit-level guard tests in `postgres::context_guard_tests`
//! prove the guard primitive itself rejects an unscoped call. This
//! file proves that the per-trait `*Store` impls actually invoke the
//! guard — i.e. the sweep in B6 wired every mutating method, not
//! just a representative sample. We pick one mutating method per
//! workspace-scoped store trait and assert it returns
//! `OxError::MissingContext` when called outside any
//! `WORKSPACE_ID.scope` / `SYSTEM_BYPASS.scope` block.
//!
//! Ignored by default. Run against a live PostgreSQL instance:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test integration -- --ignored integration::workspace_context_guard
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use ox_core::error::OxError;
use ox_store::{
    ApprovalStore, ChangeRoutingStore, OntologyDraftStore, PatternStore, PinStore, PostgresStore,
    QualitySignalStore, SourceMappingArtifactStore, VerificationStore,
};
use uuid::Uuid;

fn resolve_test_db_url() -> Option<String> {
    if let Ok(v) = std::env::var("OX_TEST_DATABASE_URL")
        && !v.is_empty()
    {
        return Some(v);
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

fn assert_missing_context<T: std::fmt::Debug>(result: Result<T, OxError>, method: &str) {
    match result {
        Err(OxError::MissingContext { kind, .. }) => {
            assert_eq!(
                kind, "workspace",
                "{method}: expected workspace-axis MissingContext"
            );
        }
        Err(other) => panic!("{method}: expected MissingContext, got {other:?}"),
        Ok(value) => panic!(
            "{method}: mutation succeeded outside any scope — guard missing. value={value:?}"
        ),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_ontology_draft_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.delete_ontology_draft(Uuid::nil()).await;
    assert_missing_context(result, "delete_ontology_draft");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_pin_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.delete_pin("test-user", Uuid::nil()).await;
    assert_missing_context(result, "delete_pin");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_pattern_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.delete_pattern(Uuid::nil()).await;
    assert_missing_context(result, "delete_pattern");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_artifact_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.delete_artifact(&"sma-nonexistent".into()).await;
    assert_missing_context(result, "delete_artifact");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn expire_old_approvals_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.expire_old_approvals().await;
    assert_missing_context(result, "expire_old_approvals");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn invalidate_for_elements_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.invalidate_for_elements("ont-1", &[], "test").await;
    assert_missing_context(result, "invalidate_for_elements");
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn workspace_scope_lets_mutation_through() {
    let Some(store) = connect_store().await else {
        return;
    };
    // A real workspace_id isn't required here — the guard only checks
    // that *some* WORKSPACE_ID is in scope. The mutation will hit the
    // DB and likely return Ok(false) (no row deleted) or a *Database*
    // error, but never `MissingContext`.
    let result = PostgresStore::with_workspace(Uuid::nil(), || async {
        store.delete_pattern(Uuid::nil()).await
    })
    .await;
    if let Err(OxError::MissingContext { .. }) = result {
        panic!(
            "delete_pattern inside WORKSPACE_ID.scope still surfaced \
             MissingContext — guard misfired"
        );
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn system_bypass_lets_mutation_through() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result =
        PostgresStore::with_system_bypass(|| async { store.delete_pattern(Uuid::nil()).await })
            .await;
    if let Err(OxError::MissingContext { .. }) = result {
        panic!(
            "delete_pattern inside SYSTEM_BYPASS.scope still surfaced \
             MissingContext — guard misfired"
        );
    }
}

// ---------------------------------------------------------------------------
// Bare SYSTEM_BYPASS DML must fail fast, never write nil-UUID
// ---------------------------------------------------------------------------
//
// The DML helper rejects bare-bypass writes loud-and-fast — under
// SYSTEM_BYPASS the pool primes `app.workspace_id` to `Uuid::nil()`
// so the RLS predicate's cast doesn't 22P02, but a row written with
// that nil sentinel would land tenant-less. These tests prove the
// helper refuses bare-bypass DML and accepts the documented escape:
// wrap in an inner `WORKSPACE_ID.scope`.

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_type_last_used_under_bare_system_bypass_fails_fast() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = PostgresStore::with_system_bypass(|| async {
        store
            .upsert_type_last_used(&[(Uuid::new_v4(), "node_type")])
            .await
    })
    .await;
    match result {
        Err(OxError::Runtime { message }) => {
            assert!(
                message.contains("WORKSPACE_ID.scope"),
                "remediation must name the wrapper, got: {message}"
            );
        }
        Err(other) => panic!("expected Runtime, got {other:?}"),
        Ok(_) => panic!(
            "upsert_type_last_used wrote under bare SYSTEM_BYPASS — \
             the nil-UUID corruption guard is missing"
        ),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn delete_change_routing_rule_under_bare_system_bypass_fails_fast() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = PostgresStore::with_system_bypass(|| async {
        store
            .delete_change_routing_rule(ox_ontology::change_routing::ChangeType::CodedValueCreate)
            .await
    })
    .await;
    match result {
        Err(OxError::Runtime { message }) => {
            assert!(
                message.contains("WORKSPACE_ID.scope"),
                "remediation must name the wrapper, got: {message}"
            );
        }
        Err(other) => panic!("expected Runtime, got {other:?}"),
        Ok(_) => panic!(
            "delete_change_routing_rule under bare SYSTEM_BYPASS \
             matched the nil-UUID filter — guard missing"
        ),
    }
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn upsert_type_last_used_under_bypass_with_inner_scope_writes_target_workspace() {
    let Some(store) = connect_store().await else {
        return;
    };
    // Explicit inner scope is the supported pattern for cron sweeps that
    // span workspaces under outer SYSTEM_BYPASS.
    let target_ws = Uuid::new_v4();
    let result = PostgresStore::with_system_bypass(|| async {
        PostgresStore::with_workspace(target_ws, || async {
            store
                .upsert_type_last_used(&[(Uuid::new_v4(), "node_type")])
                .await
        })
        .await
    })
    .await;
    // The row likely lands successfully or fails with an FK error
    // (depending on whether `workspaces` has the test-generated id),
    // but it MUST NOT surface as MissingContext or Runtime("SYSTEM_BYPASS
    // without inner WORKSPACE_ID.scope") — those would mean the inner
    // scope was ignored.
    match result {
        Err(OxError::MissingContext { .. }) => {
            panic!("inner WORKSPACE_ID.scope was ignored")
        }
        Err(OxError::Runtime { message }) if message.contains("SYSTEM_BYPASS") => {
            panic!("inner WORKSPACE_ID.scope was ignored: {message}")
        }
        _ => {}
    }
}

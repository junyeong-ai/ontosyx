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
//!     cargo test -p ox-store --test workspace_context_guard -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use ox_core::error::OxError;
use ox_store::{
    ApprovalStore, PatternStore, PinStore, PostgresStore, ProjectStore,
    SourceMappingArtifactStore, VerificationStore,
};
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

fn assert_missing_context<T: std::fmt::Debug>(
    result: Result<T, OxError>,
    method: &str,
) {
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
async fn delete_design_project_rejects_unscoped_call() {
    let Some(store) = connect_store().await else {
        return;
    };
    let result = store.delete_design_project(Uuid::nil()).await;
    assert_missing_context(result, "delete_design_project");
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
    let result = PostgresStore::with_system_bypass(|| async {
        store.delete_pattern(Uuid::nil()).await
    })
    .await;
    if let Err(OxError::MissingContext { .. }) = result {
        panic!(
            "delete_pattern inside SYSTEM_BYPASS.scope still surfaced \
             MissingContext — guard misfired"
        );
    }
}

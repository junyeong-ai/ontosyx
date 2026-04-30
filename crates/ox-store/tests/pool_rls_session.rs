//! Regression coverage for the per-acquire RLS session-variable
//! setup (`crates/ox-store/src/postgres/rls_session.rs`).
//!
//! The pool wires the same body into BOTH `after_connect` (fresh
//! connections) and `before_acquire` (idle re-acquires). Without
//! the `after_connect` half, the *first* query on a brand-new
//! connection runs with no session vars — RLS WITH CHECK 42501s,
//! and the failure is non-deterministic depending on whether the
//! pool reused an idle connection or grew a new one.
//!
//! These tests pin the contract under both paths so a sqlx upgrade
//! that reshapes the acquire flow surfaces the regression
//! immediately rather than as a flaky `42501` halfway through a
//! test run.
//!
//! Ignored by default — run against a live PostgreSQL:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test pool_rls_session -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use ox_store::{PostgresStore, SYSTEM_BYPASS};
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
    // `max_connections=1` forces every acquire after the first
    // release to take the same connection slot — but the *first*
    // acquire still goes through the freshly-opened path, which is
    // exactly the case `after_connect` exists to handle. Subsequent
    // acquires (after RESET ALL on release) ride `before_acquire`.
    let store = PostgresStore::connect(&url, 1).await.expect("connect");
    store.migrate().await.expect("migrate");
    Some(store)
}

async fn read_session_var(store: &PostgresStore, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT current_setting($1, true)")
        .bind(key)
        .fetch_one(store.pool())
        .await
        .expect("read session var")
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn after_connect_sets_workspace_id_on_first_acquire() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();

    // First query under workspace scope — connection is fresh, the
    // pool grows past `min_connections=0` to serve it. The only
    // hook that fires is `after_connect`. If the body is missing,
    // session var stays unset and 22P02s on cast.
    let observed = PostgresStore::with_workspace(workspace_id, || async {
        read_session_var(&store, "app.workspace_id").await
    })
    .await;

    assert_eq!(
        observed.as_deref(),
        Some(workspace_id.to_string().as_str()),
        "after_connect must set app.workspace_id on the first query"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn before_acquire_resets_workspace_id_across_scopes() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_a = Uuid::new_v4();
    let workspace_b = Uuid::new_v4();

    // Drive two acquires back-to-back. The second one returns from
    // the idle queue (RESET ALL cleared the prior session) and must
    // pick up the new workspace via `before_acquire`. A leak from
    // workspace A into workspace B's connection would defeat
    // tenancy isolation.
    let a = PostgresStore::with_workspace(workspace_a, || async {
        read_session_var(&store, "app.workspace_id").await
    })
    .await;
    let b = PostgresStore::with_workspace(workspace_b, || async {
        read_session_var(&store, "app.workspace_id").await
    })
    .await;

    assert_eq!(a.as_deref(), Some(workspace_a.to_string().as_str()));
    assert_eq!(b.as_deref(), Some(workspace_b.to_string().as_str()));
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn after_connect_sets_system_bypass_on_first_acquire() {
    let Some(store) = connect_store().await else {
        return;
    };

    let observed = SYSTEM_BYPASS
        .scope(true, async { read_session_var(&store, "app.system_bypass").await })
        .await;

    assert_eq!(
        observed.as_deref(),
        Some("true"),
        "after_connect must set app.system_bypass under SYSTEM_BYPASS scope"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn no_scope_leaves_session_vars_unset() {
    let Some(store) = connect_store().await else {
        return;
    };

    // Outside any scope. The hook body's `try_with` returns Err for
    // both task-locals; both branches skip; session vars stay at
    // their connection defaults (NULL). RLS-protected reads return
    // empty (`current_setting(..., true)` returns NULL on missing,
    // `NULL::uuid` is NULL, comparison fails) — the safe deny-all
    // default.
    let workspace = read_session_var(&store, "app.workspace_id").await;
    let bypass = read_session_var(&store, "app.system_bypass").await;
    assert!(
        workspace.is_none() || workspace.as_deref() == Some(""),
        "no scope must leave app.workspace_id unset, got {workspace:?}"
    );
    assert!(
        bypass.is_none() || bypass.as_deref() == Some(""),
        "no scope must leave app.system_bypass unset, got {bypass:?}"
    );
}

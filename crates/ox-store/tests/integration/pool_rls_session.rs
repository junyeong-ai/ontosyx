//! Regression coverage for the per-acquire RLS session-variable
//! setup (`crates/ox-store/src/postgres/rls_session.rs`).
//!
//! The pool wires the same body into BOTH `after_connect` (fresh
//! connections) and `before_acquire` (idle re-acquires), so every
//! connection lands with `app.workspace_id` / `app.system_bypass`
//! configured from the calling task's task-locals before serving
//! its first query — regardless of which acquisition path the pool
//! took. These tests pin that combined invariant. A future sqlx
//! upgrade that reshapes the acquire flow (or a regression that
//! drops one of the two hooks) surfaces immediately rather than as
//! a flaky `42501` halfway through a workspace-scoped INSERT.
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
    // `max_connections=1` keeps the pool tight: every release
    // returns the only slot to the idle queue, every subsequent
    // acquire re-runs `before_acquire` against fresh task-local
    // values. The boot acquire `PgPoolOptions::connect` performs
    // already exercises the `after_connect` half, so any failure
    // here surfaces a regression in either hook.
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
async fn pool_sets_app_workspace_id_under_workspace_scope() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_id = Uuid::new_v4();

    let observed = PostgresStore::with_workspace(workspace_id, || async {
        read_session_var(&store, "app.workspace_id").await
    })
    .await;

    assert_eq!(
        observed.as_deref(),
        Some(workspace_id.to_string().as_str()),
        "the pool must set app.workspace_id from the active task-local"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn pool_resets_app_workspace_id_across_workspace_switch() {
    let Some(store) = connect_store().await else {
        return;
    };
    let workspace_a = Uuid::new_v4();
    let workspace_b = Uuid::new_v4();

    // Drive two acquires back-to-back. Workspace A's value must not
    // leak into B's connection — that would defeat tenancy
    // isolation. `RESET ALL` (in `after_release`) clears the prior
    // session, the per-acquire hook re-establishes from B's
    // task-local.
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
async fn pool_sets_app_system_bypass_under_bypass_scope() {
    let Some(store) = connect_store().await else {
        return;
    };

    let observed = SYSTEM_BYPASS
        .scope(true, async { read_session_var(&store, "app.system_bypass").await })
        .await;

    assert_eq!(
        observed.as_deref(),
        Some("true"),
        "the pool must set app.system_bypass under SYSTEM_BYPASS scope"
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn pool_leaves_session_vars_unset_outside_any_scope() {
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

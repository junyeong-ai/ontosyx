//! RLS (Row Level Security) behavior tests.
//!
//! These tests validate that `0004_rls.sql` policies actually isolate
//! workspaces at the row level, that `SYSTEM_BYPASS` reveals all rows,
//! that unset task-locals deny all access, and — critically — that
//! `FORCE ROW LEVEL SECURITY` makes the policies apply even to the
//! table owner role (the role that created the table via migrations).
//!
//! Ignored by default. Run against a live PostgreSQL instance:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test rls_enforcement -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::let_underscore_must_use
)]

use ox_store::PostgresStore;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
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

/// Connect using `PostgresStore` so the pool's `before_acquire`
/// configures RLS session variables from the task-locals on every
/// connection.
async fn connect_store() -> Option<PostgresStore> {
    let url = resolve_test_db_url()?;
    let store = PostgresStore::connect(&url, 4).await.ok()?;
    store.migrate().await.ok()?;
    Some(store)
}

/// Raw admin pool that bypasses the RLS-aware `before_acquire`. Used
/// only for test setup (creating fixture rows we control entirely).
async fn admin_pool(url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await
        .expect("admin pool connect")
}

/// Synthetic user + two workspaces + a run-unique suffix. All inserts
/// run under `SYSTEM_BYPASS` so RLS does not block fixture setup. The
/// suffix is echoed back so callers can disambiguate pattern rows per
/// test (tests run in parallel; a fixed pattern name would collide on
/// the UNIQUE (user_id, ontology_lineage_id, name) constraint).
struct Fixtures {
    user_id: Uuid,
    ws_a: Uuid,
    ws_b: Uuid,
    suffix: String,
}

async fn seed_fixtures(store: &PostgresStore) -> Fixtures {
    let suffix = Uuid::new_v4().simple().to_string();
    let user_email = format!("rls-test-{}@example.com", &suffix[..8]);
    let slug_a = format!("rls-ws-a-{}", &suffix[..8]);
    let slug_b = format!("rls-ws-b-{}", &suffix[..8]);

    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let provider_sub = format!("rls-test-sub-{}", &suffix[..8]);
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, name, provider, provider_sub, role) \
             VALUES ($1, 'RLS Test User', 'test', $2, 'designer') \
             RETURNING id",
        )
        .bind(&user_email)
        .bind(&provider_sub)
        .fetch_one(pool)
        .await
        .expect("insert user");

        let ws_a: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('RLS Workspace A', $1, $2) \
             RETURNING id",
        )
        .bind(&slug_a)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace A");

        let ws_b: Uuid = sqlx::query_scalar(
            "INSERT INTO workspaces (name, slug, owner_id) \
             VALUES ('RLS Workspace B', $1, $2) \
             RETURNING id",
        )
        .bind(&slug_b)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("insert workspace B");

        Fixtures {
            user_id,
            ws_a,
            ws_b,
            suffix,
        }
    })
    .await
}

async fn insert_pattern(store: &PostgresStore, ws: Uuid, user_id: &str, name: &str) -> Uuid {
    PostgresStore::with_workspace(ws, || async {
        let pool = store.pool();
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO saved_query_patterns \
             (user_id, ontology_lineage_id, name, pattern_ir) \
             VALUES ($1, 'test-lineage', $2, $3) \
             RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .bind(json!({"nodes": [], "edges": [], "filters": []}))
        .fetch_one(pool)
        .await
        .expect("insert pattern")
    })
    .await
}

/// Scope the count to our fixture user so concurrent tests never
/// read each other's rows when policies DO let them through.
async fn count_our_patterns(store: &PostgresStore, user_id: &str) -> i64 {
    let pool = store.pool();
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM saved_query_patterns WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("count patterns")
}

async fn cleanup(store: &PostgresStore, fx: &Fixtures) {
    PostgresStore::with_system_bypass(|| async {
        let pool = store.pool();
        let _ = sqlx::query("DELETE FROM saved_query_patterns WHERE workspace_id IN ($1, $2)")
            .bind(fx.ws_a)
            .bind(fx.ws_b)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM workspaces WHERE id IN ($1, $2)")
            .bind(fx.ws_a)
            .bind(fx.ws_b)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(fx.user_id)
            .execute(pool)
            .await;
    })
    .await;
}

// ---------------------------------------------------------------------------
// 1. `ws_isolation` policy — each workspace sees only its own rows.
//
// Because the test database's application user IS the table owner
// (migrations run as that role), a passing result here implicitly
// validates that `FORCE ROW LEVEL SECURITY` is in effect — without
// FORCE, the owner would see every row regardless of the policy.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn ws_isolation_limits_visibility_to_owning_workspace() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixtures(&store).await;
    let user_tag = format!("rls-iso-{}", &fx.suffix[..8]);
    insert_pattern(&store, fx.ws_a, &user_tag, "pattern-a").await;
    insert_pattern(&store, fx.ws_b, &user_tag, "pattern-b").await;

    let count_from_a =
        PostgresStore::with_workspace(fx.ws_a, || count_our_patterns(&store, &user_tag)).await;
    assert_eq!(
        count_from_a, 1,
        "workspace A must see exactly its own pattern (force+isolation must hide B's)"
    );

    let count_from_b =
        PostgresStore::with_workspace(fx.ws_b, || count_our_patterns(&store, &user_tag)).await;
    assert_eq!(
        count_from_b, 1,
        "workspace B must see exactly its own pattern"
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 2. `system_bypass` policy — privileged paths see every row.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn system_bypass_exposes_all_workspace_rows() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixtures(&store).await;
    let user_tag = format!("rls-bypass-{}", &fx.suffix[..8]);
    insert_pattern(&store, fx.ws_a, &user_tag, "pattern-a").await;
    insert_pattern(&store, fx.ws_b, &user_tag, "pattern-b").await;

    let count_under_bypass =
        PostgresStore::with_system_bypass(|| count_our_patterns(&store, &user_tag)).await;
    assert_eq!(
        count_under_bypass, 2,
        "SYSTEM_BYPASS must surface rows from every workspace"
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 3. No task-local context — fail loud, not silent. A caller that
//    forgets to enter either `with_workspace` or `with_system_bypass`
//    must not read anything: `before_acquire` leaves `app.workspace_id`
//    unset, the RLS policy's `current_setting(...)::uuid` cast fails
//    on the empty string, and PostgreSQL returns error 22P02. That is
//    the correct production behavior — an empty result (silent deny)
//    would be indistinguishable from "no rows exist" and could mask
//    missing workspace scope in higher layers.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn missing_task_local_fails_loud_not_silent() {
    let Some(store) = connect_store().await else {
        return;
    };
    let fx = seed_fixtures(&store).await;
    let user_tag = format!("rls-deny-{}", &fx.suffix[..8]);
    insert_pattern(&store, fx.ws_a, &user_tag, "pattern-a").await;
    insert_pattern(&store, fx.ws_b, &user_tag, "pattern-b").await;

    // Directly issue the query without entering any scope: task-locals
    // are unset. We *do not* use `count_our_patterns` here because it
    // panics on sqlx errors; we want the raw Result.
    let pool = store.pool();
    let result: Result<i64, sqlx::Error> = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM saved_query_patterns WHERE user_id = $1",
    )
    .bind(&user_tag)
    .fetch_one(pool)
    .await;

    let err = result.expect_err("unscoped access must fail, not silently succeed");
    let message = err.to_string();
    assert!(
        message.contains("uuid") || message.contains("22P02"),
        "error must be the RLS uuid-cast failure (fail loud), got: {message}"
    );

    cleanup(&store, &fx).await;
}

// ---------------------------------------------------------------------------
// 4. `FORCE ROW LEVEL SECURITY` explicit check — every workspace-
//    scoped table that has `ENABLE ROW LEVEL SECURITY` must also have
//    `FORCE`, because the migrations run as the application role and
//    that role is the table owner in standard deployments. Without
//    FORCE, every query from the application would bypass RLS.
// ---------------------------------------------------------------------------
#[tokio::test]
#[ignore]
async fn every_rls_table_also_forces_the_policies() {
    let Some(url) = resolve_test_db_url() else {
        return;
    };
    let pool = admin_pool(&url).await;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("apply migrations");

    let rows = sqlx::query(
        "SELECT c.relname AS table_name, \
                c.relrowsecurity AS has_rls, \
                c.relforcerowsecurity AS has_force \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' \
           AND c.relkind = 'r' \
           AND c.relrowsecurity = true \
         ORDER BY c.relname",
    )
    .fetch_all(&pool)
    .await
    .expect("query pg_class");

    assert!(
        !rows.is_empty(),
        "expected RLS-enabled tables (migrations may not have run)"
    );

    let mut missing_force: Vec<String> = Vec::new();
    for r in &rows {
        let name: String = r.try_get("table_name").unwrap();
        let forced: bool = r.try_get("has_force").unwrap();
        if !forced {
            missing_force.push(name);
        }
    }
    assert!(
        missing_force.is_empty(),
        "tables have RLS enabled but not FORCED (owner role silently bypasses): {missing_force:?}"
    );
}

//! Catalog-level invariants the middleware relies on.
//!
//! `crates/ox-api/src/middleware.rs::workspace_context` calls
//! `find_default_workspace(user_id)` and `get_member_role(workspace_id,
//! user_id)` *before* `WORKSPACE_ID.scope` wraps the request — the
//! `scope_request` integration test in `middleware.rs::tests` proves
//! the *post-scope* path; this test pins the assumption the
//! *pre-scope* path leans on:
//!
//!   `workspaces`, `workspace_members`, and `users` carry **no**
//!   PostgreSQL RLS policies. They are visible to any connection
//!   regardless of `app.workspace_id` / `app.system_bypass` session
//!   state.
//!
//! If a future migration adds RLS to one of these tables, this test
//! fails — and the failure message names the cross-cutting
//! middleware site that will silently break (the same `[22P02]
//! invalid input syntax for type uuid: ""` regression the original
//! ACL ordering bug surfaced as).
//!
//! Ignored by default. Run against a live PostgreSQL instance:
//!
//! ```sh
//! OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx \
//!     cargo test -p ox-store --test rls_invariants -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::let_underscore_must_use
)]

use sqlx::Row;

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

const PRE_SCOPE_TABLES: &[&str] = &["workspaces", "workspace_members", "users"];

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn pre_scope_tables_carry_no_rls_policies() {
    let Some(url) = resolve_test_db_url() else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };

    // Bring the schema up via the canonical entry-point so the
    // RLS pool hooks (`after_connect` / `before_acquire`) are
    // attached the same way production runs them. The pool is
    // re-acquired below for the catalog queries — those are admin
    // reads against `pg_policies` / `pg_class`, not RLS-gated user
    // data, so they pass regardless of session-var state.
    let store = ox_store::PostgresStore::connect(&url, 2)
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let pool = store.pool().clone();

    let rows = sqlx::query(
        "SELECT tablename, policyname \
         FROM pg_policies \
         WHERE schemaname = 'public' AND tablename = ANY($1)",
    )
    .bind(PRE_SCOPE_TABLES)
    .fetch_all(&pool)
    .await
    .expect("pg_policies query");

    let offenders: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("tablename"),
                r.get::<String, _>("policyname"),
            )
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "RLS policies must NOT live on pre-scope tables \
         ({tables:?}) — `crates/ox-api/src/middleware.rs::workspace_context` \
         calls `find_default_workspace` / `get_member_role` *before* \
         `WORKSPACE_ID.scope` wraps the request. Adding RLS to any of these \
         tables silently re-introduces the `[22P02] invalid input syntax \
         for type uuid: \"\"` regression the ACL/scope ordering test \
         (middleware::tests::scope_request_loads_acl_snapshot_inside_workspace_scope) \
         was added to guard against. Update both tests together. \
         Offending policies: {offenders:?}",
        tables = PRE_SCOPE_TABLES,
    );
}

/// Catalog scan: every public table that carries a `workspace_id`
/// column MUST be full-RLS protected (rowsecurity + forcerowsecurity
/// + `ws_isolation` policy + `system_bypass` policy). Tripping this
/// assertion means a recent migration added `workspace_id` without
/// the matching `ALTER TABLE … ENABLE / FORCE ROW LEVEL SECURITY`
/// + `CREATE POLICY` block — silently exposing rows across
/// workspaces. The CLAUDE.md "RLS Policy Pattern (required for all
/// workspace-scoped tables)" section codifies the contract; this
/// test enforces it.
///
/// Pre-scope tables (`workspaces`, `workspace_members`, `users`)
/// are excluded by name — their non-RLS status is the OPPOSITE
/// invariant pinned by `pre_scope_tables_carry_no_rls_policies`.
#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn workspace_scoped_tables_have_full_rls_protection() {
    let Some(url) = resolve_test_db_url() else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };

    let store = ox_store::PostgresStore::connect(&url, 2)
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let pool = store.pool().clone();

    // Tables that hold a `workspace_id` column — the universe of
    // workspace-scoped data we care about.
    let candidates = sqlx::query(
        "SELECT table_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND column_name = 'workspace_id'",
    )
    .fetch_all(&pool)
    .await
    .expect("information_schema query");
    let tables: Vec<String> = candidates
        .iter()
        .map(|r| r.get::<String, _>("table_name"))
        .filter(|name| !PRE_SCOPE_TABLES.contains(&name.as_str()))
        .collect();

    assert!(
        !tables.is_empty(),
        "expected at least one workspace-scoped table — schema may not be migrated",
    );

    // rowsecurity + forcerowsecurity flags
    let class_rows = sqlx::query(
        "SELECT relname, relrowsecurity, relforcerowsecurity \
         FROM pg_class \
         WHERE relnamespace = 'public'::regnamespace AND relname = ANY($1)",
    )
    .bind(&tables)
    .fetch_all(&pool)
    .await
    .expect("pg_class query");

    let mut missing_enable: Vec<String> = Vec::new();
    let mut missing_force: Vec<String> = Vec::new();
    for r in &class_rows {
        let name: String = r.get("relname");
        let enabled: bool = r.get("relrowsecurity");
        let forced: bool = r.get("relforcerowsecurity");
        if !enabled {
            missing_enable.push(name.clone());
        }
        if !forced {
            missing_force.push(name);
        }
    }

    // Policy presence checked semantically — a tenant-gate policy
    // is any policy whose `qual` references `app.workspace_id`. The
    // canonical name is `ws_isolation`, but tables that support
    // global-or-workspace dual tenancy intentionally use other
    // names (`ws_or_global`, `ws_write`, `ws_or_global_read`) — all
    // of them satisfy the gate because the WHERE clause itself
    // mentions the session var. Checking the SQL text avoids
    // false-positives when a migration introduces a clearer name.
    let policy_rows = sqlx::query(
        "SELECT tablename, policyname, qual FROM pg_policies \
         WHERE schemaname = 'public' AND tablename = ANY($1)",
    )
    .bind(&tables)
    .fetch_all(&pool)
    .await
    .expect("pg_policies query");

    use std::collections::{HashMap, HashSet};
    let mut policies_by_table: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for r in &policy_rows {
        let table: String = r.get("tablename");
        let policy: String = r.get("policyname");
        let qual: String = r.try_get("qual").unwrap_or_default();
        policies_by_table
            .entry(table)
            .or_default()
            .push((policy, qual));
    }

    let mut missing_tenant_gate: Vec<String> = Vec::new();
    let mut missing_bypass: Vec<String> = Vec::new();
    for table in &tables {
        let entries = policies_by_table.get(table).cloned().unwrap_or_default();
        let policy_names: HashSet<&str> =
            entries.iter().map(|(name, _)| name.as_str()).collect();
        // `system_bypass` is named consistently — admin paths look
        // for that exact name when configuring sessions.
        if !policy_names.contains("system_bypass") {
            missing_bypass.push(table.clone());
        }
        // Tenant gate: any policy whose qual references the session
        // var. The qual text in pg_policies expands `current_setting`
        // canonically to `current_setting(...)` — match liberally.
        let has_tenant_gate = entries.iter().any(|(_, qual)| {
            qual.contains("app.workspace_id") || qual.contains("workspace_id")
        });
        if !has_tenant_gate {
            missing_tenant_gate.push(table.clone());
        }
    }

    let problems = [
        ("ENABLE ROW LEVEL SECURITY", &missing_enable),
        ("FORCE ROW LEVEL SECURITY", &missing_force),
        ("tenant-gate policy referencing app.workspace_id", &missing_tenant_gate),
        ("CREATE POLICY system_bypass", &missing_bypass),
    ];
    let any_failure = problems.iter().any(|(_, list)| !list.is_empty());

    assert!(
        !any_failure,
        "Workspace-scoped tables are missing required RLS protection. \
         The CLAUDE.md `RLS Policy Pattern` section requires all four of \
         ENABLE / FORCE / ws_isolation / system_bypass on every table that \
         carries `workspace_id`. Add the missing clauses to the migration \
         that introduced the column. Offenders by clause:\n\
         {}",
        problems
            .iter()
            .filter(|(_, list)| !list.is_empty())
            .map(|(label, list)| format!("  {label}: {list:?}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[tokio::test]
#[ignore = "requires OX_TEST_DATABASE_URL"]
async fn force_row_level_security_is_off_on_pre_scope_tables() {
    let Some(url) = resolve_test_db_url() else {
        eprintln!("OX_TEST_DATABASE_URL not set — skipping");
        return;
    };

    let store = ox_store::PostgresStore::connect(&url, 2)
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let pool = store.pool().clone();

    // `pg_class.relrowsecurity` is the "ENABLE ROW LEVEL SECURITY"
    // flag; `relforcerowsecurity` is the "FORCE" flag that applies
    // policies even to the table owner. Either being on for the
    // pre-scope tables means RLS is active and the middleware
    // assumption breaks.
    let rows = sqlx::query(
        "SELECT relname, relrowsecurity, relforcerowsecurity \
         FROM pg_class \
         WHERE relnamespace = 'public'::regnamespace \
           AND relname = ANY($1)",
    )
    .bind(PRE_SCOPE_TABLES)
    .fetch_all(&pool)
    .await
    .expect("pg_class query");

    let offenders: Vec<(String, bool, bool)> = rows
        .iter()
        .filter_map(|r| {
            let name: String = r.get("relname");
            let enabled: bool = r.get("relrowsecurity");
            let forced: bool = r.get("relforcerowsecurity");
            (enabled || forced).then_some((name, enabled, forced))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "RLS must remain DISABLED on pre-scope tables ({tables:?}). \
         If a security audit demands turning it on, the middleware site \
         in `crates/ox-api/src/middleware.rs::workspace_context` must \
         move both `find_default_workspace` and `get_member_role` inside \
         `WORKSPACE_ID.scope` — and the integration test \
         `middleware::tests::scope_request_loads_acl_snapshot_inside_workspace_scope` \
         must be extended to cover the human-user fallback path. \
         Offenders: {offenders:?}",
        tables = PRE_SCOPE_TABLES,
    );
}

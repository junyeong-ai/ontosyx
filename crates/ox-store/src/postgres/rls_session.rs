//! Per-connection PostgreSQL session-variable setup for the RLS
//! policies in `0001_schema.sql`.
//!
//! The same body runs on every acquire path the pool can take:
//!
//! - `after_connect` — fires once per freshly-opened connection. New
//!   connections never go through `before_acquire` in sqlx 0.8
//!   (`PoolInner::connect` is the path the pool takes when growing
//!   past current size), so without this hook the *first* query on
//!   a brand-new connection runs with no session variables set, the
//!   ws_isolation `WITH CHECK` raises 42501 even for callers that
//!   correctly entered `WORKSPACE_ID.scope`, and the failure is
//!   non-deterministic — depends on whether the pool reused an idle
//!   connection or grew a new one.
//! - `before_acquire` — fires every time the pool hands back an idle
//!   connection. `RESET ALL` (in `after_release`) cleared the prior
//!   session state, so we re-establish session vars from the current
//!   task-local before serving the next caller.
//!
//! Priority: SYSTEM_BYPASS > WORKSPACE_ID > none. The "none" path
//! leaves both variables unset; RLS-protected reads return empty,
//! RLS-protected writes raise 42501 — the safe deny-all default
//! that initialisation paths (`PgPool::connect`'s internal health
//! check, OIDC provider boot) rely on.

use uuid::Uuid;

use super::{SYSTEM_BYPASS, WORKSPACE_ID};

pub(super) async fn configure_rls_session_vars(
    conn: &mut sqlx::PgConnection,
) -> Result<(), sqlx::Error> {
    if SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
        sqlx::query("SELECT set_config('app.system_bypass', 'true', false)")
            .execute(&mut *conn)
            .await?;
        // PostgreSQL evaluates PERMISSIVE policies as OR but still
        // casts every policy's predicate expression. `ws_isolation`'s
        // `current_setting('app.workspace_id', true)::uuid` raises
        // 22P02 on an empty session var even when `system_bypass`
        // would have matched. Set a nil sentinel so the cast always
        // succeeds; it never matches a real workspace row, and the
        // policy OR resolves through `system_bypass`.
        sqlx::query("SELECT set_config('app.workspace_id', $1, false)")
            .bind(Uuid::nil().to_string())
            .execute(&mut *conn)
            .await?;
        // Best-effort: prime to the actual default workspace if it
        // exists, so INSERT DEFAULTs resolve to a real id when the
        // system task creates new rows. The earlier sentinel keeps
        // the cast safe whether or not this query matches; "relation
        // does not exist" during first-boot is also tolerated.
        #[allow(clippy::let_underscore_must_use)]
        let _ = sqlx::query(
            "SELECT set_config('app.workspace_id', id::text, false) \
             FROM workspaces WHERE slug = 'default' LIMIT 1",
        )
        .execute(&mut *conn)
        .await;
        return Ok(());
    }
    if let Ok(ws_id) = WORKSPACE_ID.try_with(|id| *id) {
        sqlx::query("SELECT set_config('app.workspace_id', $1, false)")
            .bind(ws_id.to_string())
            .execute(&mut *conn)
            .await?;
        // ADR-0041: bound the worst-case query and idle-in-
        // transaction durations. `RESET ALL` clears these on release
        // so we re-apply per acquire. Bypass paths (migrations /
        // cron sweeps) intentionally skip — those are bounded by
        // their outer scheduler and may legitimately run long.
        sqlx::query("SET statement_timeout = 30000")
            .execute(&mut *conn)
            .await?;
        sqlx::query("SET idle_in_transaction_session_timeout = 5000")
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

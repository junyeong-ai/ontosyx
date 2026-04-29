//! PostgreSQL-backed [`crate::store::Store`] implementation.
//!
//! This module is split by trait: each `impl <Trait>Store for
//! PostgresStore` block lives in its own sibling file (see
//! declarations at the bottom). The public surface remains exactly
//! the old `postgres::PostgresStore` + `postgres::{WORKSPACE_ID,
//! SYSTEM_BYPASS}` re-exports — nothing outside this crate should
//! care that the file was split.
//!
//! ## Why split by trait, not by entity
//!
//! The old monolithic `postgres.rs` had grown to ~7700 lines with
//! 38 impl blocks. Each sibling file pulls in `use super::*;` so
//! the crate-wide imports (sqlx, uuid, ox_core, ox_ontology, store
//! traits, models, ...) defined once here flow into every impl.
//! Trait-level files keep each concern isolated — a reviewer
//! auditing `QualitySignalStore` only loads `quality_signal.rs`
//! plus this header, never the whole file.
//!
//! ## Shared helpers
//!
//! - [`build_cursor_page`] — compound `timestamp|uuid` cursor
//!   pagination used by every listing endpoint.
//! - [`check_cas_result`] — CAS (compare-and-swap) verifier for
//!   revision-gated updates; distinguishes "no rows matched the
//!   revision filter" from "update succeeded".
//! - [`to_ox_error`] — PostgreSQL SQLSTATE → [`OxError`] mapper.
//!   Every sibling file maps `sqlx::Error` through this helper so
//!   SQLSTATE semantics stay consistent across impls.
//!
//! ## Task-local workspace context
//!
//! [`WORKSPACE_ID`] and [`SYSTEM_BYPASS`] are set by the workspace
//! middleware and by scheduled tasks respectively. The pool's
//! `before_acquire` hook reads them and configures PostgreSQL
//! session variables (`app.workspace_id`, `app.system_bypass`) on
//! every connection acquire — these drive the RLS policies declared
//! in the migration files. The impl blocks themselves never touch
//! the task-local directly.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::info;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};

use crate::models::*;
use crate::store::{
    AclStore, AgentSessionStore, AmbiguityStore, AnalysisResultStore, AnalysisSnapshot,
    ApprovalCommentStore, ApprovalStore, AuditRecord, AuditStore, AuditTrailFilter,
    AuditTrailStore, ChangeRoutingStore, ConfigStore, CursorPage, CursorParams, DashboardStore,
    EmbeddingRetryStore, ExtendResult, HealthStore, InsightStore, KnowledgeStore,
    LineageStore, LoadCheckpointStore, MeteringStore, PatternStore, PerspectiveStore, PinStore,
    ProjectStore,
    PromptTemplateStore, QualitySignalStore, QualityStore, QueryStore, RecipeStore, ReportStore,
    ScheduledTaskStore, SourceMappingArtifactStore, StaleConceptProposalStore, ToolApprovalStore, UserStore,
    VerificationStore, WorkspaceStore,
};

tokio::task_local! {
    /// Per-request workspace ID. Set by the workspace middleware.
    /// Used by PgPool's `before_acquire` to configure RLS session variable.
    pub static WORKSPACE_ID: Uuid;

    /// When true, `before_acquire` sets `app.system_bypass` instead of
    /// `app.workspace_id`. Used by scheduled tasks, cleanup, and migrations
    /// that need cross-workspace access.
    pub static SYSTEM_BYPASS: bool;
}

/// Assert that the caller is inside a `WORKSPACE_ID.scope(...)` or a
/// `SYSTEM_BYPASS.scope(true, ...)` block. Mutating store methods
/// call this at the top so a programming error (forgot to wrap a
/// background task in `with_workspace`) surfaces as a structured
/// `MissingContext` error instead of a silent zero-rows-affected
/// from RLS.
///
/// Read-only methods don't have to call this — RLS will simply
/// return an empty result when no context is set, which is the
/// safe deny-all default. Mutations need explicit-fail because a
/// silently-skipped write looks like success to the caller.
///
/// `kind` should describe the missing axis (`"workspace"` for the
/// canonical case). Future axes (`"project"`, `"user"`) reuse the
/// same `OxError::MissingContext` shape.
pub fn require_workspace_context() -> OxResult<()> {
    if SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
        return Ok(());
    }
    if WORKSPACE_ID.try_with(|_| ()).is_ok() {
        return Ok(());
    }
    Err(OxError::MissingContext {
        kind: "workspace".to_string(),
        message: "store mutation invoked outside any \
                  WORKSPACE_ID.scope or SYSTEM_BYPASS.scope. \
                  Wrap the call with PostgresStore::with_workspace \
                  or PostgresStore::with_system_bypass."
            .to_string(),
    })
}

/// Resolve the workspace UUID a per-row DML statement must bind into the
/// `workspace_id` column. Use this helper *whenever* a store impl needs to
/// stamp a row with the caller's workspace — never read it back from the
/// PostgreSQL session variable inside the SQL itself.
///
/// Why: the pool's `before_acquire` primes `app.workspace_id` to
/// [`Uuid::nil`] under [`SYSTEM_BYPASS`] so the RLS predicate's
/// `::uuid` cast doesn't 22P02. That sentinel is correct for *reads*
/// (the OR-with `system_bypass` policy lets the row through anyway)
/// but wrong for *writes*: a SQL like
/// `VALUES (current_setting('app.workspace_id', true)::uuid, ...)`
/// silently writes the nil sentinel as the row's tenant. Cross-workspace
/// cron paths that rely on this idiom land every row under the same
/// nil-UUID workspace — silent data corruption.
///
/// Resolution order:
/// 1. Inner [`WORKSPACE_ID`] scope wins, even when [`SYSTEM_BYPASS`] is
///    also active. Cron sweeps are expected to wrap per-workspace work
///    in `WORKSPACE_ID.scope(target_ws, ...)` *inside* the outer
///    `SYSTEM_BYPASS.scope(true, ...)`.
/// 2. [`SYSTEM_BYPASS`] alone is rejected — the caller has not declared
///    which workspace owns the new row. The error message names the fix.
/// 3. No scope at all surfaces as [`OxError::MissingContext`] just like
///    [`require_workspace_context`].
pub(crate) fn bound_workspace_id_for_dml() -> OxResult<Uuid> {
    if let Ok(id) = WORKSPACE_ID.try_with(|id| *id) {
        return Ok(id);
    }
    if SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
        return Err(OxError::Runtime {
            message: "store DML invoked under SYSTEM_BYPASS without an \
                      inner WORKSPACE_ID.scope. Wrap the call in \
                      `WORKSPACE_ID.scope(target_workspace_id, async { ... })` \
                      so the row binds to a real workspace instead of \
                      the nil-UUID sentinel."
                .to_string(),
        });
    }
    Err(OxError::MissingContext {
        kind: "workspace".to_string(),
        message: "store DML requires a WORKSPACE_ID context".to_string(),
    })
}

// ---------------------------------------------------------------------------
// PostgresStore — Store implementation backed by PostgreSQL
// ---------------------------------------------------------------------------

pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(url: &str, max_connections: u32) -> OxResult<Self> {
        Self::connect_with_min(url, max_connections, 0).await
    }

    pub async fn connect_with_min(
        url: &str,
        max_connections: u32,
        min_connections: u32,
    ) -> OxResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .idle_timeout(std::time::Duration::from_secs(300))
            // RLS: configure session variables on every connection acquire.
            // Priority: SYSTEM_BYPASS > WORKSPACE_ID > (no context = deny all)
            .before_acquire(|conn, _meta| {
                Box::pin(async move {
                    if SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
                        // System task: bypass RLS for cross-workspace access.
                        sqlx::query("SELECT set_config('app.system_bypass', 'true', false)")
                            .execute(&mut *conn)
                            .await?;
                        // PostgreSQL evaluates PERMISSIVE policies as
                        // OR but still casts every policy's predicate
                        // expression — `ws_isolation`'s
                        // `current_setting('app.workspace_id', true)::uuid`
                        // raises 22P02 on an empty session var even when
                        // `system_bypass` would have matched. Set a nil
                        // sentinel so the cast always succeeds; it never
                        // matches a real workspace row, and policy OR
                        // resolves through `system_bypass`.
                        sqlx::query("SELECT set_config('app.workspace_id', $1, false)")
                            .bind(Uuid::nil().to_string())
                            .execute(&mut *conn)
                            .await?;
                        // Best-effort: prime to the actual default
                        // workspace if it exists, so INSERT DEFAULTs
                        // resolve to a real id when the system task
                        // creates new rows. The earlier sentinel keeps
                        // the cast safe whether or not this query
                        // matches; "relation does not exist" during
                        // first-boot is also tolerated.
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = sqlx::query(
                            "SELECT set_config('app.workspace_id', id::text, false) \
                             FROM workspaces WHERE slug = 'default' LIMIT 1",
                        )
                        .execute(&mut *conn)
                        .await;
                    } else if let Ok(ws_id) = WORKSPACE_ID.try_with(|id| *id) {
                        // Normal request: scope to workspace via RLS
                        sqlx::query("SELECT set_config('app.workspace_id', $1, false)")
                            .bind(ws_id.to_string())
                            .execute(&mut *conn)
                            .await?;
                    }
                    // No context set: RLS returns empty results (safe deny-all default).
                    // This is expected during migrations and OIDC provider initialization.
                    Ok(true)
                })
            })
            // RLS: clear workspace context when connection returns to pool
            .after_release(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("RESET ALL").execute(&mut *conn).await.ok();
                    Ok(true)
                })
            })
            .connect(url)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("PostgreSQL connection failed: {e}"),
            })?;

        info!(
            max = max_connections,
            min = min_connections,
            "Connected to PostgreSQL"
        );
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the underlying connection pool (for sharing with PgVectorStore).
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Run database migrations to create/update tables.
    ///
    /// Wraps the sqlx migration run in [`SYSTEM_BYPASS`] so the
    /// schema's workspace-scoped RLS policies let the seed INSERTs
    /// through — specifically the global (`workspace_id IS NULL`)
    /// rows in `change_routing_rules`, which would otherwise fail
    /// the `ws_write` WITH CHECK clause. `app.system_bypass = 'true'`
    /// matches the `system_bypass` policy on every workspace-scoped
    /// table and covers the first-boot seed path end-to-end.
    pub async fn migrate(&self) -> OxResult<()> {
        SYSTEM_BYPASS
            .scope(true, async {
                sqlx::migrate!("./migrations")
                    .run(&self.pool)
                    .await
                    .map_err(|e| OxError::Runtime {
                        message: format!("Migration failed: {e}"),
                    })
            })
            .await?;

        info!("Database migrations applied");
        Ok(())
    }

    /// Run a future within a workspace context.
    /// Sets the task-local so `before_acquire` configures RLS on every connection.
    /// Used by the workspace middleware and background tasks targeting a specific workspace.
    pub async fn with_workspace<F, Fut, T>(workspace_id: Uuid, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        WORKSPACE_ID.scope(workspace_id, f()).await
    }

    /// Run a future with system bypass (cross-workspace access).
    /// Sets the task-local so `before_acquire` configures `app.system_bypass`
    /// instead of `app.workspace_id`. Used by scheduled tasks, cleanup, and migrations.
    pub async fn with_system_bypass<F, Fut, T>(f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        SYSTEM_BYPASS.scope(true, f()).await
    }
}

// ---------------------------------------------------------------------------
// Shared helpers — referenced by sibling modules via `use super::*;`
// ---------------------------------------------------------------------------

/// Build a CursorPage from a fetched Vec (fetched with limit+1).
/// Uses compound cursor "timestamp|uuid" to guarantee no row is skipped
/// even when multiple rows share the same timestamp.
pub(crate) fn build_cursor_page<T, F>(
    mut rows: Vec<T>,
    limit: i64,
    cursor_extractor: F,
) -> CursorPage<T>
where
    T: serde::Serialize,
    F: Fn(&T) -> (DateTime<Utc>, Uuid),
{
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        rows.last().map(|last| {
            let (ts, id) = cursor_extractor(last);
            format!("{}|{}", ts.format("%Y-%m-%dT%H:%M:%S%.fZ"), id)
        })
    } else {
        None
    };
    CursorPage {
        items: rows,
        next_cursor,
    }
}

/// CAS-update verifier. Distinguishes "no row matched the revision
/// filter" (409 Conflict) from "update succeeded" (0 rows affected
/// would otherwise be silently ignored).
pub(crate) fn check_cas_result(rows_affected: u64) -> OxResult<()> {
    if rows_affected == 0 {
        Err(OxError::Conflict {
            message: "Project was modified by another session (revision mismatch) or is in an invalid state for this operation".to_string(),
        })
    } else {
        Ok(())
    }
}

/// Map a `sqlx::Error` (particularly `Database(db_err)` carrying a
/// PostgreSQL SQLSTATE) into the typed [`OxError`] the API surface
/// expects. Every impl runs DB errors through this so SQLSTATE
/// semantics stay consistent across store implementations.
pub(crate) fn to_ox_error(e: sqlx::Error) -> OxError {
    match &e {
        sqlx::Error::Database(db_err) => {
            let code = db_err.code().unwrap_or_default();
            match code.as_ref() {
                "23505" => OxError::Conflict {
                    message: format!("Duplicate entry: {db_err}"),
                },
                "23503" => OxError::NotFound {
                    entity: format!("Referenced entity: {db_err}"),
                },
                "23502" => OxError::Validation {
                    field: "unknown".to_string(),
                    message: format!("Not-null constraint violated: {db_err}"),
                },
                "23514" => OxError::Validation {
                    field: "unknown".to_string(),
                    message: format!("Check constraint violated: {db_err}"),
                },
                _ => OxError::Runtime {
                    message: format!("Database error [{code}]: {e}"),
                },
            }
        }
        sqlx::Error::PoolTimedOut => OxError::Runtime {
            message: "Database connection pool exhausted".to_string(),
        },
        _ => OxError::Runtime {
            message: format!("Database error: {e}"),
        },
    }
}

// ---------------------------------------------------------------------------
// Per-trait impl submodules
// ---------------------------------------------------------------------------
//
// Each module declares `impl <Trait>Store for PostgresStore { ... }`
// and pulls the crate-wide imports in via `use super::*;`. Visibility
// stays private — callers reach the store via the public trait.

mod acl;
mod agent_session;
mod ambiguity;
mod analysis_result;
mod api_key;
mod approval;
mod approval_comment;
mod audit;
mod audit_trail;
mod change_routing;
mod config;
mod dashboard;
mod data_source;
mod embedding_retry;
mod health;
mod insight;
mod knowledge;
mod lineage;
mod load_checkpoint;
mod metering;
mod model_config;
mod notification;
mod ontology_materialize;
mod ontology_navigation;
mod ontology_version;
mod pattern;
mod perspective;
mod pin;
mod project;
mod prompt_template;
mod quality;
mod quality_baseline;
mod quality_signal;
mod query;
mod recipe;
mod report;
mod scheduled_task;
mod source_mapping;
mod stale_concept_proposal;
mod tool_approval;
mod user;
mod verification;
mod workspace;


#[cfg(test)]
mod context_guard_tests {
    use super::*;

    #[tokio::test]
    async fn require_workspace_context_passes_inside_workspace_scope() {
        let result = WORKSPACE_ID
            .scope(Uuid::nil(), async { require_workspace_context() })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn require_workspace_context_passes_inside_system_bypass() {
        let result =
            SYSTEM_BYPASS.scope(true, async { require_workspace_context() }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn require_workspace_context_rejects_when_no_scope() {
        let err = require_workspace_context().expect_err("must reject");
        match err {
            OxError::MissingContext { kind, .. } => assert_eq!(kind, "workspace"),
            other => panic!("expected MissingContext, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dml_helper_returns_workspace_id_inside_workspace_scope() {
        let target = Uuid::new_v4();
        let resolved = WORKSPACE_ID
            .scope(target, async {
                bound_workspace_id_for_dml().expect("must resolve")
            })
            .await;
        assert_eq!(resolved, target);
    }

    #[tokio::test]
    async fn dml_helper_prefers_inner_workspace_scope_under_system_bypass() {
        let target = Uuid::new_v4();
        let resolved = SYSTEM_BYPASS
            .scope(true, async {
                WORKSPACE_ID
                    .scope(target, async {
                        bound_workspace_id_for_dml().expect("inner scope wins")
                    })
                    .await
            })
            .await;
        assert_eq!(resolved, target);
    }

    #[tokio::test]
    async fn dml_helper_rejects_bare_system_bypass() {
        let err = SYSTEM_BYPASS
            .scope(true, async { bound_workspace_id_for_dml() })
            .await
            .expect_err("bare bypass must fail");
        match err {
            OxError::Runtime { message } => {
                assert!(
                    message.contains("WORKSPACE_ID.scope"),
                    "remediation must name the wrapper, got: {message}"
                );
            }
            other => panic!("expected Runtime, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dml_helper_rejects_no_scope_with_missing_context() {
        let err = bound_workspace_id_for_dml().expect_err("no context must fail");
        match err {
            OxError::MissingContext { kind, .. } => assert_eq!(kind, "workspace"),
            other => panic!("expected MissingContext, got {other:?}"),
        }
    }
}

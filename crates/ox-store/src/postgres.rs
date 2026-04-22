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
    ApprovalStore, AuditStore, ChangeRoutingStore, ConfigStore, CursorPage, CursorParams,
    DashboardStore, EmbeddingRetryStore, ExtendResult, HealthStore, KnowledgeStore, LineageStore,
    LoadCheckpointStore, MeteringStore, PatternStore, PerspectiveStore, PinStore, ProjectStore,
    PromptTemplateStore, QualitySignalStore, QualityStore, QueryStore, RecipeStore, ReportStore,
    ScheduledTaskStore, StaleConceptProposalStore, ToolApprovalStore, UserStore,
    VerificationStore, WorkspaceStore,
};

// ---------------------------------------------------------------------------
// Per-request workspace context via task-local
// ---------------------------------------------------------------------------
// The workspace middleware sets WORKSPACE_ID on the tokio task.
// PgPool's `before_acquire` callback reads it and runs
//   SET app.workspace_id = '...'
// on the connection before handing it out.
// `after_release` runs RESET ALL to prevent cross-request leakage.
//
// This means ALL existing store queries are automatically workspace-scoped
// through PostgreSQL RLS — zero trait or query changes needed.
// ---------------------------------------------------------------------------

tokio::task_local! {
    /// Per-request workspace ID. Set by the workspace middleware.
    /// Used by PgPool's `before_acquire` to configure RLS session variable.
    pub static WORKSPACE_ID: Uuid;

    /// When true, `before_acquire` sets `app.system_bypass` instead of
    /// `app.workspace_id`. Used by scheduled tasks, cleanup, and migrations
    /// that need cross-workspace access.
    pub static SYSTEM_BYPASS: bool;
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
                        // Also set workspace_id to the default workspace so that
                        // INSERT DEFAULT values resolve correctly.
                        sqlx::query("SELECT set_config('app.system_bypass', 'true', false)")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query(
                            "SELECT set_config('app.workspace_id', id::text, false) \
                             FROM workspaces WHERE slug = 'default' LIMIT 1",
                        )
                        .execute(&mut *conn)
                        .await?;
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
    /// Migrations run outside workspace context (RESET ALL clears state after).
    pub async fn migrate(&self) -> OxResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Migration failed: {e}"),
            })?;

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
// Cursor-pagination helper
// ---------------------------------------------------------------------------

/// Build a CursorPage from a fetched Vec (fetched with limit+1).
/// Uses compound cursor "timestamp|uuid" to guarantee no row is skipped
/// even when multiple rows share the same timestamp.
fn build_cursor_page<T, F>(mut rows: Vec<T>, limit: i64, cursor_extractor: F) -> CursorPage<T>
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

// ---------------------------------------------------------------------------
// QueryStore
// ---------------------------------------------------------------------------

#[async_trait]
impl QueryStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_query_execution(&self, exec: &QueryExecution) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO query_executions
             (id, user_id, question, ontology_lineage_id, ontology_version,
              ontology_id, ontology_snapshot,
              query_ir, compiled_target, compiled_query,
              results, widget, explanation, model, execution_time_ms,
              query_bindings, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
        )
        .bind(exec.id)
        .bind(&exec.user_id)
        .bind(&exec.question)
        .bind(&exec.ontology_lineage_id)
        .bind(exec.ontology_version)
        .bind(exec.ontology_id)
        .bind(&exec.ontology_snapshot)
        .bind(&exec.query_ir)
        .bind(&exec.compiled_target)
        .bind(&exec.compiled_query)
        .bind(&exec.results)
        .bind(&exec.widget)
        .bind(&exec.explanation)
        .bind(&exec.model)
        .bind(exec.execution_time_ms)
        .bind(&exec.query_bindings)
        .bind(exec.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_query_execution(
        &self,
        user_id: &str,
        id: Uuid,
    ) -> OxResult<Option<QueryExecution>> {
        // Returns the raw row — no JOIN to hydrate `ontology_snapshot`.
        // Under the Λ storage model, committed ontologies live in a
        // content-addressed graph spanning four tables; a LEFT JOIN
        // trick no longer substitutes for `load_version`. Callers that
        // need the IR follow up with
        // `OntologyVersionStore::resolve_version_at(ontology_id, created_at)`.
        sqlx::query_as::<_, QueryExecution>(
            "SELECT id, user_id, question, ontology_lineage_id, ontology_version,
                    ontology_id, ontology_snapshot,
                    query_ir, compiled_target, compiled_query,
                    results, widget, explanation, model,
                    execution_time_ms, query_bindings, created_at
             FROM query_executions
             WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_query_executions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<QueryExecutionSummary>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let query = "SELECT id, question, ontology_lineage_id, ontology_version,
                            compiled_target, model, execution_time_ms,
                            jsonb_array_length(COALESCE(results->'rows', '[]'::jsonb))::bigint AS row_count,
                            widget IS NOT NULL AS has_widget,
                            created_at
                     FROM query_executions
                     WHERE user_id = $1";

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, QueryExecutionSummary>(&format!(
                "{query} AND (created_at, id) < ($2, $3) ORDER BY created_at DESC, id DESC LIMIT $4"
            ))
            .bind(user_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, QueryExecutionSummary>(&format!(
                "{query} ORDER BY created_at DESC, id DESC LIMIT $2"
            ))
            .bind(user_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |e| (e.created_at, e.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_query_feedback(
        &self,
        user_id: &str,
        id: Uuid,
        feedback: Option<&str>,
    ) -> OxResult<bool> {
        let result =
            sqlx::query("UPDATE query_executions SET feedback = $1 WHERE id = $2 AND user_id = $3")
                .bind(feedback)
                .bind(id)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// PinStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PinStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_pin(&self, user_id: &str, item: &PinboardItem) -> OxResult<()> {
        // Verify ownership: query_execution must belong to the principal
        let result = sqlx::query(
            "INSERT INTO pinboard_items (id, query_execution_id, user_id, widget_spec, title, pinned_at)
             SELECT $1, $2, $6, $3, $4, $5
             FROM query_executions
             WHERE id = $2 AND user_id = $6",
        )
        .bind(item.id)
        .bind(item.query_execution_id)
        .bind(&item.widget_spec)
        .bind(&item.title)
        .bind(item.pinned_at)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: "QueryExecution".to_string(),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pins(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<PinboardItem>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, PinboardItem>(
                "SELECT *
                 FROM pinboard_items
                 WHERE user_id = $1
                   AND (pinned_at, id) < ($2, $3)
                 ORDER BY pinned_at DESC, id DESC
                 LIMIT $4",
            )
            .bind(user_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, PinboardItem>(
                "SELECT *
                 FROM pinboard_items
                 WHERE user_id = $1
                 ORDER BY pinned_at DESC, id DESC
                 LIMIT $2",
            )
            .bind(user_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |p| (p.pinned_at, p.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_pin(&self, user_id: &str, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query(
            "DELETE FROM pinboard_items
             WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// ProjectStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ProjectStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_design_project(&self, project: &DesignProject) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO design_projects
             (id, user_id, status, revision, title, source_config, source_data,
              source_schema, source_profile, analysis_report, design_options,
              source_mapping, ontology, quality_report, source_history, analyzed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        )
        .bind(project.id)
        .bind(&project.user_id)
        .bind(&project.status)
        .bind(project.revision)
        .bind(&project.title)
        .bind(&project.source_config)
        .bind(&project.source_data)
        .bind(&project.source_schema)
        .bind(&project.source_profile)
        .bind(&project.analysis_report)
        .bind(&project.design_options)
        .bind(&project.source_mapping)
        .bind(&project.ontology)
        .bind(&project.quality_report)
        .bind(&project.source_history)
        .bind(project.analyzed_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_design_project(&self, id: Uuid) -> OxResult<Option<DesignProject>> {
        sqlx::query_as::<_, DesignProject>("SELECT * FROM design_projects WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_design_projects(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<DesignProjectSummary>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, DesignProjectSummary>(
                "SELECT id, status, revision, user_id, title, source_config, ontology_id,
                        created_at, updated_at, analyzed_at
                 FROM design_projects
                 WHERE archived_at IS NULL AND (updated_at, id) < ($1, $2)
                 ORDER BY updated_at DESC, id DESC
                 LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, DesignProjectSummary>(
                "SELECT id, status, revision, user_id, title, source_config, ontology_id,
                        created_at, updated_at, analyzed_at
                 FROM design_projects
                 WHERE archived_at IS NULL
                 ORDER BY updated_at DESC, id DESC
                 LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |p| (p.updated_at, p.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_design_options(
        &self,
        id: Uuid,
        options: &serde_json::Value,
        expected_revision: i32,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE design_projects
             SET design_options = $1, updated_at = NOW(),
                 revision = revision + 1
             WHERE id = $2 AND revision = $3 ",
        )
        .bind(options)
        .bind(id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        check_cas_result(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_design_result(
        &self,
        id: Uuid,
        ontology: &serde_json::Value,
        source_mapping: Option<&serde_json::Value>,
        quality_report: Option<&serde_json::Value>,
        expected_revision: i32,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE design_projects
             SET ontology = $1, source_mapping = $2, quality_report = $3, status = 'designed',
                 updated_at = NOW(), revision = revision + 1
             WHERE id = $4 AND revision = $5 ",
        )
        .bind(ontology)
        .bind(source_mapping)
        .bind(quality_report)
        .bind(id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        check_cas_result(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_extend_result(
        &self,
        id: Uuid,
        result: &ExtendResult,
        expected_revision: i32,
    ) -> OxResult<()> {
        let rows = sqlx::query(
            "UPDATE design_projects
             SET ontology = $1, source_mapping = $2, quality_report = $3,
                 source_schema = $4, source_profile = $5,
                 source_history = $6,
                 status = 'designed', updated_at = NOW(), revision = revision + 1
             WHERE id = $7 AND revision = $8 ",
        )
        .bind(&result.ontology)
        .bind(&result.source_mapping)
        .bind(&result.quality_report)
        .bind(&result.source_schema)
        .bind(&result.source_profile)
        .bind(&result.source_history)
        .bind(id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        check_cas_result(rows.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn replace_analysis_snapshot(
        &self,
        id: Uuid,
        snapshot: &AnalysisSnapshot,
        expected_revision: i32,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE design_projects
             SET source_config = $1, source_data = $2,
                 source_schema = $3, source_profile = $4, analysis_report = $5,
                 design_options = $6, source_mapping = NULL, ontology = NULL, quality_report = NULL,
                 status = 'analyzed', analyzed_at = NOW(),
                 updated_at = NOW(), revision = revision + 1
             WHERE id = $7 AND revision = $8 ",
        )
        .bind(&snapshot.source_config)
        .bind(&snapshot.source_data)
        .bind(&snapshot.source_schema)
        .bind(&snapshot.source_profile)
        .bind(&snapshot.analysis_report)
        .bind(&snapshot.design_options)
        .bind(id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        check_cas_result(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn complete_design_project(
        &self,
        project_id: Uuid,
        ontology_id: Uuid,
        expected_revision: i32,
    ) -> OxResult<()> {
        // The caller has already committed a new version through
        // OntologyVersionStore; this path only links the project row.
        // Single-statement path — no transaction needed now that the
        // saved_ontologies INSERT is gone.
        let result = sqlx::query(
            "UPDATE design_projects
             SET status = 'completed', ontology_id = $1,
                 updated_at = NOW(), revision = revision + 1
             WHERE id = $2 AND revision = $3 AND status = 'designed'",
        )
        .bind(ontology_id)
        .bind(project_id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        check_cas_result(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_design_project(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM design_projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn archive_stale_projects(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        // RETURNING the workspace_id of each affected row, then GROUP
        // BY in SQL — keeps the per-workspace breakdown server-side
        // instead of round-tripping every row to Rust.
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 UPDATE design_projects
                 SET archived_at = NOW()
                 WHERE status IN ('analyzed', 'designed')
                   AND updated_at < NOW() - ($1 || ' days')::interval
                   AND archived_at IS NULL
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .bind(max_age_days)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_archived_projects(&self, max_archive_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 DELETE FROM design_projects
                 WHERE archived_at IS NOT NULL
                   AND archived_at < NOW() - ($1 || ' days')::interval
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .bind(max_archive_days)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }

    // --- Ontology Snapshots ---

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_ontology_snapshot(
        &self,
        project_id: Uuid,
        revision: i32,
        ontology: &serde_json::Value,
        source_mapping: Option<&serde_json::Value>,
        quality_report: Option<&serde_json::Value>,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO ontology_snapshots (project_id, revision, ontology, source_mapping, quality_report)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (project_id, revision) DO NOTHING",
        )
        .bind(project_id)
        .bind(revision)
        .bind(ontology)
        .bind(source_mapping)
        .bind(quality_report)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_ontology_snapshots(
        &self,
        project_id: Uuid,
    ) -> OxResult<Vec<OntologySnapshotSummary>> {
        let rows = sqlx::query_as::<_, (Uuid, i32, DateTime<Utc>, Option<i64>, Option<i64>)>(
            "SELECT id, revision, created_at,
                    jsonb_array_length(ontology->'node_types') AS node_count,
                    jsonb_array_length(ontology->'edge_types') AS edge_count
             FROM ontology_snapshots
             WHERE project_id = $1
             ORDER BY revision DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, revision, created_at, node_count, edge_count)| OntologySnapshotSummary {
                    id,
                    revision,
                    created_at,
                    node_count: node_count.unwrap_or(0),
                    edge_count: edge_count.unwrap_or(0),
                },
            )
            .collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_ontology_snapshot(
        &self,
        project_id: Uuid,
        revision: i32,
    ) -> OxResult<Option<OntologySnapshot>> {
        sqlx::query_as::<_, OntologySnapshot>(
            "SELECT * FROM ontology_snapshots
             WHERE project_id = $1 AND revision = $2",
        )
        .bind(project_id)
        .bind(revision)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// PerspectiveStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PerspectiveStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_perspective(&self, p: &WorkbenchPerspective) -> OxResult<()> {
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // When saving a default perspective, clear any existing defaults for this ontology
        if p.is_default {
            sqlx::query(
                "UPDATE workbench_perspectives SET is_default = false
                 WHERE user_id = $1 AND lineage_id = $2 AND is_default = true AND id != $3",
            )
            .bind(&p.user_id)
            .bind(&p.lineage_id)
            .bind(p.id)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }

        sqlx::query(
            "INSERT INTO workbench_perspectives
             (id, user_id, lineage_id, topology_signature, project_id,
              name, positions, viewport, filters, collapsed_groups,
              is_default, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (user_id, lineage_id, name)
             DO UPDATE SET
                topology_signature = EXCLUDED.topology_signature,
                project_id = EXCLUDED.project_id,
                positions = EXCLUDED.positions,
                viewport = EXCLUDED.viewport,
                filters = EXCLUDED.filters,
                collapsed_groups = EXCLUDED.collapsed_groups,
                is_default = EXCLUDED.is_default,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(p.id)
        .bind(&p.user_id)
        .bind(&p.lineage_id)
        .bind(&p.topology_signature)
        .bind(p.project_id)
        .bind(&p.name)
        .bind(&p.positions)
        .bind(&p.viewport)
        .bind(&p.filters)
        .bind(&p.collapsed_groups)
        .bind(p.is_default)
        .bind(p.created_at)
        .bind(p.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        name: &str,
    ) -> OxResult<Option<WorkbenchPerspective>> {
        sqlx::query_as::<_, WorkbenchPerspective>(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND lineage_id = $2 AND name = $3",
        )
        .bind(user_id)
        .bind(lineage_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_default_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Option<WorkbenchPerspective>> {
        sqlx::query_as::<_, WorkbenchPerspective>(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND lineage_id = $2 AND is_default = true
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
        )
        .bind(user_id)
        .bind(lineage_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_best_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        topology_signature: &str,
    ) -> OxResult<Option<WorkbenchPerspective>> {
        // Tier 1: exact lineage match (same ontology lineage)
        let exact = sqlx::query_as::<_, WorkbenchPerspective>(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND lineage_id = $2 AND is_default = true
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
        )
        .bind(user_id)
        .bind(lineage_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if exact.is_some() {
            return Ok(exact);
        }

        // Tier 2: topology match (different lineage but same structural shape)
        let topology_match = sqlx::query_as::<_, WorkbenchPerspective>(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND topology_signature = $2 AND is_default = true
             ORDER BY updated_at DESC, id DESC
             LIMIT 1",
        )
        .bind(user_id)
        .bind(topology_signature)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(topology_match)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_perspectives(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Vec<WorkbenchPerspective>> {
        sqlx::query_as::<_, WorkbenchPerspective>(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND lineage_id = $2
             ORDER BY is_default DESC, updated_at DESC",
        )
        .bind(user_id)
        .bind(lineage_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_perspective(&self, user_id: &str, id: Uuid) -> OxResult<bool> {
        let result =
            sqlx::query("DELETE FROM workbench_perspectives WHERE id = $1 AND user_id = $2")
                .bind(id)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// ConfigStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ConfigStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_all_config(&self) -> OxResult<Vec<SystemConfigRow>> {
        sqlx::query_as::<_, SystemConfigRow>(
            "SELECT category, key, value, data_type, description, updated_at
             FROM system_config
             ORDER BY category, key",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_config(&self, key: &str) -> OxResult<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_config(&self, category: &str, key: &str, value: &str) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE system_config SET value = $3, updated_at = NOW()
             WHERE category = $1 AND key = $2",
        )
        .bind(category)
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("Config key {category}.{key}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_config_batch(&self, updates: &[(String, String, String)]) -> OxResult<()> {
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        for (category, key, value) in updates {
            let result = sqlx::query(
                "UPDATE system_config SET value = $3, updated_at = NOW()
                 WHERE category = $1 AND key = $2",
            )
            .bind(category)
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;

            if result.rows_affected() == 0 {
                return Err(OxError::NotFound {
                    entity: format!("Config key {category}.{key}"),
                });
            }
        }

        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// UserStore
// ---------------------------------------------------------------------------

#[async_trait]
impl UserStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_user(&self, user: &User) -> OxResult<User> {
        sqlx::query_as::<_, User>(
            "INSERT INTO users (id, email, name, picture, provider, provider_sub, role, created_at, last_login_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (provider, provider_sub)
             DO UPDATE SET
                email = EXCLUDED.email,
                name = EXCLUDED.name,
                picture = EXCLUDED.picture,
                last_login_at = EXCLUDED.last_login_at
             RETURNING *",
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.name)
        .bind(&user.picture)
        .bind(&user.provider)
        .bind(&user.provider_sub)
        .bind(&user.role)
        .bind(user.created_at)
        .bind(user.last_login_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_by_id(&self, id: Uuid) -> OxResult<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_by_provider(
        &self,
        provider: &str,
        provider_sub: &str,
    ) -> OxResult<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE provider = $1 AND provider_sub = $2")
            .bind(provider)
            .bind(provider_sub)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_users(&self, pagination: &CursorParams) -> OxResult<CursorPage<User>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, User>(
                "SELECT * FROM users
                     WHERE (created_at, id) < ($1, $2)
                     ORDER BY created_at DESC, id DESC
                     LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, User>(
                "SELECT * FROM users
                     ORDER BY created_at DESC, id DESC
                     LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |u| (u.created_at, u.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_user_role(&self, id: Uuid, role: &str) -> OxResult<()> {
        let result = sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: "User".to_string(),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_count(&self) -> OxResult<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// RecipeStore
// ---------------------------------------------------------------------------

#[async_trait]
impl RecipeStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_recipe(&self, r: &AnalysisRecipe) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO analysis_recipes
             (id, name, description, algorithm_type, code_template, parameters,
              required_columns, output_description, created_by, created_at,
              version, status, parent_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                code_template = EXCLUDED.code_template,
                parameters = EXCLUDED.parameters,
                required_columns = EXCLUDED.required_columns,
                output_description = EXCLUDED.output_description,
                version = EXCLUDED.version,
                status = EXCLUDED.status,
                parent_id = EXCLUDED.parent_id",
        )
        .bind(r.id)
        .bind(&r.name)
        .bind(&r.description)
        .bind(&r.algorithm_type)
        .bind(&r.code_template)
        .bind(&r.parameters)
        .bind(&r.required_columns)
        .bind(&r.output_description)
        .bind(&r.created_by)
        .bind(r.created_at)
        .bind(r.version)
        .bind(&r.status)
        .bind(r.parent_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_recipe(&self, id: Uuid) -> OxResult<Option<AnalysisRecipe>> {
        sqlx::query_as::<_, AnalysisRecipe>("SELECT * FROM analysis_recipes WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_recipes(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AnalysisRecipe>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, AnalysisRecipe>(
                "SELECT * FROM analysis_recipes
                     WHERE (created_at, id) < ($1, $2)
                     ORDER BY created_at DESC, id DESC
                     LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, AnalysisRecipe>(
                "SELECT * FROM analysis_recipes
                     ORDER BY created_at DESC, id DESC
                     LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |r| (r.created_at, r.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_recipe(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM analysis_recipes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_recipe_status(&self, id: Uuid, status: &str) -> OxResult<()> {
        sqlx::query("UPDATE analysis_recipes SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_recipe_version(&self, recipe: &AnalysisRecipe) -> OxResult<()> {
        self.upsert_recipe(recipe).await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_recipe_versions(&self, parent_id: Uuid) -> OxResult<Vec<AnalysisRecipe>> {
        sqlx::query_as::<_, AnalysisRecipe>(
            "SELECT * FROM analysis_recipes
             WHERE parent_id = $1 OR id = $1
             ORDER BY version DESC",
        )
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_recipes_batch(&self, recipes: &[AnalysisRecipe]) -> OxResult<()> {
        if recipes.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;
        for r in recipes {
            sqlx::query(
                "INSERT INTO analysis_recipes
                 (id, name, description, algorithm_type, code_template, parameters,
                  required_columns, output_description, created_by, created_at,
                  version, status, parent_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    description = EXCLUDED.description,
                    code_template = EXCLUDED.code_template,
                    parameters = EXCLUDED.parameters,
                    required_columns = EXCLUDED.required_columns,
                    output_description = EXCLUDED.output_description,
                    version = EXCLUDED.version,
                    status = EXCLUDED.status,
                    parent_id = EXCLUDED.parent_id",
            )
            .bind(r.id)
            .bind(&r.name)
            .bind(&r.description)
            .bind(&r.algorithm_type)
            .bind(&r.code_template)
            .bind(&r.parameters)
            .bind(&r.required_columns)
            .bind(&r.output_description)
            .bind(&r.created_by)
            .bind(r.created_at)
            .bind(r.version)
            .bind(&r.status)
            .bind(r.parent_id)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }
        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DashboardStore
// ---------------------------------------------------------------------------

#[async_trait]
impl DashboardStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_dashboard(&self, d: &Dashboard) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO dashboards (id, workspace_id, user_id, name, description, layout, is_public, share_token, shared_at, share_expires_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(d.id)
        .bind(d.workspace_id)
        .bind(&d.user_id)
        .bind(&d.name)
        .bind(&d.description)
        .bind(&d.layout)
        .bind(d.is_public)
        .bind(&d.share_token)
        .bind(d.shared_at)
        .bind(d.share_expires_at)
        .bind(d.created_at)
        .bind(d.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_dashboard(&self, id: Uuid) -> OxResult<Option<Dashboard>> {
        sqlx::query_as::<_, Dashboard>("SELECT * FROM dashboards WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_dashboards(
        &self,
        user_id: &str,
        is_admin: bool,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<Dashboard>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = if is_admin {
            // Admin sees all dashboards
            match pagination.cursor_parts() {
                Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, Dashboard>(
                    "SELECT * FROM dashboards
                         WHERE (updated_at, id) < ($1, $2)
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $3",
                )
                .bind(cursor_ts)
                .bind(cursor_id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
                None => sqlx::query_as::<_, Dashboard>(
                    "SELECT * FROM dashboards
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $1",
                )
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
            }
        } else {
            // Non-admin: own dashboards + public dashboards
            match pagination.cursor_parts() {
                Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, Dashboard>(
                    "SELECT * FROM dashboards
                         WHERE (user_id = $1 OR is_public = true) AND (updated_at, id) < ($2, $3)
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $4",
                )
                .bind(user_id)
                .bind(cursor_ts)
                .bind(cursor_id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
                None => sqlx::query_as::<_, Dashboard>(
                    "SELECT * FROM dashboards
                         WHERE user_id = $1 OR is_public = true
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $2",
                )
                .bind(user_id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
            }
        };

        Ok(build_cursor_page(rows, limit, |d| (d.updated_at, d.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_dashboard(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        layout: &serde_json::Value,
        is_public: bool,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE dashboards SET name = $1, description = $2, layout = $3, is_public = $4, updated_at = NOW() WHERE id = $5",
        )
        .bind(name)
        .bind(description)
        .bind(layout)
        .bind(is_public)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_dashboard(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM dashboards WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_dashboard_share_token(
        &self,
        id: Uuid,
        token: Option<&str>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> OxResult<()> {
        if let Some(token) = token {
            sqlx::query(
                "UPDATE dashboards
                 SET share_token = $1, shared_at = NOW(),
                     share_expires_at = $2, updated_at = NOW()
                 WHERE id = $3",
            )
            .bind(token)
            .bind(expires_at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        } else {
            sqlx::query(
                "UPDATE dashboards
                 SET share_token = NULL, shared_at = NULL,
                     share_expires_at = NULL, updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_dashboard_by_share_token(&self, token: &str) -> OxResult<Option<Dashboard>> {
        // Returns the row even if `share_expires_at` is in the past so the
        // caller can render a 410 Gone instead of a generic 404. The route
        // is responsible for the expiry check.
        sqlx::query_as::<_, Dashboard>("SELECT * FROM dashboards WHERE share_token = $1")
            .bind(token)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_widget(&self, w: &DashboardWidget) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO dashboard_widgets
             (id, dashboard_id, title, widget_type, query, widget_spec, position,
              refresh_interval_secs, thresholds, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(w.id)
        .bind(w.dashboard_id)
        .bind(&w.title)
        .bind(&w.widget_type)
        .bind(&w.query)
        .bind(&w.widget_spec)
        .bind(&w.position)
        .bind(w.refresh_interval_secs)
        .bind(&w.thresholds)
        .bind(w.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_widgets(&self, dashboard_id: Uuid) -> OxResult<Vec<DashboardWidget>> {
        sqlx::query_as::<_, DashboardWidget>(
            "SELECT * FROM dashboard_widgets WHERE dashboard_id = $1 ORDER BY created_at",
        )
        .bind(dashboard_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_widget(
        &self,
        id: Uuid,
        title: Option<&str>,
        widget_type: Option<&str>,
        query: Option<&str>,
        refresh_interval_secs: Option<i32>,
        thresholds: Option<&serde_json::Value>,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE dashboard_widgets SET
               title = COALESCE($1, title),
               widget_type = COALESCE($2, widget_type),
               query = COALESCE($3, query),
               refresh_interval_secs = COALESCE($4, refresh_interval_secs),
               thresholds = COALESCE($5, thresholds)
             WHERE id = $6",
        )
        .bind(title)
        .bind(widget_type)
        .bind(query)
        .bind(refresh_interval_secs)
        .bind(thresholds)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_widget_result(&self, id: Uuid, result: &serde_json::Value) -> OxResult<()> {
        sqlx::query(
            "UPDATE dashboard_widgets SET last_result = $1, last_refreshed = NOW() WHERE id = $2",
        )
        .bind(result)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_widget(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM dashboard_widgets WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_widgets_batch(&self, widgets: &[DashboardWidget]) -> OxResult<()> {
        if widgets.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;
        for w in widgets {
            sqlx::query(
                "INSERT INTO dashboard_widgets
                 (id, dashboard_id, title, widget_type, query, widget_spec, position,
                  refresh_interval_secs, thresholds, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(w.id)
            .bind(w.dashboard_id)
            .bind(&w.title)
            .bind(&w.widget_type)
            .bind(&w.query)
            .bind(&w.widget_spec)
            .bind(&w.position)
            .bind(w.refresh_interval_secs)
            .bind(&w.thresholds)
            .bind(w.created_at)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }
        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ReportStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ReportStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_report(&self, r: &SavedReport) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO saved_reports
             (id, user_id, ontology_lineage_id, title, description, query_template,
              parameters, widget_type, is_public, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(r.id)
        .bind(&r.user_id)
        .bind(&r.ontology_lineage_id)
        .bind(&r.title)
        .bind(&r.description)
        .bind(&r.query_template)
        .bind(&r.parameters)
        .bind(&r.widget_type)
        .bind(r.is_public)
        .bind(r.created_at)
        .bind(r.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_report(&self, id: Uuid) -> OxResult<Option<SavedReport>> {
        sqlx::query_as::<_, SavedReport>("SELECT * FROM saved_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_reports(
        &self,
        user_id: &str,
        ontology_lineage_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedReport>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, SavedReport>(
                "SELECT * FROM saved_reports
                     WHERE (user_id = $1 OR is_public = true)
                       AND ontology_lineage_id = $2
                       AND (updated_at, id) < ($3, $4)
                     ORDER BY updated_at DESC, id DESC
                     LIMIT $5",
            )
            .bind(user_id)
            .bind(ontology_lineage_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, SavedReport>(
                "SELECT * FROM saved_reports
                     WHERE (user_id = $1 OR is_public = true)
                       AND ontology_lineage_id = $2
                     ORDER BY updated_at DESC, id DESC
                     LIMIT $3",
            )
            .bind(user_id)
            .bind(ontology_lineage_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |r| (r.updated_at, r.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_report(
        &self,
        id: Uuid,
        title: &str,
        description: Option<&str>,
        query_template: &str,
        parameters: &serde_json::Value,
        widget_type: Option<&str>,
        is_public: bool,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE saved_reports
             SET title = $1, description = $2, query_template = $3,
                 parameters = $4, widget_type = $5, is_public = $6,
                 updated_at = NOW()
             WHERE id = $7",
        )
        .bind(title)
        .bind(description)
        .bind(query_template)
        .bind(parameters)
        .bind(widget_type)
        .bind(is_public)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_report(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM saved_reports WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// PatternStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PatternStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_pattern(&self, p: &SavedQueryPattern) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO saved_query_patterns
             (id, user_id, ontology_lineage_id, name, description, pattern_ir,
              created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(p.id)
        .bind(&p.user_id)
        .bind(&p.ontology_lineage_id)
        .bind(&p.name)
        .bind(&p.description)
        .bind(&p.pattern_ir)
        .bind(p.created_at)
        .bind(p.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_pattern(&self, id: Uuid) -> OxResult<Option<SavedQueryPattern>> {
        sqlx::query_as::<_, SavedQueryPattern>("SELECT * FROM saved_query_patterns WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_patterns(
        &self,
        user_id: &str,
        ontology_lineage_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedQueryPattern>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, SavedQueryPattern>(
                "SELECT * FROM saved_query_patterns
                     WHERE user_id = $1
                       AND ontology_lineage_id = $2
                       AND (updated_at, id) < ($3, $4)
                     ORDER BY updated_at DESC, id DESC
                     LIMIT $5",
            )
            .bind(user_id)
            .bind(ontology_lineage_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, SavedQueryPattern>(
                "SELECT * FROM saved_query_patterns
                     WHERE user_id = $1
                       AND ontology_lineage_id = $2
                     ORDER BY updated_at DESC, id DESC
                     LIMIT $3",
            )
            .bind(user_id)
            .bind(ontology_lineage_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |r| (r.updated_at, r.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_pattern(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        pattern_ir: &serde_json::Value,
    ) -> OxResult<bool> {
        let result = sqlx::query(
            "UPDATE saved_query_patterns
             SET name = $1, description = $2, pattern_ir = $3, updated_at = NOW()
             WHERE id = $4",
        )
        .bind(name)
        .bind(description)
        .bind(pattern_ir)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_pattern(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM saved_query_patterns WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// AnalysisResultStore
// ---------------------------------------------------------------------------

#[async_trait]
impl AnalysisResultStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_analysis_result(&self, r: &AnalysisResult) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO analysis_results (id, recipe_id, ontology_lineage_id, input_hash, output, duration_ms, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(r.id)
        .bind(r.recipe_id)
        .bind(&r.ontology_lineage_id)
        .bind(&r.input_hash)
        .bind(&r.output)
        .bind(r.duration_ms)
        .bind(r.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_cached_result(
        &self,
        input_hash: &str,
        recipe_id: Option<Uuid>,
    ) -> OxResult<Option<AnalysisResult>> {
        let result = if let Some(rid) = recipe_id {
            sqlx::query_as(
                "SELECT * FROM analysis_results WHERE input_hash = $1 AND recipe_id = $2
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(input_hash)
            .bind(rid)
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT * FROM analysis_results WHERE input_hash = $1 AND recipe_id IS NULL
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(input_hash)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(result)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_analysis_results(
        &self,
        recipe_id: Uuid,
        limit: i64,
    ) -> OxResult<Vec<AnalysisResult>> {
        sqlx::query_as(
            "SELECT * FROM analysis_results WHERE recipe_id = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(recipe_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn cleanup_old_results(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 DELETE FROM analysis_results
                 WHERE created_at < NOW() - make_interval(days => $1)
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .bind(max_age_days as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }
}

// ---------------------------------------------------------------------------
// ScheduledTaskStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ScheduledTaskStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_scheduled_task(&self, t: &ScheduledTask) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO scheduled_tasks (id, recipe_id, ontology_lineage_id, cron_expression, description,
             enabled, next_run_at, webhook_url, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(t.id)
        .bind(t.recipe_id)
        .bind(&t.ontology_lineage_id)
        .bind(&t.cron_expression)
        .bind(&t.description)
        .bind(t.enabled)
        .bind(t.next_run_at)
        .bind(&t.webhook_url)
        .bind(&t.created_by)
        .bind(t.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_scheduled_task(&self, id: Uuid) -> OxResult<Option<ScheduledTask>> {
        sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_scheduled_tasks(&self, recipe_id: Option<Uuid>) -> OxResult<Vec<ScheduledTask>> {
        match recipe_id {
            Some(rid) => sqlx::query_as::<_, ScheduledTask>(
                "SELECT * FROM scheduled_tasks WHERE recipe_id = $1 ORDER BY created_at DESC",
            )
            .bind(rid)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            None => sqlx::query_as::<_, ScheduledTask>(
                "SELECT * FROM scheduled_tasks ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_due_tasks(&self) -> OxResult<Vec<ScheduledTask>> {
        sqlx::query_as::<_, ScheduledTask>(
            "SELECT * FROM scheduled_tasks WHERE enabled = true AND next_run_at <= NOW()",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_task_after_run(
        &self,
        id: Uuid,
        next_run_at: DateTime<Utc>,
        status: &str,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE scheduled_tasks SET last_run_at = NOW(), next_run_at = $2, last_status = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(next_run_at)
        .bind(status)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_scheduled_task_enabled(&self, id: Uuid, enabled: bool) -> OxResult<()> {
        sqlx::query("UPDATE scheduled_tasks SET enabled = $2 WHERE id = $1")
            .bind(id)
            .bind(enabled)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_scheduled_task(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// HealthStore
// ---------------------------------------------------------------------------

#[async_trait]
impl HealthStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.pool).await.is_ok()
    }
}

// ---------------------------------------------------------------------------
// NotificationStore
// ---------------------------------------------------------------------------

#[async_trait]
impl crate::store::NotificationStore for PostgresStore {
    async fn create_notification_channel(&self, ch: &NotificationChannel) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO notification_channels (id, workspace_id, name, channel_type, config, events, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(ch.id)
        .bind(ch.workspace_id)
        .bind(&ch.name)
        .bind(&ch.channel_type)
        .bind(&ch.config)
        .bind(&ch.events)
        .bind(ch.enabled)
        .bind(ch.created_at)
        .bind(ch.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn get_notification_channel(&self, id: Uuid) -> OxResult<Option<NotificationChannel>> {
        sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notification_channels WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn list_notification_channels(&self) -> OxResult<Vec<NotificationChannel>> {
        sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notification_channels ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_notification_channel(
        &self,
        id: Uuid,
        name: Option<&str>,
        config: Option<&serde_json::Value>,
        events: Option<&[String]>,
        enabled: Option<bool>,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE notification_channels SET
                name = COALESCE($1, name),
                config = COALESCE($2, config),
                events = COALESCE($3, events),
                enabled = COALESCE($4, enabled),
                updated_at = NOW()
             WHERE id = $5",
        )
        .bind(name)
        .bind(config)
        .bind(events)
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn delete_notification_channel(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM notification_channels WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_channels_for_event(
        &self,
        event_type: &str,
    ) -> OxResult<Vec<NotificationChannel>> {
        sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notification_channels WHERE enabled = true AND $1 = ANY(events)",
        )
        .bind(event_type)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn create_notification_log(&self, log: &NotificationLog) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO notification_log (id, workspace_id, channel_id, event_type, subject, body, status, error, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(log.id)
        .bind(log.workspace_id)
        .bind(log.channel_id)
        .bind(&log.event_type)
        .bind(&log.subject)
        .bind(&log.body)
        .bind(&log.status)
        .bind(&log.error)
        .bind(log.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn list_notification_logs(&self, limit: i64) -> OxResult<Vec<NotificationLog>> {
        sqlx::query_as::<_, NotificationLog>(
            "SELECT * FROM notification_log ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// PostgreSQL error code mapping
// ---------------------------------------------------------------------------

fn check_cas_result(rows_affected: u64) -> OxResult<()> {
    if rows_affected == 0 {
        Err(OxError::Conflict {
            message: "Project was modified by another session (revision mismatch) or is in an invalid state for this operation".to_string(),
        })
    } else {
        Ok(())
    }
}

fn to_ox_error(e: sqlx::Error) -> OxError {
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
// AgentSessionStore
// ---------------------------------------------------------------------------

#[async_trait]
impl AgentSessionStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_agent_session(&self, s: &AgentSession) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO agent_sessions (id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
             model_id, model_config, user_message, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(s.id)
        .bind(&s.user_id)
        .bind(&s.ontology_lineage_id)
        .bind(&s.prompt_hash)
        .bind(&s.tool_schema_hash)
        .bind(&s.model_id)
        .bind(&s.model_config)
        .bind(&s.user_message)
        .bind(s.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn complete_agent_session(&self, id: Uuid, final_text: Option<&str>) -> OxResult<()> {
        sqlx::query(
            "UPDATE agent_sessions SET final_text = $2, completed_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(final_text)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_agent_session(&self, id: Uuid) -> OxResult<Option<AgentSession>> {
        sqlx::query_as(
            "SELECT id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
                    model_id, model_config, user_message, final_text,
                    created_at, completed_at
             FROM agent_sessions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_agent_sessions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AgentSession>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let items: Vec<AgentSession> = match &pagination.cursor {
            Some(cursor) => {
                let cursor_time: DateTime<Utc> =
                    cursor.parse().map_err(|e: chrono::format::ParseError| {
                        OxError::Parse {
                            field: "cursor".into(),
                            source: Box::new(e),
                        }
                    })?;
                sqlx::query_as(
                    "SELECT id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
                            model_id, model_config, user_message, final_text,
                            created_at, completed_at
                     FROM agent_sessions WHERE user_id = $1 AND created_at < $2
                     ORDER BY created_at DESC LIMIT $3",
                )
                .bind(user_id)
                .bind(cursor_time)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Database error: {e}"),
                })?
            }
            None => sqlx::query_as(
                "SELECT id, user_id, ontology_lineage_id, prompt_hash, tool_schema_hash,
                            model_id, model_config, user_message, final_text,
                            created_at, completed_at
                     FROM agent_sessions WHERE user_id = $1
                     ORDER BY created_at DESC LIMIT $2",
            )
            .bind(user_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?,
        };

        Ok(build_cursor_page(items, limit, |s| (s.created_at, s.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_agent_event(&self, e: &AgentEvent) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO agent_events (id, session_id, workspace_id, sequence, event_type, payload, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(e.id)
        .bind(e.session_id)
        .bind(e.workspace_id)
        .bind(e.sequence)
        .bind(&e.event_type)
        .bind(&e.payload)
        .bind(e.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_agent_events(&self, session_id: Uuid) -> OxResult<Vec<AgentEvent>> {
        sqlx::query_as("SELECT * FROM agent_events WHERE session_id = $1 ORDER BY sequence")
            .bind(session_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_agent_session(&self, id: Uuid) -> OxResult<bool> {
        // Delete events first (explicit rather than relying on CASCADE)
        sqlx::query("DELETE FROM agent_events WHERE session_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;

        let result = sqlx::query("DELETE FROM agent_sessions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;

        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn cleanup_old_sessions(&self, retention_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        // Delete events first (CASCADE would handle this but be explicit)
        sqlx::query(
            "DELETE FROM agent_events WHERE session_id IN (
                SELECT id FROM agent_sessions WHERE created_at < NOW() - ($1 || ' days')::interval
            )",
        )
        .bind(retention_days)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;

        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 DELETE FROM agent_sessions
                 WHERE created_at < NOW() - ($1 || ' days')::interval
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .bind(retention_days)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;

        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }
}

// ---------------------------------------------------------------------------
// EmbeddingRetryStore
// ---------------------------------------------------------------------------

#[async_trait]
impl EmbeddingRetryStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_pending_embedding(
        &self,
        content: &str,
        metadata: &serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query("INSERT INTO pending_embeddings (content, metadata) VALUES ($1, $2)")
            .bind(content)
            .bind(metadata)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pending_embeddings(&self, limit: i64) -> OxResult<Vec<PendingEmbedding>> {
        sqlx::query_as(
            "SELECT * FROM pending_embeddings WHERE retry_count < 3 ORDER BY created_at LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn mark_embedding_failed(&self, id: Uuid, error: &str) -> OxResult<()> {
        sqlx::query(
            "UPDATE pending_embeddings SET retry_count = retry_count + 1, last_error = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_pending_embedding(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM pending_embeddings WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// PromptTemplateStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PromptTemplateStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_prompt_templates(&self, active_only: bool) -> OxResult<Vec<PromptTemplateRow>> {
        let rows: Vec<PromptTemplateRow> = if active_only {
            sqlx::query_as(
                "SELECT * FROM prompt_templates WHERE is_active = true ORDER BY name, version DESC",
            )
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as("SELECT * FROM prompt_templates ORDER BY name, version DESC")
                .fetch_all(&self.pool)
                .await
        }
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(rows)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_prompt_template(&self, id: Uuid) -> OxResult<Option<PromptTemplateRow>> {
        sqlx::query_as("SELECT * FROM prompt_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_active_prompt(&self, name: &str) -> OxResult<Option<PromptTemplateRow>> {
        // Active global template (workspace_id IS NULL). Sort by parsed
        // semver components (CHECK constraint guarantees `<int>.<int>.<int>`)
        // then `created_at` as the tie-breaker for the rare case of two
        // active rows at the same version.
        sqlx::query_as(
            "SELECT * FROM prompt_templates
             WHERE name = $1 AND is_active = true AND workspace_id IS NULL
             ORDER BY string_to_array(version, '.')::int[] DESC,
                      created_at DESC
             LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_active_prompt_for_workspace(
        &self,
        name: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<PromptTemplateRow>> {
        // Visibility rule:
        //   - workspace_id = Some(ws): see ws-specific override (workspace_id = ws)
        //                              or the global template (workspace_id IS NULL)
        //   - workspace_id = None:     see ONLY the global template
        //
        // This prevents the previous bug where `$2 IS NULL` widened the
        // WHERE clause to match every workspace's overrides indiscriminately.
        //
        // Tie-breaker (when both ws-specific and global match):
        //   1. ws-specific first (`workspace_id IS NULL` = FALSE sorts first)
        //   2. highest semver (CHECK constraint in migration 0006
        //      guarantees `<int>.<int>.<int>` so the array cast is safe)
        //   3. most recently created (deterministic for cosmetic ties)
        sqlx::query_as(
            "SELECT * FROM prompt_templates
             WHERE name = $1
               AND is_active = true
               AND (workspace_id IS NULL
                    OR ($2::uuid IS NOT NULL AND workspace_id = $2))
             ORDER BY (workspace_id IS NULL),
                      string_to_array(version, '.')::int[] DESC,
                      created_at DESC
             LIMIT 1",
        )
        .bind(name)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_prompt_template(&self, r: &PromptTemplateRow) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO prompt_templates (id, name, version, content, variables, metadata, created_by, created_at, is_active, workspace_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (name, version) DO NOTHING",
        )
        .bind(r.id)
        .bind(&r.name)
        // PromptVersion: serialize to its canonical "x.y.z" form for the
        // TEXT column. The CHECK constraint in migration 0006 enforces
        // the same format on the DB side.
        .bind(r.version.to_string())
        .bind(&r.content)
        .bind(&r.variables)
        .bind(&r.metadata)
        .bind(&r.created_by)
        .bind(r.created_at)
        .bind(r.is_active)
        .bind(r.workspace_id)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_prompt_template(
        &self,
        id: Uuid,
        content: &str,
        variables: &serde_json::Value,
        is_active: bool,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE prompt_templates SET content = $2, variables = $3, is_active = $4 WHERE id = $1",
        )
        .bind(id)
        .bind(content)
        .bind(variables)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_prompt_template(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM prompt_templates WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_prompt_template_active_only(
        &self,
        name: &str,
        exclude_id: Uuid,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE prompt_templates SET is_active = false WHERE name = $1 AND id != $2 AND is_active = true",
        )
        .bind(name)
        .bind(exclude_id)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// VerificationStore
// ---------------------------------------------------------------------------

#[async_trait]
impl VerificationStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn verify_element(&self, v: &ElementVerification) -> OxResult<Uuid> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO ontology_verifications
             (ontology_lineage_id, element_id, element_kind, verified_by, review_notes)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (ontology_lineage_id, element_id, verified_by)
                WHERE invalidated_at IS NULL
             DO UPDATE SET review_notes = EXCLUDED.review_notes
             RETURNING id",
        )
        .bind(&v.ontology_lineage_id)
        .bind(&v.element_id)
        .bind(&v.element_kind)
        .bind(v.verified_by)
        .bind(&v.review_notes)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_verifications(
        &self,
        ontology_lineage_id: &str,
    ) -> OxResult<Vec<ElementVerification>> {
        sqlx::query_as(
            "SELECT v.id, v.ontology_lineage_id, v.element_id, v.element_kind,
                    v.verified_by, COALESCE(u.name, u.email) AS verified_by_name,
                    v.review_notes, v.invalidated_at, v.invalidation_reason, v.created_at
             FROM ontology_verifications v
             LEFT JOIN users u ON u.id = v.verified_by
             WHERE v.ontology_lineage_id = $1 AND v.invalidated_at IS NULL
             ORDER BY v.created_at DESC",
        )
        .bind(ontology_lineage_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn invalidate_for_elements(
        &self,
        ontology_lineage_id: &str,
        element_ids: &[&str],
        reason: &str,
    ) -> OxResult<u64> {
        let result = sqlx::query(
            "UPDATE ontology_verifications
             SET invalidated_at = NOW(), invalidation_reason = $3
             WHERE ontology_lineage_id = $1
               AND element_id = ANY($2)
               AND invalidated_at IS NULL",
        )
        .bind(ontology_lineage_id)
        .bind(element_ids)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_verification(
        &self,
        ontology_lineage_id: &str,
        element_id: &str,
        user_id: Uuid,
    ) -> OxResult<bool> {
        let result = sqlx::query(
            "UPDATE ontology_verifications
             SET invalidated_at = NOW(), invalidation_reason = 'manually_revoked'
             WHERE ontology_lineage_id = $1 AND element_id = $2 AND verified_by = $3
               AND invalidated_at IS NULL",
        )
        .bind(ontology_lineage_id)
        .bind(element_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// ToolApprovalStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ToolApprovalStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_tool_approval(&self, a: &ToolApproval) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO tool_approvals
             (session_id, tool_call_id, approved, reason, modified_input, user_id)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (session_id, tool_call_id) DO UPDATE
             SET approved = EXCLUDED.approved,
                 reason = EXCLUDED.reason,
                 modified_input = EXCLUDED.modified_input,
                 user_id = EXCLUDED.user_id",
        )
        .bind(a.session_id)
        .bind(&a.tool_call_id)
        .bind(a.approved)
        .bind(&a.reason)
        .bind(&a.modified_input)
        .bind(&a.user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_tool_approval(
        &self,
        session_id: Uuid,
        tool_call_id: &str,
    ) -> OxResult<Option<ToolApproval>> {
        sqlx::query_as("SELECT * FROM tool_approvals WHERE session_id = $1 AND tool_call_id = $2")
            .bind(session_id)
            .bind(tool_call_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// WorkspaceStore — workspace and membership management
// ---------------------------------------------------------------------------
// These queries are NOT subject to RLS because workspaces/workspace_members
// tables don't have RLS enabled (they're the source of truth for isolation).
// ---------------------------------------------------------------------------

#[async_trait]
impl WorkspaceStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_workspace(&self, w: &Workspace) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, settings, primary_locale, locale_fallback)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(w.id)
        .bind(&w.name)
        .bind(&w.slug)
        .bind(w.owner_id)
        .bind(&w.settings)
        .bind(&w.primary_locale)
        .bind(&w.locale_fallback)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_workspace(&self, id: Uuid) -> OxResult<Option<Workspace>> {
        sqlx::query_as("SELECT * FROM workspaces WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_workspace_by_slug(&self, slug: &str) -> OxResult<Option<Workspace>> {
        sqlx::query_as("SELECT * FROM workspaces WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_user_workspaces(&self, user_id: Uuid) -> OxResult<Vec<WorkspaceSummary>> {
        sqlx::query_as(
            "SELECT w.id, w.name, w.slug, w.owner_id, wm.role, w.created_at,
                    (SELECT COUNT(*) FROM workspace_members wm2 WHERE wm2.workspace_id = w.id) AS member_count
             FROM workspaces w
             JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.user_id = $1
             ORDER BY w.created_at",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_workspace(
        &self,
        id: Uuid,
        name: &str,
        settings: &serde_json::Value,
    ) -> OxResult<()> {
        let result = sqlx::query("UPDATE workspaces SET name = $2, settings = $3 WHERE id = $1")
            .bind(id)
            .bind(name)
            .bind(settings)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("workspace {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_workspace(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_workspace_locale(
        &self,
        id: Uuid,
        primary_locale: &str,
        locale_fallback: &serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE workspaces
                SET primary_locale = $2,
                    locale_fallback = $3
              WHERE id = $1",
        )
        .bind(id)
        .bind(primary_locale)
        .bind(locale_fallback)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn add_workspace_member(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO workspace_members (workspace_id, user_id, role)
             VALUES ($1, $2, $3)
             ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn remove_workspace_member(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<bool> {
        let result =
            sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
                .bind(workspace_id)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_member_role(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("workspace_member {workspace_id}/{user_id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_member_role(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(|r| r.0))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_workspace_members(&self, workspace_id: Uuid) -> OxResult<Vec<WorkspaceMember>> {
        sqlx::query_as(
            "SELECT workspace_id, user_id, role, joined_at
             FROM workspace_members WHERE workspace_id = $1 ORDER BY joined_at",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_default_workspace(&self, user_id: Uuid) -> OxResult<Option<Workspace>> {
        // Prefer the "default" slug workspace, then fall back to the first joined workspace
        sqlx::query_as(
            "SELECT w.*
             FROM workspaces w
             JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.user_id = $1
             ORDER BY (w.slug = 'default') DESC, wm.joined_at
             LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// AuditStore — append-only event log
// ---------------------------------------------------------------------------

#[async_trait]
impl AuditStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_audit(
        &self,
        user_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()> {
        self.record_audit_for_workspace(user_id, None, action, resource_type, resource_id, details)
            .await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_audit_for_workspace(
        &self,
        user_id: Option<Uuid>,
        affected_workspace_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO audit_log (user_id, action, resource_type, resource_id, details, affected_workspace_id)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(user_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&details)
        .bind(affected_workspace_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_audit_events(&self, params: CursorParams) -> OxResult<CursorPage<AuditEntry>> {
        let limit = params.effective_limit();

        let rows: Vec<AuditEntry> = if let Some((cursor_ts, cursor_id)) = params.cursor_parts() {
            sqlx::query_as(
                "SELECT id, user_id, workspace_id, affected_workspace_id, action, resource_type, resource_id, details, created_at
                 FROM audit_log
                 WHERE (created_at, id) < ($1, $2)
                 ORDER BY created_at DESC, id DESC
                 LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        } else {
            sqlx::query_as(
                "SELECT id, user_id, workspace_id, affected_workspace_id, action, resource_type, resource_id, details, created_at
                 FROM audit_log
                 ORDER BY created_at DESC, id DESC
                 LIMIT $1",
            )
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        };

        Ok(build_cursor_page(rows, limit, |entry| {
            (entry.created_at, entry.id)
        }))
    }
}

// ---------------------------------------------------------------------------
// ApiKeyStore — DB-backed API keys (replaces the static `auth.api_key` config)
// ---------------------------------------------------------------------------

#[async_trait]
impl crate::store::ApiKeyStore for PostgresStore {
    async fn create_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        role: &str,
    ) -> OxResult<(crate::models::ApiKey, String)> {
        // 256 bits of CSPRNG entropy. Plaintext is shown to the caller
        // exactly once; only the SHA-256 hash is persisted, so a leaked
        // DB row cannot be used to authenticate.
        let plaintext = crate::secret_token::generate_hex(32);
        let key_hash = crate::secret_token::secret_hash_sha256(plaintext.as_bytes());
        let row = self
            .insert_api_key(label, workspace_id, created_by, &key_hash, role)
            .await?;
        Ok((row, plaintext))
    }

    async fn insert_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        key_hash: &[u8],
        role: &str,
    ) -> OxResult<crate::models::ApiKey> {
        let id = Uuid::new_v4();
        let created_at = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO api_keys \
               (id, label, key_hash, created_by, workspace_id, created_at, role) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(label)
        .bind(key_hash)
        .bind(created_by)
        .bind(workspace_id)
        .bind(created_at)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(crate::models::ApiKey {
            id,
            label: label.to_string(),
            key_hash: key_hash.to_vec(),
            created_by: created_by.to_string(),
            workspace_id,
            role: role.to_string(),
            created_at,
            revoked_at: None,
        })
    }

    async fn find_api_key_by_hash(&self, hash: &[u8]) -> OxResult<Option<crate::models::ApiKey>> {
        sqlx::query_as::<_, crate::models::ApiKey>(
            "SELECT id, label, key_hash, created_by, workspace_id, role, created_at, revoked_at \
             FROM api_keys \
             WHERE key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn list_api_keys(&self) -> OxResult<Vec<crate::models::ApiKey>> {
        sqlx::query_as::<_, crate::models::ApiKey>(
            "SELECT id, label, key_hash, created_by, workspace_id, role, created_at, revoked_at \
             FROM api_keys \
             WHERE revoked_at IS NULL \
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_api_key_revoked(&self, id: Uuid) -> OxResult<bool> {
        let res = sqlx::query(
            "UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(res.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// MeteringStore — cost/usage tracking
// ---------------------------------------------------------------------------

#[async_trait]
impl MeteringStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_usage(
        &self,
        user_id: Option<Uuid>,
        resource_type: &str,
        provider: Option<&str>,
        model: Option<&str>,
        operation: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        duration_ms: i64,
        cost_usd: f64,
        metadata: serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO usage_records
             (user_id, resource_type, provider, model, operation,
              input_tokens, output_tokens, duration_ms, cost_usd, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(user_id)
        .bind(resource_type)
        .bind(provider)
        .bind(model)
        .bind(operation)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(duration_ms)
        .bind(cost_usd)
        .bind(&metadata)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn usage_summary(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> OxResult<Vec<UsageSummary>> {
        sqlx::query_as::<_, UsageSummary>(
            "SELECT
                resource_type,
                COALESCE(SUM(input_tokens), 0)::int8 AS total_input_tokens,
                COALESCE(SUM(output_tokens), 0)::int8 AS total_output_tokens,
                COALESCE(SUM(cost_usd), 0)::float8 AS total_cost_usd,
                COUNT(*)::int8 AS request_count
             FROM usage_records
             WHERE created_at >= $1 AND created_at < $2
             GROUP BY resource_type
             ORDER BY total_cost_usd DESC",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// LineageStore — data provenance tracking
// ---------------------------------------------------------------------------

#[async_trait]
impl LineageStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_lineage_entry(&self, e: &LineageEntry) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO data_lineage
             (id, project_id, graph_label, graph_element_type, source_type,
              source_name, source_table, source_columns, load_plan_hash,
              property_mappings, record_count, loaded_by, started_at, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(e.id)
        .bind(e.project_id)
        .bind(&e.graph_label)
        .bind(&e.graph_element_type)
        .bind(&e.source_type)
        .bind(&e.source_name)
        .bind(&e.source_table)
        .bind(&e.source_columns)
        .bind(&e.load_plan_hash)
        .bind(&e.property_mappings)
        .bind(e.record_count)
        .bind(e.loaded_by)
        .bind(e.started_at)
        .bind(&e.status)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn complete_lineage_entry(
        &self,
        id: Uuid,
        record_count: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE data_lineage
             SET record_count = $2, status = $3, error_message = $4, completed_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(record_count)
        .bind(status)
        .bind(error_message)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_lineage_for_label(&self, graph_label: &str) -> OxResult<Vec<LineageEntry>> {
        sqlx::query_as("SELECT * FROM data_lineage WHERE graph_label = $1 ORDER BY started_at DESC")
            .bind(graph_label)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_lineage_for_project(&self, project_id: Uuid) -> OxResult<Vec<LineageEntry>> {
        sqlx::query_as("SELECT * FROM data_lineage WHERE project_id = $1 ORDER BY started_at DESC")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn lineage_summary(&self) -> OxResult<Vec<LineageSummary>> {
        sqlx::query_as(
            "SELECT
                graph_label,
                graph_element_type,
                COUNT(*) AS source_count,
                COALESCE(SUM(record_count), 0) AS total_records,
                MAX(completed_at) AS last_loaded_at
             FROM data_lineage
             WHERE status = 'completed'
             GROUP BY graph_label, graph_element_type
             ORDER BY total_records DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// ApprovalStore — configurable gates for schema deployment & migration
// ---------------------------------------------------------------------------

#[async_trait]
impl ApprovalStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_approval_request(
        &self,
        requester_id: Uuid,
        action_type: &str,
        resource_type: &str,
        resource_id: &str,
        payload: serde_json::Value,
    ) -> OxResult<ApprovalRequest> {
        sqlx::query_as(
            "INSERT INTO approval_requests
             (requester_id, action_type, resource_type, resource_id, payload)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *",
        )
        .bind(requester_id)
        .bind(action_type)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&payload)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>> {
        sqlx::query_as("SELECT * FROM approval_requests WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pending_approvals(&self, workspace_id: Uuid) -> OxResult<Vec<ApprovalRequest>> {
        sqlx::query_as(
            "SELECT * FROM approval_requests
             WHERE workspace_id = $1 AND status = 'pending' AND expires_at > NOW()
             ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn review_approval(
        &self,
        id: Uuid,
        reviewer_id: Uuid,
        approved: bool,
        notes: Option<&str>,
    ) -> OxResult<()> {
        let status = if approved { "approved" } else { "rejected" };
        let result = sqlx::query(
            "UPDATE approval_requests
             SET status = $1, reviewer_id = $2, review_notes = $3, reviewed_at = NOW()
             WHERE id = $4 AND status = 'pending'",
        )
        .bind(status)
        .bind(reviewer_id)
        .bind(notes)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("pending approval request {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>> {
        // Strict `<` so a request whose `expires_at == NOW()` is still
        // valid for its last clock tick — matches the share-token
        // semantics in `get_dashboard_by_share_token`.
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 UPDATE approval_requests
                 SET status = 'expired'
                 WHERE status = 'pending' AND expires_at < NOW()
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }
}

// ---------------------------------------------------------------------------
// QualityStore
// ---------------------------------------------------------------------------

#[async_trait]
impl QualityStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_quality_rule(&self, rule: &QualityRule) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO quality_rules
             (id, ontology_lineage_id, name, description, rule_type, target_label,
              target_property, threshold, cypher_check, severity, is_active,
              created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(rule.id)
        .bind(&rule.ontology_lineage_id)
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(&rule.rule_type)
        .bind(&rule.target_label)
        .bind(&rule.target_property)
        .bind(rule.threshold)
        .bind(&rule.cypher_check)
        .bind(&rule.severity)
        .bind(rule.is_active)
        .bind(rule.created_by)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_quality_rule(&self, id: Uuid) -> OxResult<Option<QualityRule>> {
        sqlx::query_as("SELECT * FROM quality_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_quality_rules(
        &self,
        ontology_lineage_id: Option<&str>,
        target_label: Option<&str>,
    ) -> OxResult<Vec<QualityRule>> {
        // Build the WHERE clause dynamically — RLS on workspace_id is
        // always applied by the pool-level session var, so only the
        // optional lineage + label filters need per-call parameters.
        let mut conditions = Vec::new();
        if ontology_lineage_id.is_some() {
            conditions.push(format!("ontology_lineage_id = ${}", conditions.len() + 1));
        }
        if target_label.is_some() {
            conditions.push(format!("target_label = ${}", conditions.len() + 1));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql =
            format!("SELECT * FROM quality_rules {where_clause} ORDER BY severity DESC, name");

        let mut query = sqlx::query_as::<_, QualityRule>(&sql);
        if let Some(lineage) = ontology_lineage_id {
            query = query.bind(lineage);
        }
        if let Some(label) = target_label {
            query = query.bind(label);
        }
        query.fetch_all(&self.pool).await.map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_quality_rule(
        &self,
        id: Uuid,
        name: &str,
        threshold: f64,
        is_active: bool,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE quality_rules
             SET name = $1, threshold = $2, is_active = $3, updated_at = NOW()
             WHERE id = $4",
        )
        .bind(name)
        .bind(threshold)
        .bind(is_active)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("quality rule {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_quality_rule(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM quality_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_quality_result(&self, result: &QualityResult) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO quality_results (id, rule_id, passed, actual_value, details, evaluated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(result.id)
        .bind(result.rule_id)
        .bind(result.passed)
        .bind(result.actual_value)
        .bind(&result.details)
        .bind(result.evaluated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_latest_results(&self, rule_id: Uuid, limit: i64) -> OxResult<Vec<QualityResult>> {
        sqlx::query_as(
            "SELECT * FROM quality_results
             WHERE rule_id = $1
             ORDER BY evaluated_at DESC
             LIMIT $2",
        )
        .bind(rule_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_quality_dashboard(&self) -> OxResult<Vec<QualityDashboardEntry>> {
        sqlx::query_as(
            "SELECT qr.id AS rule_id, qr.name, qr.rule_type, qr.target_label,
                    qr.severity, qr.threshold::float8 AS threshold,
                    res.passed AS latest_passed,
                    res.actual_value::float8 AS latest_value,
                    res.evaluated_at AS latest_evaluated_at
             FROM quality_rules qr
             LEFT JOIN LATERAL (
                 SELECT passed, actual_value, evaluated_at
                 FROM quality_results
                 WHERE rule_id = qr.id
                 ORDER BY evaluated_at DESC LIMIT 1
             ) res ON true
             WHERE qr.is_active = true
             ORDER BY qr.severity DESC, qr.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// AclStore — fine-grained attribute-based access control
// ---------------------------------------------------------------------------

#[async_trait]
impl AclStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_acl_policy(&self, p: &AclPolicy) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO acl_policies
             (id, name, description, subject_type, subject_value,
              resource_type, resource_value, action, properties,
              mask_pattern, priority, is_active, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(p.id)
        .bind(&p.name)
        .bind(&p.description)
        .bind(&p.subject_type)
        .bind(&p.subject_value)
        .bind(&p.resource_type)
        .bind(&p.resource_value)
        .bind(&p.action)
        .bind(&p.properties)
        .bind(&p.mask_pattern)
        .bind(p.priority)
        .bind(p.is_active)
        .bind(p.created_by)
        .bind(p.created_at)
        .bind(p.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_acl_policy(&self, id: Uuid) -> OxResult<Option<AclPolicy>> {
        sqlx::query_as("SELECT * FROM acl_policies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_acl_policies(
        &self,
        subject_type: Option<&str>,
        resource_value: Option<&str>,
    ) -> OxResult<Vec<AclPolicy>> {
        // Build dynamic query based on optional filters
        match (subject_type, resource_value) {
            (Some(st), Some(rv)) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true AND subject_type = $1 AND resource_value = $2
                     ORDER BY priority DESC, name",
            )
            .bind(st)
            .bind(rv)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            (Some(st), None) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true AND subject_type = $1
                     ORDER BY priority DESC, name",
            )
            .bind(st)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            (None, Some(rv)) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true AND resource_value = $1
                     ORDER BY priority DESC, name",
            )
            .bind(rv)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            (None, None) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true
                     ORDER BY priority DESC, name",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_acl_policy(
        &self,
        id: Uuid,
        name: &str,
        action: &str,
        properties: Option<&[String]>,
        mask_pattern: Option<&str>,
        priority: i32,
        is_active: bool,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE acl_policies
             SET name = $2, action = $3, properties = $4, mask_pattern = $5,
                 priority = $6, is_active = $7, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(action)
        .bind(properties)
        .bind(mask_pattern)
        .bind(priority)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("ACL policy {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_acl_policy(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM acl_policies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_effective_policies(
        &self,
        platform_role: &str,
        workspace_role: &str,
        user_id: Option<Uuid>,
    ) -> OxResult<Vec<AclPolicy>> {
        if let Some(uid) = user_id {
            sqlx::query_as(
                "SELECT * FROM acl_policies
                 WHERE is_active = true AND (
                     (subject_type = 'role' AND subject_value = $1)
                     OR (subject_type = 'workspace_role' AND subject_value = $2)
                     OR (subject_type = 'user' AND subject_value = $3)
                 )
                 ORDER BY priority DESC",
            )
            .bind(platform_role)
            .bind(workspace_role)
            .bind(uid.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
        } else {
            sqlx::query_as(
                "SELECT * FROM acl_policies
                 WHERE is_active = true AND (
                     (subject_type = 'role' AND subject_value = $1)
                     OR (subject_type = 'workspace_role' AND subject_value = $2)
                 )
                 ORDER BY priority DESC",
            )
            .bind(platform_role)
            .bind(workspace_role)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
        }
    }
}

// ---------------------------------------------------------------------------
// ModelConfigStore
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl crate::store::ModelConfigStore for PostgresStore {
    async fn list_model_configs(
        &self,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Vec<crate::ModelConfig>> {
        let rows = sqlx::query_as::<_, crate::ModelConfig>(
            "SELECT * FROM model_configs
             WHERE workspace_id IS NOT DISTINCT FROM $1
             ORDER BY priority DESC, name",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows)
    }

    async fn get_model_config(&self, id: Uuid) -> OxResult<Option<crate::ModelConfig>> {
        sqlx::query_as::<_, crate::ModelConfig>("SELECT * FROM model_configs WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn create_model_config(
        &self,
        config: &crate::NewModelConfig,
    ) -> OxResult<crate::ModelConfig> {
        sqlx::query_as::<_, crate::ModelConfig>(
            "INSERT INTO model_configs
                (workspace_id, name, provider, model_id, max_tokens, temperature,
                 timeout_secs, cost_per_1m_input, cost_per_1m_output,
                 daily_budget_usd, priority, api_key_env, region, base_url)
             VALUES ($1, $2, $3, $4,
                     COALESCE($5, 8192), $6, COALESCE($7, 300),
                     $8, $9, $10, COALESCE($11, 0),
                     $12, $13, $14)
             RETURNING *",
        )
        .bind(config.workspace_id)
        .bind(&config.name)
        .bind(&config.provider)
        .bind(&config.model_id)
        .bind(config.max_tokens)
        .bind(config.temperature)
        .bind(config.timeout_secs)
        .bind(config.cost_per_1m_input)
        .bind(config.cost_per_1m_output)
        .bind(config.daily_budget_usd)
        .bind(config.priority)
        .bind(&config.api_key_env)
        .bind(&config.region)
        .bind(&config.base_url)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_model_config(
        &self,
        id: Uuid,
        update: &crate::ModelConfigUpdate,
    ) -> OxResult<crate::ModelConfig> {
        sqlx::query_as::<_, crate::ModelConfig>(
            "UPDATE model_configs SET
                name = COALESCE($2, name),
                provider = COALESCE($3, provider),
                model_id = COALESCE($4, model_id),
                max_tokens = COALESCE($5, max_tokens),
                temperature = COALESCE($6, temperature),
                timeout_secs = COALESCE($7, timeout_secs),
                cost_per_1m_input = COALESCE($8, cost_per_1m_input),
                cost_per_1m_output = COALESCE($9, cost_per_1m_output),
                daily_budget_usd = COALESCE($10, daily_budget_usd),
                priority = COALESCE($11, priority),
                enabled = COALESCE($12, enabled),
                api_key_env = COALESCE($13, api_key_env),
                region = COALESCE($14, region),
                base_url = COALESCE($15, base_url),
                updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(&update.name)
        .bind(&update.provider)
        .bind(&update.model_id)
        .bind(update.max_tokens)
        .bind(update.temperature)
        .bind(update.timeout_secs)
        .bind(update.cost_per_1m_input)
        .bind(update.cost_per_1m_output)
        .bind(update.daily_budget_usd)
        .bind(update.priority)
        .bind(update.enabled)
        .bind(&update.api_key_env)
        .bind(&update.region)
        .bind(&update.base_url)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn delete_model_config(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM model_configs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_routing_rules(
        &self,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Vec<crate::ModelRoutingRule>> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "SELECT * FROM model_routing_rules
             WHERE workspace_id IS NOT DISTINCT FROM $1
             ORDER BY priority DESC, operation",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn get_routing_rule(&self, id: Uuid) -> OxResult<Option<crate::ModelRoutingRule>> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "SELECT * FROM model_routing_rules WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn create_routing_rule(
        &self,
        rule: &crate::NewRoutingRule,
    ) -> OxResult<crate::ModelRoutingRule> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "INSERT INTO model_routing_rules
                (workspace_id, operation, model_config_id, priority)
             VALUES ($1, $2, $3, COALESCE($4, 0))
             RETURNING *",
        )
        .bind(rule.workspace_id)
        .bind(&rule.operation)
        .bind(rule.model_config_id)
        .bind(rule.priority)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_routing_rule(
        &self,
        id: Uuid,
        update: &crate::RoutingRuleUpdate,
    ) -> OxResult<crate::ModelRoutingRule> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "UPDATE model_routing_rules SET
                operation = COALESCE($2, operation),
                model_config_id = COALESCE($3, model_config_id),
                priority = COALESCE($4, priority),
                enabled = COALESCE($5, enabled)
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(&update.operation)
        .bind(update.model_config_id)
        .bind(update.priority)
        .bind(update.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn delete_routing_rule(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM model_routing_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_model_for_operation(
        &self,
        operation: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<crate::ModelConfig>> {
        // Priority chain: workspace-specific rules > global rules > wildcard
        sqlx::query_as::<_, crate::ModelConfig>(
            "SELECT mc.* FROM model_routing_rules r
             JOIN model_configs mc ON r.model_config_id = mc.id
             WHERE r.operation IN ($1, '*')
               AND r.enabled = true AND mc.enabled = true
               AND (r.workspace_id = $2 OR r.workspace_id IS NULL)
             ORDER BY
                 CASE WHEN r.workspace_id IS NOT NULL THEN 0 ELSE 1 END,
                 CASE WHEN r.operation = $1 THEN 0 ELSE 1 END,
                 r.priority DESC
             LIMIT 1",
        )
        .bind(operation)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

// ---------------------------------------------------------------------------
// KnowledgeStore
// ---------------------------------------------------------------------------

#[async_trait]
impl KnowledgeStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_knowledge_entry(&self, entry: &KnowledgeEntry) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO knowledge_entries (
                id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                kind, status, confidence, title, content, structured_data,
                version_checked, content_hash, source_execution_ids, source_session_id,
                affected_labels, affected_properties, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            ON CONFLICT (workspace_id, ontology_name, content_hash) DO UPDATE SET
                confidence = GREATEST(knowledge_entries.confidence, EXCLUDED.confidence),
                updated_at = now()",
        )
        .bind(entry.id)
        .bind(entry.workspace_id)
        .bind(&entry.ontology_name)
        .bind(entry.ontology_version_min)
        .bind(entry.ontology_version_max)
        .bind(&entry.kind)
        .bind(&entry.status)
        .bind(entry.confidence)
        .bind(&entry.title)
        .bind(&entry.content)
        .bind(&entry.structured_data)
        .bind(entry.version_checked)
        .bind(&entry.content_hash)
        .bind(&entry.source_execution_ids)
        .bind(entry.source_session_id)
        .bind(&entry.affected_labels)
        .bind(&entry.affected_properties)
        .bind(&entry.created_by)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_knowledge_entry(&self, id: Uuid) -> OxResult<Option<KnowledgeEntry>> {
        sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
             FROM knowledge_entries WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_entry(
        &self,
        id: Uuid,
        title: &str,
        content: &str,
        structured_data: &serde_json::Value,
        affected_labels: &[String],
        affected_properties: &[String],
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE knowledge_entries SET title = $2, content = $3, structured_data = $4,
                    affected_labels = $5, affected_properties = $6,
                    content_hash = encode(sha256((ontology_name || lower(trim($3)))::bytea), 'hex'),
                    updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(title)
        .bind(content)
        .bind(structured_data)
        .bind(affected_labels)
        .bind(affected_properties)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)
        .and_then(|r| {
            if r.rows_affected() == 0 {
                Err(ox_core::error::OxError::Runtime {
                    message: "Knowledge entry not found".to_string(),
                })
            } else {
                Ok(())
            }
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_knowledge_entry(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM knowledge_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_knowledge_entries(
        &self,
        ontology_name: Option<&str>,
        kind: Option<&str>,
        status: Option<&str>,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<KnowledgeEntry>> {
        let limit = pagination.effective_limit();
        let cursor = pagination.cursor_parts();

        let rows: Vec<KnowledgeEntry> = sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
             FROM knowledge_entries
             WHERE ($1::text IS NULL OR ontology_name = $1)
               AND ($2::text IS NULL OR kind = $2)
               AND ($3::text IS NULL OR status = $3)
               AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
             ORDER BY created_at DESC, id DESC
             LIMIT $6",
        )
        .bind(ontology_name)
        .bind(kind)
        .bind(status)
        .bind(cursor.map(|(ts, _)| ts))
        .bind(cursor.map(|(_, id)| id))
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        let has_more = rows.len() > limit as usize;
        let mut items = rows;
        if has_more {
            items.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            items
                .last()
                .map(|r| format!("{}|{}", r.created_at.to_rfc3339(), r.id))
        } else {
            None
        };

        Ok(CursorPage { items, next_cursor })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_active_knowledge(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        kinds: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>> {
        sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
             FROM knowledge_entries
             WHERE ontology_name = $1
               AND status = 'approved'
               AND ontology_version_min <= $2
               AND (ontology_version_max IS NULL OR ontology_version_max >= $2)
               AND kind = ANY($3)
             ORDER BY confidence DESC
             LIMIT $4",
        )
        .bind(ontology_name)
        .bind(ontology_version)
        .bind(kinds)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_status(
        &self,
        id: Uuid,
        status: &str,
        reviewer_id: Option<Uuid>,
        review_notes: Option<&str>,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE knowledge_entries SET status = $2, reviewed_by = $3, review_notes = $4,
                    reviewed_at = now(), updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(reviewer_id)
        .bind(review_notes)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        if result.rows_affected() == 0 {
            return Err(ox_core::error::OxError::Runtime {
                message: "Knowledge entry not found".to_string(),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_confidence(&self, id: Uuid, confidence: f64) -> OxResult<()> {
        sqlx::query(
            "UPDATE knowledge_entries SET confidence = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(confidence)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn mark_stale_by_labels(
        &self,
        ontology_name: &str,
        changed_labels: &[String],
    ) -> OxResult<u64> {
        let result = sqlx::query(
            "UPDATE knowledge_entries
             SET status = 'stale', confidence = confidence * 0.5, updated_at = now()
             WHERE ontology_name = $1
               AND status = 'approved'
               AND affected_labels && $2",
        )
        .bind(ontology_name)
        .bind(changed_labels)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_knowledge_usage(&self, ids: &[Uuid]) -> OxResult<()> {
        sqlx::query(
            "UPDATE knowledge_entries SET use_count = use_count + 1, last_used_at = now()
             WHERE id = ANY($1)",
        )
        .bind(ids)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn verify_knowledge(&self, id: Uuid, version: i32) -> OxResult<()> {
        sqlx::query(
            "UPDATE knowledge_entries SET version_checked = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(version)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn search_knowledge_by_labels(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        labels: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>> {
        sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
             FROM knowledge_entries
             WHERE ontology_name = $1
               AND status = 'approved'
               AND ontology_version_min <= $2
               AND (ontology_version_max IS NULL OR ontology_version_max >= $2)
               AND affected_labels && $3
             ORDER BY confidence DESC
             LIMIT $4",
        )
        .bind(ontology_name)
        .bind(ontology_version)
        .bind(labels)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn count_knowledge_by_status_kind(&self) -> OxResult<Vec<(String, String, i64)>> {
        sqlx::query_as::<_, (String, String, i64)>(
            "SELECT status, kind, COUNT(*) FROM knowledge_entries GROUP BY status, kind",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn cleanup_knowledge(&self, older_than_days: i64) -> OxResult<u64> {
        // Auto-deprecate low-confidence entries
        sqlx::query(
            "UPDATE knowledge_entries SET status = 'deprecated', updated_at = now()
             WHERE confidence < 0.1 AND status != 'deprecated'",
        )
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        // Delete old deprecated entries
        let result = sqlx::query(
            "DELETE FROM knowledge_entries
             WHERE status = 'deprecated'
               AND updated_at < now() - make_interval(days => $1)",
        )
        .bind(older_than_days as i32)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(result.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// LoadCheckpointStore — watermark-based incremental load state
// ---------------------------------------------------------------------------

#[async_trait]
impl LoadCheckpointStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_checkpoint(
        &self,
        project_id: Uuid,
        source_table: &str,
        graph_label: &str,
    ) -> OxResult<Option<LoadCheckpoint>> {
        sqlx::query_as(
            "SELECT * FROM load_checkpoints
             WHERE project_id = $1 AND source_table = $2 AND graph_label = $3",
        )
        .bind(project_id)
        .bind(source_table)
        .bind(graph_label)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_checkpoint(&self, c: &LoadCheckpoint) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO load_checkpoints
             (id, workspace_id, project_id, source_table, graph_label,
              watermark_column, watermark_value, record_count, loaded_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (workspace_id, project_id, source_table, graph_label)
             DO UPDATE SET
                watermark_column = EXCLUDED.watermark_column,
                watermark_value = EXCLUDED.watermark_value,
                record_count = load_checkpoints.record_count + EXCLUDED.record_count,
                loaded_at = EXCLUDED.loaded_at",
        )
        .bind(c.id)
        .bind(c.workspace_id)
        .bind(c.project_id)
        .bind(&c.source_table)
        .bind(&c.graph_label)
        .bind(&c.watermark_column)
        .bind(&c.watermark_value)
        .bind(c.record_count)
        .bind(c.loaded_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_checkpoints(&self, project_id: Uuid) -> OxResult<Vec<LoadCheckpoint>> {
        sqlx::query_as(
            "SELECT * FROM load_checkpoints
             WHERE project_id = $1
             ORDER BY loaded_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_checkpoint(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM load_checkpoints WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// DataSourceStore
//
// Workspace-scoped persistence for federation (VOL) adapter
// configurations. RLS gates every read + write via the task-local
// `app.workspace_id` the pool's `before_acquire` injects, so these
// queries never name `workspace_id` themselves — the column's default
// (set in migration 0011) picks it up, and RLS enforces isolation.
// ---------------------------------------------------------------------------

#[async_trait]
impl crate::store::DataSourceStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_data_source(&self, item: &crate::models::DataSource) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO data_sources (id, source_id, kind, config, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(item.id)
        .bind(&item.source_id)
        .bind(&item.kind)
        .bind(&item.config)
        .bind(item.created_at)
        .bind(item.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_data_source(&self, id: Uuid) -> OxResult<Option<crate::models::DataSource>> {
        sqlx::query_as::<_, crate::models::DataSource>(
            "SELECT id, workspace_id, source_id, kind, config, created_at, updated_at
             FROM data_sources
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_data_source_by_source_id(
        &self,
        source_id: &str,
    ) -> OxResult<Option<crate::models::DataSource>> {
        sqlx::query_as::<_, crate::models::DataSource>(
            "SELECT id, workspace_id, source_id, kind, config, created_at, updated_at
             FROM data_sources
             WHERE source_id = $1",
        )
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_data_sources(&self) -> OxResult<Vec<crate::models::DataSource>> {
        sqlx::query_as::<_, crate::models::DataSource>(
            "SELECT id, workspace_id, source_id, kind, config, created_at, updated_at
             FROM data_sources
             ORDER BY source_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_data_source_by_source_id(
        &self,
        source_id: &str,
        kind: &str,
        config: &serde_json::Value,
    ) -> OxResult<crate::models::DataSource> {
        // ON CONFLICT on (workspace_id, source_id) — the unique
        // constraint declared in migration 0011. The conflicting row's
        // workspace_id must match the current session's workspace_id
        // because RLS is enforced against the row already; DO UPDATE
        // therefore only replaces rows the caller is allowed to see.
        sqlx::query_as::<_, crate::models::DataSource>(
            "INSERT INTO data_sources (source_id, kind, config)
             VALUES ($1, $2, $3)
             ON CONFLICT (workspace_id, source_id) DO UPDATE
                SET kind = EXCLUDED.kind,
                    config = EXCLUDED.config,
                    updated_at = NOW()
             RETURNING id, workspace_id, source_id, kind, config, created_at, updated_at",
        )
        .bind(source_id)
        .bind(kind)
        .bind(config)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_data_source_by_source_id(&self, source_id: &str) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM data_sources WHERE source_id = $1")
            .bind(source_id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// Λ Phase — OntologyVersionStore implementation
//
// Backs the 4-Level storage model with PostgreSQL. Level 1 rows
// live in `ontologies` / `ontology_version_snapshots`; Level 2
// in `ontology_entity_versions` / `ontology_version_entities`.
// Level 3 materialised indexes (Λ-6..Λ-9) are populated by the
// callbacks inside `commit_version` as each phase lands.
// ---------------------------------------------------------------------------

#[async_trait]
impl crate::store::OntologyVersionStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_ontology(
        &self,
        name: &str,
        description: &serde_json::Value,
        lineage_id: Option<&str>,
    ) -> OxResult<crate::models::OntologyRow> {
        // Explicit lineage takes precedence; otherwise a fresh UUID
        // v4 goes into the TEXT column. Clients can always overwrite
        // later via a sibling update path — for now creation is the
        // only entry point.
        let lineage = lineage_id
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "INSERT INTO ontologies (lineage_id, name, description) \
             VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(&lineage)
        .bind(name)
        .bind(description)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_ontology(
        &self,
        id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "SELECT * FROM ontologies WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_ontologies(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<crate::models::OntologyRow>> {
        let limit = pagination.effective_limit();

        let rows = if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            sqlx::query_as::<_, crate::models::OntologyRow>(
                "SELECT * FROM ontologies \
                 WHERE (created_at, id) < ($1, $2) \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        } else {
            sqlx::query_as::<_, crate::models::OntologyRow>(
                "SELECT * FROM ontologies \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $1",
            )
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        };

        Ok(build_cursor_page(rows, limit, |o| (o.created_at, o.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_ontology_by_lineage(
        &self,
        lineage_id: &str,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "SELECT * FROM ontologies WHERE lineage_id = $1",
        )
        .bind(lineage_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_ontology_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        // RLS scopes the row set to the caller's workspace;
        // `ontologies_ws_name_uq` makes this a single-row lookup.
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "SELECT * FROM ontologies WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn commit_version(
        &self,
        ontology_id: Uuid,
        ir: &ox_ontology::OntologyIR,
        version: &str,
        parent_version_id: Option<Uuid>,
        committed_by: &str,
        commit_message: &str,
    ) -> OxResult<crate::models::OntologyVersionSnapshot> {
        // Extract content-addressed entities BEFORE opening the
        // transaction — serialisation failures should not leave a
        // half-open tx behind.
        let entities = ox_ontology::storage::extract_entities(ir)?;

        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // 1) Upsert content-addressed entity rows. ON CONFLICT
        //    (entity_hash) DO NOTHING → auto dedup across versions.
        //    Bulk INSERT via unnest to avoid N round trips.
        let mut hashes: Vec<String> = Vec::with_capacity(entities.len());
        let mut kinds: Vec<String> = Vec::with_capacity(entities.len());
        let mut contents: Vec<serde_json::Value> = Vec::with_capacity(entities.len());
        for ent in &entities {
            hashes.push(ent.hash.clone());
            kinds.push(ent.kind.as_str().to_string());
            contents.push(ent.content.clone());
        }
        sqlx::query(
            "INSERT INTO ontology_entity_versions (entity_hash, entity_kind, content) \
             SELECT * FROM UNNEST($1::text[], $2::text[], $3::jsonb[]) \
             ON CONFLICT (entity_hash) DO NOTHING",
        )
        .bind(&hashes)
        .bind(&kinds)
        .bind(&contents)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // 2) Create the version snapshot row. Default bitemporal
        //    columns: valid_from=now, valid_to=NULL, sys_from=now,
        //    sys_to=NULL. Callers that need retrospective windows
        //    land later as a separate route; this is the common
        //    "commit the current state" path.
        let snapshot = sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "INSERT INTO ontology_version_snapshots \
                (ontology_id, version, parent_version_id, committed_by, commit_message) \
             VALUES ($1, $2, $3, $4, $5) RETURNING *",
        )
        .bind(ontology_id)
        .bind(version)
        .bind(parent_version_id)
        .bind(committed_by)
        .bind(commit_message)
        .fetch_one(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // 3) Write the pointer set. One row per (kind, logical_id)
        //    → current hash. Bulk insert via unnest.
        let version_id = snapshot.id;
        let mut logical_ids: Vec<String> = Vec::with_capacity(entities.len());
        // Reuse `kinds` and `hashes` from step 1 — same set, same order.
        for ent in &entities {
            logical_ids.push(ent.logical_id.clone());
        }
        sqlx::query(
            "INSERT INTO ontology_version_entities \
                (version_id, entity_kind, entity_logical_id, entity_hash) \
             SELECT $1, k.kind, k.lid, k.hash \
             FROM UNNEST($2::text[], $3::text[], $4::text[]) \
                AS k(kind, lid, hash)",
        )
        .bind(version_id)
        .bind(&kinds)
        .bind(&logical_ids)
        .bind(&hashes)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // 4) Materialise Level 3 indexes for this new version.
        //    Inline in the same transaction so a hydrate can see
        //    the flat/navigation/search rows the moment the
        //    version commit returns. Embeddings are intentionally
        //    skipped — they populate asynchronously via a
        //    background job.
        materialize_level3(&mut tx, version_id, ir).await?;

        tx.commit().await.map_err(to_ox_error)?;
        Ok(snapshot)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn load_version(
        &self,
        version_id: Uuid,
    ) -> OxResult<ox_ontology::OntologyIR> {
        // Hydrate every entity that belongs to this version.
        // Order is not important — the extractor / assembler
        // tolerates arbitrary arrival order and re-keys by
        // (kind, logical_id).
        let rows = sqlx::query_as::<_, crate::models::OntologyEntityJoinRow>(
            "SELECT vs.entity_kind AS entity_kind, \
                    vs.entity_logical_id AS entity_logical_id, \
                    vs.entity_hash AS entity_hash, \
                    ev.content AS content \
             FROM ontology_version_entities vs \
             JOIN ontology_entity_versions ev ON ev.entity_hash = vs.entity_hash \
             WHERE vs.version_id = $1",
        )
        .bind(version_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        assemble_ir(&rows).map_err(|e| OxError::Runtime {
            message: format!("OntologyIR hydration from version {version_id}: {e:?}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_version_snapshot(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>> {
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots WHERE id = $1",
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_versions(
        &self,
        ontology_id: Uuid,
        limit: u32,
    ) -> OxResult<Vec<crate::models::OntologyVersionSnapshot>> {
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots \
             WHERE ontology_id = $1 \
             ORDER BY created_at DESC \
             LIMIT $2",
        )
        .bind(ontology_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn resolve_version_at(
        &self,
        ontology_id: Uuid,
        as_of: DateTime<Utc>,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>> {
        // "Live at as_of": valid_from <= as_of AND (valid_to IS
        // NULL OR valid_to > as_of). Newest-first tiebreak by
        // valid_from.
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots \
             WHERE ontology_id = $1 \
               AND valid_from <= $2 \
               AND (valid_to IS NULL OR valid_to > $2) \
             ORDER BY valid_from DESC \
             LIMIT 1",
        )
        .bind(ontology_id)
        .bind(as_of)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_current_version(
        &self,
        ontology_id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>> {
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots \
             WHERE ontology_id = $1 AND valid_to IS NULL \
             ORDER BY created_at DESC \
             LIMIT 1",
        )
        .bind(ontology_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn diff_versions(
        &self,
        from_version: Uuid,
        to_version: Uuid,
    ) -> OxResult<Vec<crate::models::EntityChange>> {
        // FULL OUTER JOIN on (kind, logical_id) — rows where
        // `from_hash != to_hash`, or one side is NULL, are
        // changes. Stable ordering by (kind, logical_id) so the
        // diff reads predictably in the admin UI.
        let rows = sqlx::query_as::<_, crate::models::DiffRow>(
            "SELECT COALESCE(f.entity_kind, t.entity_kind)             AS entity_kind, \
                    COALESCE(f.entity_logical_id, t.entity_logical_id) AS entity_logical_id, \
                    f.entity_hash                                       AS from_hash, \
                    t.entity_hash                                       AS to_hash \
             FROM (SELECT * FROM ontology_version_entities WHERE version_id = $1) f \
             FULL OUTER JOIN \
                  (SELECT * FROM ontology_version_entities WHERE version_id = $2) t \
               ON f.entity_kind = t.entity_kind \
              AND f.entity_logical_id = t.entity_logical_id \
             WHERE f.entity_hash IS DISTINCT FROM t.entity_hash \
             ORDER BY entity_kind, entity_logical_id",
        )
        .bind(from_version)
        .bind(to_version)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let kind = match (row.from_hash.clone(), row.to_hash.clone()) {
                    (None, Some(to_hash)) => crate::models::EntityChangeKind::Added { to_hash },
                    (Some(from_hash), None) => {
                        crate::models::EntityChangeKind::Removed { from_hash }
                    }
                    (Some(from_hash), Some(to_hash)) => {
                        crate::models::EntityChangeKind::Modified { from_hash, to_hash }
                    }
                    // The SQL WHERE hash DISTINCT filter guarantees
                    // at least one side is populated. Surface as
                    // Modified with empty hashes so the admin UI
                    // still shows the row instead of panicking on a
                    // theoretically-impossible row the DB could in
                    // principle produce.
                    (None, None) => crate::models::EntityChangeKind::Modified {
                        from_hash: String::new(),
                        to_hash: String::new(),
                    },
                };
                crate::models::EntityChange {
                    entity_kind: row.entity_kind,
                    entity_logical_id: row.entity_logical_id,
                    kind,
                }
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Λ-11 — OntologyNavigationStore implementation.
//
// Backed by the Level 3 materialised tables: search_vector
// (GIN tsvector + trgm), entity_neighbors (1-hop edges),
// entity_hierarchy (closure), entity_embedding (pgvector HNSW).
// All queries are version-scoped.
// ---------------------------------------------------------------------------

#[async_trait]
impl crate::store::OntologyNavigationStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn search_entry_points(
        &self,
        options: crate::navigation::EntryPointSearchOptions,
    ) -> OxResult<Vec<crate::navigation::EntitySearchHit>> {
        // Trigram + full-text blend. Embedding weight is folded into
        // the `similar_entities` path — this query is the cheap
        // text-first pass so the agent hits it first for prefix / alias
        // recall; embedding kNN is the slower semantic fallback.
        //
        // The kind filter becomes `entity_kind = ANY($kinds)` when
        // supplied. Passing `NULL` (via `Option::None`) disables the
        // clause. NULL-safety via `COALESCE($kinds IS NULL, false)`
        // keeps the filter branch-free on the SQL side.
        let kind_filter: Option<Vec<String>> = options.kinds.clone();
        let trigram_w = options.blend.trigram;
        let full_text_w = options.blend.full_text;
        sqlx::query_as::<_, crate::navigation::EntitySearchHit>(
            "SELECT entity_kind::text AS entity_kind, \
                    logical_id, \
                    doc, \
                    GREATEST( \
                        similarity(doc, $2)::real * $4, \
                        COALESCE(ts_rank(tsv, plainto_tsquery('simple', $2)), 0) * $5 \
                    )::real AS score \
             FROM ontology_entity_search_vector \
             WHERE version_id = $1 \
               AND (doc ILIKE '%' || $2 || '%' \
                    OR similarity(doc, $2) > 0.1 \
                    OR tsv @@ plainto_tsquery('simple', $2)) \
               AND ($6::text[] IS NULL OR entity_kind::text = ANY($6)) \
             ORDER BY score DESC \
             LIMIT $3",
        )
        .bind(options.version_id)
        .bind(&options.query)
        .bind(options.limit as i64)
        .bind(trigram_w)
        .bind(full_text_w)
        .bind(kind_filter.as_deref())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn expand_neighbors(
        &self,
        options: crate::navigation::NeighborExpandOptions,
    ) -> OxResult<crate::navigation::Subgraph> {
        use crate::navigation::{
            EntityRef, NeighborDirection, Subgraph, SubgraphEdge, SubgraphNode,
        };
        use std::collections::HashMap;

        // Anchors seed the subgraph at depth 0. A shared HashMap keyed
        // by `(kind, logical_id)` dedups across iterations — the BFS
        // iterates until `depth` hops or `max_nodes` exceeded,
        // whichever comes first. `visited` protects against cycles in
        // the neighbor graph.
        let mut nodes: HashMap<(String, String), SubgraphNode> = HashMap::new();
        let mut edges: Vec<SubgraphEdge> = Vec::new();
        let mut truncated = false;

        for a in &options.anchors {
            nodes.insert(
                (a.kind.clone(), a.logical_id.clone()),
                SubgraphNode {
                    kind: a.kind.clone(),
                    logical_id: a.logical_id.clone(),
                    label: None,
                    doc: None,
                    depth: 0,
                },
            );
        }

        let mut frontier: Vec<EntityRef> = options.anchors.clone();
        let include_kinds = options.include_kinds.clone();
        let max_nodes = if options.max_nodes == 0 {
            u32::MAX
        } else {
            options.max_nodes
        };

        for hop in 1..=options.depth {
            if frontier.is_empty() {
                break;
            }
            let kinds: Vec<String> = frontier.iter().map(|r| r.kind.clone()).collect();
            let ids: Vec<String> = frontier.iter().map(|r| r.logical_id.clone()).collect();

            // `UNNEST` over the two anchor arrays produces the pair set
            // without needing tuple-IN support. Casting the column to
            // text keeps the join condition comparable against the bound
            // `text[]`s — Postgres enum equality against a text array
            // requires the explicit `::text` flip.
            let direction_where = match options.direction {
                NeighborDirection::Outgoing => {
                    "JOIN UNNEST($2::text[], $3::text[]) AS a(kind, id) \
                      ON n.from_kind::text = a.kind AND n.from_logical_id = a.id"
                }
                NeighborDirection::Incoming => {
                    "JOIN UNNEST($2::text[], $3::text[]) AS a(kind, id) \
                      ON n.to_kind::text = a.kind AND n.to_logical_id = a.id"
                }
                NeighborDirection::Both => {
                    "JOIN UNNEST($2::text[], $3::text[]) AS a(kind, id) \
                      ON (n.from_kind::text = a.kind AND n.from_logical_id = a.id) \
                      OR (n.to_kind::text = a.kind   AND n.to_logical_id = a.id)"
                }
            };
            let sql = format!(
                "SELECT n.from_kind::text AS from_kind, n.from_logical_id, \
                        n.to_kind::text AS to_kind, n.to_logical_id, n.relation_kind \
                 FROM ontology_entity_neighbors n \
                 {direction_where} \
                 WHERE n.version_id = $1",
            );

            #[derive(sqlx::FromRow)]
            struct NeighborRow {
                from_kind: String,
                from_logical_id: String,
                to_kind: String,
                to_logical_id: String,
                relation_kind: String,
            }

            let rows: Vec<NeighborRow> = sqlx::query_as::<_, NeighborRow>(&sql)
                .bind(options.version_id)
                .bind(&kinds)
                .bind(&ids)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?;

            let mut next_frontier: Vec<EntityRef> = Vec::new();
            for r in rows {
                let from = EntityRef::new(&r.from_kind, &r.from_logical_id);
                let to = EntityRef::new(&r.to_kind, &r.to_logical_id);
                edges.push(SubgraphEdge {
                    from: from.clone(),
                    to: to.clone(),
                    relation_kind: r.relation_kind,
                });

                for side in [from, to] {
                    let key = (side.kind.clone(), side.logical_id.clone());
                    let include_this = include_kinds
                        .as_ref()
                        .is_none_or(|ks| ks.iter().any(|k| k == &side.kind));
                    if !include_this {
                        continue;
                    }
                    if nodes.contains_key(&key) {
                        continue;
                    }
                    if (nodes.len() as u32) >= max_nodes {
                        truncated = true;
                        continue;
                    }
                    nodes.insert(
                        key,
                        SubgraphNode {
                            kind: side.kind.clone(),
                            logical_id: side.logical_id.clone(),
                            label: None,
                            doc: None,
                            depth: hop,
                        },
                    );
                    next_frontier.push(side);
                }
            }
            frontier = next_frontier;
        }

        let nodes: Vec<SubgraphNode> = nodes.into_values().collect();
        Ok(Subgraph {
            nodes,
            edges,
            truncated,
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn apply_hierarchy_and_facet(
        &self,
        subgraph: crate::navigation::Subgraph,
        options: crate::navigation::HierarchyFacetOptions,
    ) -> OxResult<crate::navigation::Subgraph> {
        use crate::navigation::{
            EntityRef, FacetFilter, HierarchyExpand, Subgraph, SubgraphEdge, SubgraphNode,
        };
        use std::collections::HashMap;

        let mut nodes: HashMap<(String, String), SubgraphNode> = subgraph
            .nodes
            .into_iter()
            .map(|n| ((n.kind.clone(), n.logical_id.clone()), n))
            .collect();
        let mut edges = subgraph.edges;
        let mut truncated = subgraph.truncated;

        // Hierarchy closure — walks `ontology_entity_hierarchy` for the
        // relation + anchor. Descendants can clamp on `max_depth`;
        // ancestors are always short so no clamp is exposed.
        if let Some(expand) = options.hierarchy_expand {
            #[derive(sqlx::FromRow)]
            struct HierarchyRow {
                relation_kind: String,
                ancestor_kind: String,
                ancestor_logical_id: String,
                descendant_kind: String,
                descendant_logical_id: String,
                depth: i32,
            }

            let rows: Vec<HierarchyRow> = match expand {
                HierarchyExpand::Descendants {
                    relation_kind,
                    anchor,
                    max_depth,
                } => sqlx::query_as::<_, HierarchyRow>(
                    "SELECT relation_kind, \
                            ancestor_kind::text AS ancestor_kind, ancestor_logical_id, \
                            descendant_kind::text AS descendant_kind, descendant_logical_id, \
                            depth \
                     FROM ontology_entity_hierarchy \
                     WHERE version_id = $1 \
                       AND relation_kind = $2 \
                       AND ancestor_kind = $3::ontology_entity_kind \
                       AND ancestor_logical_id = $4 \
                       AND depth <= $5 \
                     ORDER BY depth, descendant_logical_id",
                )
                .bind(options.version_id)
                .bind(&relation_kind)
                .bind(&anchor.kind)
                .bind(&anchor.logical_id)
                .bind(max_depth as i32)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
                HierarchyExpand::Ancestors {
                    relation_kind,
                    anchor,
                } => sqlx::query_as::<_, HierarchyRow>(
                    "SELECT relation_kind, \
                            ancestor_kind::text AS ancestor_kind, ancestor_logical_id, \
                            descendant_kind::text AS descendant_kind, descendant_logical_id, \
                            depth \
                     FROM ontology_entity_hierarchy \
                     WHERE version_id = $1 \
                       AND relation_kind = $2 \
                       AND descendant_kind = $3::ontology_entity_kind \
                       AND descendant_logical_id = $4 \
                     ORDER BY depth, ancestor_logical_id",
                )
                .bind(options.version_id)
                .bind(&relation_kind)
                .bind(&anchor.kind)
                .bind(&anchor.logical_id)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
            };

            // CodeSystem child cap — if a CodeSystem accumulates too
            // many codes via hierarchy expansion, trim to
            // `max_codes_per_code_system` descendants ordered by
            // closest depth first. Keeps the LLM-render budget
            // predictable on deep taxonomies.
            let mut codes_per_system: HashMap<(String, String), u32> = HashMap::new();

            for r in rows {
                let ancestor = EntityRef::new(&r.ancestor_kind, &r.ancestor_logical_id);
                let descendant =
                    EntityRef::new(&r.descendant_kind, &r.descendant_logical_id);

                if r.depth == 0 {
                    // Self-row — already present as the anchor.
                    continue;
                }

                if r.ancestor_kind == "CodeSystem" {
                    let entry = codes_per_system
                        .entry((r.ancestor_kind.clone(), r.ancestor_logical_id.clone()))
                        .or_insert(0);
                    if *entry >= options.max_codes_per_code_system {
                        truncated = true;
                        continue;
                    }
                    *entry += 1;
                }

                edges.push(SubgraphEdge {
                    from: ancestor,
                    to: descendant.clone(),
                    relation_kind: r.relation_kind.clone(),
                });

                nodes
                    .entry((descendant.kind.clone(), descendant.logical_id.clone()))
                    .or_insert(SubgraphNode {
                        kind: descendant.kind,
                        logical_id: descendant.logical_id,
                        label: None,
                        doc: None,
                        depth: r.depth.max(0) as u8,
                    });
            }
        }

        // Facet filter — applied LAST so hierarchy enrichment can
        // still carry nodes that the final kind-filter keeps.
        if let Some(FacetFilter { kinds: Some(ks) }) = options.facet_filter {
            nodes.retain(|(k, _), _| ks.iter().any(|pat| pat == k));
            edges.retain(|e| {
                ks.iter().any(|k| k == &e.from.kind)
                    && ks.iter().any(|k| k == &e.to.kind)
            });
        }

        Ok(Subgraph {
            nodes: nodes.into_values().collect(),
            edges,
            truncated,
        })
    }

    fn render_subgraph_for_llm(
        &self,
        subgraph: &crate::navigation::Subgraph,
        options: &crate::navigation::LlmRenderOptions,
    ) -> String {
        // Pure formatter — delegated so unit tests can cover the
        // markdown shape without standing up a pool.
        crate::navigation::render_subgraph_as_llm_markdown(subgraph, options)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn similar_entities(
        &self,
        version_id: Uuid,
        entity_kind: &str,
        logical_id: &str,
        top_k: u32,
    ) -> OxResult<Vec<crate::navigation::EntitySearchHit>> {
        // Find the query vector; if absent (embedding not yet
        // populated), return empty rather than fall back to
        // something less precise. Callers can chain with
        // `search_entry_points` for the fallback behaviour.
        sqlx::query_as::<_, crate::navigation::EntitySearchHit>(
            "WITH q AS ( \
                SELECT embedding \
                FROM ontology_entity_embedding \
                WHERE version_id = $1 \
                  AND entity_kind = $2::ontology_entity_kind \
                  AND logical_id = $3 \
                  AND embedding IS NOT NULL \
                LIMIT 1 \
             ) \
             SELECT sv.entity_kind::text AS entity_kind, \
                    sv.logical_id, \
                    sv.doc, \
                    (1.0 - (e.embedding <=> (SELECT embedding FROM q)))::real AS score \
             FROM ontology_entity_embedding e \
             JOIN ontology_entity_search_vector sv \
               ON sv.version_id = e.version_id \
              AND sv.entity_kind = e.entity_kind \
              AND sv.logical_id = e.logical_id \
             WHERE e.version_id = $1 \
               AND e.embedding IS NOT NULL \
               AND (SELECT embedding FROM q) IS NOT NULL \
               AND NOT (e.entity_kind = $2::ontology_entity_kind AND e.logical_id = $3) \
             ORDER BY e.embedding <=> (SELECT embedding FROM q) \
             LIMIT $4",
        )
        .bind(version_id)
        .bind(entity_kind)
        .bind(logical_id)
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

/// Λ-10 — Level 3 populator. Called at the end of
/// `commit_version` inside the same transaction. Fans the IR's
/// already-assembled entities into the per-kind flat indexes,
/// the `entity_neighbors` 1-hop graph, and the hierarchical
/// closure table.
///
/// The `entity_hash` column on flat rows points at the OWNER's
/// hash in Level 2 — for nested entities (Property inside
/// NodeType / EdgeType; CodedValue inside CodeSystem) the hash
/// is the parent's, since Level 2 stores the parent as the
/// single immutable unit.
///
/// Embedding rows are NOT populated here. Embedding population
/// is async (Gemini API round trip), handled by a separate
/// background task that fills `ontology_entity_embedding`
/// rows when they land.
async fn materialize_level3(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    use ox_ontology::storage::extract_entities;

    let entities = extract_entities(ir)?;
    // Build a quick `(kind, logical_id) → hash` lookup so
    // neighbour edges reference the right hash without a second
    // extract pass.
    let hash_by_id: std::collections::HashMap<
        (ox_ontology::storage::EntityKind, String),
        String,
    > = entities
        .iter()
        .map(|e| ((e.kind, e.logical_id.clone()), e.hash.clone()))
        .collect();

    // ------------------------------------------------------------
    // (A) Flat per-kind indexes
    // ------------------------------------------------------------

    // node_type
    for nt in ir.node_types() {
        let hash = hash_by_id
            .get(&(
                ox_ontology::storage::EntityKind::NodeType,
                nt.id.to_string(),
            ))
            .cloned()
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO ontology_node_type_index \
                (version_id, logical_id, entity_hash, label, deprecated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(nt.id.as_str())
        .bind(&hash)
        .bind(nt.label.as_str())
        .bind(nt.deprecated_at)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;

        // property (nested inside node_type)
        for prop in &nt.properties {
            insert_property_row(
                tx,
                version_id,
                "node_type",
                nt.id.as_str(),
                &hash,
                prop,
            )
            .await?;
        }
    }

    // edge_type
    for et in ir.edge_types() {
        let hash = hash_by_id
            .get(&(
                ox_ontology::storage::EntityKind::EdgeType,
                et.id.to_string(),
            ))
            .cloned()
            .unwrap_or_default();
        sqlx::query(
            "INSERT INTO ontology_edge_type_index \
                (version_id, logical_id, entity_hash, label, \
                 source_type_id, target_type_id, deprecated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(et.id.as_str())
        .bind(&hash)
        .bind(et.label.as_str())
        .bind(et.source_node_id.as_str())
        .bind(et.target_node_id.as_str())
        .bind(et.deprecated_at)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;

        for prop in &et.properties {
            insert_property_row(
                tx,
                version_id,
                "edge_type",
                et.id.as_str(),
                &hash,
                prop,
            )
            .await?;
        }
    }

    // interface
    for iface in ir.interfaces() {
        let hash = hash_for(&hash_by_id, ox_ontology::storage::EntityKind::Interface, &iface.id);
        sqlx::query(
            "INSERT INTO ontology_interface_index \
                (version_id, logical_id, entity_hash, label) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(iface.id.as_str())
        .bind(&hash)
        .bind(iface.label.as_str())
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // object_mapping
    for om in ir.object_mappings() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ObjectMapping,
            &om.id,
        );
        sqlx::query(
            "INSERT INTO ontology_object_mapping_index \
                (version_id, logical_id, entity_hash, node_type_id, \
                 source_id, precedence) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(om.id.as_str())
        .bind(&hash)
        .bind(om.node_type_id.as_str())
        .bind(om.source_id.as_str())
        .bind(om.precedence as i16)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // link_mapping
    for lm in ir.link_mappings() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::LinkMapping,
            &lm.id,
        );
        let kind_tag = match &lm.kind {
            ox_ontology::mapping::LinkMappingKind::ForeignKey { .. } => "foreign_key",
            ox_ontology::mapping::LinkMappingKind::Bridge { .. } => "bridge",
            ox_ontology::mapping::LinkMappingKind::Computed { .. } => "computed",
            ox_ontology::mapping::LinkMappingKind::Federated { .. } => "federated",
        };
        let cardinality = match lm.cardinality {
            ox_ontology::mapping::LinkCardinality::OneToOne => "one_to_one",
            ox_ontology::mapping::LinkCardinality::OneToMany => "one_to_many",
            ox_ontology::mapping::LinkCardinality::ManyToOne => "many_to_one",
            ox_ontology::mapping::LinkCardinality::ManyToMany => "many_to_many",
        };
        sqlx::query(
            "INSERT INTO ontology_link_mapping_index \
                (version_id, logical_id, entity_hash, edge_type_id, \
                 kind, cardinality, precedence) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(lm.id.as_str())
        .bind(&hash)
        .bind(lm.edge_type_id.as_str())
        .bind(kind_tag)
        .bind(cardinality)
        .bind(lm.precedence as i16)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // code_system + nested coded_value
    for cs in ir.code_systems() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::CodeSystem,
            &cs.id,
        );
        let kind_tag = match cs.kind {
            ox_ontology::code_system::CodeSystemKind::Internal => "internal",
            ox_ontology::code_system::CodeSystemKind::External { .. } => "external",
        };
        sqlx::query(
            "INSERT INTO ontology_code_system_index \
                (version_id, logical_id, entity_hash, name, uri, kind, hierarchical) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(cs.id.as_str())
        .bind(&hash)
        .bind(&cs.name)
        .bind(cs.uri.as_deref())
        .bind(kind_tag)
        .bind(cs.hierarchical)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;

        for cv in &cs.codes {
            sqlx::query(
                "INSERT INTO ontology_coded_value_index \
                    (version_id, logical_id, entity_hash, code_system_id, \
                     code, broader_id, deprecated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(version_id)
            .bind(cv.id.as_str())
            .bind(&hash)
            .bind(cs.id.as_str())
            .bind(&cv.code)
            .bind(cv.broader_id.as_ref().map(|id| id.as_str()))
            .bind(cv.deprecated_at)
            .execute(&mut **tx)
            .await
            .map_err(to_ox_error)?;
        }
    }

    // value_set
    for vs in ir.value_sets() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ValueSet,
            &vs.id,
        );
        sqlx::query(
            "INSERT INTO ontology_value_set_index \
                (version_id, logical_id, entity_hash, name) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(vs.id.as_str())
        .bind(&hash)
        .bind(&vs.name)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // notation_pattern
    for np in ir.notation_patterns() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::NotationPattern,
            &np.id,
        );
        sqlx::query(
            "INSERT INTO ontology_notation_pattern_index \
                (version_id, logical_id, entity_hash, name) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(np.id.as_str())
        .bind(&hash)
        .bind(&np.name)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // concept_map
    for cm in ir.concept_maps() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ConceptMap,
            &cm.id,
        );
        sqlx::query(
            "INSERT INTO ontology_concept_map_index \
                (version_id, logical_id, entity_hash, name, \
                 source_system_id, target_system_id) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(cm.id.as_str())
        .bind(&hash)
        .bind(&cm.name)
        .bind(cm.source_system_id.as_str())
        .bind(cm.target_system_id.as_str())
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // value_range_set
    for rs in ir.value_range_sets() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::ValueRangeSet,
            &rs.id,
        );
        sqlx::query(
            "INSERT INTO ontology_value_range_set_index \
                (version_id, logical_id, entity_hash, name) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(version_id)
        .bind(rs.id.as_str())
        .bind(&hash)
        .bind(&rs.name)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // glossary_term
    for term in ir.glossary() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::GlossaryTerm,
            &term.id,
        );
        sqlx::query(
            "INSERT INTO ontology_glossary_term_index \
                (version_id, logical_id, entity_hash, term, category, parent_term_id) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(version_id)
        .bind(term.id.as_str())
        .bind(&hash)
        .bind(&term.term)
        .bind(term.category.as_deref())
        .bind(term.parent_term_id.as_ref().map(|id| id.as_str()))
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // rule
    for rule in ir.rules() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Rule,
            &rule.id,
        );
        let kind_tag = match &rule.kind {
            ox_ontology::rule::RuleKind::NodeShape { .. } => "node_shape",
            ox_ontology::rule::RuleKind::PropertyShape { .. } => "property_shape",
            ox_ontology::rule::RuleKind::EdgeShape { .. } => "edge_shape",
            ox_ontology::rule::RuleKind::CrossEntityShape { .. } => "cross_entity_shape",
            ox_ontology::rule::RuleKind::StateMachine { .. } => "state_machine",
        };
        let severity_tag = match rule.severity {
            ox_ontology::rule::Severity::Violation => "violation",
            ox_ontology::rule::Severity::Warning => "warning",
            ox_ontology::rule::Severity::Info => "info",
        };
        sqlx::query(
            "INSERT INTO ontology_rule_index \
                (version_id, logical_id, entity_hash, kind, severity) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(rule.id.as_str())
        .bind(&hash)
        .bind(kind_tag)
        .bind(severity_tag)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // function
    for func in ir.functions() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Function,
            &func.id,
        );
        let purity_tag = match func.purity {
            ox_ontology::function::FunctionPurity::Pure => "pure",
            ox_ontology::function::FunctionPurity::Impure => "impure",
        };
        sqlx::query(
            "INSERT INTO ontology_function_index \
                (version_id, logical_id, entity_hash, name, purity) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(func.id.as_str())
        .bind(&hash)
        .bind(&func.name)
        .bind(purity_tag)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // metric
    for metric in ir.metrics() {
        let hash = hash_for(
            &hash_by_id,
            ox_ontology::storage::EntityKind::Metric,
            &metric.id,
        );
        let grain_tag = match metric.temporal_grain {
            ox_ontology::metric::TemporalGrain::Snapshot => "snapshot",
            ox_ontology::metric::TemporalGrain::Daily => "daily",
            ox_ontology::metric::TemporalGrain::Weekly => "weekly",
            ox_ontology::metric::TemporalGrain::Monthly => "monthly",
            ox_ontology::metric::TemporalGrain::Quarterly => "quarterly",
            ox_ontology::metric::TemporalGrain::Yearly => "yearly",
        };
        sqlx::query(
            "INSERT INTO ontology_metric_index \
                (version_id, logical_id, entity_hash, name, temporal_grain) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(version_id)
        .bind(metric.id.as_str())
        .bind(&hash)
        .bind(&metric.name)
        .bind(grain_tag)
        .execute(&mut **tx)
        .await
        .map_err(to_ox_error)?;
    }

    // ------------------------------------------------------------
    // (B) Neighbor edges — cross-references between entities.
    // ------------------------------------------------------------

    insert_neighbors_from_ir(tx, version_id, ir).await?;

    // ------------------------------------------------------------
    // (C) Hierarchical closure — code_system broader, glossary
    //     parent, interface implements.
    // ------------------------------------------------------------

    insert_hierarchy_closure(tx, version_id, ir).await?;

    // ------------------------------------------------------------
    // (D) Search vectors — flattened text + tsvector.
    // ------------------------------------------------------------

    insert_search_vectors(tx, version_id, ir).await?;

    Ok(())
}

/// Lookup helper for the `(kind, logical_id) → hash` cache built
/// at the start of `materialize_level3`. Missing entries return
/// an empty string, which then fails the FK check on the flat
/// insert — defensive: if the hash cache is out of sync with the
/// IR it is better to fail loudly here than to insert a flat row
/// pointing at nothing.
fn hash_for(
    cache: &std::collections::HashMap<(ox_ontology::storage::EntityKind, String), String>,
    kind: ox_ontology::storage::EntityKind,
    id: &impl ToString,
) -> String {
    cache
        .get(&(kind, id.to_string()))
        .cloned()
        .unwrap_or_default()
}

/// Insert one property row. Property is nested at the IR level,
/// so `owner_hash` is the NodeType / EdgeType's hash (the
/// content-addressed unit that owns this property).
#[allow(clippy::too_many_arguments)]
async fn insert_property_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    owner_kind: &str,
    owner_logical_id: &str,
    owner_hash: &str,
    prop: &ox_ontology::ir::PropertyDef,
) -> OxResult<()> {
    let property_type_tag = match &prop.property_type {
        ox_core::types::PropertyType::Bool => "bool",
        ox_core::types::PropertyType::Int => "int",
        ox_core::types::PropertyType::Float => "float",
        ox_core::types::PropertyType::String => "string",
        ox_core::types::PropertyType::Date => "date",
        ox_core::types::PropertyType::DateTime => "datetime",
        ox_core::types::PropertyType::Duration => "duration",
        ox_core::types::PropertyType::Bytes => "bytes",
        ox_core::types::PropertyType::List { .. } => "list",
        ox_core::types::PropertyType::Map => "map",
    };
    let aggregation_role_tag = prop.aggregation_role.map(|r| match r {
        ox_ontology::ir::AggregationRole::Measure => "measure",
        ox_ontology::ir::AggregationRole::Dimension => "dimension",
        ox_ontology::ir::AggregationRole::Attribute => "attribute",
        ox_ontology::ir::AggregationRole::Identifier => "identifier",
    });
    let semantic_type_tag = prop.semantic_type.as_ref().map(|st| match st {
        ox_ontology::ir::SemanticType::Email => "email".to_string(),
        ox_ontology::ir::SemanticType::Phone => "phone".to_string(),
        ox_ontology::ir::SemanticType::Url => "url".to_string(),
        ox_ontology::ir::SemanticType::Address => "address".to_string(),
        ox_ontology::ir::SemanticType::Coordinate => "coordinate".to_string(),
        ox_ontology::ir::SemanticType::Currency => "currency".to_string(),
        ox_ontology::ir::SemanticType::Percentage => "percentage".to_string(),
        ox_ontology::ir::SemanticType::Iso8601 => "iso8601".to_string(),
        ox_ontology::ir::SemanticType::LocalizedText => "localized_text".to_string(),
        ox_ontology::ir::SemanticType::Other(s) => format!("other:{s}"),
    });
    let pii_kind_tag = prop.pii_kind.as_ref().map(|k| {
        // Use the enum's tag-only rendering. `serde_json::to_value`
        // on an internally-tagged enum produces {"kind": "...", ...}
        // — we pull the tag out for the flat index.
        serde_json::to_value(k)
            .ok()
            .and_then(|v| v.get("kind").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_else(|| "unknown".into())
    });

    sqlx::query(
        "INSERT INTO ontology_property_index \
            (version_id, owner_kind, owner_logical_id, logical_id, \
             entity_hash, key, property_type, nullable, is_localized, \
             aggregation_role, value_set_id, notation_pattern_id, \
             value_range_set_id, semantic_type, pii_kind, unit_id, \
             glossary_term_id, deprecated_at) \
         VALUES ($1, $2::ontology_entity_kind, $3, $4, $5, $6, $7, $8, $9, \
                 $10, $11, $12, $13, $14, $15, $16, $17, $18)",
    )
    .bind(version_id)
    .bind(owner_kind)
    .bind(owner_logical_id)
    .bind(prop.id.as_str())
    .bind(owner_hash)
    .bind(prop.name.as_str())
    .bind(property_type_tag)
    .bind(prop.nullable)
    .bind(prop.is_localized)
    .bind(aggregation_role_tag)
    .bind(prop.value_set_id.as_ref().map(|id| id.as_str()))
    .bind(prop.notation_pattern_id.as_ref().map(|id| id.as_str()))
    .bind(prop.value_range_set_id.as_ref().map(|id| id.as_str()))
    .bind(semantic_type_tag)
    .bind(pii_kind_tag)
    .bind(prop.unit_id.as_ref().map(|id| id.as_str()))
    .bind(prop.glossary_term_id.as_ref().map(|id| id.as_str()))
    .bind(prop.deprecated_at)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;
    Ok(())
}

/// Harvest cross-entity references from the IR and emit 1-hop
/// neighbor edges. Kept as a free function rather than expanding
/// `materialize_level3` further so the edge-kind taxonomy is in
/// one place.
async fn insert_neighbors_from_ir(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    let mut from_kinds: Vec<&str> = Vec::new();
    let mut from_ids: Vec<String> = Vec::new();
    let mut to_kinds: Vec<&str> = Vec::new();
    let mut to_ids: Vec<String> = Vec::new();
    let mut relations: Vec<&str> = Vec::new();

    let mut push = |fk: &'static str, fi: &str, tk: &'static str, ti: &str, rk: &'static str| {
        from_kinds.push(fk);
        from_ids.push(fi.to_string());
        to_kinds.push(tk);
        to_ids.push(ti.to_string());
        relations.push(rk);
    };

    // Property → value_set / notation_pattern / value_range_set /
    // glossary_term / unit (coded_value).
    let walk_properties = |props: &[ox_ontology::ir::PropertyDef], cb: &mut dyn FnMut(&ox_ontology::ir::PropertyDef)| {
        for p in props {
            cb(p);
        }
    };

    // `push` is an FnMut closure that borrows the vecs; we call
    // it from the property walk below.
    let mut on_prop = |prop: &ox_ontology::ir::PropertyDef| {
        if let Some(vs_id) = &prop.value_set_id {
            push("property", prop.id.as_str(), "value_set", vs_id.as_str(), "references_value_set");
        }
        if let Some(np_id) = &prop.notation_pattern_id {
            push("property", prop.id.as_str(), "notation_pattern", np_id.as_str(), "references_notation_pattern");
        }
        if let Some(rs_id) = &prop.value_range_set_id {
            push("property", prop.id.as_str(), "value_range_set", rs_id.as_str(), "references_value_range_set");
        }
        if let Some(gt_id) = &prop.glossary_term_id {
            push("property", prop.id.as_str(), "glossary_term", gt_id.as_str(), "references_glossary_term");
        }
        if let Some(unit_id) = &prop.unit_id {
            push("property", prop.id.as_str(), "coded_value", unit_id.as_str(), "uses_unit");
        }
        if let Some(fn_id) = &prop.derived_from {
            push("property", prop.id.as_str(), "function", fn_id.as_str(), "derived_from");
        }
    };
    for nt in ir.node_types() {
        walk_properties(&nt.properties, &mut on_prop);
    }
    for et in ir.edge_types() {
        walk_properties(&et.properties, &mut on_prop);
    }

    // ObjectMapping → NodeType
    for om in ir.object_mappings() {
        push("object_mapping", om.id.as_str(), "node_type", om.node_type_id.as_str(), "maps_node_type");
    }

    // LinkMapping → EdgeType
    for lm in ir.link_mappings() {
        push("link_mapping", lm.id.as_str(), "edge_type", lm.edge_type_id.as_str(), "maps_edge_type");
    }

    // ConceptMap → source_system / target_system
    for cm in ir.concept_maps() {
        push("concept_map", cm.id.as_str(), "code_system", cm.source_system_id.as_str(), "concept_map_source");
        push("concept_map", cm.id.as_str(), "code_system", cm.target_system_id.as_str(), "concept_map_target");
    }

    // ValueSet → CodeSystem (composition rules)
    for vs in ir.value_sets() {
        for rule in &vs.composition {
            push(
                "value_set",
                vs.id.as_str(),
                "code_system",
                rule.system_id.as_str(),
                "value_set_includes_system",
            );
        }
    }

    if from_kinds.is_empty() {
        return Ok(());
    }

    let from_kinds_owned: Vec<String> = from_kinds.iter().map(|s| s.to_string()).collect();
    let to_kinds_owned: Vec<String> = to_kinds.iter().map(|s| s.to_string()).collect();
    let relations_owned: Vec<String> = relations.iter().map(|s| s.to_string()).collect();

    sqlx::query(
        "INSERT INTO ontology_entity_neighbors \
            (version_id, from_kind, from_logical_id, to_kind, to_logical_id, relation_kind) \
         SELECT $1, fk.fkind::ontology_entity_kind, fk.fid, \
                    fk.tkind::ontology_entity_kind, fk.tid, fk.rk \
         FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[], $6::text[]) \
              AS fk(fkind, fid, tkind, tid, rk) \
         ON CONFLICT DO NOTHING",
    )
    .bind(version_id)
    .bind(&from_kinds_owned)
    .bind(&from_ids)
    .bind(&to_kinds_owned)
    .bind(&to_ids)
    .bind(&relations_owned)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    Ok(())
}

/// Materialise the hierarchical closure. Three relations today:
///
///   code_system_broader      CodedValue.broader_id inside a
///                            hierarchical CodeSystem.
///   glossary_term_parent     GlossaryTermDef.parent_term_id.
///   interface_implements     NodeType.implements → Interface.
///
/// Closure is built in-memory via iterative fixpoint. Input
/// sizes are small (low thousands), so the O(n²) worst case is
/// fine; a future enterprise-scale growth would migrate this
/// to a recursive CTE stored proc.
async fn insert_hierarchy_closure(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    // Each row: (relation_kind, ancestor_kind, ancestor_id,
    // descendant_kind, descendant_id, depth).
    let mut rows: Vec<(String, String, String, String, String, i32)> = Vec::new();

    // 1) code_system_broader — walk CodedValue.broader_id per system.
    for cs in ir.code_systems() {
        if !cs.hierarchical {
            continue;
        }
        // Build immediate parent map.
        let parent_of: std::collections::HashMap<&str, &str> = cs
            .codes
            .iter()
            .filter_map(|cv| cv.broader_id.as_ref().map(|b| (cv.id.as_str(), b.as_str())))
            .collect();
        for cv in &cs.codes {
            // self — depth 0
            rows.push((
                "code_system_broader".into(),
                "coded_value".into(),
                cv.id.to_string(),
                "coded_value".into(),
                cv.id.to_string(),
                0,
            ));
            // Walk ancestors.
            let mut current = cv.id.as_str();
            let mut depth = 1;
            let limit = cs.codes.len() + 1;
            let mut guard = 0;
            while let Some(parent) = parent_of.get(current) {
                rows.push((
                    "code_system_broader".into(),
                    "coded_value".into(),
                    parent.to_string(),
                    "coded_value".into(),
                    cv.id.to_string(),
                    depth,
                ));
                current = parent;
                depth += 1;
                guard += 1;
                if guard >= limit {
                    break; // cycle guard
                }
            }
        }
    }

    // 2) glossary_term_parent — walk GlossaryTermDef.parent_term_id.
    let terms: Vec<_> = ir.glossary().iter().collect();
    let parent_map: std::collections::HashMap<&str, &str> = terms
        .iter()
        .filter_map(|t| t.parent_term_id.as_ref().map(|p| (t.id.as_str(), p.as_str())))
        .collect();
    for term in &terms {
        rows.push((
            "glossary_term_parent".into(),
            "glossary_term".into(),
            term.id.to_string(),
            "glossary_term".into(),
            term.id.to_string(),
            0,
        ));
        let mut current = term.id.as_str();
        let mut depth = 1;
        let limit = terms.len() + 1;
        let mut guard = 0;
        while let Some(parent) = parent_map.get(current) {
            rows.push((
                "glossary_term_parent".into(),
                "glossary_term".into(),
                parent.to_string(),
                "glossary_term".into(),
                term.id.to_string(),
                depth,
            ));
            current = parent;
            depth += 1;
            guard += 1;
            if guard >= limit {
                break;
            }
        }
    }

    // 3) interface_implements — NodeType → Interface for each of
    //    the node's `implements` entries. NodeTypeDef's
    //    `implements` field holds `Vec<InterfaceId>`.
    for nt in ir.node_types() {
        for iface_id in &nt.implements {
            rows.push((
                "interface_implements".into(),
                "node_type".into(),
                nt.id.to_string(),
                "interface".into(),
                iface_id.to_string(),
                1,
            ));
        }
    }

    if rows.is_empty() {
        return Ok(());
    }

    // Bulk insert via UNNEST of six parallel arrays.
    let mut rel: Vec<String> = Vec::with_capacity(rows.len());
    let mut ak: Vec<String> = Vec::with_capacity(rows.len());
    let mut ai: Vec<String> = Vec::with_capacity(rows.len());
    let mut dk: Vec<String> = Vec::with_capacity(rows.len());
    let mut di: Vec<String> = Vec::with_capacity(rows.len());
    let mut dp: Vec<i32> = Vec::with_capacity(rows.len());
    for r in rows {
        rel.push(r.0);
        ak.push(r.1);
        ai.push(r.2);
        dk.push(r.3);
        di.push(r.4);
        dp.push(r.5);
    }
    sqlx::query(
        "INSERT INTO ontology_entity_hierarchy \
            (version_id, relation_kind, ancestor_kind, ancestor_logical_id, \
             descendant_kind, descendant_logical_id, depth) \
         SELECT $1, \
                r.rel, \
                r.ak::ontology_entity_kind, r.ai, \
                r.dk::ontology_entity_kind, r.di, \
                r.dp \
         FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::int[]) \
              AS r(rel, ak, ai, dk, di, dp) \
         ON CONFLICT DO NOTHING",
    )
    .bind(version_id)
    .bind(&rel)
    .bind(&ak)
    .bind(&ai)
    .bind(&dk)
    .bind(&di)
    .bind(&dp)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    Ok(())
}

/// Build the `ontology_entity_search_vector` row per entity.
/// `doc` is the concatenated searchable text; `tsv` is
/// `to_tsvector('simple', doc)`. `simple` dictionary preserves
/// exact tokens across the mixed-language content our customers
/// author.
async fn insert_search_vectors(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
    ir: &ox_ontology::OntologyIR,
) -> OxResult<()> {
    // Per-entity docs. The ontology_header row covers the
    // ontology-level searchable text (name + description).
    let mut kinds: Vec<String> = Vec::new();
    let mut lids: Vec<String> = Vec::new();
    let mut docs: Vec<String> = Vec::new();

    let mut emit = |kind: &'static str, lid: &str, doc: String| {
        kinds.push(kind.to_string());
        lids.push(lid.to_string());
        docs.push(doc);
    };

    let localized_flat = |t: &ox_core::i18n::LocalizedText| {
        // Default + every translation joined with spaces. Skips
        // empty strings so the docvector doesn't inflate with
        // whitespace.
        let mut parts = Vec::new();
        if !t.default.is_empty() {
            parts.push(t.default.clone());
        }
        for v in t.translations.values() {
            if !v.is_empty() {
                parts.push(v.clone());
            }
        }
        parts.join(" ")
    };

    emit(
        "ontology_header",
        &ir.id,
        format!("{} {}", ir.name, localized_flat(&ir.description)),
    );

    for nt in ir.node_types() {
        emit(
            "node_type",
            nt.id.as_str(),
            format!(
                "{} {}",
                nt.label.as_str(),
                localized_flat(&nt.description)
            ),
        );
        for prop in &nt.properties {
            let aliases = prop
                .aliases
                .iter()
                .map(localized_flat)
                .collect::<Vec<_>>()
                .join(" ");
            emit(
                "property",
                prop.id.as_str(),
                format!(
                    "{} {} {} {}",
                    prop.name.as_str(),
                    localized_flat(&prop.display_name),
                    aliases,
                    localized_flat(&prop.description)
                ),
            );
        }
    }
    for et in ir.edge_types() {
        emit(
            "edge_type",
            et.id.as_str(),
            format!(
                "{} {}",
                et.label.as_str(),
                localized_flat(&et.description)
            ),
        );
    }
    for cs in ir.code_systems() {
        emit(
            "code_system",
            cs.id.as_str(),
            format!(
                "{} {} {}",
                cs.name,
                localized_flat(&cs.display_name),
                localized_flat(&cs.description)
            ),
        );
        for cv in &cs.codes {
            let alias = cv.aliases.join(" ");
            emit(
                "coded_value",
                cv.id.as_str(),
                format!(
                    "{} {} {} {} {}",
                    cv.code,
                    localized_flat(&cv.display),
                    localized_flat(&cv.definition),
                    alias,
                    localized_flat(&cv.scope_note)
                ),
            );
        }
    }
    for vs in ir.value_sets() {
        emit(
            "value_set",
            vs.id.as_str(),
            format!(
                "{} {} {}",
                vs.name,
                localized_flat(&vs.display_name),
                localized_flat(&vs.description)
            ),
        );
    }
    for np in ir.notation_patterns() {
        emit(
            "notation_pattern",
            np.id.as_str(),
            format!(
                "{} {} {}",
                np.name,
                localized_flat(&np.display_name),
                localized_flat(&np.description)
            ),
        );
    }
    for term in ir.glossary() {
        let aliases = term.aliases.join(" ");
        emit(
            "glossary_term",
            term.id.as_str(),
            format!(
                "{} {} {} {}",
                term.term,
                localized_flat(&term.display_name),
                aliases,
                localized_flat(&term.description)
            ),
        );
    }

    if kinds.is_empty() {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO ontology_entity_search_vector \
            (version_id, entity_kind, logical_id, doc, tsv) \
         SELECT $1, k::ontology_entity_kind, l, d, to_tsvector('simple', d) \
         FROM UNNEST($2::text[], $3::text[], $4::text[]) AS s(k, l, d)",
    )
    .bind(version_id)
    .bind(&kinds)
    .bind(&lids)
    .bind(&docs)
    .execute(&mut **tx)
    .await
    .map_err(to_ox_error)?;

    Ok(())
}

/// Assemble an `OntologyIR` from a flat list of
/// `(kind, logical_id, hash, content)` rows. Groups rows by kind
/// and routes each group into the matching IR collection. The
/// header row produces the outer IR struct.
///
/// Returns `OxResult` so a malformed stored row (kind that
/// doesn't parse, content that doesn't deserialise, missing
/// header) surfaces with a specific error — downstream callers
/// map to a 500 rather than silently filling a half-empty IR.
fn assemble_ir(
    rows: &[crate::models::OntologyEntityJoinRow],
) -> OxResult<ox_ontology::OntologyIR> {
    use ox_ontology::storage::EntityKind;

    let mut header: Option<serde_json::Value> = None;
    let mut node_types: Vec<ox_ontology::ir::NodeTypeDef> = Vec::new();
    let mut edge_types: Vec<ox_ontology::ir::EdgeTypeDef> = Vec::new();
    let mut indexes: Vec<ox_ontology::ir::IndexDef> = Vec::new();
    let mut interfaces: Vec<ox_ontology::interface::InterfaceDef> = Vec::new();
    let mut object_mappings: Vec<ox_ontology::mapping::ObjectMappingDef> = Vec::new();
    let mut link_mappings: Vec<ox_ontology::mapping::LinkMappingDef> = Vec::new();
    let mut rules: Vec<ox_ontology::rule::RuleDef> = Vec::new();
    let mut data_quality: Vec<ox_ontology::data_quality::DataQualityDef> = Vec::new();
    let mut actions: Vec<ox_ontology::action::ActionDef> = Vec::new();
    let mut provenance: Vec<ox_ontology::provenance::ProvenanceDef> = Vec::new();
    let mut functions: Vec<ox_ontology::function::FunctionDef> = Vec::new();
    let mut metrics: Vec<ox_ontology::metric::MetricDef> = Vec::new();
    let mut enrichments: Vec<ox_ontology::enrichment::EnrichmentDef> = Vec::new();
    let mut glossary: Vec<ox_ontology::glossary::GlossaryTermDef> = Vec::new();
    let mut code_systems: Vec<ox_ontology::code_system::CodeSystemDef> = Vec::new();
    let mut value_sets: Vec<ox_ontology::value_set::ValueSetDef> = Vec::new();
    let mut notation_patterns: Vec<ox_ontology::notation_pattern::NotationPatternDef> =
        Vec::new();
    let mut concept_maps: Vec<ox_ontology::concept_map::ConceptMapDef> = Vec::new();
    let mut value_range_sets: Vec<ox_ontology::value_range::ValueRangeSetDef> = Vec::new();

    for row in rows {
        let kind = EntityKind::parse(&row.entity_kind)?;
        match kind {
            EntityKind::OntologyHeader => {
                header = Some(row.content.clone());
            }
            EntityKind::NodeType => node_types.push(serde_json::from_value(row.content.clone())?),
            EntityKind::EdgeType => edge_types.push(serde_json::from_value(row.content.clone())?),
            EntityKind::IndexDef => indexes.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Interface => interfaces.push(serde_json::from_value(row.content.clone())?),
            EntityKind::ObjectMapping => {
                object_mappings.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::LinkMapping => {
                link_mappings.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::PropertyMapping => {
                // PropertyMappingDef is nested inside ObjectMappingDef in
                // the current IR — it rides along with its parent. When
                // the IR model promotes it to a top-level collection,
                // this arm routes into the new vector.
            }
            EntityKind::Rule => rules.push(serde_json::from_value(row.content.clone())?),
            EntityKind::DataQuality => {
                data_quality.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Action => actions.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Provenance => {
                provenance.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Function => functions.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Metric => metrics.push(serde_json::from_value(row.content.clone())?),
            EntityKind::Enrichment => {
                enrichments.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::GlossaryTerm => {
                glossary.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::Taxonomy => {
                // Same deferral as PropertyMapping — not yet an
                // independent IR collection. Lands when the IR model
                // promotes Taxonomy out of the glossary module.
            }
            EntityKind::CodeSystem => {
                code_systems.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::ValueSet => value_sets.push(serde_json::from_value(row.content.clone())?),
            EntityKind::NotationPattern => {
                notation_patterns.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::ConceptMap => {
                concept_maps.push(serde_json::from_value(row.content.clone())?)
            }
            EntityKind::ValueRangeSet => {
                value_range_sets.push(serde_json::from_value(row.content.clone())?)
            }
        }
    }

    // Header parse — must be present exactly once. Deserialising it
    // gives the outer-struct scalars (id, name, description, version,
    // schema_version).
    let header = header.ok_or_else(|| OxError::Runtime {
        message: "version pointer set is missing the ontology_header entity".into(),
    })?;
    #[derive(serde::Deserialize)]
    struct HeaderWire {
        id: String,
        name: String,
        #[serde(default)]
        description: ox_core::i18n::LocalizedText,
        version: ox_ontology::ir::OntologyVersion,
        #[serde(default)]
        schema_version: u32,
    }
    let h: HeaderWire = serde_json::from_value(header)?;
    let _ = h.schema_version; // the current build's version is authoritative

    let mut ir = ox_ontology::OntologyIR::try_new(
        h.id,
        h.name,
        h.description,
        h.version,
        node_types,
        edge_types,
        indexes,
    )
    .map_err(|e| OxError::Runtime {
        message: format!("OntologyIR::try_new rejected rebuilt topology: {e:?}"),
    })?;

    for iface in interfaces {
        ir.add_interface(iface).map_err(|e| OxError::Runtime {
            message: format!("add_interface during hydration: {e:?}"),
        })?;
    }
    for om in object_mappings {
        ir.add_object_mapping(om).map_err(|e| OxError::Runtime {
            message: format!("add_object_mapping during hydration: {e:?}"),
        })?;
    }
    for lm in link_mappings {
        ir.add_link_mapping(lm).map_err(|e| OxError::Runtime {
            message: format!("add_link_mapping during hydration: {e:?}"),
        })?;
    }
    for rule in rules {
        ir.add_rule(rule).map_err(|e| OxError::Runtime {
            message: format!("add_rule during hydration: {e:?}"),
        })?;
    }
    for dq in data_quality {
        ir.add_data_quality(dq).map_err(|e| OxError::Runtime {
            message: format!("add_data_quality during hydration: {e:?}"),
        })?;
    }
    for action in actions {
        ir.add_action(action).map_err(|e| OxError::Runtime {
            message: format!("add_action during hydration: {e:?}"),
        })?;
    }
    for prov in provenance {
        ir.add_provenance(prov);
    }
    for f in functions {
        ir.add_function(f).map_err(|e| OxError::Runtime {
            message: format!("add_function during hydration: {e:?}"),
        })?;
    }
    for m in metrics {
        ir.add_metric(m).map_err(|e| OxError::Runtime {
            message: format!("add_metric during hydration: {e:?}"),
        })?;
    }
    for e in enrichments {
        ir.add_enrichment(e).map_err(|err| OxError::Runtime {
            message: format!("add_enrichment during hydration: {err:?}"),
        })?;
    }
    for term in glossary {
        ir.add_glossary_term(term).map_err(|e| OxError::Runtime {
            message: format!("add_glossary_term during hydration: {e:?}"),
        })?;
    }
    for cs in code_systems {
        ir.add_code_system(cs).map_err(|e| OxError::Runtime {
            message: format!("add_code_system during hydration: {e:?}"),
        })?;
    }
    for vs in value_sets {
        ir.add_value_set(vs).map_err(|e| OxError::Runtime {
            message: format!("add_value_set during hydration: {e:?}"),
        })?;
    }
    for np in notation_patterns {
        ir.add_notation_pattern(np).map_err(|e| OxError::Runtime {
            message: format!("add_notation_pattern during hydration: {e:?}"),
        })?;
    }
    for cm in concept_maps {
        ir.add_concept_map(cm).map_err(|e| OxError::Runtime {
            message: format!("add_concept_map during hydration: {e:?}"),
        })?;
    }
    for rs in value_range_sets {
        ir.add_value_range_set(rs).map_err(|e| OxError::Runtime {
            message: format!("add_value_range_set during hydration: {e:?}"),
        })?;
    }

    Ok(ir)
}

// ---------------------------------------------------------------------------
// AmbiguityStore — detector outputs + resolver history
// ---------------------------------------------------------------------------
//
// Schema: see `migrations/0024_ambiguity.sql`. All access is workspace-scoped
// via RLS; the active-resolution invariant (one per context) is enforced by
// a partial unique index plus an in-transaction revoke step on create.

fn ambiguity_context_from_row(row: &sqlx::postgres::PgRow) -> OxResult<ox_ontology::ambiguity::AmbiguityContext> {
    use sqlx::Row;
    let id_text: &str = row.try_get("id").map_err(to_ox_error)?;
    let id_uuid: uuid::Uuid = id_text.parse().map_err(|e: uuid::Error| OxError::Runtime {
        message: format!("ambiguity context id parse: {e}"),
    })?;
    let source_id: &str = row.try_get("source_id").map_err(to_ox_error)?;
    let relation: &str = row.try_get("relation").map_err(to_ox_error)?;
    let column_name: &str = row.try_get("column_name").map_err(to_ox_error)?;
    let kind_text: &str = row.try_get("kind").map_err(to_ox_error)?;
    let kind = match kind_text {
        "numeric_code" => ox_ontology::ambiguity::AmbiguityKind::NumericCode,
        "opaque_short_code" => ox_ontology::ambiguity::AmbiguityKind::OpaqueShortCode,
        "overloaded_name" => ox_ontology::ambiguity::AmbiguityKind::OverloadedName,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown ambiguity kind in DB row: {other}"),
            });
        }
    };
    let sample_values_json: serde_json::Value =
        row.try_get("sample_values").map_err(to_ox_error)?;
    let sample_values: Vec<String> =
        serde_json::from_value(sample_values_json).map_err(|e| OxError::Runtime {
            message: format!("sample_values decode: {e}"),
        })?;
    let distinct_estimate: Option<i64> = row.try_get("distinct_estimate").map_err(to_ox_error)?;
    let nullable: bool = row.try_get("nullable").map_err(to_ox_error)?;
    let clarification_prompt: &str =
        row.try_get("clarification_prompt").map_err(to_ox_error)?;
    let detection_source_hash: &str =
        row.try_get("detection_source_hash").map_err(to_ox_error)?;
    let repo_hint_json: Option<serde_json::Value> =
        row.try_get("repo_hint").map_err(to_ox_error)?;
    let repo_hint = match repo_hint_json {
        Some(v) => Some(
            serde_json::from_value::<ox_ontology::ambiguity::RepoHint>(v).map_err(|e| {
                OxError::Runtime {
                    message: format!("repo_hint decode: {e}"),
                }
            })?,
        ),
        None => None,
    };
    let detected_at: DateTime<Utc> = row.try_get("detected_at").map_err(to_ox_error)?;

    Ok(ox_ontology::ambiguity::AmbiguityContext {
        id: ox_ontology::ambiguity::AmbiguityId::new(id_uuid.to_string()),
        source_id: ox_ontology::mapping::refs::SourceId::new(source_id),
        column: ox_ontology::mapping::refs::ColumnRef {
            relation: relation.to_string(),
            column: column_name.to_string(),
        },
        kind,
        sample_values,
        distinct_estimate: distinct_estimate.map(|v| v as u64),
        nullable,
        clarification_prompt: clarification_prompt.to_string(),
        detection_source_hash: detection_source_hash.to_string(),
        repo_hint,
        detected_at,
    })
}

fn ambiguity_resolution_from_row(
    row: &sqlx::postgres::PgRow,
) -> OxResult<ox_ontology::ambiguity::AmbiguityResolution> {
    use sqlx::Row;
    let id_text: &str = row.try_get("id").map_err(to_ox_error)?;
    let id_uuid: uuid::Uuid = id_text.parse().map_err(|e: uuid::Error| OxError::Runtime {
        message: format!("ambiguity resolution id parse: {e}"),
    })?;
    let context_id_text: &str = row.try_get("context_id").map_err(to_ox_error)?;
    let context_uuid: uuid::Uuid = context_id_text.parse().map_err(|e: uuid::Error| {
        OxError::Runtime {
            message: format!("context_id parse: {e}"),
        }
    })?;
    let context_source_hash: &str =
        row.try_get("context_source_hash").map_err(to_ox_error)?;
    let mapping_json: serde_json::Value = row.try_get("mapping").map_err(to_ox_error)?;
    let mapping = serde_json::from_value::<ox_ontology::ambiguity::AmbiguityMapping>(mapping_json)
        .map_err(|e| OxError::Runtime {
            message: format!("ambiguity mapping decode: {e}"),
        })?;
    let resolved_at: DateTime<Utc> = row.try_get("resolved_at").map_err(to_ox_error)?;
    let resolved_by_user_id: Option<Uuid> =
        row.try_get("resolved_by_user_id").map_err(to_ox_error)?;
    let supersedes_uuid: Option<Uuid> = row.try_get("supersedes_id").map_err(to_ox_error)?;
    let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at").map_err(to_ox_error)?;

    Ok(ox_ontology::ambiguity::AmbiguityResolution {
        id: ox_ontology::ambiguity::AmbiguityResolutionId::new(id_uuid.to_string()),
        context_id: ox_ontology::ambiguity::AmbiguityId::new(context_uuid.to_string()),
        context_source_hash: context_source_hash.to_string(),
        mapping,
        resolved_at,
        resolved_by_user_id,
        supersedes: supersedes_uuid
            .map(|u| ox_ontology::ambiguity::AmbiguityResolutionId::new(u.to_string())),
        revoked_at,
    })
}

#[async_trait]
impl AmbiguityStore for PostgresStore {
    async fn list_ambiguity_contexts(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>> {
        let rows = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts \
             WHERE source_id = $1 \
             ORDER BY relation, column_name",
        )
        .bind(source_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(ambiguity_context_from_row).collect()
    }

    async fn list_ambiguity_contexts_in_workspace(
        &self,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>> {
        // RLS narrows the rows to the current workspace; the query
        // itself carries no workspace bind so the admin dashboard
        // and agent code-paths share one SQL.
        let rows = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts \
             ORDER BY detected_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(ambiguity_context_from_row).collect()
    }

    async fn get_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>> {
        let uuid: Uuid = id.as_str().parse().map_err(|e: uuid::Error| OxError::Runtime {
            message: format!("ambiguity id must be a uuid: {e}"),
        })?;
        let row = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(ambiguity_context_from_row).transpose()
    }

    async fn find_ambiguity_context_by_column(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>> {
        let row = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts \
             WHERE source_id = $1 AND relation = $2 AND column_name = $3",
        )
        .bind(source_id.as_str())
        .bind(&column.relation)
        .bind(&column.column)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(ambiguity_context_from_row).transpose()
    }

    async fn upsert_ambiguity_context(
        &self,
        context: ox_ontology::ambiguity::AmbiguityContext,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityContext> {
        // Workspace column uses the `app.workspace_id` setting the pool
        // injects on each connection acquisition — we read it back via
        // `current_setting(...)::uuid` so the row lands in the right
        // tenant without an extra bind variable.
        let ctx_uuid: Uuid = context.id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("ambiguity id must be uuid: {e}"),
            }
        })?;
        let kind_text = match context.kind {
            ox_ontology::ambiguity::AmbiguityKind::NumericCode => "numeric_code",
            ox_ontology::ambiguity::AmbiguityKind::OpaqueShortCode => "opaque_short_code",
            ox_ontology::ambiguity::AmbiguityKind::OverloadedName => "overloaded_name",
        };
        let sample_json = serde_json::to_value(&context.sample_values).map_err(|e| {
            OxError::Runtime {
                message: format!("sample_values encode: {e}"),
            }
        })?;
        let repo_hint_json = match &context.repo_hint {
            Some(h) => Some(serde_json::to_value(h).map_err(|e| OxError::Runtime {
                message: format!("repo_hint encode: {e}"),
            })?),
            None => None,
        };

        sqlx::query(
            "INSERT INTO ambiguity_contexts \
             (id, workspace_id, source_id, relation, column_name, kind, sample_values, \
              distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
              repo_hint, detected_at) \
             VALUES ($1, current_setting('app.workspace_id', true)::uuid, $2, $3, $4, $5, $6, \
                     $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (workspace_id, source_id, relation, column_name) DO UPDATE SET \
                 id = EXCLUDED.id, \
                 kind = EXCLUDED.kind, \
                 sample_values = EXCLUDED.sample_values, \
                 distinct_estimate = EXCLUDED.distinct_estimate, \
                 nullable = EXCLUDED.nullable, \
                 clarification_prompt = EXCLUDED.clarification_prompt, \
                 detection_source_hash = EXCLUDED.detection_source_hash, \
                 repo_hint = EXCLUDED.repo_hint, \
                 detected_at = EXCLUDED.detected_at",
        )
        .bind(ctx_uuid)
        .bind(context.source_id.as_str())
        .bind(&context.column.relation)
        .bind(&context.column.column)
        .bind(kind_text)
        .bind(&sample_json)
        .bind(context.distinct_estimate.map(|v| v as i64))
        .bind(context.nullable)
        .bind(&context.clarification_prompt)
        .bind(&context.detection_source_hash)
        .bind(repo_hint_json.as_ref())
        .bind(context.detected_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(context)
    }

    async fn delete_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool> {
        let uuid: Uuid = id.as_str().parse().map_err(|e: uuid::Error| OxError::Runtime {
            message: format!("ambiguity id must be uuid: {e}"),
        })?;
        let result = sqlx::query("DELETE FROM ambiguity_contexts WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_ambiguity_resolutions(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityResolution>> {
        let uuid: Uuid = context_id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("context id must be uuid: {e}"),
            }
        })?;
        let rows = sqlx::query(
            "SELECT id::text AS id, context_id::text AS context_id, context_source_hash, \
             mapping, resolved_at, resolved_by_user_id, \
             supersedes_id, revoked_at \
             FROM ambiguity_resolutions WHERE context_id = $1 \
             ORDER BY resolved_at DESC",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(ambiguity_resolution_from_row).collect()
    }

    async fn get_active_ambiguity_resolution(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityResolution>> {
        let row = sqlx::query(
            "SELECT r.id::text AS id, r.context_id::text AS context_id, r.context_source_hash, \
             r.mapping, r.resolved_at, r.resolved_by_user_id, \
             r.supersedes_id, r.revoked_at \
             FROM ambiguity_resolutions r \
             JOIN ambiguity_contexts c ON c.id = r.context_id \
             WHERE c.source_id = $1 AND c.relation = $2 AND c.column_name = $3 \
               AND r.revoked_at IS NULL \
             LIMIT 1",
        )
        .bind(source_id.as_str())
        .bind(&column.relation)
        .bind(&column.column)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(ambiguity_resolution_from_row).transpose()
    }

    async fn create_ambiguity_resolution(
        &self,
        resolution: ox_ontology::ambiguity::AmbiguityResolution,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityResolution> {
        // Atomic: revoke the current active row (if any) and insert the
        // new one in a single transaction so the partial unique index
        // (one active per context) is never violated at read time.
        let res_uuid: Uuid = resolution.id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("resolution id must be uuid: {e}"),
            }
        })?;
        let ctx_uuid: Uuid = resolution.context_id.as_str().parse().map_err(
            |e: uuid::Error| OxError::Runtime {
                message: format!("context id must be uuid: {e}"),
            },
        )?;
        let supersedes_uuid: Option<Uuid> = match &resolution.supersedes {
            Some(s) => Some(s.as_str().parse().map_err(|e: uuid::Error| {
                OxError::Runtime {
                    message: format!("supersedes id must be uuid: {e}"),
                }
            })?),
            None => None,
        };
        let mapping_json = serde_json::to_value(&resolution.mapping).map_err(|e| {
            OxError::Runtime {
                message: format!("mapping encode: {e}"),
            }
        })?;

        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // Revoke the current active resolution, if any. UPDATE ... RETURNING
        // gives us the row id to chain as `supersedes` when the caller
        // didn't supply one explicitly.
        let revoked = sqlx::query(
            "UPDATE ambiguity_resolutions \
             SET revoked_at = now() \
             WHERE context_id = $1 AND revoked_at IS NULL \
             RETURNING id",
        )
        .bind(ctx_uuid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        let supersedes_final: Option<Uuid> = supersedes_uuid.or_else(|| {
            use sqlx::Row;
            revoked.as_ref().and_then(|r| r.try_get("id").ok())
        });

        sqlx::query(
            "INSERT INTO ambiguity_resolutions \
             (id, workspace_id, context_id, context_source_hash, mapping, \
              resolved_at, resolved_by_user_id, supersedes_id, revoked_at) \
             VALUES ($1, current_setting('app.workspace_id', true)::uuid, $2, $3, $4, \
                     $5, $6, $7, NULL)",
        )
        .bind(res_uuid)
        .bind(ctx_uuid)
        .bind(&resolution.context_source_hash)
        .bind(&mapping_json)
        .bind(resolution.resolved_at)
        .bind(resolution.resolved_by_user_id)
        .bind(supersedes_final)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        tx.commit().await.map_err(to_ox_error)?;

        Ok(ox_ontology::ambiguity::AmbiguityResolution {
            supersedes: supersedes_final
                .map(|u| ox_ontology::ambiguity::AmbiguityResolutionId::new(u.to_string())),
            ..resolution
        })
    }

    async fn revoke_active_ambiguity_resolution(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool> {
        let ctx_uuid: Uuid = context_id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("context id must be uuid: {e}"),
            }
        })?;
        let result = sqlx::query(
            "UPDATE ambiguity_resolutions \
             SET revoked_at = now() \
             WHERE context_id = $1 AND revoked_at IS NULL",
        )
        .bind(ctx_uuid)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// ChangeRoutingStore — resolves workspace override vs global default
// ---------------------------------------------------------------------------

fn change_type_to_str(ct: ox_ontology::change_routing::ChangeType) -> &'static str {
    use ox_ontology::change_routing::ChangeType;
    match ct {
        ChangeType::CodedValueCreate => "coded_value_create",
        ChangeType::CodedValueDeprecate => "coded_value_deprecate",
        ChangeType::GlossaryTermCreate => "glossary_term_create",
        ChangeType::GlossaryAliasAdd => "glossary_alias_add",
        ChangeType::NotationPatternCreate => "notation_pattern_create",
        ChangeType::CustomerSegmentCreate => "customer_segment_create",
        ChangeType::ColumnRename => "column_rename",
        ChangeType::TableMerge => "table_merge",
        ChangeType::DataSourceRegister => "data_source_register",
        ChangeType::StaleConceptDeprecate => "stale_concept_deprecate",
        ChangeType::OntologyVersionRollback => "ontology_version_rollback",
    }
}

fn change_type_from_str(
    s: &str,
) -> OxResult<ox_ontology::change_routing::ChangeType> {
    use ox_ontology::change_routing::ChangeType;
    Ok(match s {
        "coded_value_create" => ChangeType::CodedValueCreate,
        "coded_value_deprecate" => ChangeType::CodedValueDeprecate,
        "glossary_term_create" => ChangeType::GlossaryTermCreate,
        "glossary_alias_add" => ChangeType::GlossaryAliasAdd,
        "notation_pattern_create" => ChangeType::NotationPatternCreate,
        "customer_segment_create" => ChangeType::CustomerSegmentCreate,
        "column_rename" => ChangeType::ColumnRename,
        "table_merge" => ChangeType::TableMerge,
        "data_source_register" => ChangeType::DataSourceRegister,
        "stale_concept_deprecate" => ChangeType::StaleConceptDeprecate,
        "ontology_version_rollback" => ChangeType::OntologyVersionRollback,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown change_type in DB row: {other}"),
            });
        }
    })
}

fn risk_level_to_str(r: ox_ontology::change_routing::RiskLevel) -> &'static str {
    use ox_ontology::change_routing::RiskLevel;
    match r {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    }
}

fn risk_level_from_str(
    s: &str,
) -> OxResult<ox_ontology::change_routing::RiskLevel> {
    use ox_ontology::change_routing::RiskLevel;
    Ok(match s {
        "low" => RiskLevel::Low,
        "medium" => RiskLevel::Medium,
        "high" => RiskLevel::High,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown risk_level in DB row: {other}"),
            });
        }
    })
}

fn routing_rule_from_row(
    row: &sqlx::postgres::PgRow,
) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule> {
    use sqlx::Row;
    let id_uuid: Uuid = row.try_get("id").map_err(to_ox_error)?;
    let workspace_id: Option<Uuid> = row.try_get("workspace_id").map_err(to_ox_error)?;
    let change_type_text: &str = row.try_get("change_type").map_err(to_ox_error)?;
    let routing_json: serde_json::Value = row.try_get("routing").map_err(to_ox_error)?;
    let risk_level_text: &str = row.try_get("risk_level").map_err(to_ox_error)?;
    let priority: i32 = row.try_get("priority").map_err(to_ox_error)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(to_ox_error)?;

    let routing = serde_json::from_value::<
        ox_ontology::change_routing::ApprovalRouting,
    >(routing_json)
    .map_err(|e| OxError::Runtime {
        message: format!("routing JSONB decode: {e}"),
    })?;

    Ok(ox_ontology::change_routing::ChangeRoutingRule {
        id: ox_ontology::change_routing::ChangeRoutingRuleId::new(id_uuid.to_string()),
        workspace_id,
        change_type: change_type_from_str(change_type_text)?,
        routing,
        risk_level: risk_level_from_str(risk_level_text)?,
        priority,
        created_at,
    })
}

#[async_trait]
impl ChangeRoutingStore for PostgresStore {
    async fn list_change_routing_rules(
        &self,
    ) -> OxResult<Vec<ox_ontology::change_routing::ChangeRoutingRule>> {
        let rows = sqlx::query(
            "SELECT id, workspace_id, change_type, routing, risk_level, priority, created_at \
             FROM change_routing_rules \
             ORDER BY change_type, priority DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(routing_rule_from_row).collect()
    }

    async fn resolve_change_routing(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<Option<ox_ontology::change_routing::ChangeRoutingRule>> {
        // Workspace row wins when present via the RLS `ws_or_global_read`
        // policy unioning global + override rows; priority DESC then
        // workspace-row first on tie keeps the resolution deterministic.
        let row = sqlx::query(
            "SELECT id, workspace_id, change_type, routing, risk_level, priority, created_at \
             FROM change_routing_rules \
             WHERE change_type = $1 \
             ORDER BY priority DESC, (workspace_id IS NOT NULL) DESC \
             LIMIT 1",
        )
        .bind(change_type_to_str(change_type))
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(routing_rule_from_row).transpose()
    }

    async fn upsert_change_routing_rule(
        &self,
        rule: ox_ontology::change_routing::ChangeRoutingRule,
    ) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule> {
        let id_uuid: Uuid = rule.id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("routing rule id must be uuid: {e}"),
            }
        })?;
        let routing_json = serde_json::to_value(&rule.routing).map_err(|e| {
            OxError::Runtime {
                message: format!("routing JSONB encode: {e}"),
            }
        })?;

        sqlx::query(
            "INSERT INTO change_routing_rules \
             (id, workspace_id, change_type, routing, risk_level, priority, created_at) \
             VALUES ($1, current_setting('app.workspace_id', true)::uuid, $2, $3, $4, $5, $6) \
             ON CONFLICT (workspace_id, change_type) DO UPDATE SET \
                 routing = EXCLUDED.routing, \
                 risk_level = EXCLUDED.risk_level, \
                 priority = EXCLUDED.priority",
        )
        .bind(id_uuid)
        .bind(change_type_to_str(rule.change_type))
        .bind(&routing_json)
        .bind(risk_level_to_str(rule.risk_level))
        .bind(rule.priority)
        .bind(rule.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rule)
    }

    async fn delete_change_routing_rule(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<bool> {
        // Delete only the workspace override — the global default row
        // lives under `workspace_id IS NULL` and is never touched
        // through this path (migrations or SYSTEM_BYPASS own it).
        let result = sqlx::query(
            "DELETE FROM change_routing_rules \
             WHERE workspace_id = current_setting('app.workspace_id', true)::uuid \
               AND change_type = $1",
        )
        .bind(change_type_to_str(change_type))
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// QualitySignalStore — per-query signal log + dashboard aggregation
// ---------------------------------------------------------------------------

fn shacl_failure_from_str(
    s: &str,
) -> OxResult<crate::quality_signal::ShaclFailureKind> {
    use crate::quality_signal::ShaclFailureKind;
    Ok(match s {
        "cardinality_violation" => ShaclFailureKind::CardinalityViolation,
        "measure_group_by" => ShaclFailureKind::MeasureGroupBy,
        "unknown_coded_value" => ShaclFailureKind::UnknownCodedValue,
        "mandatory_property_missing" => ShaclFailureKind::MandatoryPropertyMissing,
        "temporal_grain_mismatch" => ShaclFailureKind::TemporalGrainMismatch,
        "other" => ShaclFailureKind::Other,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown shacl_failure_kind: {other}"),
            });
        }
    })
}

fn shacl_failure_to_str(
    k: crate::quality_signal::ShaclFailureKind,
) -> &'static str {
    use crate::quality_signal::ShaclFailureKind;
    match k {
        ShaclFailureKind::CardinalityViolation => "cardinality_violation",
        ShaclFailureKind::MeasureGroupBy => "measure_group_by",
        ShaclFailureKind::UnknownCodedValue => "unknown_coded_value",
        ShaclFailureKind::MandatoryPropertyMissing => "mandatory_property_missing",
        ShaclFailureKind::TemporalGrainMismatch => "temporal_grain_mismatch",
        ShaclFailureKind::Other => "other",
    }
}

/// Row used only inside `aggregate_quality_metrics` — flat numeric
/// counters so a single SQL round-trip collects every window stat.
/// Not exposed outside this file.
#[derive(Debug, sqlx::FromRow)]
struct WindowCounters {
    samples: i64,
    anchor_matched: i64,
    glossary_hit: i64,
    clarified: i64,
    clarified_success: i64,
    reproducible: i64,
    shacl_passed: i64,
}

async fn fetch_window_counters(
    pool: &PgPool,
    days: i64,
    older_than_days: i64,
) -> OxResult<WindowCounters> {
    // `older_than_days > 0` picks the PREVIOUS window (for trend
    // calc): rows older than `older_than_days` days but still
    // within `days + older_than_days` days. `older_than_days == 0`
    // picks the CURRENT window (last `days` days).
    //
    // Reproducibility = count of signal rows whose
    // `query_ir_normalized_hash` appears more than once in the
    // window (meaning "the same plan ran at least twice" → the
    // question is reproducible). Computed against the window's
    // signal set so a one-off query never counts against itself.
    let sql = "WITH window_rows AS ( \
                   SELECT * FROM query_execution_signals \
                   WHERE captured_at >= now() - ($1::bigint || ' days')::interval \
                         - ($2::bigint || ' days')::interval \
                     AND captured_at < now() - ($2::bigint || ' days')::interval \
               ), hashes AS ( \
                   SELECT query_ir_normalized_hash, COUNT(*) AS c \
                   FROM window_rows \
                   GROUP BY query_ir_normalized_hash \
               ) \
               SELECT \
                 (SELECT COUNT(*) FROM window_rows)::bigint AS samples, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE anchor_top_score IS NOT NULL AND anchor_top_score >= 0.5)::bigint AS anchor_matched, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE array_length(glossary_term_hits, 1) > 0)::bigint AS glossary_hit, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE ambiguity_was_clarified)::bigint AS clarified, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE ambiguity_was_clarified AND shacl_passed)::bigint AS clarified_success, \
                 COALESCE((SELECT SUM(c) FROM hashes WHERE c > 1), 0)::bigint AS reproducible, \
                 (SELECT COUNT(*) FROM window_rows WHERE shacl_passed)::bigint AS shacl_passed";
    sqlx::query_as::<_, WindowCounters>(sql)
        .bind(days)
        .bind(older_than_days)
        .fetch_one(pool)
        .await
        .map_err(to_ox_error)
}

#[async_trait]
impl QualitySignalStore for PostgresStore {
    async fn create_query_execution_signal(
        &self,
        signal: &crate::quality_signal::QueryExecutionSignal,
    ) -> OxResult<()> {
        let failure_text = signal.shacl_failure_kind.map(shacl_failure_to_str);
        sqlx::query(
            "INSERT INTO query_execution_signals \
             (execution_id, workspace_id, captured_at, anchor_top_score, anchor_hit_kinds, \
              glossary_term_hits, ambiguity_resolution_ids, ambiguity_was_clarified, \
              shacl_passed, shacl_failure_kind, query_ir_normalized_hash, referenced_type_ids) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (execution_id) DO NOTHING",
        )
        .bind(signal.execution_id)
        .bind(signal.workspace_id)
        .bind(signal.captured_at)
        .bind(signal.anchor_top_score.map(|v| v as f64))
        .bind(&signal.anchor_hit_kinds)
        .bind(&signal.glossary_term_hits)
        .bind(&signal.ambiguity_resolution_ids)
        .bind(signal.ambiguity_was_clarified)
        .bind(signal.shacl_passed)
        .bind(failure_text)
        .bind(&signal.query_ir_normalized_hash)
        .bind(&signal.referenced_type_ids)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn aggregate_quality_metrics(
        &self,
        window: crate::quality_signal::MetricWindow,
    ) -> OxResult<crate::quality_signal::QualityMetricsReport> {
        use crate::quality_signal::{MetricValue, QualityMetricsReport};

        let days = window.as_days();
        let current = fetch_window_counters(&self.pool, days, 0).await?;
        let previous = fetch_window_counters(&self.pool, days, days).await?;

        #[derive(sqlx::FromRow)]
        struct StaleRatio {
            total: i64,
            stale: i64,
        }
        let ratio: StaleRatio = sqlx::query_as::<_, StaleRatio>(
            "SELECT COUNT(*)::bigint AS total, \
                    COUNT(*) FILTER (WHERE last_used_at < now() - INTERVAL '180 days')::bigint AS stale \
             FROM ontology_type_last_used",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;

        fn prop(counters: &WindowCounters, numerator: i64) -> f64 {
            if counters.samples == 0 {
                0.0
            } else {
                (numerator as f64) / (counters.samples as f64)
            }
        }

        fn prop_clarified(counters: &WindowCounters) -> f64 {
            if counters.clarified == 0 {
                0.0
            } else {
                (counters.clarified_success as f64) / (counters.clarified as f64)
            }
        }

        let prev_anchor = prop(&previous, previous.anchor_matched);
        let prev_gloss = prop(&previous, previous.glossary_hit);
        let prev_clar = prop_clarified(&previous);
        let prev_repro = prop(&previous, previous.reproducible);
        let prev_shacl = prop(&previous, previous.shacl_passed);

        let report = QualityMetricsReport {
            anchor_match_rate: MetricValue::wilson_proportion(
                current.anchor_matched as u64,
                current.samples as u64,
                prev_anchor,
            ),
            glossary_hit_rate: MetricValue::wilson_proportion(
                current.glossary_hit as u64,
                current.samples as u64,
                prev_gloss,
            ),
            clarification_success_rate: MetricValue::wilson_proportion(
                current.clarified_success as u64,
                current.clarified as u64,
                prev_clar,
            ),
            query_reproducibility: MetricValue::wilson_proportion(
                current.reproducible as u64,
                current.samples as u64,
                prev_repro,
            ),
            shacl_pass_rate: MetricValue::wilson_proportion(
                current.shacl_passed as u64,
                current.samples as u64,
                prev_shacl,
            ),
            stale_concept_ratio: if ratio.total == 0 {
                MetricValue::empty()
            } else {
                MetricValue::wilson_proportion(ratio.stale as u64, ratio.total as u64, 0.0)
            },
            sample_size: current.samples as u64,
            window,
        };
        Ok(report)
    }

    async fn list_shacl_failure_distribution(
        &self,
        window: crate::quality_signal::MetricWindow,
    ) -> OxResult<Vec<crate::quality_signal::ShaclFailureCount>> {
        use crate::quality_signal::ShaclFailureCount;

        #[derive(sqlx::FromRow)]
        struct Row {
            kind: String,
            count: i64,
        }
        let rows: Vec<Row> = sqlx::query_as::<_, Row>(
            "SELECT shacl_failure_kind AS kind, COUNT(*)::bigint AS count \
             FROM query_execution_signals \
             WHERE captured_at >= now() - ($1::bigint || ' days')::interval \
               AND shacl_failure_kind IS NOT NULL \
             GROUP BY shacl_failure_kind \
             ORDER BY count DESC",
        )
        .bind(window.as_days())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        rows.into_iter()
            .map(|r| {
                Ok(ShaclFailureCount {
                    kind: shacl_failure_from_str(&r.kind)?,
                    count: r.count as u64,
                })
            })
            .collect()
    }

    async fn upsert_type_last_used(
        &self,
        type_ids: &[(uuid::Uuid, &str)],
    ) -> OxResult<()> {
        if type_ids.is_empty() {
            return Ok(());
        }
        for (id, kind) in type_ids {
            sqlx::query(
                "INSERT INTO ontology_type_last_used \
                 (workspace_id, type_id, type_kind, last_used_at, use_count_7d, use_count_30d) \
                 VALUES (current_setting('app.workspace_id', true)::uuid, $1, $2, now(), 1, 1) \
                 ON CONFLICT (workspace_id, type_id) DO UPDATE SET \
                     last_used_at  = now(), \
                     use_count_7d  = ontology_type_last_used.use_count_7d + 1, \
                     use_count_30d = ontology_type_last_used.use_count_30d + 1, \
                     updated_at    = now()",
            )
            .bind(id)
            .bind(kind)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        }
        Ok(())
    }

    async fn list_stale_types(
        &self,
        stale_after_days: i64,
    ) -> OxResult<Vec<crate::quality_signal::StaleTypeEntry>> {
        use crate::quality_signal::StaleTypeEntry;

        #[derive(sqlx::FromRow)]
        struct Row {
            workspace_id: Uuid,
            type_id: Uuid,
            type_kind: String,
            last_used_at: Option<DateTime<Utc>>,
            days_since: Option<f64>,
        }
        let rows: Vec<Row> = sqlx::query_as::<_, Row>(
            "SELECT workspace_id, type_id, type_kind, last_used_at, \
                    EXTRACT(EPOCH FROM (now() - last_used_at)) / 86400.0 AS days_since \
             FROM ontology_type_last_used \
             WHERE last_used_at < now() - ($1::bigint || ' days')::interval \
             ORDER BY last_used_at ASC \
             LIMIT 500",
        )
        .bind(stale_after_days)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rows
            .into_iter()
            .map(|r| StaleTypeEntry {
                workspace_id: r.workspace_id,
                type_id: r.type_id,
                type_kind: r.type_kind,
                last_used_at: r.last_used_at,
                days_since_last_use: r.days_since.map(|v| v as i64).unwrap_or(0),
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// StaleConceptProposalStore — durable deprecation proposals
// ---------------------------------------------------------------------------

fn proposal_from_row(
    row: &sqlx::postgres::PgRow,
) -> OxResult<crate::quality_signal::StaleConceptProposal> {
    use sqlx::Row;
    let id: Uuid = row.try_get("id").map_err(to_ox_error)?;
    let workspace_id: Uuid = row.try_get("workspace_id").map_err(to_ox_error)?;
    let type_id: Uuid = row.try_get("type_id").map_err(to_ox_error)?;
    let type_kind: String = row.try_get("type_kind").map_err(to_ox_error)?;
    let last_used_at: Option<DateTime<Utc>> =
        row.try_get("last_used_at").map_err(to_ox_error)?;
    let days_since_last_use: i32 =
        row.try_get("days_since_last_use").map_err(to_ox_error)?;
    let proposed_at: DateTime<Utc> = row.try_get("proposed_at").map_err(to_ox_error)?;
    let decision_text: String = row.try_get("decision").map_err(to_ox_error)?;
    let decision = crate::quality_signal::StaleProposalDecision::try_from_db(&decision_text)
        .ok_or_else(|| OxError::Runtime {
            message: format!("unknown stale_concept decision: {decision_text}"),
        })?;
    let decided_at: Option<DateTime<Utc>> = row.try_get("decided_at").map_err(to_ox_error)?;
    let decided_by_user_id: Option<Uuid> = row.try_get("decided_by_user_id").map_err(to_ox_error)?;
    let reason: Option<String> = row.try_get("reason").map_err(to_ox_error)?;

    Ok(crate::quality_signal::StaleConceptProposal {
        id,
        workspace_id,
        type_id,
        type_kind,
        last_used_at,
        days_since_last_use: days_since_last_use as i64,
        proposed_at,
        decision,
        decided_at,
        decided_by_user_id,
        reason,
    })
}

#[async_trait]
impl StaleConceptProposalStore for PostgresStore {
    async fn list_stale_concept_proposals(
        &self,
        pending_only: bool,
    ) -> OxResult<Vec<crate::quality_signal::StaleConceptProposal>> {
        // RLS scopes workspace automatically; the `pending_only`
        // filter feeds the admin dashboard's "open work" view.
        let sql = if pending_only {
            "SELECT id, workspace_id, type_id, type_kind, last_used_at, \
                    days_since_last_use, proposed_at, decision, decided_at, \
                    decided_by_user_id, reason \
             FROM stale_concept_proposals \
             WHERE decision = 'pending' \
             ORDER BY proposed_at DESC \
             LIMIT 500"
        } else {
            "SELECT id, workspace_id, type_id, type_kind, last_used_at, \
                    days_since_last_use, proposed_at, decision, decided_at, \
                    decided_by_user_id, reason \
             FROM stale_concept_proposals \
             ORDER BY proposed_at DESC \
             LIMIT 500"
        };
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;
        rows.iter().map(proposal_from_row).collect()
    }

    async fn get_stale_concept_proposal(
        &self,
        id: Uuid,
    ) -> OxResult<Option<crate::quality_signal::StaleConceptProposal>> {
        let row = sqlx::query(
            "SELECT id, workspace_id, type_id, type_kind, last_used_at, \
                    days_since_last_use, proposed_at, decision, decided_at, \
                    decided_by_user_id, reason \
             FROM stale_concept_proposals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(proposal_from_row).transpose()
    }

    async fn upsert_stale_concept_proposal(
        &self,
        proposal: crate::quality_signal::StaleConceptProposal,
    ) -> OxResult<crate::quality_signal::StaleConceptProposal> {
        // Cron-friendly: natural key dedup. A re-proposal after a
        // previous `dismissed` decision needs the admin to clear the
        // old row first — we don't auto-resurrect, because that
        // would flap every scan.
        let row = sqlx::query(
            "INSERT INTO stale_concept_proposals \
             (id, workspace_id, type_id, type_kind, last_used_at, \
              days_since_last_use, proposed_at, decision) \
             VALUES ($1, current_setting('app.workspace_id', true)::uuid, $2, $3, $4, \
                     $5, $6, 'pending') \
             ON CONFLICT (workspace_id, type_id) DO UPDATE SET \
                 last_used_at = EXCLUDED.last_used_at, \
                 days_since_last_use = EXCLUDED.days_since_last_use \
             RETURNING id, workspace_id, type_id, type_kind, last_used_at, \
                       days_since_last_use, proposed_at, decision, decided_at, \
                       decided_by_user_id, reason",
        )
        .bind(proposal.id)
        .bind(proposal.type_id)
        .bind(&proposal.type_kind)
        .bind(proposal.last_used_at)
        .bind(proposal.days_since_last_use as i32)
        .bind(proposal.proposed_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        proposal_from_row(&row)
    }

    async fn record_stale_proposal_decision(
        &self,
        id: Uuid,
        decision: crate::quality_signal::StaleProposalDecision,
        decided_by_user_id: Option<Uuid>,
        reason: Option<String>,
    ) -> OxResult<crate::quality_signal::StaleConceptProposal> {
        // Only transition from `pending` — repeated decisions are
        // silent no-ops that return the existing row (so the UI can
        // double-click the button without error). Terminal → terminal
        // transitions would erode the audit trail and aren't useful.
        let row = sqlx::query(
            "UPDATE stale_concept_proposals \
             SET decision = $2, \
                 decided_at = now(), \
                 decided_by_user_id = $3, \
                 reason = $4 \
             WHERE id = $1 AND decision = 'pending' \
             RETURNING id, workspace_id, type_id, type_kind, last_used_at, \
                       days_since_last_use, proposed_at, decision, decided_at, \
                       decided_by_user_id, reason",
        )
        .bind(id)
        .bind(decision.as_str())
        .bind(decided_by_user_id)
        .bind(reason.as_deref())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if let Some(row) = row {
            return proposal_from_row(&row);
        }
        // No row updated → already terminal OR RLS-invisible. Return
        // the current row when visible, otherwise propagate a
        // `NotFound` shape callers already expect from `.get_*`.
        let current = self.get_stale_concept_proposal(id).await?;
        current.ok_or_else(|| OxError::Runtime {
            message: format!("stale_concept_proposal {id} not found"),
        })
    }
}


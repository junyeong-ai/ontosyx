use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::info;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};

use crate::models::*;
use crate::store::{
    AclStore, AgentSessionStore, AnalysisResultStore, AnalysisSnapshot, ApprovalStore, AuditStore,
    ConfigStore, CursorPage, CursorParams, DashboardStore, EmbeddingRetryStore, ExtendResult,
    HealthStore, KnowledgeStore, LineageStore, LoadCheckpointStore, MeteringStore, OntologyStore,
    PerspectiveStore, PinStore, ProjectStore, PromptTemplateStore, QualityStore, QueryStore,
    RecipeStore, ReportStore, ScheduledTaskStore, ToolApprovalStore, UserStore, VerificationStore,
    WorkspaceStore,
};

/// System user identifier for automated operations (schema adoption, seeding).
const SYSTEM_USER: &str = "system";

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
    async fn create_query_execution(&self, exec: &QueryExecution) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO query_executions
             (id, user_id, question, ontology_id, ontology_version,
              saved_ontology_id, ontology_snapshot,
              query_ir, compiled_target, compiled_query,
              results, widget, explanation, model, execution_time_ms,
              query_bindings, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
        )
        .bind(exec.id)
        .bind(&exec.user_id)
        .bind(&exec.question)
        .bind(&exec.ontology_id)
        .bind(exec.ontology_version)
        .bind(exec.saved_ontology_id)
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

    async fn get_query_execution(
        &self,
        user_id: &str,
        id: Uuid,
    ) -> OxResult<Option<QueryExecution>> {
        // Resolve ontology_snapshot via JOIN when saved_ontology_id is set
        sqlx::query_as::<_, QueryExecution>(
            "SELECT qe.id, qe.user_id, qe.question, qe.ontology_id, qe.ontology_version,
                    qe.saved_ontology_id,
                    COALESCE(qe.ontology_snapshot, so.ontology_ir) AS ontology_snapshot,
                    qe.query_ir, qe.compiled_target, qe.compiled_query,
                    qe.results, qe.widget, qe.explanation, qe.model,
                    qe.execution_time_ms, qe.query_bindings, qe.created_at
             FROM query_executions qe
             LEFT JOIN saved_ontologies so ON so.id = qe.saved_ontology_id
             WHERE qe.id = $1 AND qe.user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn list_query_executions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<QueryExecutionSummary>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let query = "SELECT id, question, ontology_id, ontology_version,
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
// OntologyStore
// ---------------------------------------------------------------------------

#[async_trait]
impl OntologyStore for PostgresStore {
    async fn get_saved_ontology(&self, id: Uuid) -> OxResult<Option<SavedOntology>> {
        sqlx::query_as::<_, SavedOntology>("SELECT * FROM saved_ontologies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn list_saved_ontologies(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedOntology>> {
        let limit = pagination.effective_limit();

        let rows = if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            sqlx::query_as::<_, SavedOntology>(
                "SELECT * FROM saved_ontologies
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
            sqlx::query_as::<_, SavedOntology>(
                "SELECT * FROM saved_ontologies
                 ORDER BY created_at DESC, id DESC
                 LIMIT $1",
            )
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        };

        Ok(build_cursor_page(rows, limit, |o| (o.created_at, o.id)))
    }

    async fn get_latest_ontology(&self, name: &str) -> OxResult<Option<SavedOntology>> {
        sqlx::query_as::<_, SavedOntology>(
            "SELECT * FROM saved_ontologies WHERE name = $1 ORDER BY version DESC LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn create_standalone_ontology(
        &self,
        name: &str,
        ontology_ir: &serde_json::Value,
    ) -> OxResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO saved_ontologies (id, name, version, ontology_ir, created_by, created_at)
             VALUES ($1, $2, 1, $3, $4, NOW())",
        )
        .bind(id)
        .bind(name)
        .bind(ontology_ir)
        .bind(SYSTEM_USER)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(id)
    }

    async fn update_ontology_ir(&self, id: Uuid, ontology_ir: &serde_json::Value) -> OxResult<()> {
        let rows = sqlx::query("UPDATE saved_ontologies SET ontology_ir = $1 WHERE id = $2")
            .bind(ontology_ir)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;

        if rows.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("saved_ontology {id}"),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PinStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PinStore for PostgresStore {
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

    async fn get_design_project(&self, id: Uuid) -> OxResult<Option<DesignProject>> {
        sqlx::query_as::<_, DesignProject>("SELECT * FROM design_projects WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn list_design_projects(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<DesignProjectSummary>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, DesignProjectSummary>(
                "SELECT id, status, revision, user_id, title, source_config, saved_ontology_id,
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
                "SELECT id, status, revision, user_id, title, source_config, saved_ontology_id,
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

    async fn complete_design_project(
        &self,
        project_id: Uuid,
        ontology: &SavedOntology,
        expected_revision: i32,
    ) -> OxResult<()> {
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        sqlx::query(
            "INSERT INTO saved_ontologies (id, name, description, version, ontology_ir, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(ontology.id)
        .bind(&ontology.name)
        .bind(&ontology.description)
        .bind(ontology.version)
        .bind(&ontology.ontology_ir)
        .bind(&ontology.created_by)
        .bind(ontology.created_at)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        let result = sqlx::query(
            "UPDATE design_projects
             SET status = 'completed', saved_ontology_id = $1,
                 updated_at = NOW(), revision = revision + 1
             WHERE id = $2 AND revision = $3 AND status = 'designed'",
        )
        .bind(ontology.id)
        .bind(project_id)
        .bind(expected_revision)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        check_cas_result(result.rows_affected())?;

        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }

    async fn delete_design_project(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM design_projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn archive_stale_projects(&self, max_age_days: i64) -> OxResult<u64> {
        let result = sqlx::query(
            "UPDATE design_projects
             SET archived_at = NOW()
             WHERE status IN ('analyzed', 'designed')
               AND updated_at < NOW() - ($1 || ' days')::interval
               AND archived_at IS NULL",
        )
        .bind(max_age_days)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    async fn delete_archived_projects(&self, max_archive_days: i64) -> OxResult<u64> {
        let result = sqlx::query(
            "DELETE FROM design_projects
             WHERE archived_at IS NOT NULL
               AND archived_at < NOW() - ($1 || ' days')::interval",
        )
        .bind(max_archive_days)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    // --- Ontology Snapshots ---

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

    async fn get_config(&self, key: &str) -> OxResult<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row)
    }

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

    async fn get_user_by_id(&self, id: Uuid) -> OxResult<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn get_recipe(&self, id: Uuid) -> OxResult<Option<AnalysisRecipe>> {
        sqlx::query_as::<_, AnalysisRecipe>("SELECT * FROM analysis_recipes WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn delete_recipe(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM analysis_recipes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_recipe_status(&self, id: Uuid, status: &str) -> OxResult<()> {
        sqlx::query("UPDATE analysis_recipes SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
    }

    async fn create_recipe_version(&self, recipe: &AnalysisRecipe) -> OxResult<()> {
        self.upsert_recipe(recipe).await
    }

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
    async fn create_dashboard(&self, d: &Dashboard) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO dashboards (id, workspace_id, user_id, name, description, layout, is_public, share_token, shared_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
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
        .bind(d.created_at)
        .bind(d.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn get_dashboard(&self, id: Uuid) -> OxResult<Option<Dashboard>> {
        sqlx::query_as::<_, Dashboard>("SELECT * FROM dashboards WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn delete_dashboard(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM dashboards WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_dashboard_share_token(&self, id: Uuid, token: Option<&str>) -> OxResult<()> {
        if let Some(token) = token {
            sqlx::query(
                "UPDATE dashboards SET share_token = $1, shared_at = NOW(), updated_at = NOW() WHERE id = $2",
            )
            .bind(token)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        } else {
            sqlx::query(
                "UPDATE dashboards SET share_token = NULL, shared_at = NULL, updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        }
        Ok(())
    }

    async fn get_dashboard_by_share_token(&self, token: &str) -> OxResult<Option<Dashboard>> {
        sqlx::query_as::<_, Dashboard>("SELECT * FROM dashboards WHERE share_token = $1")
            .bind(token)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn list_widgets(&self, dashboard_id: Uuid) -> OxResult<Vec<DashboardWidget>> {
        sqlx::query_as::<_, DashboardWidget>(
            "SELECT * FROM dashboard_widgets WHERE dashboard_id = $1 ORDER BY created_at",
        )
        .bind(dashboard_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

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

    async fn delete_widget(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM dashboard_widgets WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

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
    async fn create_report(&self, r: &SavedReport) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO saved_reports
             (id, user_id, ontology_id, title, description, query_template,
              parameters, widget_type, is_public, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(r.id)
        .bind(&r.user_id)
        .bind(&r.ontology_id)
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

    async fn get_report(&self, id: Uuid) -> OxResult<Option<SavedReport>> {
        sqlx::query_as::<_, SavedReport>("SELECT * FROM saved_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn list_reports(
        &self,
        user_id: &str,
        ontology_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedReport>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, SavedReport>(
                "SELECT * FROM saved_reports
                     WHERE (user_id = $1 OR is_public = true)
                       AND ontology_id = $2
                       AND (updated_at, id) < ($3, $4)
                     ORDER BY updated_at DESC, id DESC
                     LIMIT $5",
            )
            .bind(user_id)
            .bind(ontology_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, SavedReport>(
                "SELECT * FROM saved_reports
                     WHERE (user_id = $1 OR is_public = true)
                       AND ontology_id = $2
                     ORDER BY updated_at DESC, id DESC
                     LIMIT $3",
            )
            .bind(user_id)
            .bind(ontology_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |r| (r.updated_at, r.id)))
    }

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
// AnalysisResultStore
// ---------------------------------------------------------------------------

#[async_trait]
impl AnalysisResultStore for PostgresStore {
    async fn create_analysis_result(&self, r: &AnalysisResult) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO analysis_results (id, recipe_id, ontology_id, input_hash, output, duration_ms, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(r.id)
        .bind(r.recipe_id)
        .bind(&r.ontology_id)
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

    async fn cleanup_old_results(&self, max_age_days: i64) -> OxResult<u64> {
        let result = sqlx::query(
            "DELETE FROM analysis_results
             WHERE created_at < NOW() - make_interval(days => $1)",
        )
        .bind(max_age_days as i32)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// ScheduledTaskStore
// ---------------------------------------------------------------------------

#[async_trait]
impl ScheduledTaskStore for PostgresStore {
    async fn create_scheduled_task(&self, t: &ScheduledTask) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO scheduled_tasks (id, recipe_id, ontology_id, cron_expression, description,
             enabled, next_run_at, webhook_url, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(t.id)
        .bind(t.recipe_id)
        .bind(&t.ontology_id)
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

    async fn get_scheduled_task(&self, id: Uuid) -> OxResult<Option<ScheduledTask>> {
        sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn list_due_tasks(&self) -> OxResult<Vec<ScheduledTask>> {
        sqlx::query_as::<_, ScheduledTask>(
            "SELECT * FROM scheduled_tasks WHERE enabled = true AND next_run_at <= NOW()",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

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

    async fn update_scheduled_task_enabled(&self, id: Uuid, enabled: bool) -> OxResult<()> {
        sqlx::query("UPDATE scheduled_tasks SET enabled = $2 WHERE id = $1")
            .bind(id)
            .bind(enabled)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
    }

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
    async fn create_agent_session(&self, s: &AgentSession) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO agent_sessions (id, user_id, ontology_id, prompt_hash, tool_schema_hash,
             model_id, model_config, user_message, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(s.id)
        .bind(&s.user_id)
        .bind(&s.ontology_id)
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

    async fn get_agent_session(&self, id: Uuid) -> OxResult<Option<AgentSession>> {
        sqlx::query_as(
            "SELECT id, user_id, ontology_id, prompt_hash, tool_schema_hash,
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
                    cursor.parse().map_err(|_| OxError::Validation {
                        field: "cursor".into(),
                        message: "Invalid cursor".into(),
                    })?;
                sqlx::query_as(
                    "SELECT id, user_id, ontology_id, prompt_hash, tool_schema_hash,
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
                "SELECT id, user_id, ontology_id, prompt_hash, tool_schema_hash,
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

    async fn list_agent_events(&self, session_id: Uuid) -> OxResult<Vec<AgentEvent>> {
        sqlx::query_as("SELECT * FROM agent_events WHERE session_id = $1 ORDER BY sequence")
            .bind(session_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })
    }

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

    async fn cleanup_old_sessions(&self, retention_days: i64) -> OxResult<u64> {
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

        let result = sqlx::query(
            "DELETE FROM agent_sessions WHERE created_at < NOW() - ($1 || ' days')::interval",
        )
        .bind(retention_days)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;

        Ok(result.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// EmbeddingRetryStore
// ---------------------------------------------------------------------------

#[async_trait]
impl EmbeddingRetryStore for PostgresStore {
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

    async fn delete_pending_embedding(&self, id: Uuid) -> OxResult<()> {
        sqlx::query("DELETE FROM pending_embeddings WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PromptTemplateStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PromptTemplateStore for PostgresStore {
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

    async fn get_prompt_template(&self, id: Uuid) -> OxResult<Option<PromptTemplateRow>> {
        sqlx::query_as("SELECT * FROM prompt_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })
    }

    async fn get_active_prompt(&self, name: &str) -> OxResult<Option<PromptTemplateRow>> {
        sqlx::query_as(
            "SELECT * FROM prompt_templates WHERE name = $1 AND is_active = true ORDER BY version DESC LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    async fn create_prompt_template(&self, r: &PromptTemplateRow) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO prompt_templates (id, name, version, content, variables, metadata, created_by, created_at, is_active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (name, version) DO NOTHING",
        )
        .bind(r.id)
        .bind(&r.name)
        .bind(&r.version)
        .bind(&r.content)
        .bind(&r.variables)
        .bind(&r.metadata)
        .bind(&r.created_by)
        .bind(r.created_at)
        .bind(r.is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

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

    async fn delete_prompt_template(&self, id: Uuid) -> OxResult<()> {
        sqlx::query("DELETE FROM prompt_templates WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(())
    }

    async fn deactivate_other_versions(&self, name: &str, exclude_id: Uuid) -> OxResult<()> {
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
    async fn verify_element(&self, v: &ElementVerification) -> OxResult<Uuid> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO ontology_verifications
             (ontology_id, element_id, element_kind, verified_by, review_notes)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (ontology_id, element_id, verified_by)
                WHERE invalidated_at IS NULL
             DO UPDATE SET review_notes = EXCLUDED.review_notes
             RETURNING id",
        )
        .bind(&v.ontology_id)
        .bind(&v.element_id)
        .bind(&v.element_kind)
        .bind(v.verified_by)
        .bind(&v.review_notes)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.0)
    }

    async fn get_verifications(&self, ontology_id: &str) -> OxResult<Vec<ElementVerification>> {
        sqlx::query_as(
            "SELECT v.id, v.ontology_id, v.element_id, v.element_kind,
                    v.verified_by, COALESCE(u.name, u.email) AS verified_by_name,
                    v.review_notes, v.invalidated_at, v.invalidation_reason, v.created_at
             FROM ontology_verifications v
             LEFT JOIN users u ON u.id = v.verified_by
             WHERE v.ontology_id = $1 AND v.invalidated_at IS NULL
             ORDER BY v.created_at DESC",
        )
        .bind(ontology_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn invalidate_for_elements(
        &self,
        ontology_id: &str,
        element_ids: &[&str],
        reason: &str,
    ) -> OxResult<u64> {
        let result = sqlx::query(
            "UPDATE ontology_verifications
             SET invalidated_at = NOW(), invalidation_reason = $3
             WHERE ontology_id = $1
               AND element_id = ANY($2)
               AND invalidated_at IS NULL",
        )
        .bind(ontology_id)
        .bind(element_ids)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    async fn delete_verification(
        &self,
        ontology_id: &str,
        element_id: &str,
        user_id: Uuid,
    ) -> OxResult<bool> {
        let result = sqlx::query(
            "UPDATE ontology_verifications
             SET invalidated_at = NOW(), invalidation_reason = 'manually_revoked'
             WHERE ontology_id = $1 AND element_id = $2 AND verified_by = $3
               AND invalidated_at IS NULL",
        )
        .bind(ontology_id)
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
    async fn create_workspace(&self, w: &Workspace) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, settings)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(w.id)
        .bind(&w.name)
        .bind(&w.slug)
        .bind(w.owner_id)
        .bind(&w.settings)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn get_workspace(&self, id: Uuid) -> OxResult<Option<Workspace>> {
        sqlx::query_as("SELECT * FROM workspaces WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn get_workspace_by_slug(&self, slug: &str) -> OxResult<Option<Workspace>> {
        sqlx::query_as("SELECT * FROM workspaces WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn delete_workspace(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

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
    async fn record_audit(
        &self,
        user_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO audit_log (user_id, action, resource_type, resource_id, details)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&details)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn list_audit_events(&self, params: CursorParams) -> OxResult<CursorPage<AuditEntry>> {
        let limit = params.effective_limit();

        let rows: Vec<AuditEntry> = if let Some((cursor_ts, cursor_id)) = params.cursor_parts() {
            sqlx::query_as(
                "SELECT id, user_id, workspace_id, action, resource_type, resource_id, details, created_at
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
                "SELECT id, user_id, workspace_id, action, resource_type, resource_id, details, created_at
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
// MeteringStore — cost/usage tracking
// ---------------------------------------------------------------------------

#[async_trait]
impl MeteringStore for PostgresStore {
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

    async fn get_lineage_for_label(&self, graph_label: &str) -> OxResult<Vec<LineageEntry>> {
        sqlx::query_as("SELECT * FROM data_lineage WHERE graph_label = $1 ORDER BY started_at DESC")
            .bind(graph_label)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn get_lineage_for_project(&self, project_id: Uuid) -> OxResult<Vec<LineageEntry>> {
        sqlx::query_as("SELECT * FROM data_lineage WHERE project_id = $1 ORDER BY started_at DESC")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>> {
        sqlx::query_as("SELECT * FROM approval_requests WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn expire_old_approvals(&self) -> OxResult<u64> {
        let result = sqlx::query(
            "UPDATE approval_requests
             SET status = 'expired'
             WHERE status = 'pending' AND expires_at <= NOW()",
        )
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// QualityStore
// ---------------------------------------------------------------------------

#[async_trait]
impl QualityStore for PostgresStore {
    async fn create_quality_rule(&self, rule: &QualityRule) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO quality_rules
             (id, name, description, rule_type, target_label, target_property,
              threshold, cypher_check, severity, is_active, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(rule.id)
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

    async fn get_quality_rule(&self, id: Uuid) -> OxResult<Option<QualityRule>> {
        sqlx::query_as("SELECT * FROM quality_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn list_quality_rules(&self, target_label: Option<&str>) -> OxResult<Vec<QualityRule>> {
        if let Some(label) = target_label {
            sqlx::query_as(
                "SELECT * FROM quality_rules
                 WHERE target_label = $1
                 ORDER BY severity DESC, name",
            )
            .bind(label)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
        } else {
            sqlx::query_as(
                "SELECT * FROM quality_rules
                 ORDER BY severity DESC, name",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
        }
    }

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

    async fn delete_quality_rule(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM quality_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

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

    async fn get_acl_policy(&self, id: Uuid) -> OxResult<Option<AclPolicy>> {
        sqlx::query_as("SELECT * FROM acl_policies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

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

    async fn delete_acl_policy(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM acl_policies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

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

    async fn delete_model_config(&self, id: Uuid) -> OxResult<()> {
        sqlx::query("DELETE FROM model_configs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
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

    async fn delete_routing_rule(&self, id: Uuid) -> OxResult<()> {
        sqlx::query("DELETE FROM model_routing_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
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

    async fn delete_knowledge_entry(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM knowledge_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

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

    async fn count_knowledge_by_status_kind(&self) -> OxResult<Vec<(String, String, i64)>> {
        sqlx::query_as::<_, (String, String, i64)>(
            "SELECT status, kind, COUNT(*) FROM knowledge_entries GROUP BY status, kind",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

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

    async fn delete_checkpoint(&self, id: Uuid) -> OxResult<()> {
        sqlx::query("DELETE FROM load_checkpoints WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
    }
}

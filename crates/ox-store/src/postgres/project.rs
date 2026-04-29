//! [`ProjectStore`] — design project lifecycle with CAS-gated revision updates.

use super::*;

#[async_trait]
impl ProjectStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_design_project(&self, project: &DesignProject) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO design_projects
             (id, user_id, status, revision, title, source_config, source_id,
              source_data, source_schema, source_profile, analysis_report,
              design_options, initial_selection, ontology, quality_report,
              source_history, analyzed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                     $14, $15, $16, $17)",
        )
        .bind(project.id)
        .bind(&project.user_id)
        .bind(&project.status)
        .bind(project.revision)
        .bind(&project.title)
        .bind(&project.source_config)
        .bind(&project.source_id)
        .bind(&project.source_data)
        .bind(&project.source_schema)
        .bind(&project.source_profile)
        .bind(&project.analysis_report)
        .bind(&project.design_options)
        .bind(&project.initial_selection)
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
        super::require_workspace_context()?;
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
        quality_report: Option<&serde_json::Value>,
        expected_revision: i32,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        let result = sqlx::query(
            "UPDATE design_projects
             SET ontology = $1, quality_report = $2, status = 'designed',
                 updated_at = NOW(), revision = revision + 1
             WHERE id = $3 AND revision = $4 ",
        )
        .bind(ontology)
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
        super::require_workspace_context()?;
        let rows = sqlx::query(
            "UPDATE design_projects
             SET ontology = $1, quality_report = $2,
                 source_schema = $3, source_profile = $4,
                 source_history = $5,
                 status = 'designed', updated_at = NOW(), revision = revision + 1
             WHERE id = $6 AND revision = $7 ",
        )
        .bind(&result.ontology)
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
        super::require_workspace_context()?;
        let result = sqlx::query(
            "UPDATE design_projects
             SET source_config = $1, source_id = $2, source_data = $3,
                 source_schema = $4, source_profile = $5, analysis_report = $6,
                 design_options = $7, ontology = NULL, quality_report = NULL,
                 status = 'analyzed', analyzed_at = NOW(),
                 updated_at = NOW(), revision = revision + 1
             WHERE id = $8 AND revision = $9 ",
        )
        .bind(&snapshot.source_config)
        .bind(&snapshot.source_id)
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
        super::require_workspace_context()?;
        // The caller has already committed a new version through
        // OntologyVersionStore; this path only links the project
        // row to the new ontology identity. Single-statement path
        // (no transaction needed — one UPDATE).
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
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM design_projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn archive_stale_projects(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
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
        quality_report: Option<&serde_json::Value>,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        // idempotent: `(project_id, revision)` uniquely identifies a
        // snapshot of a project version — the same revision pinned
        // twice carries the same ontology JSONB, so DO NOTHING is the
        // intended behavior.
        sqlx::query(
            "INSERT INTO ontology_snapshots (project_id, revision, ontology, quality_report)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (project_id, revision) DO NOTHING",
        )
        .bind(project_id)
        .bind(revision)
        .bind(ontology)
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

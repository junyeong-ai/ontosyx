//! [`DataSourceStore`] — federation adapter registry (one row per workspace + source_id).

use super::*;

#[derive(sqlx::FromRow)]
struct DataSourceRow {
    id: Uuid,
    workspace_id: Uuid,
    source_id: String,
    kind: String,
    config: serde_json::Value,
    last_analysis_snapshot: Option<serde_json::Value>,
    schema_fingerprints:
        sqlx::types::Json<std::collections::BTreeMap<String, ox_core::SchemaFingerprint>>,
    last_analyzed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DataSourceRow> for crate::models::DataSource {
    fn from(row: DataSourceRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            source_id: row.source_id,
            kind: row.kind,
            config: row.config,
            last_analysis_snapshot: row.last_analysis_snapshot,
            schema_fingerprints: row.schema_fingerprints.0,
            last_analyzed_at: row.last_analyzed_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[async_trait]
impl crate::store::DataSourceStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_data_source(&self, item: &crate::models::DataSource) -> OxResult<()> {
        super::require_workspace_context()?;
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
        let row = sqlx::query_as::<_, DataSourceRow>(
            "SELECT id, workspace_id, source_id, kind, config,
                    last_analysis_snapshot, schema_fingerprints, last_analyzed_at,
                    created_at, updated_at
             FROM data_sources
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(Into::into))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_data_source_by_source_id(
        &self,
        source_id: &str,
    ) -> OxResult<Option<crate::models::DataSource>> {
        let row = sqlx::query_as::<_, DataSourceRow>(
            "SELECT id, workspace_id, source_id, kind, config,
                    last_analysis_snapshot, schema_fingerprints, last_analyzed_at,
                    created_at, updated_at
             FROM data_sources
             WHERE source_id = $1",
        )
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(Into::into))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_data_sources(&self) -> OxResult<Vec<crate::models::DataSource>> {
        let rows = sqlx::query_as::<_, DataSourceRow>(
            "SELECT id, workspace_id, source_id, kind, config,
                    last_analysis_snapshot, schema_fingerprints, last_analyzed_at,
                    created_at, updated_at
             FROM data_sources
             ORDER BY source_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_data_source_by_source_id(
        &self,
        source_id: &str,
        kind: &str,
        config: &serde_json::Value,
    ) -> OxResult<crate::models::DataSource> {
        super::require_workspace_context()?;
        // ON CONFLICT on (workspace_id, source_id) — the schema-level
        // identity constraint for registered sources. The conflicting row's
        // workspace_id must match the current session's workspace_id
        // because RLS is enforced against the row already; DO UPDATE
        // therefore only replaces rows the caller is allowed to see.
        let row = sqlx::query_as::<_, DataSourceRow>(
            "INSERT INTO data_sources (source_id, kind, config)
             VALUES ($1, $2, $3)
             ON CONFLICT (workspace_id, source_id) DO UPDATE
                SET kind = EXCLUDED.kind,
                    config = EXCLUDED.config,
                    updated_at = NOW()
             RETURNING id, workspace_id, source_id, kind, config,
                       last_analysis_snapshot, schema_fingerprints,
                       last_analyzed_at, created_at, updated_at",
        )
        .bind(source_id)
        .bind(kind)
        .bind(config)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_data_source_by_source_id(&self, source_id: &str) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM data_sources WHERE source_id = $1")
            .bind(source_id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_data_source_analysis(
        &self,
        source_id: &str,
        snapshot: &serde_json::Value,
        fingerprints: &std::collections::BTreeMap<String, ox_core::SchemaFingerprint>,
    ) -> OxResult<crate::models::DataSource> {
        super::require_workspace_context()?;
        // RLS scopes the update — we never need to bind workspace_id
        // explicitly. Returns the row so callers can surface the
        // freshly-stamped `last_analyzed_at`.
        let row = sqlx::query_as::<_, DataSourceRow>(
            "UPDATE data_sources
                SET last_analysis_snapshot = $2,
                    schema_fingerprints = $3,
                    last_analyzed_at = NOW(),
                    updated_at = NOW()
              WHERE source_id = $1
             RETURNING id, workspace_id, source_id, kind, config,
                       last_analysis_snapshot, schema_fingerprints, last_analyzed_at,
                       created_at, updated_at",
        )
        .bind(source_id)
        .bind(snapshot)
        .bind(sqlx::types::Json(fingerprints))
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }
}

//! [`SourceMappingArtifactStore`] — content-addressed store for the
//! source-to-IR mapping artifacts described in ADR 0011. Inserts
//! collapse on the `(workspace_id, source_id, schema_snapshot_hash,
//! content_hash)` unique constraint so re-running the design action
//! against an unchanged schema replays the previous artifact instead
//! of writing a duplicate row.

use super::*;

use ox_ontology::source_mapping::{
    SourceMappingArtifact, SourceMappingArtifactId,
};

#[async_trait]
impl SourceMappingArtifactStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_artifact(
        &self,
        artifact: SourceMappingArtifact,
    ) -> OxResult<SourceMappingArtifact> {
        super::require_workspace_context()?;
        let body = serde_json::to_value(&artifact)?;
        let content_hash = artifact.content_hash();

        let inserted: Option<(serde_json::Value,)> = sqlx::query_as(
            "INSERT INTO source_mapping_artifacts
                (id, source_id, schema_snapshot_hash, content_hash, body,
                 created_at, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (workspace_id, source_id, schema_snapshot_hash, content_hash)
             DO NOTHING
             RETURNING body",
        )
        .bind(artifact.id.as_str())
        .bind(&artifact.source_id.0)
        .bind(&artifact.schema_snapshot_hash)
        .bind(&content_hash)
        .bind(&body)
        .bind(artifact.created_at)
        .bind(&artifact.created_by)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if let Some((row,)) = inserted {
            return Ok(serde_json::from_value(row)?);
        }

        // Conflict path — body already exists. Re-read the row to
        // return the canonical persisted shape (preserves whatever
        // id was minted on the first insert, which may differ from
        // the caller's optimistic id).
        let existing: Option<(serde_json::Value,)> = sqlx::query_as(
            "SELECT body FROM source_mapping_artifacts
             WHERE source_id = $1
               AND schema_snapshot_hash = $2
               AND content_hash = $3",
        )
        .bind(&artifact.source_id.0)
        .bind(&artifact.schema_snapshot_hash)
        .bind(&content_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        match existing {
            Some((row,)) => Ok(serde_json::from_value(row)?),
            None => Err(OxError::Runtime {
                message: "INSERT … ON CONFLICT DO NOTHING returned no row \
                          and the conflicting row could not be re-read"
                    .to_string(),
            }),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_artifact(
        &self,
        id: &SourceMappingArtifactId,
    ) -> OxResult<Option<SourceMappingArtifact>> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            "SELECT body FROM source_mapping_artifacts WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        match row {
            None => Ok(None),
            Some((body,)) => Ok(Some(serde_json::from_value(body)?)),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_artifacts_by_source(
        &self,
        source_id: &str,
        limit: i64,
    ) -> OxResult<Vec<SourceMappingArtifact>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            "SELECT body FROM source_mapping_artifacts
             WHERE source_id = $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(source_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        rows.into_iter()
            .map(|(body,)| Ok(serde_json::from_value(body)?))
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_artifact(
        &self,
        id: &SourceMappingArtifactId,
    ) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result =
            sqlx::query("DELETE FROM source_mapping_artifacts WHERE id = $1")
                .bind(id.as_str())
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

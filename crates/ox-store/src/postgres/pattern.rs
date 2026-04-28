//! [`PatternStore`] — saved query patterns (workspace-scoped PatternIR library).

use super::*;

#[async_trait]
impl PatternStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_pattern(&self, p: &SavedQueryPattern) -> OxResult<()> {
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM saved_query_patterns WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

//! [`ReportStore`] — saved reports — parameterised query templates with widget bindings.

use super::*;

#[async_trait]
impl ReportStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_report(&self, r: &SavedReport) -> OxResult<()> {
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM saved_reports WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

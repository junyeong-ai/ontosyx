//! [`QueryStore`] — query executions — natural language questions → compiled query → results.

use super::*;

#[async_trait]
impl QueryStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_query_execution(&self, exec: &QueryExecution) -> OxResult<()> {
        super::require_workspace_context()?;
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
        // trick no longer substitutes for `get_ontology_ir`. Callers
        // that need the IR follow up with
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
        id: Uuid,
        user_id: &str,
        feedback: Option<&str>,
    ) -> OxResult<bool> {
        super::require_workspace_context()?;
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

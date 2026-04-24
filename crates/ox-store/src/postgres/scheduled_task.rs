//! [`ScheduledTaskStore`] — cron-driven recipe runs with last-run/next-run tracking.

use super::*;

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

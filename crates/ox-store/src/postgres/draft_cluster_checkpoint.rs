//! [`DraftClusterCheckpointStore`] — per-cluster checkpoint cache
//! for `design_ontology_batch` (ADR-0027).
//!
//! Workspace isolation rides RLS — every read/write below carries
//! the `super::require_workspace_context()?` guard so a missing
//! `WORKSPACE_ID` task-local fails loudly instead of leaking across
//! tenants. The cleanup sweep is the one exception: it runs under
//! `SYSTEM_BYPASS::scope(true, …)` from the cron driver and uses
//! the bypass-policy branch on the table.

use super::*;

#[async_trait]
impl DraftClusterCheckpointStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_draft_cluster_checkpoint(
        &self,
        c: &DraftClusterCheckpointRow,
    ) -> OxResult<()> {
        // Bind the row's `workspace_id` from the active task-local
        // rather than the caller-supplied field — RLS enforces that
        // `workspace_id = current_setting('app.workspace_id')`, so a
        // mismatch between the two would 42501 even when the caller
        // intended the same workspace. ADR-0039: stamp every DML row
        // with the bound id (never trust user-provided workspace_id).
        let workspace_id = super::bound_workspace_id_for_dml()?;
        // Split insert/update instead of `INSERT … ON CONFLICT DO
        // UPDATE`. The UPSERT shape was failing under RLS in the
        // sqlx 0.8 + PgPool path: `before_acquire` did not always set
        // `app.workspace_id` on the per-acquire connection (likely a
        // task-local-vs-pool-driver interaction), and Postgres
        // evaluates the WITH CHECK against `current_setting(...)` for
        // *both* the INSERT and the UPDATE arm of `ON CONFLICT DO
        // UPDATE`, so a missed scope on the UPDATE arm 42501s even
        // when the INSERT WITH CHECK would have passed.
        //
        // The split-write pattern is RLS-clean: each statement is
        // either pure INSERT (USING never evaluated) or pure UPDATE
        // (USING + WITH CHECK on a single row that already exists in
        // the workspace by RLS construction). Idempotency rides on
        // the natural-key UNIQUE constraint plus the lookup-then-
        // mutate sequence — concurrent identical writes serialise via
        // PG's row-level locking.
        let existing_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM draft_cluster_checkpoints
             WHERE project_id = $1 AND source_id = $2 AND signature = $3",
        )
        .bind(c.project_id)
        .bind(&c.source_id)
        .bind(&c.signature)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if let Some(existing_id) = existing_id {
            sqlx::query(
                "UPDATE draft_cluster_checkpoints
                 SET cluster_id = $1, output = $2, created_at = $3,
                     expires_at = $4
                 WHERE id = $5",
            )
            .bind(c.cluster_id)
            .bind(&c.output)
            .bind(c.created_at)
            .bind(c.expires_at)
            .bind(existing_id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        } else {
            sqlx::query(
                "INSERT INTO draft_cluster_checkpoints
                    (id, workspace_id, project_id, source_id, signature,
                     cluster_id, output, created_at, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(c.id)
            .bind(workspace_id)
            .bind(c.project_id)
            .bind(&c.source_id)
            .bind(&c.signature)
            .bind(c.cluster_id)
            .bind(&c.output)
            .bind(c.created_at)
            .bind(c.expires_at)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_draft_cluster_checkpoint_by_signature(
        &self,
        project_id: Uuid,
        source_id: &str,
        signature: &str,
    ) -> OxResult<Option<DraftClusterCheckpointRow>> {
        super::require_workspace_context()?;
        sqlx::query_as(
            "SELECT * FROM draft_cluster_checkpoints
             WHERE project_id = $1 AND source_id = $2 AND signature = $3",
        )
        .bind(project_id)
        .bind(source_id)
        .bind(signature)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_draft_cluster_checkpoints_for_project(
        &self,
        project_id: Uuid,
    ) -> OxResult<Vec<DraftClusterCheckpointRow>> {
        super::require_workspace_context()?;
        sqlx::query_as(
            "SELECT * FROM draft_cluster_checkpoints
             WHERE project_id = $1
             ORDER BY created_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_expired_draft_cluster_checkpoints(&self) -> OxResult<u64> {
        // Cron-driven cleanup runs under SYSTEM_BYPASS::scope; the
        // RLS policy whitelists `app.system_bypass = 'true'` so the
        // sweep sees every workspace. No explicit context guard
        // here — that would reject the cron driver's bypass scope.
        let result = sqlx::query(
            "DELETE FROM draft_cluster_checkpoints WHERE expires_at < now()",
        )
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_draft_cluster_checkpoints_for_project(
        &self,
        project_id: Uuid,
    ) -> OxResult<u64> {
        super::require_workspace_context()?;
        let result = sqlx::query(
            "DELETE FROM draft_cluster_checkpoints WHERE project_id = $1",
        )
        .bind(project_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}

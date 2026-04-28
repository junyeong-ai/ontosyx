//! [`PerspectiveStore`] — workbench perspectives — user × ontology × topology-signature.

use super::*;

#[async_trait]
impl PerspectiveStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_perspective(&self, p: &WorkbenchPerspective) -> OxResult<()> {
        super::require_workspace_context()?;
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
        super::require_workspace_context()?;
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

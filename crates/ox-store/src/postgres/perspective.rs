//! [`PerspectiveStore`] — workbench perspectives — user × ontology × topology-signature.

use super::*;

#[derive(sqlx::FromRow)]
struct WorkbenchPerspectiveRow {
    id: Uuid,
    user_id: String,
    workspace_id: Uuid,
    lineage_id: String,
    topology_signature: String,
    ontology_draft_id: Option<Uuid>,
    name: String,
    positions: sqlx::types::Json<std::collections::BTreeMap<String, CanvasPosition>>,
    viewport: sqlx::types::Json<CanvasViewport>,
    filters: serde_json::Value,
    collapsed_groups: sqlx::types::Json<Vec<String>>,
    is_default: bool,
    retrieval_profile_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<WorkbenchPerspectiveRow> for WorkbenchPerspective {
    fn from(row: WorkbenchPerspectiveRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            workspace_id: row.workspace_id,
            lineage_id: row.lineage_id,
            topology_signature: row.topology_signature,
            ontology_draft_id: row.ontology_draft_id,
            name: row.name,
            positions: row.positions.0,
            viewport: row.viewport.0,
            filters: row.filters,
            collapsed_groups: row.collapsed_groups.0,
            is_default: row.is_default,
            retrieval_profile_id: row
                .retrieval_profile_id
                .map(ox_ontology::RetrievalProfileId::new),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

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
             (id, user_id, lineage_id, topology_signature, ontology_draft_id,
              name, positions, viewport, filters, collapsed_groups,
              is_default, retrieval_profile_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
             ON CONFLICT (user_id, lineage_id, name)
             DO UPDATE SET
                topology_signature = EXCLUDED.topology_signature,
                ontology_draft_id = EXCLUDED.ontology_draft_id,
                positions = EXCLUDED.positions,
                viewport = EXCLUDED.viewport,
                filters = EXCLUDED.filters,
                collapsed_groups = EXCLUDED.collapsed_groups,
                is_default = EXCLUDED.is_default,
                retrieval_profile_id = EXCLUDED.retrieval_profile_id,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(p.id)
        .bind(&p.user_id)
        .bind(&p.lineage_id)
        .bind(&p.topology_signature)
        .bind(p.ontology_draft_id)
        .bind(&p.name)
        .bind(sqlx::types::Json(&p.positions))
        .bind(sqlx::types::Json(&p.viewport))
        .bind(&p.filters)
        .bind(sqlx::types::Json(&p.collapsed_groups))
        .bind(p.is_default)
        .bind(p.retrieval_profile_id.as_ref().map(|id| id.as_str()))
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
        let row = sqlx::query_as::<_, WorkbenchPerspectiveRow>(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND lineage_id = $2 AND name = $3",
        )
        .bind(user_id)
        .bind(lineage_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(WorkbenchPerspective::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_default_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Option<WorkbenchPerspective>> {
        let row = sqlx::query_as::<_, WorkbenchPerspectiveRow>(
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
        Ok(row.map(WorkbenchPerspective::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_best_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        topology_signature: &str,
    ) -> OxResult<Option<WorkbenchPerspective>> {
        // Tier 1: exact lineage match (same ontology lineage)
        let exact = sqlx::query_as::<_, WorkbenchPerspectiveRow>(
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

        if let Some(exact) = exact {
            return Ok(Some(WorkbenchPerspective::from(exact)));
        }

        // Tier 2: topology match (different lineage but same structural shape)
        let topology_match = sqlx::query_as::<_, WorkbenchPerspectiveRow>(
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

        Ok(topology_match.map(WorkbenchPerspective::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_perspectives(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Vec<WorkbenchPerspective>> {
        let rows: Vec<WorkbenchPerspectiveRow> = sqlx::query_as(
            "SELECT * FROM workbench_perspectives
             WHERE user_id = $1 AND lineage_id = $2
             ORDER BY is_default DESC, updated_at DESC",
        )
        .bind(user_id)
        .bind(lineage_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(WorkbenchPerspective::from).collect())
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

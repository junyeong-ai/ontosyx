//! [`LineageStore`] — data lineage records (graph label × source table × load run).

use super::*;

#[async_trait]
impl LineageStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_lineage_entry(&self, e: &LineageEntry) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO data_lineage
             (id, ontology_draft_id, graph_label, graph_element_type, source_type,
              source_name, source_table, source_columns, load_plan_hash,
              property_mappings, record_count, loaded_by, started_at, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(e.id)
        .bind(e.ontology_draft_id)
        .bind(&e.graph_label)
        .bind(&e.graph_element_type)
        .bind(&e.source_type)
        .bind(&e.source_name)
        .bind(&e.source_table)
        .bind(&e.source_columns)
        .bind(&e.load_plan_hash)
        .bind(&e.property_mappings)
        .bind(e.record_count)
        .bind(e.loaded_by)
        .bind(e.started_at)
        .bind(&e.status)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn complete_lineage_entry(
        &self,
        id: Uuid,
        record_count: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE data_lineage
             SET record_count = $2, status = $3, error_message = $4, completed_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(record_count)
        .bind(status)
        .bind(error_message)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_lineage_for_label(&self, graph_label: &str) -> OxResult<Vec<LineageEntry>> {
        sqlx::query_as("SELECT * FROM data_lineage WHERE graph_label = $1 ORDER BY started_at DESC")
            .bind(graph_label)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_lineage_for_ontology_draft(&self, ontology_draft_id: Uuid) -> OxResult<Vec<LineageEntry>> {
        sqlx::query_as("SELECT * FROM data_lineage WHERE ontology_draft_id = $1 ORDER BY started_at DESC")
            .bind(ontology_draft_id)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn lineage_summary(&self) -> OxResult<Vec<LineageSummary>> {
        sqlx::query_as(
            "SELECT
                graph_label,
                graph_element_type,
                COUNT(*) AS source_count,
                COALESCE(SUM(record_count), 0) AS total_records,
                MAX(completed_at) AS last_loaded_at
             FROM data_lineage
             WHERE status = 'completed'
             GROUP BY graph_label, graph_element_type
             ORDER BY total_records DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

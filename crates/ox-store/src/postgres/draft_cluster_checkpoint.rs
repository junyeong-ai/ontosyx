//! [`DraftClusterCheckpointStore`] — per-cluster checkpoint cache
//! for `design_ontology_batch`.
//!
//! Workspace isolation rides RLS — every read/write below carries
//! the `super::require_workspace_context()?` guard so a missing
//! `WORKSPACE_ID` task-local fails loudly instead of leaking across
//! tenants. The cleanup sweep is the one exception: it runs under
//! `SYSTEM_BYPASS::scope(true, …)` from the cron driver and uses
//! the bypass-policy branch on the table.

use ox_ontology::cluster_checkpoint::{ClusterSignature, DraftClusterCheckpoint};
use ox_ontology::input::InputOntologyDef;

use super::*;

/// Crate-private row mirror for `draft_cluster_checkpoints`. Lives
/// here only because sqlx's `FromRow` cannot decode the typed
/// `InputOntologyDef` directly off the JSONB column —
/// [`Self::into_domain`] lifts the row to the canonical
/// [`DraftClusterCheckpoint`] in one place.
#[derive(sqlx::FromRow)]
struct DraftClusterCheckpointRow {
    id: Uuid,
    workspace_id: Uuid,
    ontology_draft_id: Uuid,
    source_id: String,
    signature: String,
    cluster_id: i32,
    output: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl DraftClusterCheckpointRow {
    fn into_domain(self) -> OxResult<DraftClusterCheckpoint> {
        let output: InputOntologyDef =
            serde_json::from_value(self.output).map_err(|e| OxError::Runtime {
                message: format!("draft cluster checkpoint output parse failed: {e}"),
            })?;
        let signature =
            ClusterSignature::from_hex(self.signature).map_err(|e| OxError::Runtime {
                message: format!("draft cluster checkpoint signature parse failed: {e}"),
            })?;
        Ok(DraftClusterCheckpoint {
            id: Some(self.id),
            workspace_id: Some(self.workspace_id),
            ontology_draft_id: self.ontology_draft_id,
            source_id: self.source_id,
            signature,
            cluster_id: self.cluster_id as usize,
            output,
            created_at: self.created_at,
            expires_at: self.expires_at,
        })
    }
}

#[async_trait]
impl DraftClusterCheckpointStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_draft_cluster_checkpoint(&self, c: &DraftClusterCheckpoint) -> OxResult<()> {
        // Bind workspace_id from the active task-local rather than
        // the caller-supplied field — RLS enforces row.workspace_id =
        // current_setting('app.workspace_id'), so a mismatch would
        // 42501 even when the caller intended the same workspace.
        // The table's `id` column carries `DEFAULT gen_random_uuid()`
        // so the surrogate key falls out of the schema.
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let output_json = serde_json::to_value(&c.output).map_err(|e| OxError::Runtime {
            message: format!("draft cluster checkpoint output serialise failed: {e}"),
        })?;
        let cluster_id = i32::try_from(c.cluster_id).map_err(|_| OxError::Runtime {
            message: format!("cluster_id {} exceeds i32 range", c.cluster_id),
        })?;
        sqlx::query(
            "INSERT INTO draft_cluster_checkpoints
                (workspace_id, ontology_draft_id, source_id, signature,
                 cluster_id, output, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (workspace_id, ontology_draft_id, source_id, signature)
             DO UPDATE SET
                cluster_id = EXCLUDED.cluster_id,
                output = EXCLUDED.output,
                created_at = EXCLUDED.created_at,
                expires_at = EXCLUDED.expires_at",
        )
        .bind(workspace_id)
        .bind(c.ontology_draft_id)
        .bind(&c.source_id)
        .bind(c.signature.as_str())
        .bind(cluster_id)
        .bind(output_json)
        .bind(c.created_at)
        .bind(c.expires_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_draft_cluster_checkpoint_by_signature(
        &self,
        ontology_draft_id: Uuid,
        source_id: &str,
        signature: &str,
    ) -> OxResult<Option<DraftClusterCheckpoint>> {
        super::require_workspace_context()?;
        let row: Option<DraftClusterCheckpointRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_draft_id, source_id, signature,
                    cluster_id, output, created_at, expires_at
             FROM draft_cluster_checkpoints
             WHERE ontology_draft_id = $1 AND source_id = $2 AND signature = $3",
        )
        .bind(ontology_draft_id)
        .bind(source_id)
        .bind(signature)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(DraftClusterCheckpointRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_draft_cluster_checkpoints_by_project(
        &self,
        ontology_draft_id: Uuid,
    ) -> OxResult<Vec<DraftClusterCheckpoint>> {
        super::require_workspace_context()?;
        let rows: Vec<DraftClusterCheckpointRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_draft_id, source_id, signature,
                    cluster_id, output, created_at, expires_at
             FROM draft_cluster_checkpoints
             WHERE ontology_draft_id = $1
             ORDER BY created_at DESC",
        )
        .bind(ontology_draft_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(DraftClusterCheckpointRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn sweep_expired_draft_cluster_checkpoints(&self) -> OxResult<u64> {
        // Cron-driven cleanup runs under SYSTEM_BYPASS::scope; the
        // RLS policy whitelists `app.system_bypass = 'true'` so the
        // sweep sees every workspace.
        let result = sqlx::query("DELETE FROM draft_cluster_checkpoints WHERE expires_at < now()")
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_draft_cluster_checkpoints_by_project(
        &self,
        ontology_draft_id: Uuid,
    ) -> OxResult<u64> {
        super::require_workspace_context()?;
        let result =
            sqlx::query("DELETE FROM draft_cluster_checkpoints WHERE ontology_draft_id = $1")
                .bind(ontology_draft_id)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}

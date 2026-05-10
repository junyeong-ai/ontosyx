//! [`SourceContractStore`] postgres impl.

use async_trait::async_trait;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{ColumnSpec, SourceContractDef, mapping::SourceId};

use crate::store::SourceContractStore;

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct SourceContractRow {
    workspace_id: uuid::Uuid,
    source_id: String,
    relation: String,
    columns: serde_json::Value,
    primary_key: serde_json::Value,
    fingerprint: String,
    introspected_at: chrono::DateTime<chrono::Utc>,
}

impl SourceContractRow {
    fn into_domain(self) -> OxResult<SourceContractDef> {
        let columns: Vec<ColumnSpec> =
            serde_json::from_value(self.columns).map_err(|e| OxError::Runtime {
                message: format!("decode source_contracts.columns failed: {e}"),
            })?;
        let primary_key: Vec<String> =
            serde_json::from_value(self.primary_key).map_err(|e| OxError::Runtime {
                message: format!("decode source_contracts.primary_key failed: {e}"),
            })?;
        let _ = self.workspace_id; // RLS-bound; carried for parity but not surfaced.
        Ok(SourceContractDef {
            source_id: SourceId::new(self.source_id),
            relation: self.relation,
            columns,
            primary_key,
            fingerprint: self.fingerprint,
            introspected_at: self.introspected_at,
        })
    }
}

#[async_trait]
impl SourceContractStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(
        source_id = %contract.source_id.as_str(),
        relation = %contract.relation,
    ))]
    async fn upsert_source_contract(
        &self,
        contract: &SourceContractDef,
    ) -> OxResult<SourceContractDef> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let columns_json =
            serde_json::to_value(&contract.columns).map_err(|e| OxError::Runtime {
                message: format!("encode SourceContractDef.columns failed: {e}"),
            })?;
        let pk_json =
            serde_json::to_value(&contract.primary_key).map_err(|e| OxError::Runtime {
                message: format!("encode SourceContractDef.primary_key failed: {e}"),
            })?;
        // Recompute the fingerprint server-side so the persisted
        // value can never drift from the canonical formula. Clients
        // that submit a stale fingerprint silently get the
        // canonical value back on the row.
        let fingerprint =
            SourceContractDef::compute_fingerprint(&contract.columns, &contract.primary_key);
        let row: SourceContractRow = sqlx::query_as(
            "INSERT INTO source_contracts
                (workspace_id, source_id, relation,
                 columns, primary_key, fingerprint, introspected_at)
             VALUES ($1, $2, $3, $4, $5, $6, now())
             ON CONFLICT (workspace_id, source_id, relation) DO UPDATE SET
                columns = EXCLUDED.columns,
                primary_key = EXCLUDED.primary_key,
                fingerprint = EXCLUDED.fingerprint,
                introspected_at = now()
             RETURNING workspace_id, source_id, relation,
                       columns, primary_key, fingerprint, introspected_at",
        )
        .bind(workspace_id)
        .bind(contract.source_id.as_str())
        .bind(&contract.relation)
        .bind(&columns_json)
        .bind(&pk_json)
        .bind(&fingerprint)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        source_id = %source_id.as_str(),
        relation = %relation,
    ))]
    async fn find_source_contract(
        &self,
        source_id: &SourceId,
        relation: &str,
    ) -> OxResult<Option<SourceContractDef>> {
        super::require_workspace_context()?;
        let row: Option<SourceContractRow> = sqlx::query_as(
            "SELECT workspace_id, source_id, relation,
                    columns, primary_key, fingerprint, introspected_at
             FROM source_contracts
             WHERE source_id = $1 AND relation = $2
             LIMIT 1",
        )
        .bind(source_id.as_str())
        .bind(relation)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(SourceContractRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_source_contracts(&self) -> OxResult<Vec<SourceContractDef>> {
        super::require_workspace_context()?;
        let rows: Vec<SourceContractRow> = sqlx::query_as(
            "SELECT workspace_id, source_id, relation,
                    columns, primary_key, fingerprint, introspected_at
             FROM source_contracts
             ORDER BY source_id ASC, relation ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(SourceContractRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(source_id = %source_id.as_str()))]
    async fn list_source_contracts_for_source(
        &self,
        source_id: &SourceId,
    ) -> OxResult<Vec<SourceContractDef>> {
        super::require_workspace_context()?;
        let rows: Vec<SourceContractRow> = sqlx::query_as(
            "SELECT workspace_id, source_id, relation,
                    columns, primary_key, fingerprint, introspected_at
             FROM source_contracts
             WHERE source_id = $1
             ORDER BY relation ASC",
        )
        .bind(source_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(SourceContractRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        source_id = %source_id.as_str(),
        relation = %relation,
    ))]
    async fn delete_source_contract(&self, source_id: &SourceId, relation: &str) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result =
            sqlx::query("DELETE FROM source_contracts WHERE source_id = $1 AND relation = $2")
                .bind(source_id.as_str())
                .bind(relation)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

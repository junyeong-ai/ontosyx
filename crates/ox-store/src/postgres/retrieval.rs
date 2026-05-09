//! [`RetrievalProfileStore`] postgres impl.

use async_trait::async_trait;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{
    RetrievalLimits, RetrievalProfile, RetrievalProfileId, TraversalStrategy,
};

use crate::store::RetrievalProfileStore;

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct RetrievalProfileRow {
    id: String,
    workspace_id: uuid::Uuid,
    name: String,
    description: String,
    edge_weights: serde_json::Value,
    default_edge_weight: f64,
    traversal: serde_json::Value,
    limits: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl RetrievalProfileRow {
    fn into_domain(self) -> OxResult<RetrievalProfile> {
        let edge_weights = serde_json::from_value(self.edge_weights).map_err(|e| {
            OxError::Runtime {
                message: format!("decode retrieval_profiles.edge_weights failed: {e}"),
            }
        })?;
        let traversal: TraversalStrategy =
            serde_json::from_value(self.traversal).map_err(|e| OxError::Runtime {
                message: format!("decode retrieval_profiles.traversal failed: {e}"),
            })?;
        let limits: RetrievalLimits =
            serde_json::from_value(self.limits).map_err(|e| OxError::Runtime {
                message: format!("decode retrieval_profiles.limits failed: {e}"),
            })?;
        Ok(RetrievalProfile {
            id: RetrievalProfileId::new(self.id),
            workspace_id: self.workspace_id,
            name: self.name,
            description: self.description,
            edge_weights,
            default_edge_weight: self.default_edge_weight as f32,
            traversal,
            limits,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl RetrievalProfileStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(profile.name = %profile.name))]
    async fn upsert_retrieval_profile(
        &self,
        profile: &RetrievalProfile,
    ) -> OxResult<RetrievalProfile> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let edge_weights = serde_json::to_value(&profile.edge_weights).map_err(|e| {
            OxError::Runtime {
                message: format!("encode RetrievalProfile.edge_weights failed: {e}"),
            }
        })?;
        let traversal = serde_json::to_value(&profile.traversal).map_err(|e| OxError::Runtime {
            message: format!("encode RetrievalProfile.traversal failed: {e}"),
        })?;
        let limits = serde_json::to_value(profile.limits).map_err(|e| OxError::Runtime {
            message: format!("encode RetrievalProfile.limits failed: {e}"),
        })?;
        let row: RetrievalProfileRow = sqlx::query_as(
            "INSERT INTO retrieval_profiles
                (id, workspace_id, name, description,
                 edge_weights, default_edge_weight, traversal, limits,
                 created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now())
             ON CONFLICT (workspace_id, name) DO UPDATE SET
                description = EXCLUDED.description,
                edge_weights = EXCLUDED.edge_weights,
                default_edge_weight = EXCLUDED.default_edge_weight,
                traversal = EXCLUDED.traversal,
                limits = EXCLUDED.limits,
                updated_at = now()
             RETURNING id, workspace_id, name, description,
                       edge_weights, default_edge_weight, traversal, limits,
                       created_at, updated_at",
        )
        .bind(profile.id.as_str())
        .bind(workspace_id)
        .bind(&profile.name)
        .bind(&profile.description)
        .bind(&edge_weights)
        .bind(profile.default_edge_weight as f64)
        .bind(&traversal)
        .bind(&limits)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(profile.id = %id.as_str()))]
    async fn get_retrieval_profile(
        &self,
        id: &RetrievalProfileId,
    ) -> OxResult<Option<RetrievalProfile>> {
        super::require_workspace_context()?;
        let row: Option<RetrievalProfileRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description,
                    edge_weights, default_edge_weight, traversal, limits,
                    created_at, updated_at
             FROM retrieval_profiles WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(RetrievalProfileRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(profile.name = %name))]
    async fn find_retrieval_profile_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<RetrievalProfile>> {
        super::require_workspace_context()?;
        let row: Option<RetrievalProfileRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description,
                    edge_weights, default_edge_weight, traversal, limits,
                    created_at, updated_at
             FROM retrieval_profiles
             WHERE name = $1
             LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(RetrievalProfileRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_retrieval_profiles(&self) -> OxResult<Vec<RetrievalProfile>> {
        super::require_workspace_context()?;
        let rows: Vec<RetrievalProfileRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description,
                    edge_weights, default_edge_weight, traversal, limits,
                    created_at, updated_at
             FROM retrieval_profiles
             ORDER BY updated_at DESC, id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(RetrievalProfileRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(profile.id = %id.as_str()))]
    async fn delete_retrieval_profile(&self, id: &RetrievalProfileId) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM retrieval_profiles WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

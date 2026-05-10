//! [`CommunityDetectionPolicyStore`] postgres impl.

use async_trait::async_trait;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{CommunityDetectionPolicy, CommunityDetectionPolicyId};

use crate::store::CommunityDetectionPolicyStore;

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct CommunityDetectionPolicyRow {
    id: String,
    workspace_id: uuid::Uuid,
    name: String,
    description: String,
    resolution: f64,
    seed: i64,
    levels: i16,
    min_cluster_size: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl CommunityDetectionPolicyRow {
    fn into_domain(self) -> OxResult<CommunityDetectionPolicy> {
        let levels = u8::try_from(self.levels.max(1)).map_err(|e| OxError::Runtime {
            message: format!("community_detection_policies.levels overflow: {e}"),
        })?;
        let min_cluster_size =
            u32::try_from(self.min_cluster_size.max(1)).map_err(|e| OxError::Runtime {
                message: format!("community_detection_policies.min_cluster_size overflow: {e}"),
            })?;
        Ok(CommunityDetectionPolicy {
            id: CommunityDetectionPolicyId::new(self.id),
            workspace_id: self.workspace_id,
            name: self.name,
            description: self.description,
            resolution: self.resolution as f32,
            seed: self.seed as u64,
            levels,
            min_cluster_size,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl CommunityDetectionPolicyStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(policy.name = %policy.name))]
    async fn upsert_community_detection_policy(
        &self,
        policy: &CommunityDetectionPolicy,
    ) -> OxResult<CommunityDetectionPolicy> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: CommunityDetectionPolicyRow = sqlx::query_as(
            "INSERT INTO community_detection_policies
                (id, workspace_id, name, description,
                 resolution, seed, levels, min_cluster_size,
                 created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now())
             ON CONFLICT (workspace_id, name) DO UPDATE SET
                description = EXCLUDED.description,
                resolution = EXCLUDED.resolution,
                seed = EXCLUDED.seed,
                levels = EXCLUDED.levels,
                min_cluster_size = EXCLUDED.min_cluster_size,
                updated_at = now()
             RETURNING id, workspace_id, name, description,
                       resolution, seed, levels, min_cluster_size,
                       created_at, updated_at",
        )
        .bind(policy.id.as_str())
        .bind(workspace_id)
        .bind(&policy.name)
        .bind(&policy.description)
        .bind(policy.resolution as f64)
        .bind(policy.seed as i64)
        .bind(policy.levels as i16)
        .bind(policy.min_cluster_size as i32)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(policy.id = %id.as_str()))]
    async fn get_community_detection_policy(
        &self,
        id: &CommunityDetectionPolicyId,
    ) -> OxResult<Option<CommunityDetectionPolicy>> {
        super::require_workspace_context()?;
        let row: Option<CommunityDetectionPolicyRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description,
                    resolution, seed, levels, min_cluster_size,
                    created_at, updated_at
             FROM community_detection_policies WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(CommunityDetectionPolicyRow::into_domain)
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(policy.name = %name))]
    async fn find_community_detection_policy_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<CommunityDetectionPolicy>> {
        super::require_workspace_context()?;
        let row: Option<CommunityDetectionPolicyRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description,
                    resolution, seed, levels, min_cluster_size,
                    created_at, updated_at
             FROM community_detection_policies
             WHERE name = $1
             LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(CommunityDetectionPolicyRow::into_domain)
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_community_detection_policies(&self) -> OxResult<Vec<CommunityDetectionPolicy>> {
        super::require_workspace_context()?;
        let rows: Vec<CommunityDetectionPolicyRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description,
                    resolution, seed, levels, min_cluster_size,
                    created_at, updated_at
             FROM community_detection_policies
             ORDER BY updated_at DESC, id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(CommunityDetectionPolicyRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(policy.id = %id.as_str()))]
    async fn delete_community_detection_policy(
        &self,
        id: &CommunityDetectionPolicyId,
    ) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM community_detection_policies WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

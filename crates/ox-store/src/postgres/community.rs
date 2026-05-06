//! [`CommunitySummaryStore`] postgres impl.
//!
//! Backed by `ontology_community_summaries` (migration 0013).
//! Single-row UPSERT on `(ontology_version_id, community_id)`;
//! list / search / reverse-lookup walk dedicated indexes
//! (gin_trgm on title + summary, gin array on member_logical_ids).

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::community::CommunitySummary;
use crate::store::CommunitySummaryStore;

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct CommunitySummaryRow {
    id: Uuid,
    workspace_id: Uuid,
    ontology_version_id: Uuid,
    community_id: String,
    level: i16,
    member_entity_kinds: Vec<String>,
    member_logical_ids: Vec<String>,
    title: String,
    summary: String,
    generated_at: chrono::DateTime<chrono::Utc>,
}

impl From<CommunitySummaryRow> for CommunitySummary {
    fn from(r: CommunitySummaryRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            ontology_version_id: r.ontology_version_id,
            community_id: r.community_id,
            level: r.level.max(0) as u32,
            member_entity_kinds: r.member_entity_kinds,
            member_logical_ids: r.member_logical_ids,
            title: r.title,
            summary: r.summary,
            generated_at: r.generated_at,
        }
    }
}

#[async_trait]
impl CommunitySummaryStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(
        community_id = %summary.community_id,
        version = %summary.ontology_version_id,
    ))]
    async fn upsert_community_summary(
        &self,
        summary: &CommunitySummary,
    ) -> OxResult<CommunitySummary> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        // Postgres `level` column is SMALLINT (i16); the
        // domain shape carries u32 because the platform
        // doesn't impose a depth ceiling, but in practice
        // Microsoft GraphRAG produces 3-5 levels — well
        // within i16 range. Saturating cast guards against
        // an operator stuffing u32::MAX.
        let level_i16: i16 = summary.level.try_into().unwrap_or(i16::MAX);
        let row: CommunitySummaryRow = sqlx::query_as(
            "INSERT INTO ontology_community_summaries
                (id, workspace_id, ontology_version_id, community_id,
                 level, member_entity_kinds, member_logical_ids,
                 title, summary, generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (ontology_version_id, community_id) DO UPDATE SET
                level = EXCLUDED.level,
                member_entity_kinds = EXCLUDED.member_entity_kinds,
                member_logical_ids = EXCLUDED.member_logical_ids,
                title = EXCLUDED.title,
                summary = EXCLUDED.summary,
                generated_at = EXCLUDED.generated_at
             RETURNING id, workspace_id, ontology_version_id, community_id,
                       level, member_entity_kinds, member_logical_ids,
                       title, summary, generated_at",
        )
        .bind(summary.id)
        .bind(workspace_id)
        .bind(summary.ontology_version_id)
        .bind(&summary.community_id)
        .bind(level_i16)
        .bind(&summary.member_entity_kinds)
        .bind(&summary.member_logical_ids)
        .bind(&summary.title)
        .bind(&summary.summary)
        .bind(summary.generated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(version = %version_id))]
    async fn list_community_summaries_for_version(
        &self,
        version_id: Uuid,
    ) -> OxResult<Vec<CommunitySummary>> {
        super::require_workspace_context()?;
        let rows: Vec<CommunitySummaryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, community_id,
                    level, member_entity_kinds, member_logical_ids,
                    title, summary, generated_at
             FROM ontology_community_summaries
             WHERE ontology_version_id = $1
             ORDER BY level ASC, community_id ASC",
        )
        .bind(version_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(CommunitySummary::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        version = %version_id, q = %query, top_k = top_k,
    ))]
    async fn search_community_summaries(
        &self,
        version_id: Uuid,
        query: &str,
        top_k: u32,
    ) -> OxResult<Vec<CommunitySummary>> {
        super::require_workspace_context()?;
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let capped = (top_k.max(1)).min(100) as i64;
        // Trigram blend: title-similarity weighted higher than
        // summary-similarity (`* 1.5`) — title is a tight
        // headline, summary is paragraph-shaped, and a query
        // hitting the title is a stronger anchor than the
        // same query in a longer summary. The
        // `similarity > 0.05` floor on each axis suppresses
        // pure noise matches; the GIN trgm index on both
        // columns keeps the WHERE branch cheap.
        let rows: Vec<CommunitySummaryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, community_id,
                    level, member_entity_kinds, member_logical_ids,
                    title, summary, generated_at
             FROM ontology_community_summaries
             WHERE ontology_version_id = $1
               AND (similarity(title, $2) > 0.05
                    OR similarity(summary, $2) > 0.05)
             ORDER BY (similarity(title, $2) * 1.5 + similarity(summary, $2)) DESC
             LIMIT $3",
        )
        .bind(version_id)
        .bind(query)
        .bind(capped)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(CommunitySummary::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        version = %version_id, kind = %entity_kind, logical_id = %logical_id,
    ))]
    async fn list_communities_for_entity(
        &self,
        version_id: Uuid,
        entity_kind: &str,
        logical_id: &str,
    ) -> OxResult<Vec<CommunitySummary>> {
        super::require_workspace_context()?;
        // The GIN array index on `member_logical_ids` answers
        // `@>` (contains) cheaply; the kind sub-array narrow
        // is a post-filter because the platform doesn't
        // index the parallel kind array (the logical id
        // already disambiguates within a version, kind is a
        // sanity guard).
        let rows: Vec<CommunitySummaryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, community_id,
                    level, member_entity_kinds, member_logical_ids,
                    title, summary, generated_at
             FROM ontology_community_summaries
             WHERE ontology_version_id = $1
               AND member_logical_ids @> ARRAY[$2]::text[]
               AND member_entity_kinds @> ARRAY[$3]::text[]
             ORDER BY level ASC, community_id ASC",
        )
        .bind(version_id)
        .bind(logical_id)
        .bind(entity_kind)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(CommunitySummary::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(community_summary_id = %id))]
    async fn delete_community_summary(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result =
            sqlx::query("DELETE FROM ontology_community_summaries WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

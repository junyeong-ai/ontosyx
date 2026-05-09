//! [`CommunitySummaryStore`] postgres impl.
//!
//! Backed by `ontology_community_summaries` from the schema baseline.
//! Single-row UPSERT on `(ontology_version_id, community_id)`;
//! list / search / reverse-lookup walk dedicated indexes
//! (gin_trgm on title + summary, gin array on member_logical_ids).

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};

use crate::community::CommunitySummary;
use crate::store::CommunitySummaryStore;

use ox_core::pgvector::format_vector as format_pgvector;

use super::{PostgresStore, to_ox_error};

const MAX_LEVEL: u32 = i16::MAX as u32;
const MAX_MEMBERS: usize = 10_000;

#[derive(sqlx::FromRow)]
struct CommunitySummaryRow {
    id: Uuid,
    workspace_id: Uuid,
    ontology_version_id: Uuid,
    community_id: String,
    level: i16,
    member_entity_kinds: Vec<String>,
    member_logical_ids: Vec<String>,
    member_fingerprint: String,
    title: String,
    summary: String,
    tokenized_text: String,
    tokenizer_dict_fingerprint: String,
    generated_at: chrono::DateTime<chrono::Utc>,
}

/// Columns for hydrating a `CommunitySummary` row. Lifted out
/// of the inline SQL strings so adding a new column lands in
/// one place; `embedding` is intentionally omitted because the
/// hot retrieval paths don't need to ship 1024-dim vectors over
/// the wire when the cosine NN is already evaluated server-side.
const COMMUNITY_SUMMARY_COLUMNS: &str =
    "id, workspace_id, ontology_version_id, community_id,
     level, member_entity_kinds, member_logical_ids,
     member_fingerprint, title, summary,
     tokenized_text, tokenizer_dict_fingerprint,
     generated_at";

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
            member_fingerprint: r.member_fingerprint,
            title: r.title,
            summary: r.summary,
            tokenized_text: r.tokenized_text,
            tokenizer_dict_fingerprint: r.tokenizer_dict_fingerprint,
            embedding: None,
            generated_at: r.generated_at,
        }
    }
}

fn validate_community_summary(summary: &CommunitySummary) -> OxResult<()> {
    if summary.community_id.trim().is_empty() {
        return Err(OxError::Validation {
            field: "community_id".into(),
            message: "community_id must not be empty".into(),
        });
    }
    if summary.level > MAX_LEVEL {
        return Err(OxError::Validation {
            field: "level".into(),
            message: format!("level must be between 0 and {MAX_LEVEL}"),
        });
    }
    if summary.member_entity_kinds.len() != summary.member_logical_ids.len() {
        return Err(OxError::Validation {
            field: "member_entity_kinds".into(),
            message: "member_entity_kinds and member_logical_ids must have the same length".into(),
        });
    }
    if summary.member_entity_kinds.len() > MAX_MEMBERS {
        return Err(OxError::Validation {
            field: "member_entity_kinds".into(),
            message: format!("community summaries support at most {MAX_MEMBERS} members"),
        });
    }
    if summary
        .member_entity_kinds
        .iter()
        .any(|kind| kind.trim().is_empty())
    {
        return Err(OxError::Validation {
            field: "member_entity_kinds".into(),
            message: "member_entity_kinds must not contain empty values".into(),
        });
    }
    if summary
        .member_logical_ids
        .iter()
        .any(|logical_id| logical_id.trim().is_empty())
    {
        return Err(OxError::Validation {
            field: "member_logical_ids".into(),
            message: "member_logical_ids must not contain empty values".into(),
        });
    }
    if summary.title.trim().is_empty() {
        return Err(OxError::Validation {
            field: "title".into(),
            message: "title must not be empty".into(),
        });
    }
    if summary.summary.trim().is_empty() {
        return Err(OxError::Validation {
            field: "summary".into(),
            message: "summary must not be empty".into(),
        });
    }
    if summary.member_fingerprint.trim().is_empty() {
        return Err(OxError::Validation {
            field: "member_fingerprint".into(),
            message: "member_fingerprint must not be empty — use \
                      `CommunitySummary::compute_member_fingerprint`"
                .into(),
        });
    }
    Ok(())
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
        validate_community_summary(summary)?;
        let workspace_id = super::bound_workspace_id_for_dml()?;
        // Postgres `level` column is SMALLINT. The store rejects
        // out-of-range values above, so this cast is exact rather
        // than silently saturating an invalid hierarchy depth.
        let level_i16 = summary.level as i16;
        let embedding_text = summary.embedding.as_ref().map(|v| format_pgvector(v));
        let row: CommunitySummaryRow = sqlx::query_as(
            "INSERT INTO ontology_community_summaries
                (id, workspace_id, ontology_version_id, community_id,
                 level, member_entity_kinds, member_logical_ids,
                 member_fingerprint, title, summary,
                 tokenized_text, tokenizer_dict_fingerprint,
                 embedding,
                 generated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::vector, $14)
             ON CONFLICT (ontology_version_id, community_id) DO UPDATE SET
                level = EXCLUDED.level,
                member_entity_kinds = EXCLUDED.member_entity_kinds,
                member_logical_ids = EXCLUDED.member_logical_ids,
                member_fingerprint = EXCLUDED.member_fingerprint,
                title = EXCLUDED.title,
                summary = EXCLUDED.summary,
                tokenized_text = EXCLUDED.tokenized_text,
                tokenizer_dict_fingerprint = EXCLUDED.tokenizer_dict_fingerprint,
                embedding = COALESCE(EXCLUDED.embedding, ontology_community_summaries.embedding),
                generated_at = EXCLUDED.generated_at
             RETURNING id, workspace_id, ontology_version_id, community_id,
                       level, member_entity_kinds, member_logical_ids,
                       member_fingerprint, title, summary,
                       tokenized_text, tokenizer_dict_fingerprint,
                       generated_at",
        )
        .bind(summary.id)
        .bind(workspace_id)
        .bind(summary.ontology_version_id)
        .bind(&summary.community_id)
        .bind(level_i16)
        .bind(&summary.member_entity_kinds)
        .bind(&summary.member_logical_ids)
        .bind(&summary.member_fingerprint)
        .bind(&summary.title)
        .bind(&summary.summary)
        .bind(&summary.tokenized_text)
        .bind(&summary.tokenizer_dict_fingerprint)
        .bind(embedding_text)
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
                member_fingerprint, title, summary,
                tokenized_text, tokenizer_dict_fingerprint,
                generated_at
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
        version = %version_id, question_len = question.len(), top_k = top_k,
    ))]
    async fn search_community_summaries_trigram_only(
        &self,
        version_id: Uuid,
        question: &str,
        top_k: u32,
    ) -> OxResult<Vec<CommunitySummary>> {
        super::require_workspace_context()?;
        if question.trim().is_empty() {
            return Ok(Vec::new());
        }
        let capped = top_k.clamp(1, 100) as i64;
        // Trigram-only baseline. Title-weighted blend (1.5×)
        // mirrors what the hybrid path's trgm CTE does, but no
        // FTS / vector / RRF — pure pg_trgm similarity.
        let rows: Vec<CommunitySummaryRow> = sqlx::query_as(&format!(
            "SELECT {COMMUNITY_SUMMARY_COLUMNS}
             FROM ontology_community_summaries
             WHERE ontology_version_id = $1
               AND (similarity(title, $2) > 0.05 OR similarity(summary, $2) > 0.05)
             ORDER BY (similarity(title, $2) * 1.5 + similarity(summary, $2)) DESC,
                      generated_at DESC
             LIMIT $3",
            COMMUNITY_SUMMARY_COLUMNS = COMMUNITY_SUMMARY_COLUMNS,
        ))
        .bind(version_id)
        .bind(question)
        .bind(capped)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(CommunitySummary::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        version = %version_id,
        question_len = question_raw.len(),
        tokenized_len = question_tokenized.len(),
        has_embedding = query_embedding.is_some(),
        top_k = top_k,
    ))]
    async fn search_community_summaries(
        &self,
        version_id: Uuid,
        question_raw: &str,
        question_tokenized: &str,
        query_embedding: Option<&[f32]>,
        top_k: u32,
    ) -> OxResult<Vec<CommunitySummary>> {
        super::require_workspace_context()?;
        if question_raw.trim().is_empty() && question_tokenized.trim().is_empty() {
            return Ok(Vec::new());
        }
        let capped = top_k.clamp(1, 100) as i64;
        let breadth = capped * super::RRF_CANDIDATE_BREADTH;
        let vector_text = query_embedding.map(format_pgvector);

        // RRF fusion of three rankers: trigram blend (title +
        // summary, title weighted 1.5×), tokenized FTS, and
        // optional pgvector cosine. Each ranker pulls `breadth`
        // candidates; final SELECT joins to ship the row.
        let sql_with_vec = format!(
            "WITH
            trgm AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY (similarity(title, $2) * 1.5 + similarity(summary, $2)) DESC,
                             generated_at DESC
                ) AS rk
                FROM ontology_community_summaries
                WHERE ontology_version_id = $1
                  AND (similarity(title, $2) > 0.05 OR similarity(summary, $2) > 0.05)
                LIMIT $5
            ),
            fts AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY ts_rank_cd(searchable_tsv, plainto_tsquery('simple', $3)) DESC,
                             generated_at DESC
                ) AS rk
                FROM ontology_community_summaries
                WHERE ontology_version_id = $1
                  AND searchable_tsv @@ plainto_tsquery('simple', $3)
                LIMIT $5
            ),
            vec AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY embedding <=> $4::vector ASC, generated_at DESC
                ) AS rk
                FROM ontology_community_summaries
                WHERE ontology_version_id = $1
                  AND embedding IS NOT NULL
                LIMIT $5
            ),
            ranks AS (
                SELECT id, rk FROM trgm
                UNION ALL SELECT id, rk FROM fts
                UNION ALL SELECT id, rk FROM vec
            ),
            fused AS (
                SELECT id, SUM(1.0 / ($6 + rk)::numeric) AS rrf_score
                FROM ranks
                GROUP BY id
            )
            SELECT {COMMUNITY_SUMMARY_COLUMNS}
            FROM fused f
            JOIN ontology_community_summaries c USING (id)
            ORDER BY f.rrf_score DESC, c.generated_at DESC
            LIMIT $7",
            COMMUNITY_SUMMARY_COLUMNS = COMMUNITY_SUMMARY_COLUMNS,
        );
        let sql_no_vec = format!(
            "WITH
            trgm AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY (similarity(title, $2) * 1.5 + similarity(summary, $2)) DESC,
                             generated_at DESC
                ) AS rk
                FROM ontology_community_summaries
                WHERE ontology_version_id = $1
                  AND (similarity(title, $2) > 0.05 OR similarity(summary, $2) > 0.05)
                LIMIT $4
            ),
            fts AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY ts_rank_cd(searchable_tsv, plainto_tsquery('simple', $3)) DESC,
                             generated_at DESC
                ) AS rk
                FROM ontology_community_summaries
                WHERE ontology_version_id = $1
                  AND searchable_tsv @@ plainto_tsquery('simple', $3)
                LIMIT $4
            ),
            ranks AS (
                SELECT id, rk FROM trgm
                UNION ALL SELECT id, rk FROM fts
            ),
            fused AS (
                SELECT id, SUM(1.0 / ($5 + rk)::numeric) AS rrf_score
                FROM ranks
                GROUP BY id
            )
            SELECT {COMMUNITY_SUMMARY_COLUMNS}
            FROM fused f
            JOIN ontology_community_summaries c USING (id)
            ORDER BY f.rrf_score DESC, c.generated_at DESC
            LIMIT $6",
            COMMUNITY_SUMMARY_COLUMNS = COMMUNITY_SUMMARY_COLUMNS,
        );

        let rows: Vec<CommunitySummaryRow> = match vector_text {
            Some(vec_text) => sqlx::query_as(&sql_with_vec)
                .bind(version_id)
                .bind(question_raw)
                .bind(question_tokenized)
                .bind(vec_text)
                .bind(breadth)
                .bind(super::RRF_K)
                .bind(capped)
                .fetch_all(&self.pool)
                .await,
            None => sqlx::query_as(&sql_no_vec)
                .bind(version_id)
                .bind(question_raw)
                .bind(question_tokenized)
                .bind(breadth)
                .bind(super::RRF_K)
                .bind(capped)
                .fetch_all(&self.pool)
                .await,
        }
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
        // The parallel arrays are a logical pair:
        // `member_entity_kinds[i]` + `member_logical_ids[i]`.
        // Keep the cheap `member_logical_ids @>` pre-filter for
        // the GIN index, then use WITH ORDINALITY to prove the
        // requested kind/id occur at the same position. Independent
        // array containment would false-positive when one member
        // has the kind and a different member has the logical id.
        let rows: Vec<CommunitySummaryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, community_id,
                level, member_entity_kinds, member_logical_ids,
                member_fingerprint, title, summary,
                tokenized_text, tokenizer_dict_fingerprint,
                generated_at
             FROM ontology_community_summaries
             WHERE ontology_version_id = $1
               AND member_logical_ids @> ARRAY[$2]::text[]
               AND EXISTS (
                   SELECT 1
                   FROM unnest(member_entity_kinds) WITH ORDINALITY AS kinds(kind, ord)
                   JOIN unnest(member_logical_ids) WITH ORDINALITY AS ids(logical_id, ord)
                     USING (ord)
                   WHERE kinds.kind = $3
                     AND ids.logical_id = $2
               )
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

    #[tracing::instrument(level = "debug", skip_all, fields(
        version = %version_id, community_id = %community_id,
    ))]
    async fn find_community_summary_by_natural_key(
        &self,
        version_id: Uuid,
        community_id: &str,
    ) -> OxResult<Option<CommunitySummary>> {
        super::require_workspace_context()?;
        let row: Option<CommunitySummaryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, community_id,
                level, member_entity_kinds, member_logical_ids,
                member_fingerprint, title, summary,
                tokenized_text, tokenizer_dict_fingerprint,
                generated_at
             FROM ontology_community_summaries
             WHERE ontology_version_id = $1 AND community_id = $2
             LIMIT 1",
        )
        .bind(version_id)
        .bind(community_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(CommunitySummary::from))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(community_summary_id = %id))]
    async fn delete_community_summary(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM ontology_community_summaries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_summary() -> CommunitySummary {
        let kinds = vec!["NodeType".to_string(), "Concept".to_string()];
        let logical = vec!["nt_customer".to_string(), "c_vip".to_string()];
        let fingerprint = CommunitySummary::compute_member_fingerprint(&kinds, &logical);
        CommunitySummary {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            ontology_version_id: Uuid::now_v7(),
            community_id: "leiden:0:7".into(),
            level: 0,
            member_entity_kinds: kinds,
            member_logical_ids: logical,
            member_fingerprint: fingerprint,
            title: "Premium customer cluster".into(),
            summary: "Customers with VIP tier and high-value order behavior.".into(),
            tokenized_text: String::new(),
            tokenizer_dict_fingerprint: String::new(),
            embedding: None,
            generated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn validates_member_fingerprint_non_empty() {
        let mut summary = valid_summary();
        summary.member_fingerprint.clear();
        assert!(validate_community_summary(&summary).is_err());
    }

    #[test]
    fn member_fingerprint_is_order_independent() {
        let kinds_a = vec!["NodeType".into(), "Concept".into()];
        let logical_a = vec!["nt_customer".into(), "c_vip".into()];
        let kinds_b = vec!["Concept".into(), "NodeType".into()];
        let logical_b = vec!["c_vip".into(), "nt_customer".into()];
        assert_eq!(
            CommunitySummary::compute_member_fingerprint(&kinds_a, &logical_a),
            CommunitySummary::compute_member_fingerprint(&kinds_b, &logical_b),
        );
    }

    #[test]
    fn member_fingerprint_changes_when_member_added() {
        let kinds_a = vec!["NodeType".into()];
        let logical_a = vec!["nt_customer".into()];
        let kinds_b = vec!["NodeType".into(), "Concept".into()];
        let logical_b = vec!["nt_customer".into(), "c_vip".into()];
        assert_ne!(
            CommunitySummary::compute_member_fingerprint(&kinds_a, &logical_a),
            CommunitySummary::compute_member_fingerprint(&kinds_b, &logical_b),
        );
    }

    #[test]
    fn validates_parallel_member_arrays() {
        let mut summary = valid_summary();
        summary.member_logical_ids.pop();

        assert!(validate_community_summary(&summary).is_err());
    }

    #[test]
    fn validates_member_values_are_non_empty() {
        let mut summary = valid_summary();
        summary.member_entity_kinds[0] = " ".into();
        assert!(validate_community_summary(&summary).is_err());

        let mut summary = valid_summary();
        summary.member_logical_ids[0] = " ".into();
        assert!(validate_community_summary(&summary).is_err());
    }

    #[test]
    fn validates_level_fits_postgres_smallint() {
        let mut summary = valid_summary();
        summary.level = MAX_LEVEL + 1;

        assert!(validate_community_summary(&summary).is_err());
    }

    #[test]
    fn accepts_valid_community_summary() {
        assert!(validate_community_summary(&valid_summary()).is_ok());
    }
}

//! [`VerifiedQueryStore`] postgres impl.

use async_trait::async_trait;

use ox_core::error::{OxError, OxResult};
use ox_ontology::{
    AgentRef, ComplexityClass, VerifiedQueryDef, VerifiedQueryId, VerifiedQueryStatus,
};

use crate::store::VerifiedQueryStore;

use ox_core::pgvector::format_vector as format_pgvector;

use super::{PostgresStore, to_ox_error};

#[derive(sqlx::FromRow)]
struct VerifiedQueryRow {
    id: String,
    workspace_id: uuid::Uuid,
    question: String,
    question_hash: String,
    query_ir: serde_json::Value,
    complexity_class: String,
    status: String,
    author: serde_json::Value,
    description: String,
    tokenized_text: String,
    tokenizer_dict_fingerprint: String,
    verified_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl VerifiedQueryRow {
    fn into_domain(self) -> OxResult<VerifiedQueryDef> {
        let complexity_class =
            ComplexityClass::from_wire_str(&self.complexity_class).ok_or_else(|| {
                OxError::Runtime {
                    message: format!(
                        "verified_queries.complexity_class `{}` is not a known wire string",
                        self.complexity_class
                    ),
                }
            })?;
        let status =
            VerifiedQueryStatus::from_wire_str(&self.status).ok_or_else(|| OxError::Runtime {
                message: format!(
                    "verified_queries.status `{}` is not a known wire string",
                    self.status
                ),
            })?;
        let author: AgentRef =
            serde_json::from_value(self.author).map_err(|e| OxError::Runtime {
                message: format!("decode verified_queries.author failed: {e}"),
            })?;
        Ok(VerifiedQueryDef {
            id: VerifiedQueryId::new(self.id),
            workspace_id: self.workspace_id,
            question: self.question,
            question_hash: self.question_hash,
            query_ir: self.query_ir,
            complexity_class,
            status,
            author,
            description: self.description,
            verified_at: self.verified_at,
            updated_at: self.updated_at,
            // List / find paths intentionally do NOT round-trip
            // the embedding back to Rust — 1024 f32s × N rows
            // ships kilobytes of payload the FE / Brain don't
            // need (Brain's NN search uses the embedding inside
            // the SQL `<=>` operator, never out-of-band). Use
            // the dedicated `…_with_embedding` accessor when the
            // caller actually needs the vector body.
            embedding: None,
            tokenized_text: self.tokenized_text,
            tokenizer_dict_fingerprint: self.tokenizer_dict_fingerprint,
        })
    }
}

#[async_trait]
impl VerifiedQueryStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(
        question_hash = %query.question_hash,
        complexity = query.complexity_class.as_str(),
        status = query.status.as_str(),
    ))]
    async fn upsert_verified_query(&self, query: &VerifiedQueryDef) -> OxResult<VerifiedQueryDef> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let author = serde_json::to_value(&query.author).map_err(|e| OxError::Runtime {
            message: format!("encode VerifiedQueryDef.author failed: {e}"),
        })?;
        // Φ11.5 — embedding rides as the pgvector text format
        // (`[1.0,2.0,...]`) cast to `vector` server-side. NULL when
        // the caller hasn't computed an embedding (cold-start
        // promotion before the embedder is attached). The column
        // dimension is `vector(1024)`; a mismatched length is
        // rejected by Postgres rather than silently truncated.
        let embedding_text = query.embedding.as_ref().map(|v| format_pgvector(v));
        let row: VerifiedQueryRow = sqlx::query_as(
            "INSERT INTO verified_queries
                (id, workspace_id, question, question_hash, query_ir,
                 complexity_class, status, author, description,
                 tokenized_text, tokenizer_dict_fingerprint,
                 verified_at, updated_at, embedding)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now(), $13::vector)
             ON CONFLICT (workspace_id, question_hash) DO UPDATE SET
                question = EXCLUDED.question,
                query_ir = EXCLUDED.query_ir,
                complexity_class = EXCLUDED.complexity_class,
                status = EXCLUDED.status,
                author = EXCLUDED.author,
                description = EXCLUDED.description,
                tokenized_text = EXCLUDED.tokenized_text,
                tokenizer_dict_fingerprint = EXCLUDED.tokenizer_dict_fingerprint,
                updated_at = now(),
                embedding = COALESCE(EXCLUDED.embedding, verified_queries.embedding)
             RETURNING id, workspace_id, question, question_hash, query_ir,
                       complexity_class, status, author, description,
                       tokenized_text, tokenizer_dict_fingerprint,
                       verified_at, updated_at",
        )
        .bind(query.id.as_str())
        .bind(workspace_id)
        .bind(&query.question)
        .bind(&query.question_hash)
        .bind(&query.query_ir)
        .bind(query.complexity_class.as_str())
        .bind(query.status.as_str())
        .bind(&author)
        .bind(&query.description)
        .bind(&query.tokenized_text)
        .bind(&query.tokenizer_dict_fingerprint)
        .bind(query.verified_at)
        .bind(embedding_text)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(query_id = %id.as_str()))]
    async fn get_verified_query(&self, id: &VerifiedQueryId) -> OxResult<Option<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let row: Option<VerifiedQueryRow> = sqlx::query_as(
            "SELECT id, workspace_id, question, question_hash, query_ir,
                    complexity_class, status, author, description,
                    tokenized_text, tokenizer_dict_fingerprint,
                    verified_at, updated_at
             FROM verified_queries WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(VerifiedQueryRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(question_hash = %question_hash))]
    async fn find_verified_query_by_hash(
        &self,
        question_hash: &str,
    ) -> OxResult<Option<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let row: Option<VerifiedQueryRow> = sqlx::query_as(
            "SELECT id, workspace_id, question, question_hash, query_ir,
                    complexity_class, status, author, description,
                    tokenized_text, tokenizer_dict_fingerprint,
                    verified_at, updated_at
             FROM verified_queries
             WHERE question_hash = $1
             LIMIT 1",
        )
        .bind(question_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(VerifiedQueryRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        status = ?status_filter.map(|s| s.as_str()),
        limit = limit,
    ))]
    async fn list_verified_queries(
        &self,
        status_filter: Option<VerifiedQueryStatus>,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let limit_capped = limit.clamp(1, 1000) as i64;
        let rows: Vec<VerifiedQueryRow> = match status_filter {
            Some(status) => sqlx::query_as(
                "SELECT id, workspace_id, question, question_hash, query_ir,
                        complexity_class, status, author, description,
                        tokenized_text, tokenizer_dict_fingerprint,
                        verified_at, updated_at
                 FROM verified_queries
                 WHERE status = $1
                 ORDER BY updated_at DESC, id ASC
                 LIMIT $2",
            )
            .bind(status.as_str())
            .bind(limit_capped),
            None => sqlx::query_as(
                "SELECT id, workspace_id, question, question_hash, query_ir,
                        complexity_class, status, author, description,
                        tokenized_text, tokenizer_dict_fingerprint,
                        verified_at, updated_at
                 FROM verified_queries
                 ORDER BY updated_at DESC, id ASC
                 LIMIT $1",
            )
            .bind(limit_capped),
        }
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(VerifiedQueryRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        query_id = %id.as_str(),
        new_status = new_status.as_str(),
    ))]
    async fn transition_verified_query_status(
        &self,
        id: &VerifiedQueryId,
        new_status: VerifiedQueryStatus,
    ) -> OxResult<VerifiedQueryDef> {
        super::require_workspace_context()?;
        let row: Option<VerifiedQueryRow> = sqlx::query_as(
            "UPDATE verified_queries
             SET status = $2, updated_at = now()
             WHERE id = $1
             RETURNING id, workspace_id, question, question_hash, query_ir,
                       complexity_class, status, author, description,
                       tokenized_text, tokenizer_dict_fingerprint,
                       verified_at, updated_at",
        )
        .bind(id.as_str())
        .bind(new_status.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.ok_or_else(|| OxError::NotFound {
            entity: format!("verified_queries id={}", id.as_str()),
        })?
        .into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(query_id = %id.as_str()))]
    async fn delete_verified_query(&self, id: &VerifiedQueryId) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM verified_queries WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        embedding_dim = query_embedding.len(),
        limit = limit,
    ))]
    async fn search_verified_queries_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let limit_capped = limit.clamp(1, 50) as i64;
        let vector_text = format_pgvector(query_embedding);
        let rows: Vec<VerifiedQueryRow> = sqlx::query_as(
            "SELECT id, workspace_id, question, question_hash, query_ir,
                    complexity_class, status, author, description,
                    tokenized_text, tokenizer_dict_fingerprint,
                    verified_at, updated_at
             FROM verified_queries
             WHERE embedding IS NOT NULL
               AND status = 'verified'
               AND complexity_class <> 'trivial'
             ORDER BY embedding <=> $1::vector ASC, updated_at DESC
             LIMIT $2",
        )
        .bind(&vector_text)
        .bind(limit_capped)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(VerifiedQueryRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        question_len = question.len(),
        limit = limit,
    ))]
    async fn search_verified_queries_for_icl(
        &self,
        question: &str,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let limit_capped = limit.clamp(1, 50) as i64;
        let rows: Vec<VerifiedQueryRow> = sqlx::query_as(
            "SELECT id, workspace_id, question, question_hash, query_ir,
                    complexity_class, status, author, description,
                    tokenized_text, tokenizer_dict_fingerprint,
                    verified_at, updated_at
             FROM verified_queries
             WHERE question % $1
               AND status = 'verified'
               AND complexity_class <> 'trivial'
             ORDER BY similarity(question, $1) DESC, updated_at DESC
             LIMIT $2",
        )
        .bind(question)
        .bind(limit_capped)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(VerifiedQueryRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        question_len = question_raw.len(),
        tokenized_len = question_tokenized.len(),
        has_embedding = query_embedding.is_some(),
        limit = limit,
    ))]
    async fn hybrid_search_verified_queries_for_icl(
        &self,
        question_raw: &str,
        question_tokenized: &str,
        query_embedding: Option<&[f32]>,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let limit_capped = limit.clamp(1, 50) as i64;
        let candidate_breadth = limit_capped * super::RRF_CANDIDATE_BREADTH;

        // Vector ranker is optional. When the embedder is offline
        // (cold start) we omit the vector CTE and degrade to a
        // 2-ranker fusion (trigram + FTS).
        let vector_text = query_embedding.map(format_pgvector);

        // The SQL is shape-stable across the embedded/non-embedded
        // branches; only the third ranker CTE flips. Inlining keeps
        // the planner's stats lookup local to the branch the runtime
        // actually executed.
        let sql_with_vec = "
            WITH
            trgm_ranked AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY similarity(question, $1) DESC, updated_at DESC
                ) AS rk
                FROM verified_queries
                WHERE question % $1
                  AND status = 'verified'
                  AND complexity_class <> 'trivial'
                LIMIT $4
            ),
            fts_ranked AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY ts_rank_cd(searchable_tsv, plainto_tsquery('simple', $2)) DESC, updated_at DESC
                ) AS rk
                FROM verified_queries
                WHERE searchable_tsv @@ plainto_tsquery('simple', $2)
                  AND status = 'verified'
                  AND complexity_class <> 'trivial'
                LIMIT $4
            ),
            vec_ranked AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY embedding <=> $3::vector ASC, updated_at DESC
                ) AS rk
                FROM verified_queries
                WHERE embedding IS NOT NULL
                  AND status = 'verified'
                  AND complexity_class <> 'trivial'
                LIMIT $4
            ),
            fused AS (
                SELECT id, SUM(1.0 / ($5 + rk)::numeric) AS rrf_score
                FROM (
                    SELECT id, rk FROM trgm_ranked
                    UNION ALL SELECT id, rk FROM fts_ranked
                    UNION ALL SELECT id, rk FROM vec_ranked
                ) AS all_ranks
                GROUP BY id
            )
            SELECT vq.id, vq.workspace_id, vq.question, vq.question_hash, vq.query_ir,
                   vq.complexity_class, vq.status, vq.author, vq.description,
                   vq.tokenized_text, vq.tokenizer_dict_fingerprint,
                   vq.verified_at, vq.updated_at
            FROM fused f
            JOIN verified_queries vq ON vq.id = f.id
            ORDER BY f.rrf_score DESC, vq.updated_at DESC
            LIMIT $6";

        let sql_no_vec = "
            WITH
            trgm_ranked AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY similarity(question, $1) DESC, updated_at DESC
                ) AS rk
                FROM verified_queries
                WHERE question % $1
                  AND status = 'verified'
                  AND complexity_class <> 'trivial'
                LIMIT $3
            ),
            fts_ranked AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY ts_rank_cd(searchable_tsv, plainto_tsquery('simple', $2)) DESC, updated_at DESC
                ) AS rk
                FROM verified_queries
                WHERE searchable_tsv @@ plainto_tsquery('simple', $2)
                  AND status = 'verified'
                  AND complexity_class <> 'trivial'
                LIMIT $3
            ),
            fused AS (
                SELECT id, SUM(1.0 / ($4 + rk)::numeric) AS rrf_score
                FROM (
                    SELECT id, rk FROM trgm_ranked
                    UNION ALL SELECT id, rk FROM fts_ranked
                ) AS all_ranks
                GROUP BY id
            )
            SELECT vq.id, vq.workspace_id, vq.question, vq.question_hash, vq.query_ir,
                   vq.complexity_class, vq.status, vq.author, vq.description,
                   vq.tokenized_text, vq.tokenizer_dict_fingerprint,
                   vq.verified_at, vq.updated_at
            FROM fused f
            JOIN verified_queries vq ON vq.id = f.id
            ORDER BY f.rrf_score DESC, vq.updated_at DESC
            LIMIT $5";

        let rows: Vec<VerifiedQueryRow> = match vector_text {
            Some(vec_text) => {
                sqlx::query_as(sql_with_vec)
                    .bind(question_raw)
                    .bind(question_tokenized)
                    .bind(vec_text)
                    .bind(candidate_breadth)
                    .bind(super::RRF_K)
                    .bind(limit_capped)
                    .fetch_all(&self.pool)
                    .await
            }
            None => {
                sqlx::query_as(sql_no_vec)
                    .bind(question_raw)
                    .bind(question_tokenized)
                    .bind(candidate_breadth)
                    .bind(super::RRF_K)
                    .bind(limit_capped)
                    .fetch_all(&self.pool)
                    .await
            }
        }
        .map_err(to_ox_error)?;

        rows.into_iter()
            .map(VerifiedQueryRow::into_domain)
            .collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        query_text_len = query_text.len(),
        complexity = ?complexity_filter.map(|c| c.as_str()),
        limit = limit,
    ))]
    async fn search_verified_queries_by_text(
        &self,
        query_text: &str,
        complexity_filter: Option<ComplexityClass>,
        limit: u32,
    ) -> OxResult<Vec<VerifiedQueryDef>> {
        super::require_workspace_context()?;
        let limit_capped = limit.clamp(1, 200) as i64;
        // Trigram similarity ranking against the GIN index on
        // `question`. The pg_trgm `%` operator gates rows by the
        // `pg_trgm.similarity_threshold` GUC (default 0.3); ORDER
        // BY similarity desc surfaces the closest match first.
        let rows: Vec<VerifiedQueryRow> = match complexity_filter {
            Some(complexity) => sqlx::query_as(
                "SELECT id, workspace_id, question, question_hash, query_ir,
                        complexity_class, status, author, description,
                        tokenized_text, tokenizer_dict_fingerprint,
                        verified_at, updated_at
                 FROM verified_queries
                 WHERE question % $1
                   AND complexity_class = $2
                 ORDER BY similarity(question, $1) DESC, updated_at DESC
                 LIMIT $3",
            )
            .bind(query_text)
            .bind(complexity.as_str())
            .bind(limit_capped),
            None => sqlx::query_as(
                "SELECT id, workspace_id, question, question_hash, query_ir,
                        complexity_class, status, author, description,
                        tokenized_text, tokenizer_dict_fingerprint,
                        verified_at, updated_at
                 FROM verified_queries
                 WHERE question % $1
                 ORDER BY similarity(question, $1) DESC, updated_at DESC
                 LIMIT $2",
            )
            .bind(query_text)
            .bind(limit_capped),
        }
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(VerifiedQueryRow::into_domain)
            .collect()
    }
}

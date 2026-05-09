//! [`KnowledgeStore`] — failure-driven knowledge entries — embeddings + lifecycle.

use super::*;

#[derive(sqlx::FromRow)]
struct KnowledgeEntryRow {
    id: Uuid,
    workspace_id: Uuid,
    ontology_name: String,
    ontology_version_min: i32,
    ontology_version_max: Option<i32>,
    kind: String,
    status: String,
    confidence: f64,
    title: String,
    content: String,
    structured_data: serde_json::Value,
    version_checked: i32,
    content_hash: String,
    source_execution_ids: Vec<Uuid>,
    source_session_id: Option<Uuid>,
    affected_labels: Vec<String>,
    affected_properties: Vec<String>,
    created_by: String,
    reviewed_by: Option<Uuid>,
    reviewed_at: Option<DateTime<Utc>>,
    review_notes: Option<String>,
    use_count: i64,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    tokenized_text: String,
    tokenizer_dict_fingerprint: String,
}

impl TryFrom<KnowledgeEntryRow> for KnowledgeEntry {
    type Error = OxError;

    fn try_from(row: KnowledgeEntryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            ontology_name: row.ontology_name,
            ontology_version_min: row.ontology_version_min,
            ontology_version_max: row.ontology_version_max,
            kind: row
                .kind
                .parse()
                .map_err(|message| OxError::Runtime { message })?,
            status: row
                .status
                .parse()
                .map_err(|message| OxError::Runtime { message })?,
            confidence: row.confidence,
            title: row.title,
            content: row.content,
            structured_data: row.structured_data,
            embedding: None,
            version_checked: row.version_checked,
            content_hash: row.content_hash,
            source_execution_ids: row.source_execution_ids,
            source_session_id: row.source_session_id,
            affected_labels: row.affected_labels,
            affected_properties: row.affected_properties,
            created_by: row.created_by,
            reviewed_by: row.reviewed_by,
            reviewed_at: row.reviewed_at,
            review_notes: row.review_notes,
            use_count: row.use_count,
            last_used_at: row.last_used_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
            tokenized_text: row.tokenized_text,
            tokenizer_dict_fingerprint: row.tokenizer_dict_fingerprint,
        })
    }
}

#[async_trait]
impl KnowledgeStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_knowledge_entry(&self, entry: &KnowledgeEntry) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO knowledge_entries (
                id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                kind, status, confidence, title, content, structured_data,
                version_checked, content_hash, source_execution_ids, source_session_id,
                affected_labels, affected_properties, created_by,
                tokenized_text, tokenizer_dict_fingerprint
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            ON CONFLICT (workspace_id, ontology_name, content_hash) DO UPDATE SET
                confidence = GREATEST(knowledge_entries.confidence, EXCLUDED.confidence),
                tokenized_text = EXCLUDED.tokenized_text,
                tokenizer_dict_fingerprint = EXCLUDED.tokenizer_dict_fingerprint,
                updated_at = now()",
        )
        .bind(entry.id)
        .bind(entry.workspace_id)
        .bind(&entry.ontology_name)
        .bind(entry.ontology_version_min)
        .bind(entry.ontology_version_max)
        .bind(entry.kind.as_str())
        .bind(entry.status.as_str())
        .bind(entry.confidence)
        .bind(&entry.title)
        .bind(&entry.content)
        .bind(&entry.structured_data)
        .bind(entry.version_checked)
        .bind(&entry.content_hash)
        .bind(&entry.source_execution_ids)
        .bind(entry.source_session_id)
        .bind(&entry.affected_labels)
        .bind(&entry.affected_properties)
        .bind(&entry.created_by)
        .bind(&entry.tokenized_text)
        .bind(&entry.tokenizer_dict_fingerprint)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_knowledge_entry(&self, id: Uuid) -> OxResult<Option<KnowledgeEntry>> {
        let row = sqlx::query_as::<_, KnowledgeEntryRow>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at,
                    tokenized_text, tokenizer_dict_fingerprint
             FROM knowledge_entries WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(KnowledgeEntry::try_from).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_entry(
        &self,
        id: Uuid,
        title: &str,
        content: &str,
        structured_data: &serde_json::Value,
        affected_labels: &[String],
        affected_properties: &[String],
        tokenized_text: &str,
        tokenizer_dict_fingerprint: &str,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE knowledge_entries SET title = $2, content = $3, structured_data = $4,
                    affected_labels = $5, affected_properties = $6,
                    tokenized_text = $7, tokenizer_dict_fingerprint = $8,
                    content_hash = encode(sha256((ontology_name || lower(trim($3)))::bytea), 'hex'),
                    updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(title)
        .bind(content)
        .bind(structured_data)
        .bind(affected_labels)
        .bind(affected_properties)
        .bind(tokenized_text)
        .bind(tokenizer_dict_fingerprint)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)
        .and_then(|r| {
            if r.rows_affected() == 0 {
                Err(ox_core::error::OxError::Runtime {
                    message: "Knowledge entry not found".to_string(),
                })
            } else {
                Ok(())
            }
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_knowledge_entry(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM knowledge_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_knowledge_entries(
        &self,
        ontology_name: Option<&str>,
        kind: Option<&str>,
        status: Option<&str>,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<KnowledgeEntry>> {
        let limit = pagination.effective_limit();
        let cursor = pagination.cursor_parts();

        let rows: Vec<KnowledgeEntryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at,
                    tokenized_text, tokenizer_dict_fingerprint
             FROM knowledge_entries
             WHERE ($1::text IS NULL OR ontology_name = $1)
               AND ($2::text IS NULL OR kind = $2)
               AND ($3::text IS NULL OR status = $3)
               AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
             ORDER BY created_at DESC, id DESC
             LIMIT $6",
        )
        .bind(ontology_name)
        .bind(kind)
        .bind(status)
        .bind(cursor.map(|(ts, _)| ts))
        .bind(cursor.map(|(_, id)| id))
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        let has_more = rows.len() > limit as usize;
        let mut items: Vec<KnowledgeEntry> = rows
            .into_iter()
            .map(KnowledgeEntry::try_from)
            .collect::<Result<_, _>>()?;
        if has_more {
            items.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            items
                .last()
                .map(|r| format!("{}|{}", r.created_at.to_rfc3339(), r.id))
        } else {
            None
        };

        Ok(CursorPage { items, next_cursor })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_active_knowledge(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        kinds: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>> {
        let rows: Vec<KnowledgeEntryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at,
                    tokenized_text, tokenizer_dict_fingerprint
             FROM knowledge_entries
             WHERE ontology_name = $1
               AND status = 'approved'
               AND ontology_version_min <= $2
               AND (ontology_version_max IS NULL OR ontology_version_max >= $2)
               AND kind = ANY($3)
             ORDER BY confidence DESC
             LIMIT $4",
        )
        .bind(ontology_name)
        .bind(ontology_version)
        .bind(kinds)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter().map(KnowledgeEntry::try_from).collect()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_status(
        &self,
        id: Uuid,
        status: KnowledgeStatus,
        reviewer_id: Option<Uuid>,
        review_notes: Option<&str>,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        let result = sqlx::query(
            "UPDATE knowledge_entries SET status = $2, reviewed_by = $3, review_notes = $4,
                    reviewed_at = now(), updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(status.as_str())
        .bind(reviewer_id)
        .bind(review_notes)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        if result.rows_affected() == 0 {
            return Err(ox_core::error::OxError::Runtime {
                message: "Knowledge entry not found".to_string(),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_confidence(&self, id: Uuid, confidence: f64) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE knowledge_entries SET confidence = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(confidence)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn expire_knowledge_by_labels(
        &self,
        ontology_name: &str,
        changed_labels: &[String],
    ) -> OxResult<u64> {
        super::require_workspace_context()?;
        let result = sqlx::query(
            "UPDATE knowledge_entries
             SET status = 'stale', confidence = confidence * 0.5, updated_at = now()
             WHERE ontology_name = $1
               AND status = 'approved'
               AND affected_labels && $2",
        )
        .bind(ontology_name)
        .bind(changed_labels)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_knowledge_usage(&self, ids: &[Uuid]) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE knowledge_entries SET use_count = use_count + 1, last_used_at = now()
             WHERE id = ANY($1)",
        )
        .bind(ids)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn verify_knowledge(&self, id: Uuid, version: i32) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE knowledge_entries SET version_checked = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(version)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn search_knowledge_by_labels(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        labels: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>> {
        let rows: Vec<KnowledgeEntryRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at,
                    tokenized_text, tokenizer_dict_fingerprint
             FROM knowledge_entries
             WHERE ontology_name = $1
               AND status = 'approved'
               AND ontology_version_min <= $2
               AND (ontology_version_max IS NULL OR ontology_version_max >= $2)
               AND affected_labels && $3
             ORDER BY confidence DESC
             LIMIT $4",
        )
        .bind(ontology_name)
        .bind(ontology_version)
        .bind(labels)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter().map(KnowledgeEntry::try_from).collect()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        question_len = question_raw.len(),
        tokenized_len = question_tokenized.len(),
        has_embedding = query_embedding.is_some(),
        labels = label_hints.len(),
        limit = limit,
    ))]
    async fn hybrid_search_knowledge_entries(
        &self,
        question_raw: &str,
        question_tokenized: &str,
        query_embedding: Option<&[f32]>,
        ontology_name: &str,
        ontology_version: i32,
        label_hints: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>> {
        let limit_capped = limit.clamp(1, 100);
        let candidate_breadth = limit_capped * super::RRF_CANDIDATE_BREADTH;
        let vector_text = query_embedding.map(ox_core::pgvector::format_vector);
        let sql = build_knowledge_hybrid_sql(vector_text.is_some(), !label_hints.is_empty());

        let mut q = sqlx::query_as::<_, KnowledgeEntryRow>(&sql)
            .bind(ontology_name)
            .bind(ontology_version)
            .bind(question_raw)
            .bind(question_tokenized);
        if let Some(vec) = vector_text.as_deref() {
            q = q.bind(vec);
        }
        if !label_hints.is_empty() {
            q = q.bind(label_hints);
        }
        let rows: Vec<KnowledgeEntryRow> = q
            .bind(candidate_breadth)
            .bind(super::RRF_K)
            .bind(limit_capped)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;
        rows.into_iter().map(KnowledgeEntry::try_from).collect()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn count_knowledge_by_status_kind(&self) -> OxResult<Vec<(String, String, i64)>> {
        sqlx::query_as::<_, (String, String, i64)>(
            "SELECT status, kind, COUNT(*) FROM knowledge_entries GROUP BY status, kind",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn cleanup_knowledge(&self, older_than_days: i64) -> OxResult<u64> {
        super::require_workspace_context()?;
        // Auto-deprecate low-confidence entries
        sqlx::query(
            "UPDATE knowledge_entries SET status = 'deprecated', updated_at = now()
             WHERE confidence < 0.1 AND status != 'deprecated'",
        )
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        // Delete old deprecated entries
        let result = sqlx::query(
            "DELETE FROM knowledge_entries
             WHERE status = 'deprecated'
               AND updated_at < now() - make_interval(days => $1)",
        )
        .bind(older_than_days as i32)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(result.rows_affected())
    }
}

/// Compose the knowledge-entries hybrid retrieval SQL.
///
/// Slot layout — positional parameters bound at call time:
///
/// ```text
/// $1 ontology_name        — eligibility filter
/// $2 ontology_version     — eligibility filter
/// $3 question_raw         — trigram on title + content
/// $4 question_tokenized   — FTS plainto_tsquery
/// $5? vector_text         — pgvector cosine NN, present iff has_vector
/// $N? label_hints         — text[] for the label boost ranker, present iff has_labels
/// trailing
///   $K   candidate_breadth — per-ranker LIMIT
///   $K+1 rrf_k             — RRF constant (60)
///   $K+2 limit             — final LIMIT
/// ```
///
/// The composer pulls out into a free function so every ranker
/// permutation (vector × labels = 4 cases) has unit-test coverage
/// without a live pool.
fn build_knowledge_hybrid_sql(has_vector: bool, has_labels: bool) -> String {
    let eligible = "FROM knowledge_entries
            WHERE ontology_name = $1
              AND status = 'approved'
              AND ontology_version_min <= $2
              AND (ontology_version_max IS NULL OR ontology_version_max >= $2)";

    let mut next_slot: i32 = 5;
    let vector_slot = has_vector.then(|| {
        let s = next_slot;
        next_slot += 1;
        s
    });
    let labels_slot = has_labels.then(|| {
        let s = next_slot;
        next_slot += 1;
        s
    });
    let breadth_slot = next_slot;
    let rrf_k_slot = next_slot + 1;
    let limit_slot = next_slot + 2;

    let mut sql = String::with_capacity(2048);
    sql.push_str("WITH\n");
    sql.push_str(&format!(
        "trgm_title AS (
            SELECT id, ROW_NUMBER() OVER (
                ORDER BY similarity(title, $3) DESC, confidence DESC
            ) AS rk
            {eligible}
              AND title % $3
            LIMIT ${breadth_slot}
        ),\n",
    ));
    sql.push_str(&format!(
        "trgm_content AS (
            SELECT id, ROW_NUMBER() OVER (
                ORDER BY similarity(content, $3) DESC, confidence DESC
            ) AS rk
            {eligible}
              AND content % $3
            LIMIT ${breadth_slot}
        ),\n",
    ));
    sql.push_str(&format!(
        "fts AS (
            SELECT id, ROW_NUMBER() OVER (
                ORDER BY ts_rank_cd(searchable_tsv, plainto_tsquery('simple', $4)) DESC, confidence DESC
            ) AS rk
            {eligible}
              AND searchable_tsv @@ plainto_tsquery('simple', $4)
            LIMIT ${breadth_slot}
        ),\n",
    ));
    if let Some(vs) = vector_slot {
        sql.push_str(&format!(
            "vec AS (
                SELECT id, ROW_NUMBER() OVER (
                    ORDER BY embedding <=> ${vs}::vector ASC, confidence DESC
                ) AS rk
                {eligible}
                  AND embedding IS NOT NULL
                LIMIT ${breadth_slot}
            ),\n",
        ));
    }
    if let Some(ls) = labels_slot {
        // The label arm is binary — matched rows enter at rank 1
        // (boost); unmatched rows aren't represented, so the
        // fusion treats them as rank infinity (no boost).
        sql.push_str(&format!(
            "label_boost AS (
                SELECT id, 1::bigint AS rk
                {eligible}
                  AND affected_labels && ${ls}
                LIMIT ${breadth_slot}
            ),\n",
        ));
    }

    sql.push_str("ranks AS (\n");
    sql.push_str("    SELECT id, rk FROM trgm_title\n");
    sql.push_str("    UNION ALL SELECT id, rk FROM trgm_content\n");
    sql.push_str("    UNION ALL SELECT id, rk FROM fts\n");
    if vector_slot.is_some() {
        sql.push_str("    UNION ALL SELECT id, rk FROM vec\n");
    }
    if labels_slot.is_some() {
        sql.push_str("    UNION ALL SELECT id, rk FROM label_boost\n");
    }
    sql.push_str("),\n");
    sql.push_str(&format!(
        "fused AS (
            SELECT id, SUM(1.0 / (${rrf_k_slot} + rk)::numeric) AS rrf_score
            FROM ranks
            GROUP BY id
        )\n",
    ));
    sql.push_str(
        "SELECT k.id, k.workspace_id, k.ontology_name, k.ontology_version_min, k.ontology_version_max,
                k.kind, k.status, k.confidence, k.title, k.content, k.structured_data,
                k.version_checked, k.content_hash, k.source_execution_ids, k.source_session_id,
                k.affected_labels, k.affected_properties, k.created_by, k.reviewed_by, k.reviewed_at, k.review_notes,
                k.use_count, k.last_used_at, k.created_at, k.updated_at,
                k.tokenized_text, k.tokenizer_dict_fingerprint
         FROM fused f
         JOIN knowledge_entries k ON k.id = f.id\n",
    );
    // Confidence multiplies into the final fusion — operator-set
    // confidence carries domain trust beyond the ranker fusion.
    sql.push_str("ORDER BY (f.rrf_score * k.confidence) DESC, k.updated_at DESC\n");
    sql.push_str(&format!("LIMIT ${limit_slot}"));

    sql
}

#[cfg(test)]
mod hybrid_sql_tests {
    use super::build_knowledge_hybrid_sql;

    /// 2-ranker fusion (no vector, no labels): trgm title + trgm
    /// content + FTS. Trailing slots are `$5..$7`.
    #[test]
    fn two_ranker_fusion_no_optional_arms() {
        let sql = build_knowledge_hybrid_sql(false, false);
        assert!(sql.contains("trgm_title AS"));
        assert!(sql.contains("trgm_content AS"));
        assert!(sql.contains("fts AS"));
        assert!(!sql.contains("vec AS"));
        assert!(!sql.contains("label_boost AS"));
        assert!(sql.contains("LIMIT $5")); // breadth
        assert!(sql.contains("(1.0 / ($6 + rk)")); // RRF_K
        assert!(sql.ends_with("LIMIT $7"));
    }

    /// 4-ranker fusion (vector ON, labels OFF): the vector slot
    /// occupies $5; trailing slots are $6..$8.
    #[test]
    fn vector_only_optional_arm() {
        let sql = build_knowledge_hybrid_sql(true, false);
        assert!(sql.contains("vec AS"));
        assert!(sql.contains("$5::vector"));
        assert!(!sql.contains("label_boost AS"));
        assert!(sql.contains("UNION ALL SELECT id, rk FROM vec"));
        assert!(sql.contains("LIMIT $6")); // breadth
        assert!(sql.contains("(1.0 / ($7 + rk)"));
        assert!(sql.ends_with("LIMIT $8"));
    }

    /// 4-ranker fusion (labels ON, vector OFF): the label slot
    /// occupies $5; trailing slots are $6..$8.
    #[test]
    fn labels_only_optional_arm() {
        let sql = build_knowledge_hybrid_sql(false, true);
        assert!(sql.contains("label_boost AS"));
        assert!(sql.contains("affected_labels && $5"));
        assert!(!sql.contains("vec AS"));
        assert!(sql.contains("UNION ALL SELECT id, rk FROM label_boost"));
        assert!(sql.contains("LIMIT $6"));
        assert!(sql.contains("(1.0 / ($7 + rk)"));
        assert!(sql.ends_with("LIMIT $8"));
    }

    /// 5-ranker fusion (both optional arms): vector at $5, labels
    /// at $6, trailing slots $7..$9.
    #[test]
    fn vector_and_labels_both_active() {
        let sql = build_knowledge_hybrid_sql(true, true);
        assert!(sql.contains("vec AS"));
        assert!(sql.contains("$5::vector"));
        assert!(sql.contains("label_boost AS"));
        assert!(sql.contains("affected_labels && $6"));
        assert!(sql.contains("UNION ALL SELECT id, rk FROM vec"));
        assert!(sql.contains("UNION ALL SELECT id, rk FROM label_boost"));
        assert!(sql.contains("LIMIT $7"));
        assert!(sql.contains("(1.0 / ($8 + rk)"));
        assert!(sql.ends_with("LIMIT $9"));
    }

    /// Eligibility predicate appears once per ranker arm — the
    /// planner shares the predicate across CTEs.
    #[test]
    fn eligibility_predicate_present_in_every_ranker_arm() {
        let sql = build_knowledge_hybrid_sql(true, true);
        let eligibility_count = sql.matches("ontology_name = $1").count();
        // 3 mandatory rankers + vec + label_boost = 5
        assert_eq!(eligibility_count, 5);
    }

    /// Confidence multiplies the rrf_score in the outer ORDER BY.
    /// Pinned because the ordering shape is the operator-trust
    /// signal layered on top of the fusion ranker.
    #[test]
    fn outer_order_multiplies_confidence_into_rrf_score() {
        let sql = build_knowledge_hybrid_sql(false, false);
        assert!(sql.contains("ORDER BY (f.rrf_score * k.confidence) DESC"));
    }
}

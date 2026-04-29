//! [`KnowledgeStore`] — failure-driven knowledge entries — embeddings + lifecycle.

use super::*;

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
                affected_labels, affected_properties, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            ON CONFLICT (workspace_id, ontology_name, content_hash) DO UPDATE SET
                confidence = GREATEST(knowledge_entries.confidence, EXCLUDED.confidence),
                updated_at = now()",
        )
        .bind(entry.id)
        .bind(entry.workspace_id)
        .bind(&entry.ontology_name)
        .bind(entry.ontology_version_min)
        .bind(entry.ontology_version_max)
        .bind(&entry.kind)
        .bind(&entry.status)
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
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_knowledge_entry(&self, id: Uuid) -> OxResult<Option<KnowledgeEntry>> {
        sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
             FROM knowledge_entries WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
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
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE knowledge_entries SET title = $2, content = $3, structured_data = $4,
                    affected_labels = $5, affected_properties = $6,
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

        let rows: Vec<KnowledgeEntry> = sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
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
        let mut items = rows;
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
        sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
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
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_knowledge_status(
        &self,
        id: Uuid,
        status: &str,
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
        .bind(status)
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
    async fn mark_stale_by_labels(
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
        sqlx::query_as::<_, KnowledgeEntry>(
            "SELECT id, workspace_id, ontology_name, ontology_version_min, ontology_version_max,
                    kind, status, confidence, title, content, structured_data,
                    version_checked, content_hash, source_execution_ids, source_session_id,
                    affected_labels, affected_properties, created_by, reviewed_by, reviewed_at, review_notes,
                    use_count, last_used_at, created_at, updated_at
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
        .map_err(to_ox_error)
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

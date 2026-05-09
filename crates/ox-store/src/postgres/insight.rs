//! [`InsightStore`] — persisted insight artefacts.
//!
//! Server owns identity (UUID v7) and timestamps so multi-author
//! editing stays race-free. `query_ir` and `original_provenance`
//! round-trip as JSONB blobs; the typed shape lives in
//! `ox_query_ir::insight::InsightDef`. Workspace isolation via RLS.

use super::*;

use ox_query_ir::insight::{InsightDef, InsightId};

use crate::store::{CreateInsightInput, InsightFilter, UpdateInsightInput};

/// Map a database row to the typed [`InsightDef`]. Single source of
/// truth so the column order across read/write paths stays in sync
/// with the underlying schema.
fn row_to_insight(row: sqlx::postgres::PgRow) -> InsightDef {
    use sqlx::Row;
    InsightDef {
        id: InsightId::new(row.get::<String, _>("id")),
        question: serde_json::from_value(row.get::<serde_json::Value, _>("question"))
            .unwrap_or_default(),
        description: serde_json::from_value(row.get::<serde_json::Value, _>("description"))
            .unwrap_or_default(),
        tags: row.get::<Vec<String>, _>("tags"),
        concept_anchors: row.get::<Vec<String>, _>("concept_anchors"),
        query_ir: row.get::<serde_json::Value, _>("query_ir"),
        original_provenance: row.get::<Option<serde_json::Value>, _>("original_provenance"),
        author_id: row.get::<Uuid, _>("author_id"),
        expires_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at"),
        created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    }
}

#[async_trait]
impl InsightStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_insight(&self, input: CreateInsightInput) -> OxResult<InsightDef> {
        super::require_workspace_context()?;
        // UUID v7 is timestamp-ordered (RFC 9562) — every insert
        // produces an id that sorts by creation time, so cursor
        // pagination on `(updated_at, id)` stays stable even when
        // server clock jitter clusters two writes in the same
        // millisecond.
        let id = Uuid::now_v7().to_string();
        let row = sqlx::query(
            "INSERT INTO insights \
                (id, question, description, tags, concept_anchors, query_ir, \
                 original_provenance, author_id, expires_at, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), now()) \
             RETURNING *",
        )
        .bind(&id)
        .bind(
            serde_json::to_value(&input.question).map_err(|e| OxError::Runtime {
                message: format!("serialise question: {e}"),
            })?,
        )
        .bind(
            serde_json::to_value(&input.description).map_err(|e| OxError::Runtime {
                message: format!("serialise description: {e}"),
            })?,
        )
        .bind(&input.tags)
        .bind(&input.concept_anchors)
        .bind(&input.query_ir)
        .bind(&input.original_provenance)
        .bind(input.author_id)
        .bind(input.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row_to_insight(row))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(insight_id = %id.as_str()))]
    async fn update_insight(
        &self,
        id: &InsightId,
        input: UpdateInsightInput,
    ) -> OxResult<InsightDef> {
        super::require_workspace_context()?;
        let row = sqlx::query(
            "UPDATE insights SET \
                question = $2, \
                description = $3, \
                tags = $4, \
                concept_anchors = $5, \
                query_ir = $6, \
                original_provenance = $7, \
                expires_at = $8, \
                updated_at = now() \
             WHERE id = $1 AND updated_at = $9 \
             RETURNING *",
        )
        .bind(id.as_str())
        .bind(
            serde_json::to_value(&input.question).map_err(|e| OxError::Runtime {
                message: format!("serialise question: {e}"),
            })?,
        )
        .bind(
            serde_json::to_value(&input.description).map_err(|e| OxError::Runtime {
                message: format!("serialise description: {e}"),
            })?,
        )
        .bind(&input.tags)
        .bind(&input.concept_anchors)
        .bind(&input.query_ir)
        .bind(&input.original_provenance)
        .bind(input.expires_at)
        .bind(input.expected_updated_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        match row {
            Some(r) => Ok(row_to_insight(r)),
            None => {
                // Either the row doesn't exist (or RLS hides it)
                // or the optimistic-CAS check failed. Disambiguate
                // so the caller surfaces the right HTTP status.
                let exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM insights WHERE id = $1)",
                )
                .bind(id.as_str())
                .fetch_one(&self.pool)
                .await
                .map_err(to_ox_error)?;
                if exists {
                    Err(OxError::Conflict {
                        message: format!(
                            "Insight '{}' was modified concurrently — \
                             reload and reapply",
                            id.as_str()
                        ),
                    })
                } else {
                    Err(OxError::NotFound {
                        entity: "Insight".to_string(),
                    })
                }
            }
        }
    }

    #[tracing::instrument(level = "debug", skip_all, fields(insight_id = %id.as_str()))]
    async fn get_insight(&self, id: &InsightId) -> OxResult<Option<InsightDef>> {
        let row = sqlx::query("SELECT * FROM insights WHERE id = $1")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row.map(row_to_insight))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_insights(
        &self,
        filter: &InsightFilter,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<InsightDef>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        // Dynamic builder — every filter axis emits a `AND ...`
        // clause keyed off a fresh placeholder. The cursor sentinel
        // (when present) emits the `(updated_at, id) < (...)` tuple
        // comparison that pairs with the GIN-on-array indices to keep
        // pagination O(limit). Empty axes contribute nothing — the
        // overlap-on-empty short-circuit in `InsightFilter` keeps the
        // SQL flat instead of guard-padding every clause.
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT * FROM insights WHERE TRUE");

        if let Some(author) = filter.author_id {
            qb.push(" AND author_id = ");
            qb.push_bind(author);
        }
        if !filter.concept_anchors.is_empty() {
            qb.push(" AND concept_anchors && ");
            qb.push_bind(filter.concept_anchors.clone());
        }
        if !filter.tags.is_empty() {
            qb.push(" AND tags && ");
            qb.push_bind(filter.tags.clone());
        }
        if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            qb.push(" AND (updated_at, id) < (");
            qb.push_bind(cursor_ts);
            qb.push(", ");
            qb.push_bind(cursor_id.to_string());
            qb.push(")");
        }
        qb.push(" ORDER BY updated_at DESC, id DESC LIMIT ");
        qb.push_bind(fetch_limit);

        let rows = qb
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;

        let items: Vec<InsightDef> = rows.into_iter().map(row_to_insight).collect();
        // UUID v7 ids are timestamp-ordered, so parsing the id back
        // to a Uuid (when it round-trips) gives the cursor a
        // monotonic tie-break under `(updated_at DESC, id DESC)`.
        // Hand-authored ids that can't parse fall back to nil —
        // they collide on the same tie-break value, but legitimate
        // ordering still holds via `updated_at`.
        Ok(build_cursor_page(items, limit, |i| {
            let parsed = Uuid::parse_str(i.id.as_str()).unwrap_or(Uuid::nil());
            (i.updated_at, parsed)
        }))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(insight_id = %id.as_str()))]
    async fn delete_insight(&self, id: &InsightId) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM insights WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

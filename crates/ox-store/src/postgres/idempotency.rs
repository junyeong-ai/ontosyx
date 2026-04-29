//! [`IdempotencyStore`] — backing layer for the Idempotency-Key
//! middleware (ADR-0047). One row per replayable response,
//! scoped to `(workspace_id, user_id, method, path, key)`.
//!
//! Reads and writes are tiny by design: the middleware sits on
//! every mutating LLM endpoint and the round-trip cost is the
//! defence against duplicate token charges, so the SQL stays as
//! narrow as the cache layer in front of it would expect.

use super::*;

#[async_trait]
impl IdempotencyStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_idempotency_record(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        method: &str,
        path: &str,
        key: &str,
    ) -> OxResult<Option<IdempotencyRecord>> {
        // `expires_at > now()` short-circuits a stale replay even
        // when the cleanup cron has not yet swept the row. The
        // cron is bounded; the freshness check here is hot path
        // and lets us keep the cron at a leisurely cadence.
        sqlx::query_as::<_, IdempotencyRecord>(
            "SELECT workspace_id, user_id, method, path, key, request_hash, \
                    response_status, response_body, response_content_type, \
                    created_at, expires_at \
             FROM idempotency_records \
             WHERE workspace_id = $1 AND user_id = $2 \
               AND method = $3 AND path = $4 AND key = $5 \
               AND expires_at > now()",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(method)
        .bind(path)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_idempotency_record(
        &self,
        record: &IdempotencyRecord,
    ) -> OxResult<()> {
        // First writer wins. A concurrent racer with the same key
        // either had its body match (and would have seen the cache
        // hit on its second look) or carried a different payload —
        // the stored response from whichever request landed first
        // is authoritative for the key, mirroring Stripe.
        sqlx::query(
            "INSERT INTO idempotency_records \
                 (workspace_id, user_id, method, path, key, request_hash, \
                  response_status, response_body, response_content_type, \
                  created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (workspace_id, user_id, method, path, key) \
             DO NOTHING",
        )
        .bind(record.workspace_id)
        .bind(record.user_id)
        .bind(&record.method)
        .bind(&record.path)
        .bind(&record.key)
        .bind(&record.request_hash)
        .bind(record.response_status)
        .bind(&record.response_body)
        .bind(record.response_content_type.as_deref())
        .bind(record.created_at)
        .bind(record.expires_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_expired_idempotency_records(&self) -> OxResult<u64> {
        let result =
            sqlx::query("DELETE FROM idempotency_records WHERE expires_at < now()")
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}

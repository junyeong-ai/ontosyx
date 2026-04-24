//! [`MeteringStore`] — usage_records — token counts, costs, per-operation duration.

use super::*;

#[async_trait]
impl MeteringStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_usage(
        &self,
        user_id: Option<Uuid>,
        resource_type: &str,
        provider: Option<&str>,
        model: Option<&str>,
        operation: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        duration_ms: i64,
        cost_usd: f64,
        metadata: serde_json::Value,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO usage_records
             (user_id, resource_type, provider, model, operation,
              input_tokens, output_tokens, duration_ms, cost_usd, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(user_id)
        .bind(resource_type)
        .bind(provider)
        .bind(model)
        .bind(operation)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(duration_ms)
        .bind(cost_usd)
        .bind(&metadata)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn usage_summary(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> OxResult<Vec<UsageSummary>> {
        sqlx::query_as::<_, UsageSummary>(
            "SELECT
                resource_type,
                COALESCE(SUM(input_tokens), 0)::int8 AS total_input_tokens,
                COALESCE(SUM(output_tokens), 0)::int8 AS total_output_tokens,
                COALESCE(SUM(cost_usd), 0)::float8 AS total_cost_usd,
                COUNT(*)::int8 AS request_count
             FROM usage_records
             WHERE created_at >= $1 AND created_at < $2
             GROUP BY resource_type
             ORDER BY total_cost_usd DESC",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

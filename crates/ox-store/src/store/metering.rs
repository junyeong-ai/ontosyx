//! Cost / usage tracking for billing and budgeting.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::UsageSummary;

#[async_trait]
pub trait MeteringStore: Send + Sync {
    /// Record a usage event (LLM call, query execution, etc.)
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
    ) -> OxResult<()>;

    /// Get aggregated usage summary for a time range.
    async fn usage_summary(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> OxResult<Vec<UsageSummary>>;
}

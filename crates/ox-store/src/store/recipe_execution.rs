use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::RecipeExecutionResult;

/// Storage for analysis execution results with input-hash-based caching.
#[async_trait]
pub trait RecipeExecutionStore: Send + Sync {
    async fn create_analysis_result(&self, result: &RecipeExecutionResult) -> OxResult<()>;
    async fn find_cached_result(
        &self,
        input_hash: &str,
        recipe_id: Option<Uuid>,
    ) -> OxResult<Option<RecipeExecutionResult>>;
    async fn list_analysis_results(
        &self,
        recipe_id: Uuid,
        limit: i64,
    ) -> OxResult<Vec<RecipeExecutionResult>>;
    /// Delete analysis results older than `max_age_days`. Returns
    /// per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn cleanup_old_results(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>>;
}

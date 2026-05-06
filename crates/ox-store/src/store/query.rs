use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{QueryExecution, QueryExecutionSummary};

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait QueryStore: Send + Sync {
    async fn create_query_execution(&self, execution: &QueryExecution) -> OxResult<()>;

    async fn get_query_execution(
        &self,
        user_id: &str,
        id: Uuid,
    ) -> OxResult<Option<QueryExecution>>;

    async fn list_query_executions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<QueryExecutionSummary>>;

    /// Update feedback on a query execution. Returns false if not found or not owned by user.
    async fn update_query_feedback(
        &self,
        id: Uuid,
        user_id: &str,
        feedback: Option<&str>,
    ) -> OxResult<bool>;
}

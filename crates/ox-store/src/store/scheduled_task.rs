use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::ScheduledTask;

/// Cron-based scheduled recipe execution.
#[async_trait]
pub trait ScheduledTaskStore: Send + Sync {
    async fn create_scheduled_task(&self, task: &ScheduledTask) -> OxResult<()>;
    async fn get_scheduled_task(&self, id: Uuid) -> OxResult<Option<ScheduledTask>>;
    async fn list_scheduled_tasks(&self, recipe_id: Option<Uuid>) -> OxResult<Vec<ScheduledTask>>;
    async fn list_due_tasks(&self) -> OxResult<Vec<ScheduledTask>>;
    async fn update_task_after_run(
        &self,
        id: Uuid,
        next_run_at: DateTime<Utc>,
        status: &str,
    ) -> OxResult<()>;
    async fn update_scheduled_task_enabled(&self, id: Uuid, enabled: bool) -> OxResult<()>;
    async fn delete_scheduled_task(&self, id: Uuid) -> OxResult<bool>;
}

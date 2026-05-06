//! Session recording for replay and audit.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{AgentEvent, AgentSession};

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait AgentSessionStore: Send + Sync {
    async fn create_agent_session(&self, session: &AgentSession) -> OxResult<()>;
    async fn complete_agent_session(&self, id: Uuid, final_text: Option<&str>) -> OxResult<()>;
    async fn get_agent_session(&self, id: Uuid) -> OxResult<Option<AgentSession>>;
    async fn list_agent_sessions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AgentSession>>;
    async fn create_agent_event(&self, event: &AgentEvent) -> OxResult<()>;
    async fn list_agent_events(&self, session_id: Uuid) -> OxResult<Vec<AgentEvent>>;
    async fn delete_agent_session(&self, id: Uuid) -> OxResult<bool>;
    /// Returns per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn cleanup_old_sessions(&self, retention_days: i64) -> OxResult<Vec<(Uuid, u64)>>;
}

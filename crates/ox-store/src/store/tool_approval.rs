//! HITL tool review decisions.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::ToolApproval;

#[async_trait]
pub trait ToolApprovalStore: Send + Sync {
    async fn create_tool_approval(&self, approval: &ToolApproval) -> OxResult<()>;
    async fn get_tool_approval(
        &self,
        session_id: Uuid,
        tool_call_id: &str,
    ) -> OxResult<Option<ToolApproval>>;
}

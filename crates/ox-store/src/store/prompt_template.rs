//! Versioned prompt-template management.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::PromptVersion;
use ox_core::error::OxResult;

use crate::models::PromptTemplateRow;

#[async_trait]
pub trait PromptTemplateStore: Send + Sync {
    async fn list_prompt_templates(&self, active_only: bool) -> OxResult<Vec<PromptTemplateRow>>;
    async fn get_prompt_template(&self, id: Uuid) -> OxResult<Option<PromptTemplateRow>>;
    async fn find_active_prompt(&self, name: &str) -> OxResult<Option<PromptTemplateRow>>;
    /// Exact lookup by `(name, version)` — required for the TOML seed
    /// flow's drift-detection pass. Returns the row regardless of
    /// `is_active` so seed can compare content against an operator-
    /// deactivated row.
    async fn find_prompt_template_by_name_version(
        &self,
        name: &str,
        version: &PromptVersion,
    ) -> OxResult<Option<PromptTemplateRow>>;
    /// Resolve a prompt with workspace-specific override fallback.
    /// Returns the workspace's override if one exists, otherwise the
    /// global active prompt with the same name.
    async fn find_active_prompt_for_workspace(
        &self,
        name: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<PromptTemplateRow>>;
    async fn create_prompt_template(&self, row: &PromptTemplateRow) -> OxResult<()>;
    async fn update_prompt_template(
        &self,
        id: Uuid,
        content: &str,
        variables: &[String],
        is_active: bool,
    ) -> OxResult<()>;
    async fn delete_prompt_template(&self, id: Uuid) -> OxResult<bool>;
    /// Deactivate all versions of a prompt with the given name except `exclude_id`.
    async fn update_prompt_template_active_only(
        &self,
        name: &str,
        exclude_id: Uuid,
    ) -> OxResult<()>;
}

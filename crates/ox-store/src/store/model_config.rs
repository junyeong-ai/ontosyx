//! Runtime LLM model configuration + routing rules.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{
    ModelConfig, ModelConfigUpdate, ModelRoutingRule, NewModelConfig, NewRoutingRule,
    RoutingRuleUpdate,
};

#[async_trait]
pub trait ModelConfigStore: Send + Sync {
    async fn list_model_configs(&self, workspace_id: Option<Uuid>) -> OxResult<Vec<ModelConfig>>;
    async fn get_model_config(&self, id: Uuid) -> OxResult<Option<ModelConfig>>;
    async fn create_model_config(&self, config: &NewModelConfig) -> OxResult<ModelConfig>;
    async fn update_model_config(
        &self,
        id: Uuid,
        update: &ModelConfigUpdate,
    ) -> OxResult<ModelConfig>;
    async fn delete_model_config(&self, id: Uuid) -> OxResult<bool>;

    async fn list_routing_rules(
        &self,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Vec<ModelRoutingRule>>;
    async fn get_routing_rule(&self, id: Uuid) -> OxResult<Option<ModelRoutingRule>>;
    async fn create_routing_rule(&self, rule: &NewRoutingRule) -> OxResult<ModelRoutingRule>;
    async fn update_routing_rule(
        &self,
        id: Uuid,
        update: &RoutingRuleUpdate,
    ) -> OxResult<ModelRoutingRule>;
    async fn delete_routing_rule(&self, id: Uuid) -> OxResult<bool>;

    /// Single optimized query: find the best model for an operation + workspace.
    /// Checks workspace-specific rules first, then global rules, then wildcard.
    async fn find_model_for_operation(
        &self,
        operation: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<ModelConfig>>;
}

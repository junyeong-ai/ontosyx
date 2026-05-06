use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::models::SystemConfigRow;

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn list_config(&self) -> OxResult<Vec<SystemConfigRow>>;

    /// Get a single config value by key.
    async fn get_config(&self, key: &str) -> OxResult<Option<String>>;

    /// Set a single config value (upserts).
    async fn update_config(&self, category: &str, key: &str, value: &str) -> OxResult<()>;

    /// Batch update config values in a single transaction.
    /// All updates succeed or none are applied.
    async fn update_config_batch(&self, updates: &[(String, String, String)]) -> OxResult<()>;
}

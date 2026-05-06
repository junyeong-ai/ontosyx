//! Federation (VOL) adapter configurations.
//!
//! One row per registered `source_id` the planner can resolve at
//! query time. Workspace-scoped via RLS — every CRUD below runs
//! through the workspace context set on the pool.
//!
//! `upsert_data_source_by_source_id` is the method the admin HTTP
//! endpoint calls: register-or-replace semantics on the
//! `(workspace_id, source_id)` natural key.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::DataSource;

#[async_trait]
pub trait DataSourceStore: Send + Sync {
    async fn create_data_source(&self, item: &DataSource) -> OxResult<()>;

    async fn get_data_source(&self, id: Uuid) -> OxResult<Option<DataSource>>;

    async fn find_data_source_by_source_id(
        &self,
        source_id: &str,
    ) -> OxResult<Option<DataSource>>;

    async fn list_data_sources(&self) -> OxResult<Vec<DataSource>>;

    async fn upsert_data_source_by_source_id(
        &self,
        source_id: &str,
        kind: &str,
        config: &serde_json::Value,
    ) -> OxResult<DataSource>;

    async fn delete_data_source_by_source_id(&self, source_id: &str) -> OxResult<bool>;

    /// Persist the most-recent introspection result for a source.
    /// Stores the full `RecipeExecutionResult` (schema + profile + warnings)
    /// alongside the per-table fingerprint map so subsequent re-scans
    /// can compute a delta without describing every table again.
    /// `last_analyzed_at` is set to `now()` on the server side.
    ///
    /// Returns the updated row. Errors when the source_id is unknown
    /// in the current workspace (RLS-scoped).
    async fn update_data_source_analysis(
        &self,
        source_id: &str,
        snapshot: &serde_json::Value,
        fingerprints: &serde_json::Value,
    ) -> OxResult<DataSource>;
}

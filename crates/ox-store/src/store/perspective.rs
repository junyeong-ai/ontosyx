use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::WorkbenchPerspective;

#[async_trait]
pub trait PerspectiveStore: Send + Sync {
    async fn upsert_perspective(&self, perspective: &WorkbenchPerspective) -> OxResult<()>;

    async fn get_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        name: &str,
    ) -> OxResult<Option<WorkbenchPerspective>>;

    async fn find_default_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Option<WorkbenchPerspective>>;

    /// 2-tier perspective lookup:
    /// 1. Exact match: lineage_id + default
    /// 2. Topology match: different lineage but same topology_signature
    /// Returns the best matching perspective, or None.
    async fn find_best_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        topology_signature: &str,
    ) -> OxResult<Option<WorkbenchPerspective>>;

    async fn list_perspectives(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Vec<WorkbenchPerspective>>;

    async fn delete_perspective(&self, user_id: &str, id: Uuid) -> OxResult<bool>;
}

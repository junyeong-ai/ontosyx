//! Community-detection policy persistence.
//!
//! Backs `community_detection_policies` from the schema baseline.
//! Drives the offline cron that materialises
//! `ontology_community_summaries` (the existing community store).
//! Orthogonal to `RetrievalProfileStore` — algorithm choice +
//! hierarchical depth + min-cluster threshold here, retrieval
//! shape there.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::{CommunityDetectionPolicy, CommunityDetectionPolicyId};

#[async_trait]
pub trait CommunityDetectionPolicyStore: Send + Sync {
    async fn upsert_community_detection_policy(
        &self,
        policy: &CommunityDetectionPolicy,
    ) -> OxResult<CommunityDetectionPolicy>;

    async fn get_community_detection_policy(
        &self,
        id: &CommunityDetectionPolicyId,
    ) -> OxResult<Option<CommunityDetectionPolicy>>;

    async fn find_community_detection_policy_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<CommunityDetectionPolicy>>;

    async fn list_community_detection_policies(&self) -> OxResult<Vec<CommunityDetectionPolicy>>;

    async fn delete_community_detection_policy(
        &self,
        id: &CommunityDetectionPolicyId,
    ) -> OxResult<bool>;
}

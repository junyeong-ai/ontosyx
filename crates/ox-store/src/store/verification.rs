//! Element-level verification tracking.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::ElementVerification;

#[async_trait]
pub trait VerificationStore: Send + Sync {
    async fn verify_element(&self, v: &ElementVerification) -> OxResult<Uuid>;
    async fn list_verifications(
        &self,
        ontology_lineage_id: &str,
    ) -> OxResult<Vec<ElementVerification>>;
    async fn invalidate_for_elements(
        &self,
        ontology_lineage_id: &str,
        element_ids: &[&str],
        reason: &str,
    ) -> OxResult<u64>;
    async fn delete_verification(
        &self,
        ontology_lineage_id: &str,
        element_id: &str,
        user_id: Uuid,
    ) -> OxResult<bool>;
}

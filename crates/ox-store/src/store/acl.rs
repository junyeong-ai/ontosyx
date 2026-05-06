//! Fine-grained attribute-based access control.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::AclPolicy;

#[async_trait]
pub trait AclStore: Send + Sync {
    /// Create an ACL policy.
    async fn create_acl_policy(&self, policy: &AclPolicy) -> OxResult<()>;

    /// Get a single ACL policy.
    async fn get_acl_policy(&self, id: Uuid) -> OxResult<Option<AclPolicy>>;

    /// List active ACL policies, optionally filtered by subject or resource.
    async fn list_acl_policies(
        &self,
        subject_type: Option<&str>,
        resource_value: Option<&str>,
    ) -> OxResult<Vec<AclPolicy>>;

    /// Update an ACL policy.
    async fn update_acl_policy(
        &self,
        id: Uuid,
        name: &str,
        action: &str,
        properties: Option<&[String]>,
        mask_pattern: Option<&str>,
        priority: i32,
        is_active: bool,
    ) -> OxResult<()>;

    /// Delete an ACL policy.
    async fn delete_acl_policy(&self, id: Uuid) -> OxResult<bool>;

    /// Get all active policies applicable to a given subject (for runtime evaluation).
    /// Returns policies ordered by priority DESC (most specific first).
    async fn list_effective_policies(
        &self,
        platform_role: &str,
        workspace_role: &str,
        user_id: Option<Uuid>,
    ) -> OxResult<Vec<AclPolicy>>;
}

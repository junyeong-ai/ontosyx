//! Authenticated principal driving a single Cypher request.
//!
//! Threaded through [`crate::cypher::rewrite::RewriteContext`] so
//! every rewriter (and future validator) can consult the caller's
//! identity when shaping a transformation. Phase 5/6 use it for
//! diagnostics + future ABAC; the ACL Deny / Mask passes themselves
//! consume the pre-filtered [`crate::cypher::acl_rewriter::AclSnapshot`]
//! and only need the principal for log breadcrumbs.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RequestPrincipal {
    pub id: Uuid,
    /// Workspace role string ("owner" / "admin" / "member" / "viewer").
    /// Carried as a free-form string to keep the runtime layer free
    /// of the ox-store enum.
    pub role: String,
}

impl RequestPrincipal {
    pub fn new(id: Uuid, role: impl Into<String>) -> Self {
        Self {
            id,
            role: role.into(),
        }
    }
}

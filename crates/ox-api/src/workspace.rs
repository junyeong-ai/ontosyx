use std::future::Future;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The slug reserved for the default workspace created during migration.
pub const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// Valid workspace role values (synced with DB CHECK constraint in migration 0004).
/// Used for input validation when processing role values from external sources.
#[allow(dead_code)]
pub const VALID_WORKSPACE_ROLES: &[&str] = &["owner", "admin", "member", "viewer"];

/// Roles that can be assigned via the API (excludes "owner" which is immutable).
pub const ASSIGNABLE_WORKSPACE_ROLES: &[&str] = &["admin", "member", "viewer"];

// ---------------------------------------------------------------------------
// WorkspaceContext — per-request workspace identity
// ---------------------------------------------------------------------------

/// The authenticated workspace context for the current request.
///
/// Injected by the workspace middleware after validating:
/// 1. The workspace exists.
/// 2. The authenticated user is a member (or is a platform admin).
///
/// Handlers extract this to get workspace-scoped authorization info.
#[derive(Clone, Debug)]
pub struct WorkspaceContext {
    pub workspace_id: Uuid,
    pub workspace_role: WorkspaceRole,
}

/// Role within a workspace (distinct from `PlatformRole` in Principal).
///
/// Maps 1:1 with the `workspace_members.role` column and the
/// `valid_workspace_role` CHECK constraint in the database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl WorkspaceRole {
    pub fn from_str(s: &str) -> Self {
        match s {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            "member" => Self::Member,
            _ => Self::Viewer,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
            Self::Viewer => "viewer",
        }
    }

    /// Owner, Admin, or Member — can create/modify resources.
    /// Used by `require_editor()` for handler-level workspace write authorization.
    #[allow(dead_code)]
    pub fn can_edit(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Member)
    }

    /// Owner or Admin — can manage workspace settings and members.
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

impl WorkspaceContext {
    /// Require at least admin-level workspace access.
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.workspace_role.is_admin() {
            Ok(())
        } else {
            Err(AppError::forbidden(
                "This action requires workspace admin privileges",
            ))
        }
    }

    /// Require at least member-level workspace access (can edit).
    /// Complement to `require_admin()` for write-but-not-admin operations.
    #[allow(dead_code)]
    pub fn require_editor(&self) -> Result<(), AppError> {
        if self.workspace_role.can_edit() {
            Ok(())
        } else {
            Err(AppError::forbidden(
                "This action requires workspace edit access",
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Axum extractor — reads WorkspaceContext from request extensions
// ---------------------------------------------------------------------------

impl<S> FromRequestParts<S> for WorkspaceContext
where
    S: Send + Sync,
{
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = parts.extensions.get::<WorkspaceContext>().cloned();

        async move {
            ctx.ok_or_else(|| {
                AppError::internal(
                    "WorkspaceContext not found. Ensure workspace middleware is applied.",
                )
            })
        }
    }
}

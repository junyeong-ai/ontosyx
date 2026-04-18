use std::future::Future;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::AppError;
use crate::middleware::AuthClaims;

// ---------------------------------------------------------------------------
// PlatformRole — platform-wide authorization level
// ---------------------------------------------------------------------------

/// Platform-level role controlling what a user can do globally.
/// Distinct from `WorkspaceRole` which controls per-workspace access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformRole {
    /// Full platform access: user management, all workspaces, system config.
    Admin,
    /// Create/modify ontologies, run queries, manage projects.
    Designer,
    /// Read-only access to shared resources.
    Viewer,
}

impl PlatformRole {
    /// Parse a DB-stored role string, defaulting unknown values to the
    /// least-privileged `Viewer`. Unknown values indicate schema drift
    /// or manual DB edits, so we warn instead of silently downgrading.
    ///
    /// Named `from_db_string` (not `from_str`) to make the infallible,
    /// least-privilege-defaulting contract explicit.
    pub fn from_db_string(s: &str) -> Self {
        match s {
            "admin" => Self::Admin,
            "designer" => Self::Designer,
            "viewer" => Self::Viewer,
            unknown => {
                tracing::warn!(
                    role = unknown,
                    "Unknown platform role string — defaulting to Viewer."
                );
                Self::Viewer
            }
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Designer => "designer",
            Self::Viewer => "viewer",
        }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }

    pub fn can_design(&self) -> bool {
        matches!(self, Self::Admin | Self::Designer)
    }
}

/// Valid role values for the platform (for DB storage and API validation).
pub const VALID_PLATFORM_ROLES: &[&str] = &["admin", "designer", "viewer"];

// ---------------------------------------------------------------------------
// Principal — authenticated caller identity
// ---------------------------------------------------------------------------

/// Represents the authenticated caller's identity.
///
/// # Trust model
///
/// `Principal` is an **authenticated identity** extracted from a validated JWT
/// or API key. The `id` is the user's UUID (from `users.id`) or "system:api-key"
/// for API key access.
///
/// The `AuthClaims` are injected into request extensions by the `require_auth`
/// middleware. Handlers that need the caller's identity extract `Principal`
/// which reads from those extensions.
#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub email: String,
    pub role: PlatformRole,
}

/// Prefixes that identify non-human `sub` values produced by the
/// auth layer:
///
/// - `system:*` — internal system tasks (bootstrap seeding, migrations).
/// - `apikey:*` — DB-backed API keys (see `AuthClaims` construction in
///   `middleware.rs::require_auth`). Phase 4.2 switched API-key auth to
///   this prefix; anything downstream that still checks only `system:`
///   will silently reject live API-key callers.
///
/// Use `Principal::is_machine()` / `AuthClaims::is_machine()` instead of
/// hand-rolling the prefix check at every call site.
pub const MACHINE_SUB_PREFIXES: &[&str] = &["system:", "apikey:"];

/// Returns `true` when the given claim `sub` belongs to a non-human
/// principal (system task or API key).
pub fn is_machine_sub(sub: &str) -> bool {
    MACHINE_SUB_PREFIXES.iter().any(|p| sub.starts_with(p))
}

impl Principal {
    /// Create a Principal from authenticated claims.
    pub fn from_claims(claims: &AuthClaims) -> Self {
        Self {
            id: claims.sub.clone(),
            email: claims.email.clone(),
            role: PlatformRole::from_db_string(&claims.role),
        }
    }

    /// Whether this principal represents a non-human identity — either a
    /// system task (`system:*`) or a DB-backed API key (`apikey:*`).
    /// All middleware and handlers MUST use this predicate instead of
    /// ad-hoc `id.starts_with(...)` checks, otherwise new prefix
    /// additions will silently bypass workspace resolution.
    pub fn is_machine(&self) -> bool {
        is_machine_sub(&self.id)
    }

    /// Parse the principal's ID as a UUID. Fails for system users.
    pub fn user_uuid(&self) -> Result<uuid::Uuid, AppError> {
        uuid::Uuid::parse_str(&self.id)
            .map_err(|_| AppError::unauthorized("Invalid user ID in token"))
    }

    /// Require the caller to be an admin.
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.role.is_admin() {
            Ok(())
        } else {
            Err(AppError::forbidden("This action requires admin privileges"))
        }
    }

    /// Require the caller to be a designer or admin.
    pub fn require_designer(&self) -> Result<(), AppError> {
        if self.role.can_design() {
            Ok(())
        } else {
            Err(AppError::forbidden(
                "This action requires designer or admin privileges",
            ))
        }
    }

    /// Require the caller to be the project owner (or admin).
    /// Admins bypass ownership; designers must own the project.
    pub fn require_project_owner(&self, project_user_id: &str) -> Result<(), AppError> {
        if self.role.is_admin() {
            return Ok(());
        }
        if self.role.can_design() && self.id == project_user_id {
            return Ok(());
        }
        Err(AppError::forbidden("You can only delete your own projects"))
    }

    /// Verify the current principal owns a resource, or is admin.
    pub fn require_owner(
        &self,
        resource_owner_id: &str,
        resource_name: &str,
    ) -> Result<(), AppError> {
        if self.role.is_admin() || self.id == resource_owner_id {
            Ok(())
        } else {
            Err(AppError::forbidden(format!(
                "You do not have permission to modify this {resource_name}"
            )))
        }
    }
}

impl<S> FromRequestParts<S> for Principal
where
    S: Send + Sync,
{
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let claims = parts.extensions.get::<AuthClaims>().cloned();

        async move {
            let claims = claims.ok_or_else(|| {
                AppError::unauthorized("Authentication required. Provide a valid JWT or API key.")
            })?;

            Ok(Self::from_claims(&claims))
        }
    }
}

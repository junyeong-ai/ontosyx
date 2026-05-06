use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::RevokedJwt;

/// Per-token JWT revocation. Pairs with
/// [`super::UserStore::get_user_token_version`] for the two-axis
/// invalidation surface described in `revoked_jwts`'s schema
/// comment.
#[async_trait]
pub trait JwtRevocationStore: Send + Sync {
    /// Look up a single revoked-JWT row by `jti`. `Ok(None)` means
    /// the token is in good standing as far as the explicit revocation
    /// list is concerned (the caller must still verify `tv` against
    /// `users.token_version`).
    async fn find_revoked_jwt(&self, jti: Uuid) -> OxResult<Option<RevokedJwt>>;

    /// Insert a revocation entry. Idempotent on `jti`: re-revoking
    /// the same token is a no-op (first writer's metadata wins).
    async fn revoke_jwt(
        &self,
        jti: Uuid,
        expires_at: DateTime<Utc>,
        revoked_by_user_id: Option<Uuid>,
        reason: Option<String>,
    ) -> OxResult<()>;

    /// Drop rows whose `expires_at < now()`. The underlying tokens
    /// are already unusable (JWT `exp` claim has passed), so the
    /// revocation entry no longer carries security weight; the
    /// cleanup keeps the table bounded. Returns the number of rows
    /// removed for the cron's metric line.
    async fn delete_expired_revocations(&self) -> OxResult<u64>;
}

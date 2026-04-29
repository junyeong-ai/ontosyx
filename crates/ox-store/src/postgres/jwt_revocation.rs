//! [`JwtRevocationStore`] — explicit per-token revocation list.
//!
//! Pairs with `users.token_version` (read via `UserStore`) for the
//! two-axis invalidation surface documented in the schema:
//! `revoked_jwts` carries one row per explicitly revoked token,
//! `users.token_version` retires every issued token in one update.
//! `require_auth` consults both on every JWT request.

use super::*;

#[async_trait]
impl JwtRevocationStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_revoked_jwt(&self, jti: Uuid) -> OxResult<Option<RevokedJwt>> {
        sqlx::query_as::<_, RevokedJwt>(
            "SELECT jti, revoked_at, expires_at, revoked_by_user_id, reason \
             FROM revoked_jwts WHERE jti = $1",
        )
        .bind(jti)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revoke_jwt(
        &self,
        jti: Uuid,
        expires_at: DateTime<Utc>,
        revoked_by_user_id: Option<Uuid>,
        reason: Option<String>,
    ) -> OxResult<()> {
        // Idempotent on `jti` — re-revoking the same token is a no-op
        // and intentionally keeps the first writer's metadata. The
        // call path is "user clicked logout twice" or "two admin
        // tools raced on the same incident"; neither needs the
        // server to surface an error.
        sqlx::query(
            "INSERT INTO revoked_jwts \
                 (jti, revoked_at, expires_at, revoked_by_user_id, reason) \
             VALUES ($1, now(), $2, $3, $4) \
             ON CONFLICT (jti) DO NOTHING",
        )
        .bind(jti)
        .bind(expires_at)
        .bind(revoked_by_user_id)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_expired_revocations(&self) -> OxResult<u64> {
        let result =
            sqlx::query("DELETE FROM revoked_jwts WHERE expires_at < now()")
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}

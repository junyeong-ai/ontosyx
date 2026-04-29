//! [`UserStore`] — user identity — provider + provider_sub as the natural key.

use super::*;

#[async_trait]
impl UserStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_user(&self, user: &User) -> OxResult<User> {
        super::require_workspace_context()?;
        sqlx::query_as::<_, User>(
            "INSERT INTO users (id, email, name, picture, provider, provider_sub, role, token_version, created_at, last_login_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (provider, provider_sub)
             DO UPDATE SET
                email = EXCLUDED.email,
                name = EXCLUDED.name,
                picture = EXCLUDED.picture,
                last_login_at = EXCLUDED.last_login_at
             RETURNING *",
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.name)
        .bind(&user.picture)
        .bind(&user.provider)
        .bind(&user.provider_sub)
        .bind(&user.role)
        .bind(user.token_version)
        .bind(user.created_at)
        .bind(user.last_login_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_by_id(&self, id: Uuid) -> OxResult<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_by_provider(
        &self,
        provider: &str,
        provider_sub: &str,
    ) -> OxResult<Option<User>> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE provider = $1 AND provider_sub = $2")
            .bind(provider)
            .bind(provider_sub)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_users(&self, pagination: &CursorParams) -> OxResult<CursorPage<User>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, User>(
                "SELECT * FROM users
                     WHERE (created_at, id) < ($1, $2)
                     ORDER BY created_at DESC, id DESC
                     LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, User>(
                "SELECT * FROM users
                     ORDER BY created_at DESC, id DESC
                     LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |u| (u.created_at, u.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_user_role(&self, id: Uuid, role: &str) -> OxResult<()> {
        super::require_workspace_context()?;
        let result = sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: "User".to_string(),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn count_users(&self) -> OxResult<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(count)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_user_token_version(&self, id: Uuid) -> OxResult<Option<i64>> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT token_version FROM users WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(row.map(|(v,)| v))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn increment_user_token_version(&self, id: Uuid) -> OxResult<i64> {
        super::require_workspace_context()?;
        let row: Option<(i64,)> = sqlx::query_as(
            "UPDATE users \
             SET token_version = token_version + 1 \
             WHERE id = $1 \
             RETURNING token_version",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        row.map(|(v,)| v).ok_or_else(|| OxError::NotFound {
            entity: "User".to_string(),
        })
    }
}

//! [`ApiKeyStore`] — API-key CRUD + role-gated authentication backing.

use super::*;

#[async_trait]
impl crate::store::ApiKeyStore for PostgresStore {
    async fn create_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        role: &str,
    ) -> OxResult<(crate::models::ApiKey, String)> {
        // 256 bits of CSPRNG entropy. Plaintext is shown to the caller
        // exactly once; only the SHA-256 hash is persisted, so a leaked
        // DB row cannot be used to authenticate.
        let plaintext = crate::secret_token::generate_hex(32);
        let key_hash = crate::secret_token::secret_hash_sha256(plaintext.as_bytes());
        let row = self
            .insert_api_key(label, workspace_id, created_by, &key_hash, role)
            .await?;
        Ok((row, plaintext))
    }

    async fn insert_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        key_hash: &[u8],
        role: &str,
    ) -> OxResult<crate::models::ApiKey> {
        let id = Uuid::new_v4();
        let created_at = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO api_keys \
               (id, label, key_hash, created_by, workspace_id, created_at, role) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(label)
        .bind(key_hash)
        .bind(created_by)
        .bind(workspace_id)
        .bind(created_at)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(crate::models::ApiKey {
            id,
            label: label.to_string(),
            key_hash: key_hash.to_vec(),
            created_by: created_by.to_string(),
            workspace_id,
            role: role.to_string(),
            created_at,
            revoked_at: None,
        })
    }

    async fn find_api_key_by_hash(&self, hash: &[u8]) -> OxResult<Option<crate::models::ApiKey>> {
        sqlx::query_as::<_, crate::models::ApiKey>(
            "SELECT id, label, key_hash, created_by, workspace_id, role, created_at, revoked_at \
             FROM api_keys \
             WHERE key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn list_api_keys(&self) -> OxResult<Vec<crate::models::ApiKey>> {
        sqlx::query_as::<_, crate::models::ApiKey>(
            "SELECT id, label, key_hash, created_by, workspace_id, role, created_at, revoked_at \
             FROM api_keys \
             WHERE revoked_at IS NULL \
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_api_key_revoked(&self, id: Uuid) -> OxResult<bool> {
        let res = sqlx::query(
            "UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(res.rows_affected() > 0)
    }
}

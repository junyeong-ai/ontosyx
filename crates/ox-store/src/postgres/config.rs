//! [`ConfigStore`] — system_config key-value (category, key) persistence.

use super::*;

#[async_trait]
impl ConfigStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_config(&self) -> OxResult<Vec<SystemConfigRow>> {
        sqlx::query_as::<_, SystemConfigRow>(
            "SELECT category, key, value, data_type, description, updated_at
             FROM system_config
             ORDER BY category, key",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_config(&self, key: &str) -> OxResult<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_config(&self, category: &str, key: &str, value: &str) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE system_config SET value = $3, updated_at = NOW()
             WHERE category = $1 AND key = $2",
        )
        .bind(category)
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("Config key {category}.{key}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_config_batch(&self, updates: &[(String, String, String)]) -> OxResult<()> {
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        for (category, key, value) in updates {
            let result = sqlx::query(
                "UPDATE system_config SET value = $3, updated_at = NOW()
                 WHERE category = $1 AND key = $2",
            )
            .bind(category)
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;

            if result.rows_affected() == 0 {
                return Err(OxError::NotFound {
                    entity: format!("Config key {category}.{key}"),
                });
            }
        }

        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }
}

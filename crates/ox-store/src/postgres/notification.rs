//! [`NotificationStore`] — outbound notification channels + per-event dispatch log.

use super::*;

#[async_trait]
impl crate::store::NotificationStore for PostgresStore {
    async fn create_notification_channel(&self, ch: &NotificationChannel) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO notification_channels (id, workspace_id, name, channel_type, config, events, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(ch.id)
        .bind(ch.workspace_id)
        .bind(&ch.name)
        .bind(&ch.channel_type)
        .bind(&ch.config)
        .bind(&ch.events)
        .bind(ch.enabled)
        .bind(ch.created_at)
        .bind(ch.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn get_notification_channel(&self, id: Uuid) -> OxResult<Option<NotificationChannel>> {
        sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notification_channels WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn list_notification_channels(&self) -> OxResult<Vec<NotificationChannel>> {
        sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notification_channels ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_notification_channel(
        &self,
        id: Uuid,
        name: Option<&str>,
        config: Option<&serde_json::Value>,
        events: Option<&[String]>,
        enabled: Option<bool>,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE notification_channels SET
                name = COALESCE($1, name),
                config = COALESCE($2, config),
                events = COALESCE($3, events),
                enabled = COALESCE($4, enabled),
                updated_at = NOW()
             WHERE id = $5",
        )
        .bind(name)
        .bind(config)
        .bind(events)
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn delete_notification_channel(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM notification_channels WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_channels_for_event(
        &self,
        event_type: &str,
    ) -> OxResult<Vec<NotificationChannel>> {
        sqlx::query_as::<_, NotificationChannel>(
            "SELECT * FROM notification_channels WHERE enabled = true AND $1 = ANY(events)",
        )
        .bind(event_type)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn create_notification_log(&self, log: &NotificationLog) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO notification_log (id, workspace_id, channel_id, event_type, subject, body, status, error, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(log.id)
        .bind(log.workspace_id)
        .bind(log.channel_id)
        .bind(&log.event_type)
        .bind(&log.subject)
        .bind(&log.body)
        .bind(&log.status)
        .bind(&log.error)
        .bind(log.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn list_notification_logs(&self, limit: i64) -> OxResult<Vec<NotificationLog>> {
        sqlx::query_as::<_, NotificationLog>(
            "SELECT * FROM notification_log ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

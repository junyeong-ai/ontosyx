//! [`NotificationStore`] — outbound notification channels + per-event dispatch log.

use super::*;
use crate::models::{NotificationEventType, NotificationLogEventType, NotificationLogStatus};

#[derive(sqlx::FromRow)]
struct NotificationChannelRow {
    id: Uuid,
    workspace_id: Uuid,
    name: String,
    channel_type: String,
    config: sqlx::types::Json<WebhookNotificationConfig>,
    events: Vec<String>,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<NotificationChannelRow> for NotificationChannel {
    type Error = OxError;

    fn try_from(row: NotificationChannelRow) -> Result<Self, Self::Error> {
        let events = row
            .events
            .into_iter()
            .map(|e| {
                NotificationEventType::from_wire_str(&e).ok_or_else(|| OxError::Runtime {
                    message: format!("unknown notification event in channel row: {e}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            name: row.name,
            channel_type: row
                .channel_type
                .parse()
                .map_err(|message| OxError::Runtime { message })?,
            config: row.config.0,
            events,
            enabled: row.enabled,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct NotificationLogRow {
    id: Uuid,
    workspace_id: Uuid,
    channel_id: Uuid,
    event_type: String,
    subject: String,
    body: String,
    status: String,
    error: Option<String>,
    created_at: DateTime<Utc>,
}

impl TryFrom<NotificationLogRow> for NotificationLog {
    type Error = OxError;

    fn try_from(row: NotificationLogRow) -> Result<Self, Self::Error> {
        let event_type =
            NotificationLogEventType::from_wire_str(&row.event_type).ok_or_else(|| {
                OxError::Runtime {
                    message: format!(
                        "unknown notification log event_type: {tag}",
                        tag = row.event_type,
                    ),
                }
            })?;
        let status =
            NotificationLogStatus::from_wire_str(&row.status).ok_or_else(|| OxError::Runtime {
                message: format!("unknown notification log status: {tag}", tag = row.status,),
            })?;
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            channel_id: row.channel_id,
            event_type,
            subject: row.subject,
            body: row.body,
            status,
            error: row.error,
            created_at: row.created_at,
        })
    }
}

/// Convert a typed event-vector into the `text[]` shape sqlx
/// binds onto the `notification_channels.events` column. Kept
/// as a free helper so the create + update paths share the
/// same encoding.
fn events_as_wire(events: &[NotificationEventType]) -> Vec<&'static str> {
    events.iter().copied().map(|e| e.as_str()).collect()
}

#[async_trait]
impl crate::store::NotificationStore for PostgresStore {
    async fn create_notification_channel(&self, ch: &NotificationChannel) -> OxResult<()> {
        super::require_workspace_context()?;
        let events_wire = events_as_wire(&ch.events);
        sqlx::query(
            "INSERT INTO notification_channels (id, workspace_id, name, channel_type, config, events, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(ch.id)
        .bind(ch.workspace_id)
        .bind(&ch.name)
        .bind(ch.channel_type.as_str())
        .bind(sqlx::types::Json(&ch.config))
        .bind(&events_wire)
        .bind(ch.enabled)
        .bind(ch.created_at)
        .bind(ch.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn get_notification_channel(&self, id: Uuid) -> OxResult<Option<NotificationChannel>> {
        let row = sqlx::query_as::<_, NotificationChannelRow>(
            "SELECT * FROM notification_channels WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(NotificationChannel::try_from).transpose()
    }

    async fn list_notification_channels(&self) -> OxResult<Vec<NotificationChannel>> {
        let rows = sqlx::query_as::<_, NotificationChannelRow>(
            "SELECT * FROM notification_channels ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(NotificationChannel::try_from)
            .collect()
    }

    async fn update_notification_channel(
        &self,
        id: Uuid,
        name: Option<&str>,
        config: Option<&WebhookNotificationConfig>,
        events: Option<&[NotificationEventType]>,
        enabled: Option<bool>,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        let events_wire = events.map(events_as_wire);
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
        .bind(config.map(sqlx::types::Json))
        .bind(events_wire.as_deref())
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn delete_notification_channel(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM notification_channels WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_channels_for_event(
        &self,
        event_type: NotificationEventType,
    ) -> OxResult<Vec<NotificationChannel>> {
        let rows = sqlx::query_as::<_, NotificationChannelRow>(
            "SELECT * FROM notification_channels WHERE enabled = true AND $1 = ANY(events)",
        )
        .bind(event_type.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter()
            .map(NotificationChannel::try_from)
            .collect()
    }

    async fn create_notification_log(&self, log: &NotificationLog) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO notification_log (id, workspace_id, channel_id, event_type, subject, body, status, error, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(log.id)
        .bind(log.workspace_id)
        .bind(log.channel_id)
        .bind(log.event_type.as_str())
        .bind(&log.subject)
        .bind(&log.body)
        .bind(log.status.as_str())
        .bind(&log.error)
        .bind(log.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn list_notification_logs(&self, limit: i64) -> OxResult<Vec<NotificationLog>> {
        let rows = sqlx::query_as::<_, NotificationLogRow>(
            "SELECT * FROM notification_log ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.into_iter().map(NotificationLog::try_from).collect()
    }
}

//! Notification channels and delivery log.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{
    NotificationChannel, NotificationEventType, NotificationLog, WebhookNotificationConfig,
};

#[async_trait]
pub trait NotificationStore: Send + Sync {
    async fn create_notification_channel(&self, ch: &NotificationChannel) -> OxResult<()>;
    async fn get_notification_channel(&self, id: Uuid) -> OxResult<Option<NotificationChannel>>;
    async fn list_notification_channels(&self) -> OxResult<Vec<NotificationChannel>>;
    async fn update_notification_channel(
        &self,
        id: Uuid,
        name: Option<&str>,
        config: Option<&WebhookNotificationConfig>,
        events: Option<&[NotificationEventType]>,
        enabled: Option<bool>,
    ) -> OxResult<()>;
    async fn delete_notification_channel(&self, id: Uuid) -> OxResult<bool>;

    /// Find channels that subscribe to a given event type and are enabled.
    async fn list_channels_for_event(
        &self,
        event_type: NotificationEventType,
    ) -> OxResult<Vec<NotificationChannel>>;

    async fn create_notification_log(&self, log: &NotificationLog) -> OxResult<()>;
    async fn list_notification_logs(&self, limit: i64) -> OxResult<Vec<NotificationLog>>;
}

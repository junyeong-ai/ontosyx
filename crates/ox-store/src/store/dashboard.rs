use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{Dashboard, DashboardLayoutItem, DashboardWidget, DashboardWidgetThresholds};

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait DashboardStore: Send + Sync {
    async fn create_dashboard(&self, dashboard: &Dashboard) -> OxResult<()>;
    async fn get_dashboard(&self, id: Uuid) -> OxResult<Option<Dashboard>>;
    async fn list_dashboards(
        &self,
        user_id: &str,
        is_admin: bool,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<Dashboard>>;
    async fn update_dashboard(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        layout: &[DashboardLayoutItem],
        is_public: bool,
    ) -> OxResult<()>;
    async fn delete_dashboard(&self, id: Uuid) -> OxResult<bool>;
    /// Set or clear the share token. When `token` is `Some`, the caller
    /// must also pass `expires_at` so the token has a definite TTL.
    async fn update_dashboard_share_token(
        &self,
        id: Uuid,
        token: Option<&str>,
        expires_at: Option<DateTime<Utc>>,
    ) -> OxResult<()>;
    /// Resolve a share token to its dashboard. Returns `Ok(None)` if the
    /// token is unknown OR if the token has expired.
    async fn find_dashboard_by_share_token(&self, token: &str) -> OxResult<Option<Dashboard>>;

    async fn create_widget(&self, widget: &DashboardWidget) -> OxResult<()>;
    async fn list_widgets(&self, dashboard_id: Uuid) -> OxResult<Vec<DashboardWidget>>;
    async fn update_widget(
        &self,
        id: Uuid,
        title: Option<&str>,
        widget_type: Option<&str>,
        query: Option<&str>,
        refresh_interval_secs: Option<i32>,
        thresholds: Option<&DashboardWidgetThresholds>,
    ) -> OxResult<()>;
    async fn update_widget_result(&self, id: Uuid, result: &serde_json::Value) -> OxResult<()>;
    async fn delete_widget(&self, id: Uuid) -> OxResult<bool>;
    /// Batch create multiple widgets in a single transaction.
    async fn create_widgets_batch(&self, widgets: &[DashboardWidget]) -> OxResult<()>;
}

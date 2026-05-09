//! [`DashboardStore`] — workspace dashboards + widget layout (share_token-scoped public reads).

use super::*;

#[derive(sqlx::FromRow)]
struct DashboardRow {
    id: Uuid,
    workspace_id: Uuid,
    user_id: String,
    name: String,
    description: Option<String>,
    layout: sqlx::types::Json<Vec<DashboardLayoutItem>>,
    is_public: bool,
    share_token: Option<String>,
    shared_at: Option<DateTime<Utc>>,
    share_expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DashboardRow> for Dashboard {
    fn from(row: DashboardRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            user_id: row.user_id,
            name: row.name,
            description: row.description,
            layout: row.layout.0,
            is_public: row.is_public,
            share_token: row.share_token,
            shared_at: row.shared_at,
            share_expires_at: row.share_expires_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DashboardWidgetRow {
    id: Uuid,
    dashboard_id: Uuid,
    workspace_id: Uuid,
    title: String,
    widget_type: String,
    query: Option<String>,
    widget_spec: serde_json::Value,
    position: sqlx::types::Json<DashboardWidgetPosition>,
    refresh_interval_secs: Option<i32>,
    last_result: Option<serde_json::Value>,
    last_refreshed: Option<DateTime<Utc>>,
    thresholds: Option<sqlx::types::Json<DashboardWidgetThresholds>>,
    created_at: DateTime<Utc>,
}

impl From<DashboardWidgetRow> for DashboardWidget {
    fn from(row: DashboardWidgetRow) -> Self {
        Self {
            id: row.id,
            dashboard_id: row.dashboard_id,
            workspace_id: row.workspace_id,
            title: row.title,
            widget_type: row.widget_type,
            query: row.query,
            widget_spec: row.widget_spec,
            position: row.position.0,
            refresh_interval_secs: row.refresh_interval_secs,
            last_result: row.last_result,
            last_refreshed: row.last_refreshed,
            thresholds: row.thresholds.map(|thresholds| thresholds.0),
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl DashboardStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_dashboard(&self, d: &Dashboard) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO dashboards (id, workspace_id, user_id, name, description, layout, is_public, share_token, shared_at, share_expires_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(d.id)
        .bind(d.workspace_id)
        .bind(&d.user_id)
        .bind(&d.name)
        .bind(&d.description)
        .bind(sqlx::types::Json(&d.layout))
        .bind(d.is_public)
        .bind(&d.share_token)
        .bind(d.shared_at)
        .bind(d.share_expires_at)
        .bind(d.created_at)
        .bind(d.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_dashboard(&self, id: Uuid) -> OxResult<Option<Dashboard>> {
        let row = sqlx::query_as::<_, DashboardRow>("SELECT * FROM dashboards WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row.map(Dashboard::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_dashboards(
        &self,
        user_id: &str,
        is_admin: bool,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<Dashboard>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = if is_admin {
            // Admin sees all dashboards
            match pagination.cursor_parts() {
                Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, DashboardRow>(
                    "SELECT * FROM dashboards
                         WHERE (updated_at, id) < ($1, $2)
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $3",
                )
                .bind(cursor_ts)
                .bind(cursor_id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
                None => sqlx::query_as::<_, DashboardRow>(
                    "SELECT * FROM dashboards
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $1",
                )
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
            }
        } else {
            // Non-admin: own dashboards + public dashboards
            match pagination.cursor_parts() {
                Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, DashboardRow>(
                    "SELECT * FROM dashboards
                         WHERE (user_id = $1 OR is_public = true) AND (updated_at, id) < ($2, $3)
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $4",
                )
                .bind(user_id)
                .bind(cursor_ts)
                .bind(cursor_id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
                None => sqlx::query_as::<_, DashboardRow>(
                    "SELECT * FROM dashboards
                         WHERE user_id = $1 OR is_public = true
                         ORDER BY updated_at DESC, id DESC
                         LIMIT $2",
                )
                .bind(user_id)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
            }
        };

        let dashboards: Vec<Dashboard> = rows.into_iter().map(Dashboard::from).collect();
        Ok(build_cursor_page(dashboards, limit, |d| {
            (d.updated_at, d.id)
        }))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_dashboard(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        layout: &[DashboardLayoutItem],
        is_public: bool,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE dashboards SET name = $1, description = $2, layout = $3, is_public = $4, updated_at = NOW() WHERE id = $5",
        )
        .bind(name)
        .bind(description)
        .bind(sqlx::types::Json(layout))
        .bind(is_public)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_dashboard(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM dashboards WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_dashboard_share_token(
        &self,
        id: Uuid,
        token: Option<&str>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        if let Some(token) = token {
            sqlx::query(
                "UPDATE dashboards
                 SET share_token = $1, shared_at = NOW(),
                     share_expires_at = $2, updated_at = NOW()
                 WHERE id = $3",
            )
            .bind(token)
            .bind(expires_at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        } else {
            sqlx::query(
                "UPDATE dashboards
                 SET share_token = NULL, shared_at = NULL,
                     share_expires_at = NULL, updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_dashboard_by_share_token(&self, token: &str) -> OxResult<Option<Dashboard>> {
        // Returns the row even if `share_expires_at` is in the past so the
        // caller can render a 410 Gone instead of a generic 404. The route
        // is responsible for the expiry check.
        let row =
            sqlx::query_as::<_, DashboardRow>("SELECT * FROM dashboards WHERE share_token = $1")
                .bind(token)
                .fetch_optional(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(row.map(Dashboard::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_widget(&self, w: &DashboardWidget) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO dashboard_widgets
             (id, dashboard_id, title, widget_type, query, widget_spec, position,
              refresh_interval_secs, thresholds, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(w.id)
        .bind(w.dashboard_id)
        .bind(&w.title)
        .bind(&w.widget_type)
        .bind(&w.query)
        .bind(&w.widget_spec)
        .bind(sqlx::types::Json(&w.position))
        .bind(w.refresh_interval_secs)
        .bind(w.thresholds.as_ref().map(sqlx::types::Json))
        .bind(w.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_widgets(&self, dashboard_id: Uuid) -> OxResult<Vec<DashboardWidget>> {
        let rows: Vec<DashboardWidgetRow> = sqlx::query_as(
            "SELECT * FROM dashboard_widgets WHERE dashboard_id = $1 ORDER BY created_at",
        )
        .bind(dashboard_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(DashboardWidget::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_widget(
        &self,
        id: Uuid,
        title: Option<&str>,
        widget_type: Option<&str>,
        query: Option<&str>,
        refresh_interval_secs: Option<i32>,
        thresholds: Option<&DashboardWidgetThresholds>,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE dashboard_widgets SET
               title = COALESCE($1, title),
               widget_type = COALESCE($2, widget_type),
               query = COALESCE($3, query),
               refresh_interval_secs = COALESCE($4, refresh_interval_secs),
               thresholds = COALESCE($5, thresholds)
             WHERE id = $6",
        )
        .bind(title)
        .bind(widget_type)
        .bind(query)
        .bind(refresh_interval_secs)
        .bind(thresholds.map(sqlx::types::Json))
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_widget_result(&self, id: Uuid, result: &serde_json::Value) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE dashboard_widgets SET last_result = $1, last_refreshed = NOW() WHERE id = $2",
        )
        .bind(result)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_widget(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM dashboard_widgets WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_widgets_batch(&self, widgets: &[DashboardWidget]) -> OxResult<()> {
        super::require_workspace_context()?;
        if widgets.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;
        for w in widgets {
            sqlx::query(
                "INSERT INTO dashboard_widgets
                 (id, dashboard_id, title, widget_type, query, widget_spec, position,
                  refresh_interval_secs, thresholds, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(w.id)
            .bind(w.dashboard_id)
            .bind(&w.title)
            .bind(&w.widget_type)
            .bind(&w.query)
            .bind(&w.widget_spec)
            .bind(sqlx::types::Json(&w.position))
            .bind(w.refresh_interval_secs)
            .bind(w.thresholds.as_ref().map(sqlx::types::Json))
            .bind(w.created_at)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }
        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }
}

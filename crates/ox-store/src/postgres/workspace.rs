//! [`WorkspaceStore`] — workspace CRUD + membership with owner/admin/member/viewer roles.

use super::*;

#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    id: Uuid,
    name: String,
    slug: String,
    owner_id: Uuid,
    settings: serde_json::Value,
    created_at: DateTime<Utc>,
    primary_locale: String,
    admin_locale_fallback: sqlx::types::Json<Vec<String>>,
    llm_locale_fallback: sqlx::types::Json<Vec<String>>,
}

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            slug: row.slug,
            owner_id: row.owner_id,
            settings: row.settings,
            created_at: row.created_at,
            primary_locale: row.primary_locale,
            admin_locale_fallback: row.admin_locale_fallback.0,
            llm_locale_fallback: row.llm_locale_fallback.0,
        }
    }
}

#[async_trait]
impl WorkspaceStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_workspace(&self, w: &Workspace) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO workspaces (id, name, slug, owner_id, settings, primary_locale, admin_locale_fallback, llm_locale_fallback)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(w.id)
        .bind(&w.name)
        .bind(&w.slug)
        .bind(w.owner_id)
        .bind(&w.settings)
        .bind(&w.primary_locale)
        .bind(sqlx::types::Json(&w.admin_locale_fallback))
        .bind(sqlx::types::Json(&w.llm_locale_fallback))
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        // Φ10.5: auto-seed the workspace's `default` retrieval
        // profile + community detection policy. The constants
        // mirror `RetrievalProfile::workspace_default` /
        // `CommunityDetectionPolicy::workspace_default` so the
        // seeded row is interchangeable with the in-memory
        // fallback `query_graph` uses on miss — operators
        // override either via `upsert_*` later. RLS requires the
        // workspace_id task-local to match the row's
        // workspace_id, so the seed runs inside an explicit
        // `WORKSPACE_ID.scope(w.id, ...)` so the new workspace's
        // own id authorises the write.
        crate::WORKSPACE_ID
            .scope(w.id, async {
                let default_profile =
                    ox_ontology::RetrievalProfile::workspace_default(w.id);
                <Self as crate::store::RetrievalProfileStore>::upsert_retrieval_profile(
                    self,
                    &default_profile,
                )
                .await?;

                let default_cdp =
                    ox_ontology::CommunityDetectionPolicy::workspace_default(w.id);
                <Self as crate::store::CommunityDetectionPolicyStore>
                    ::upsert_community_detection_policy(self, &default_cdp)
                    .await?;
                Ok::<(), OxError>(())
            })
            .await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_workspace(&self, id: Uuid) -> OxResult<Option<Workspace>> {
        let row = sqlx::query_as::<_, WorkspaceRow>("SELECT * FROM workspaces WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row.map(Into::into))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_workspace_by_slug(&self, slug: &str) -> OxResult<Option<Workspace>> {
        let row = sqlx::query_as::<_, WorkspaceRow>("SELECT * FROM workspaces WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(row.map(Into::into))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_user_workspaces(&self, user_id: Uuid) -> OxResult<Vec<WorkspaceSummary>> {
        sqlx::query_as(
            "SELECT w.id, w.name, w.slug, w.owner_id, wm.role, w.created_at,
                    (SELECT COUNT(*) FROM workspace_members wm2 WHERE wm2.workspace_id = w.id) AS member_count
             FROM workspaces w
             JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.user_id = $1
             ORDER BY w.created_at",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_workspace(
        &self,
        id: Uuid,
        name: &str,
        settings: &serde_json::Value,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        let result = sqlx::query("UPDATE workspaces SET name = $2, settings = $3 WHERE id = $1")
            .bind(id)
            .bind(name)
            .bind(settings)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("workspace {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_workspace(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_workspace_locale(
        &self,
        id: Uuid,
        primary_locale: &str,
        admin_locale_fallback: &[String],
        llm_locale_fallback: &[String],
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "UPDATE workspaces
                SET primary_locale = $2,
                    admin_locale_fallback = $3,
                    llm_locale_fallback = $4
              WHERE id = $1",
        )
        .bind(id)
        .bind(primary_locale)
        .bind(sqlx::types::Json(admin_locale_fallback))
        .bind(sqlx::types::Json(llm_locale_fallback))
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn add_workspace_member(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<WorkspaceMember> {
        super::require_workspace_context()?;
        // Insert (or upsert role on a re-add) and return the row
        // joined against `users` so the caller has the display
        // identity without a follow-up lookup.
        sqlx::query_as(
            "WITH upsert AS (
                INSERT INTO workspace_members (workspace_id, user_id, role)
                VALUES ($1, $2, $3)
                ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = EXCLUDED.role
                RETURNING workspace_id, user_id, role, joined_at
             )
             SELECT m.workspace_id, m.user_id, m.role, m.joined_at,
                    u.email, u.name, u.picture
             FROM upsert m
             JOIN users u ON u.id = m.user_id",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn remove_workspace_member(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result =
            sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
                .bind(workspace_id)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_member_role(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        let result = sqlx::query(
            "UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("workspace_member {workspace_id}/{user_id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_member_role(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(|r| r.0))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_workspace_members(&self, workspace_id: Uuid) -> OxResult<Vec<WorkspaceMember>> {
        sqlx::query_as(
            "SELECT m.workspace_id, m.user_id, m.role, m.joined_at,
                    u.email, u.name, u.picture
             FROM workspace_members m
             JOIN users u ON u.id = m.user_id
             WHERE m.workspace_id = $1
             ORDER BY m.joined_at",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_workspace_ids(&self) -> OxResult<Vec<Uuid>> {
        // Meant to run under SYSTEM_BYPASS — `workspaces` has no
        // per-row RLS policy but the query is still a bare table
        // scan. Ordering keeps the cron iteration deterministic,
        // which matters for the tail-of-log debugging experience.
        let rows: Vec<(Uuid,)> =
            sqlx::query_as("SELECT id FROM workspaces ORDER BY created_at ASC")
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_default_workspace(&self, user_id: Uuid) -> OxResult<Option<Workspace>> {
        // Prefer the "default" slug workspace, then fall back to the first joined workspace
        let row = sqlx::query_as::<_, WorkspaceRow>(
            "SELECT w.*
             FROM workspaces w
             JOIN workspace_members wm ON wm.workspace_id = w.id AND wm.user_id = $1
             ORDER BY (w.slug = 'default') DESC, wm.joined_at
             LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(Into::into))
    }
}

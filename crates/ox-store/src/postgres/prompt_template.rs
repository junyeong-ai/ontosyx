//! [`PromptTemplateStore`] — prompt templates with semver CHECK + workspace-vs-global precedence.

use super::*;

#[async_trait]
impl PromptTemplateStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_prompt_templates(&self, active_only: bool) -> OxResult<Vec<PromptTemplateRow>> {
        let rows: Vec<PromptTemplateRow> = if active_only {
            sqlx::query_as(
                "SELECT * FROM prompt_templates WHERE is_active = true ORDER BY name, version DESC",
            )
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as("SELECT * FROM prompt_templates ORDER BY name, version DESC")
                .fetch_all(&self.pool)
                .await
        }
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(rows)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_prompt_template(&self, id: Uuid) -> OxResult<Option<PromptTemplateRow>> {
        sqlx::query_as("SELECT * FROM prompt_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_active_prompt(&self, name: &str) -> OxResult<Option<PromptTemplateRow>> {
        // Active global template (workspace_id IS NULL). Sort by parsed
        // semver components (CHECK constraint guarantees `<int>.<int>.<int>`)
        // then `created_at` as the tie-breaker for the rare case of two
        // active rows at the same version.
        sqlx::query_as(
            "SELECT * FROM prompt_templates
             WHERE name = $1 AND is_active = true AND workspace_id IS NULL
             ORDER BY string_to_array(version, '.')::int[] DESC,
                      created_at DESC
             LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_active_prompt_for_workspace(
        &self,
        name: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<PromptTemplateRow>> {
        // Visibility rule:
        //   - workspace_id = Some(ws): see ws-specific override (workspace_id = ws)
        //                              or the global template (workspace_id IS NULL)
        //   - workspace_id = None:     see ONLY the global template
        //
        // This prevents the previous bug where `$2 IS NULL` widened the
        // WHERE clause to match every workspace's overrides indiscriminately.
        //
        // Tie-breaker (when both ws-specific and global match):
        //   1. ws-specific first (`workspace_id IS NULL` = FALSE sorts first)
        //   2. highest semver (CHECK constraint in migration 0006
        //      guarantees `<int>.<int>.<int>` so the array cast is safe)
        //   3. most recently created (deterministic for cosmetic ties)
        sqlx::query_as(
            "SELECT * FROM prompt_templates
             WHERE name = $1
               AND is_active = true
               AND (workspace_id IS NULL
                    OR ($2::uuid IS NOT NULL AND workspace_id = $2))
             ORDER BY (workspace_id IS NULL),
                      string_to_array(version, '.')::int[] DESC,
                      created_at DESC
             LIMIT 1",
        )
        .bind(name)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_prompt_template(&self, r: &PromptTemplateRow) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO prompt_templates (id, name, version, content, variables, metadata, created_by, created_at, is_active, workspace_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (name, version) DO NOTHING",
        )
        .bind(r.id)
        .bind(&r.name)
        // PromptVersion: serialize to its canonical "x.y.z" form for the
        // TEXT column. The CHECK constraint in migration 0006 enforces
        // the same format on the DB side.
        .bind(r.version.to_string())
        .bind(&r.content)
        .bind(&r.variables)
        .bind(&r.metadata)
        .bind(&r.created_by)
        .bind(r.created_at)
        .bind(r.is_active)
        .bind(r.workspace_id)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_prompt_template(
        &self,
        id: Uuid,
        content: &str,
        variables: &serde_json::Value,
        is_active: bool,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE prompt_templates SET content = $2, variables = $3, is_active = $4 WHERE id = $1",
        )
        .bind(id)
        .bind(content)
        .bind(variables)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_prompt_template(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM prompt_templates WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Database error: {e}"),
            })?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_prompt_template_active_only(
        &self,
        name: &str,
        exclude_id: Uuid,
    ) -> OxResult<()> {
        sqlx::query(
            "UPDATE prompt_templates SET is_active = false WHERE name = $1 AND id != $2 AND is_active = true",
        )
        .bind(name)
        .bind(exclude_id)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }
}

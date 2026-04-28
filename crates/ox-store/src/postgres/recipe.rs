//! [`RecipeStore`] — analysis recipes — reusable algorithm templates + version history.

use super::*;

#[async_trait]
impl RecipeStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_recipe(&self, r: &AnalysisRecipe) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO analysis_recipes
             (id, name, description, algorithm_type, code_template, parameters,
              required_columns, output_description, created_by, created_at,
              version, status, parent_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                code_template = EXCLUDED.code_template,
                parameters = EXCLUDED.parameters,
                required_columns = EXCLUDED.required_columns,
                output_description = EXCLUDED.output_description,
                version = EXCLUDED.version,
                status = EXCLUDED.status,
                parent_id = EXCLUDED.parent_id",
        )
        .bind(r.id)
        .bind(&r.name)
        .bind(&r.description)
        .bind(&r.algorithm_type)
        .bind(&r.code_template)
        .bind(&r.parameters)
        .bind(&r.required_columns)
        .bind(&r.output_description)
        .bind(&r.created_by)
        .bind(r.created_at)
        .bind(r.version)
        .bind(&r.status)
        .bind(r.parent_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_recipe(&self, id: Uuid) -> OxResult<Option<AnalysisRecipe>> {
        sqlx::query_as::<_, AnalysisRecipe>("SELECT * FROM analysis_recipes WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_recipes(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AnalysisRecipe>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, AnalysisRecipe>(
                "SELECT * FROM analysis_recipes
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
            None => sqlx::query_as::<_, AnalysisRecipe>(
                "SELECT * FROM analysis_recipes
                     ORDER BY created_at DESC, id DESC
                     LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |r| (r.created_at, r.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_recipe(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM analysis_recipes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_recipe_status(&self, id: Uuid, status: &str) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query("UPDATE analysis_recipes SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_recipe_version(&self, recipe: &AnalysisRecipe) -> OxResult<()> {
        super::require_workspace_context()?;
        self.upsert_recipe(recipe).await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_recipe_versions(&self, parent_id: Uuid) -> OxResult<Vec<AnalysisRecipe>> {
        sqlx::query_as::<_, AnalysisRecipe>(
            "SELECT * FROM analysis_recipes
             WHERE parent_id = $1 OR id = $1
             ORDER BY version DESC",
        )
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_recipes_batch(&self, recipes: &[AnalysisRecipe]) -> OxResult<()> {
        super::require_workspace_context()?;
        if recipes.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;
        for r in recipes {
            sqlx::query(
                "INSERT INTO analysis_recipes
                 (id, name, description, algorithm_type, code_template, parameters,
                  required_columns, output_description, created_by, created_at,
                  version, status, parent_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    description = EXCLUDED.description,
                    code_template = EXCLUDED.code_template,
                    parameters = EXCLUDED.parameters,
                    required_columns = EXCLUDED.required_columns,
                    output_description = EXCLUDED.output_description,
                    version = EXCLUDED.version,
                    status = EXCLUDED.status,
                    parent_id = EXCLUDED.parent_id",
            )
            .bind(r.id)
            .bind(&r.name)
            .bind(&r.description)
            .bind(&r.algorithm_type)
            .bind(&r.code_template)
            .bind(&r.parameters)
            .bind(&r.required_columns)
            .bind(&r.output_description)
            .bind(&r.created_by)
            .bind(r.created_at)
            .bind(r.version)
            .bind(&r.status)
            .bind(r.parent_id)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }
        tx.commit().await.map_err(to_ox_error)?;
        Ok(())
    }
}

//! [`ModelConfigStore`] — LLM routing: (workspace, operation) → model config with priority fallback.

use super::*;

#[async_trait::async_trait]
impl crate::store::ModelConfigStore for PostgresStore {
    async fn list_model_configs(
        &self,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Vec<crate::ModelConfig>> {
        let rows = sqlx::query_as::<_, crate::ModelConfig>(
            "SELECT * FROM model_configs
             WHERE workspace_id IS NOT DISTINCT FROM $1
             ORDER BY priority DESC, name",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows)
    }

    async fn get_model_config(&self, id: Uuid) -> OxResult<Option<crate::ModelConfig>> {
        sqlx::query_as::<_, crate::ModelConfig>("SELECT * FROM model_configs WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    async fn create_model_config(
        &self,
        config: &crate::NewModelConfig,
    ) -> OxResult<crate::ModelConfig> {
        sqlx::query_as::<_, crate::ModelConfig>(
            "INSERT INTO model_configs
                (workspace_id, name, provider, model_id, max_tokens, temperature,
                 timeout_secs, cost_per_1m_input, cost_per_1m_output,
                 daily_budget_usd, priority, api_key_env, region, base_url)
             VALUES ($1, $2, $3, $4,
                     COALESCE($5, 8192), $6, COALESCE($7, 300),
                     $8, $9, $10, COALESCE($11, 0),
                     $12, $13, $14)
             RETURNING *",
        )
        .bind(config.workspace_id)
        .bind(&config.name)
        .bind(&config.provider)
        .bind(&config.model_id)
        .bind(config.max_tokens)
        .bind(config.temperature)
        .bind(config.timeout_secs)
        .bind(config.cost_per_1m_input)
        .bind(config.cost_per_1m_output)
        .bind(config.daily_budget_usd)
        .bind(config.priority)
        .bind(&config.api_key_env)
        .bind(&config.region)
        .bind(&config.base_url)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_model_config(
        &self,
        id: Uuid,
        update: &crate::ModelConfigUpdate,
    ) -> OxResult<crate::ModelConfig> {
        sqlx::query_as::<_, crate::ModelConfig>(
            "UPDATE model_configs SET
                name = COALESCE($2, name),
                provider = COALESCE($3, provider),
                model_id = COALESCE($4, model_id),
                max_tokens = COALESCE($5, max_tokens),
                temperature = COALESCE($6, temperature),
                timeout_secs = COALESCE($7, timeout_secs),
                cost_per_1m_input = COALESCE($8, cost_per_1m_input),
                cost_per_1m_output = COALESCE($9, cost_per_1m_output),
                daily_budget_usd = COALESCE($10, daily_budget_usd),
                priority = COALESCE($11, priority),
                enabled = COALESCE($12, enabled),
                api_key_env = COALESCE($13, api_key_env),
                region = COALESCE($14, region),
                base_url = COALESCE($15, base_url),
                updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(&update.name)
        .bind(&update.provider)
        .bind(&update.model_id)
        .bind(update.max_tokens)
        .bind(update.temperature)
        .bind(update.timeout_secs)
        .bind(update.cost_per_1m_input)
        .bind(update.cost_per_1m_output)
        .bind(update.daily_budget_usd)
        .bind(update.priority)
        .bind(update.enabled)
        .bind(&update.api_key_env)
        .bind(&update.region)
        .bind(&update.base_url)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn delete_model_config(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM model_configs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_routing_rules(
        &self,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Vec<crate::ModelRoutingRule>> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "SELECT * FROM model_routing_rules
             WHERE workspace_id IS NOT DISTINCT FROM $1
             ORDER BY priority DESC, operation",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn get_routing_rule(&self, id: Uuid) -> OxResult<Option<crate::ModelRoutingRule>> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "SELECT * FROM model_routing_rules WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn create_routing_rule(
        &self,
        rule: &crate::NewRoutingRule,
    ) -> OxResult<crate::ModelRoutingRule> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "INSERT INTO model_routing_rules
                (workspace_id, operation, model_config_id, priority)
             VALUES ($1, $2, $3, COALESCE($4, 0))
             RETURNING *",
        )
        .bind(rule.workspace_id)
        .bind(&rule.operation)
        .bind(rule.model_config_id)
        .bind(rule.priority)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn update_routing_rule(
        &self,
        id: Uuid,
        update: &crate::RoutingRuleUpdate,
    ) -> OxResult<crate::ModelRoutingRule> {
        sqlx::query_as::<_, crate::ModelRoutingRule>(
            "UPDATE model_routing_rules SET
                operation = COALESCE($2, operation),
                model_config_id = COALESCE($3, model_config_id),
                priority = COALESCE($4, priority),
                enabled = COALESCE($5, enabled)
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(&update.operation)
        .bind(update.model_config_id)
        .bind(update.priority)
        .bind(update.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    async fn delete_routing_rule(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM model_routing_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_model_for_operation(
        &self,
        operation: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<crate::ModelConfig>> {
        // Priority chain: workspace-specific rules > global rules > wildcard
        sqlx::query_as::<_, crate::ModelConfig>(
            "SELECT mc.* FROM model_routing_rules r
             JOIN model_configs mc ON r.model_config_id = mc.id
             WHERE r.operation IN ($1, '*')
               AND r.enabled = true AND mc.enabled = true
               AND (r.workspace_id = $2 OR r.workspace_id IS NULL)
             ORDER BY
                 CASE WHEN r.workspace_id IS NOT NULL THEN 0 ELSE 1 END,
                 CASE WHEN r.operation = $1 THEN 0 ELSE 1 END,
                 r.priority DESC
             LIMIT 1",
        )
        .bind(operation)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

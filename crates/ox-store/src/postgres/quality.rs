//! [`QualityStore`] — quality rules + per-evaluation results.

use super::*;

#[async_trait]
impl QualityStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_quality_rule(&self, rule: &QualityRule) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO quality_rules
             (id, ontology_lineage_id, name, description, rule_type, target_label,
              target_property, threshold, cypher_check, severity, is_active,
              created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(rule.id)
        .bind(&rule.ontology_lineage_id)
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(&rule.rule_type)
        .bind(&rule.target_label)
        .bind(&rule.target_property)
        .bind(rule.threshold)
        .bind(&rule.cypher_check)
        .bind(&rule.severity)
        .bind(rule.is_active)
        .bind(rule.created_by)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_quality_rule(&self, id: Uuid) -> OxResult<Option<QualityRule>> {
        sqlx::query_as("SELECT * FROM quality_rules WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_quality_rules(
        &self,
        ontology_lineage_id: Option<&str>,
        target_label: Option<&str>,
    ) -> OxResult<Vec<QualityRule>> {
        // Build the WHERE clause dynamically — RLS on workspace_id is
        // always applied by the pool-level session var, so only the
        // optional lineage + label filters need per-call parameters.
        let mut conditions = Vec::new();
        if ontology_lineage_id.is_some() {
            conditions.push(format!("ontology_lineage_id = ${}", conditions.len() + 1));
        }
        if target_label.is_some() {
            conditions.push(format!("target_label = ${}", conditions.len() + 1));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql =
            format!("SELECT * FROM quality_rules {where_clause} ORDER BY severity DESC, name");

        let mut query = sqlx::query_as::<_, QualityRule>(&sql);
        if let Some(lineage) = ontology_lineage_id {
            query = query.bind(lineage);
        }
        if let Some(label) = target_label {
            query = query.bind(label);
        }
        query.fetch_all(&self.pool).await.map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_quality_rule(
        &self,
        id: Uuid,
        name: &str,
        threshold: f64,
        is_active: bool,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE quality_rules
             SET name = $1, threshold = $2, is_active = $3, updated_at = NOW()
             WHERE id = $4",
        )
        .bind(name)
        .bind(threshold)
        .bind(is_active)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("quality rule {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_quality_rule(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM quality_rules WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_quality_result(&self, result: &QualityResult) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO quality_results (id, rule_id, passed, actual_value, details, evaluated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(result.id)
        .bind(result.rule_id)
        .bind(result.passed)
        .bind(result.actual_value)
        .bind(&result.details)
        .bind(result.evaluated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_latest_results(&self, rule_id: Uuid, limit: i64) -> OxResult<Vec<QualityResult>> {
        sqlx::query_as(
            "SELECT * FROM quality_results
             WHERE rule_id = $1
             ORDER BY evaluated_at DESC
             LIMIT $2",
        )
        .bind(rule_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_quality_dashboard(&self) -> OxResult<Vec<QualityDashboardEntry>> {
        sqlx::query_as(
            "SELECT qr.id AS rule_id, qr.name, qr.rule_type, qr.target_label,
                    qr.severity, qr.threshold::float8 AS threshold,
                    res.passed AS latest_passed,
                    res.actual_value::float8 AS latest_value,
                    res.evaluated_at AS latest_evaluated_at
             FROM quality_rules qr
             LEFT JOIN LATERAL (
                 SELECT passed, actual_value, evaluated_at
                 FROM quality_results
                 WHERE rule_id = qr.id
                 ORDER BY evaluated_at DESC LIMIT 1
             ) res ON true
             WHERE qr.is_active = true
             ORDER BY qr.severity DESC, qr.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}

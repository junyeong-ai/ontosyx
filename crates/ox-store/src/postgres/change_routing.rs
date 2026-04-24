//! [`ChangeRoutingStore`] — resolves the routing rule for a given
//! [`ChangeType`] by merging the workspace-local override (if any)
//! with the global default seeded in `migrations/0001_schema.sql`.
//!
//! Resolution order is `(priority DESC, workspace_id NULLS LAST)`,
//! so a workspace row always wins at its higher priority; when no
//! override exists, the NULL-workspace global row answers. The
//! `(workspace_id, change_type)` unique constraint makes that
//! ordering deterministic.

use super::*;

fn change_type_to_str(ct: ox_ontology::change_routing::ChangeType) -> &'static str {
    use ox_ontology::change_routing::ChangeType;
    match ct {
        ChangeType::CodedValueCreate => "coded_value_create",
        ChangeType::CodedValueDeprecate => "coded_value_deprecate",
        ChangeType::GlossaryTermCreate => "glossary_term_create",
        ChangeType::GlossaryAliasAdd => "glossary_alias_add",
        ChangeType::NotationPatternCreate => "notation_pattern_create",
        ChangeType::CustomerSegmentCreate => "customer_segment_create",
        ChangeType::ColumnRename => "column_rename",
        ChangeType::TableMerge => "table_merge",
        ChangeType::DataSourceRegister => "data_source_register",
        ChangeType::StaleConceptDeprecate => "stale_concept_deprecate",
        ChangeType::OntologyVersionRollback => "ontology_version_rollback",
        ChangeType::RuleCreate => "rule_create",
        ChangeType::RuleModify => "rule_modify",
        ChangeType::RuleDelete => "rule_delete",
    }
}

fn change_type_from_str(
    s: &str,
) -> OxResult<ox_ontology::change_routing::ChangeType> {
    use ox_ontology::change_routing::ChangeType;
    Ok(match s {
        "coded_value_create" => ChangeType::CodedValueCreate,
        "coded_value_deprecate" => ChangeType::CodedValueDeprecate,
        "glossary_term_create" => ChangeType::GlossaryTermCreate,
        "glossary_alias_add" => ChangeType::GlossaryAliasAdd,
        "notation_pattern_create" => ChangeType::NotationPatternCreate,
        "customer_segment_create" => ChangeType::CustomerSegmentCreate,
        "column_rename" => ChangeType::ColumnRename,
        "table_merge" => ChangeType::TableMerge,
        "data_source_register" => ChangeType::DataSourceRegister,
        "stale_concept_deprecate" => ChangeType::StaleConceptDeprecate,
        "ontology_version_rollback" => ChangeType::OntologyVersionRollback,
        "rule_create" => ChangeType::RuleCreate,
        "rule_modify" => ChangeType::RuleModify,
        "rule_delete" => ChangeType::RuleDelete,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown change_type in DB row: {other}"),
            });
        }
    })
}

fn risk_level_to_str(r: ox_ontology::change_routing::RiskLevel) -> &'static str {
    use ox_ontology::change_routing::RiskLevel;
    match r {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    }
}

fn risk_level_from_str(
    s: &str,
) -> OxResult<ox_ontology::change_routing::RiskLevel> {
    use ox_ontology::change_routing::RiskLevel;
    Ok(match s {
        "low" => RiskLevel::Low,
        "medium" => RiskLevel::Medium,
        "high" => RiskLevel::High,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown risk_level in DB row: {other}"),
            });
        }
    })
}

fn routing_rule_from_row(
    row: &sqlx::postgres::PgRow,
) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule> {
    use sqlx::Row;
    let id_uuid: Uuid = row.try_get("id").map_err(to_ox_error)?;
    let workspace_id: Option<Uuid> = row.try_get("workspace_id").map_err(to_ox_error)?;
    let change_type_text: &str = row.try_get("change_type").map_err(to_ox_error)?;
    let routing_json: serde_json::Value = row.try_get("routing").map_err(to_ox_error)?;
    let risk_level_text: &str = row.try_get("risk_level").map_err(to_ox_error)?;
    let priority: i32 = row.try_get("priority").map_err(to_ox_error)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(to_ox_error)?;

    let routing = serde_json::from_value::<
        ox_ontology::change_routing::ApprovalRouting,
    >(routing_json)
    .map_err(|e| OxError::Runtime {
        message: format!("routing JSONB decode: {e}"),
    })?;

    Ok(ox_ontology::change_routing::ChangeRoutingRule {
        id: ox_ontology::change_routing::ChangeRoutingRuleId::new(id_uuid.to_string()),
        workspace_id,
        change_type: change_type_from_str(change_type_text)?,
        routing,
        risk_level: risk_level_from_str(risk_level_text)?,
        priority,
        created_at,
    })
}

#[async_trait]
impl ChangeRoutingStore for PostgresStore {
    async fn list_change_routing_rules(
        &self,
    ) -> OxResult<Vec<ox_ontology::change_routing::ChangeRoutingRule>> {
        let rows = sqlx::query(
            "SELECT id, workspace_id, change_type, routing, risk_level, priority, created_at \
             FROM change_routing_rules \
             ORDER BY change_type, priority DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(routing_rule_from_row).collect()
    }

    async fn resolve_change_routing(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<Option<ox_ontology::change_routing::ChangeRoutingRule>> {
        // Workspace row wins when present via the RLS `ws_or_global_read`
        // policy unioning global + override rows; priority DESC then
        // workspace-row first on tie keeps the resolution deterministic.
        let row = sqlx::query(
            "SELECT id, workspace_id, change_type, routing, risk_level, priority, created_at \
             FROM change_routing_rules \
             WHERE change_type = $1 \
             ORDER BY priority DESC, (workspace_id IS NOT NULL) DESC \
             LIMIT 1",
        )
        .bind(change_type_to_str(change_type))
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(routing_rule_from_row).transpose()
    }

    async fn upsert_change_routing_rule(
        &self,
        rule: ox_ontology::change_routing::ChangeRoutingRule,
    ) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule> {
        let id_uuid: Uuid = rule.id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("routing rule id must be uuid: {e}"),
            }
        })?;
        let routing_json = serde_json::to_value(&rule.routing).map_err(|e| {
            OxError::Runtime {
                message: format!("routing JSONB encode: {e}"),
            }
        })?;

        sqlx::query(
            "INSERT INTO change_routing_rules \
             (id, workspace_id, change_type, routing, risk_level, priority, created_at) \
             VALUES ($1, current_setting('app.workspace_id', true)::uuid, $2, $3, $4, $5, $6) \
             ON CONFLICT (workspace_id, change_type) DO UPDATE SET \
                 routing = EXCLUDED.routing, \
                 risk_level = EXCLUDED.risk_level, \
                 priority = EXCLUDED.priority",
        )
        .bind(id_uuid)
        .bind(change_type_to_str(rule.change_type))
        .bind(&routing_json)
        .bind(risk_level_to_str(rule.risk_level))
        .bind(rule.priority)
        .bind(rule.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rule)
    }

    async fn delete_change_routing_rule(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<bool> {
        // Delete only the workspace override — the global default row
        // lives under `workspace_id IS NULL` and is never touched
        // through this path (migrations or SYSTEM_BYPASS own it).
        let result = sqlx::query(
            "DELETE FROM change_routing_rules \
             WHERE workspace_id = current_setting('app.workspace_id', true)::uuid \
               AND change_type = $1",
        )
        .bind(change_type_to_str(change_type))
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

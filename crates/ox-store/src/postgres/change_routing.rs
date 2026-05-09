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

fn change_type_to_str(ct: ox_ontology::change_routing::ChangeType) -> OxResult<String> {
    serde_json::to_value(ct)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| OxError::Runtime {
            message: "ChangeType must serialize as a snake_case string".to_string(),
        })
}

fn change_type_from_str(s: &str) -> OxResult<ox_ontology::change_routing::ChangeType> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| OxError::Runtime {
        message: format!("unknown change_type in DB row: {s}"),
    })
}

fn risk_level_to_str(r: ox_ontology::change_routing::RiskLevel) -> OxResult<String> {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| OxError::Runtime {
            message: "RiskLevel must serialize as a snake_case string".to_string(),
        })
}

fn risk_level_from_str(s: &str) -> OxResult<ox_ontology::change_routing::RiskLevel> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| OxError::Runtime {
        message: format!("unknown risk_level in DB row: {s}"),
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

    let routing =
        serde_json::from_value::<ox_ontology::change_routing::ApprovalRouting>(routing_json)
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
        .bind(change_type_to_str(change_type)?)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(routing_rule_from_row).transpose()
    }

    async fn upsert_change_routing_rule(
        &self,
        rule: ox_ontology::change_routing::ChangeRoutingRule,
    ) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule> {
        // Workspace overrides bind to the caller's workspace context;
        // global defaults (`workspace_id IS NULL`) are seeded by the
        // migration, never written via this path.
        let workspace_id = super::bound_workspace_id_for_dml()?;
        if let Some(supplied) = rule.workspace_id
            && supplied != workspace_id
        {
            return Err(OxError::Validation {
                field: "workspace_id".to_string(),
                message: format!(
                    "rule.workspace_id ({supplied}) must match the active \
                     WORKSPACE_ID context ({workspace_id})"
                ),
            });
        }
        let id_uuid: Uuid =
            rule.id
                .as_str()
                .parse()
                .map_err(|e: uuid::Error| OxError::Runtime {
                    message: format!("routing rule id must be uuid: {e}"),
                })?;
        let routing_json = serde_json::to_value(&rule.routing).map_err(|e| OxError::Runtime {
            message: format!("routing JSONB encode: {e}"),
        })?;

        sqlx::query(
            "INSERT INTO change_routing_rules \
             (id, workspace_id, change_type, routing, risk_level, priority, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (workspace_id, change_type) DO UPDATE SET \
                 routing = EXCLUDED.routing, \
                 risk_level = EXCLUDED.risk_level, \
                 priority = EXCLUDED.priority",
        )
        .bind(id_uuid)
        .bind(workspace_id)
        .bind(change_type_to_str(rule.change_type)?)
        .bind(&routing_json)
        .bind(risk_level_to_str(rule.risk_level)?)
        .bind(rule.priority)
        .bind(rule.created_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(ox_ontology::change_routing::ChangeRoutingRule {
            workspace_id: Some(workspace_id),
            ..rule
        })
    }

    async fn delete_change_routing_rule(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<bool> {
        // Delete only the workspace override — the global default row
        // lives under `workspace_id IS NULL` and is never touched
        // through this path (migrations or SYSTEM_BYPASS own it).
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let result = sqlx::query(
            "DELETE FROM change_routing_rules \
             WHERE workspace_id = $1 AND change_type = $2",
        )
        .bind(workspace_id)
        .bind(change_type_to_str(change_type)?)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_ontology::change_routing::{ApprovalRouting, ChangeType, RiskLevel};
    use std::collections::HashMap;

    #[test]
    fn change_type_wire_mapping_uses_serde_contract_for_every_variant() {
        for change_type in ChangeType::all() {
            let wire = change_type_to_str(*change_type).expect("serialize change type");
            let expected = serde_json::to_value(change_type)
                .expect("serde change type")
                .as_str()
                .expect("change type wire string")
                .to_string();
            assert_eq!(wire, expected);
            assert_eq!(
                change_type_from_str(&wire).expect("deserialize change type"),
                *change_type
            );
        }
    }

    #[test]
    fn risk_level_wire_mapping_uses_serde_contract_for_every_variant() {
        for risk_level in [RiskLevel::Low, RiskLevel::Medium, RiskLevel::High] {
            let wire = risk_level_to_str(risk_level).expect("serialize risk level");
            let expected = serde_json::to_value(risk_level)
                .expect("serde risk level")
                .as_str()
                .expect("risk level wire string")
                .to_string();
            assert_eq!(wire, expected);
            assert_eq!(
                risk_level_from_str(&wire).expect("deserialize risk level"),
                risk_level
            );
        }
    }

    #[test]
    fn global_seed_rows_match_rust_defaults() {
        let seed = migration_seed_rows();
        assert_eq!(
            seed.len(),
            ChangeType::all().len(),
            "0001_schema.sql must seed exactly one global row per ChangeType"
        );

        for change_type in ChangeType::all() {
            let row = seed
                .get(change_type)
                .unwrap_or_else(|| panic!("missing seed row for {change_type:?}"));
            assert_eq!(
                row.routing,
                change_type.default_routing(),
                "routing seed drift for {change_type:?}"
            );
            assert_eq!(
                row.risk_level,
                change_type.default_risk_level(),
                "risk seed drift for {change_type:?}"
            );
            assert_eq!(
                row.priority, 0,
                "global seed priority must stay below workspace override priority"
            );
        }
    }

    #[derive(Debug)]
    struct SeedRow {
        routing: ApprovalRouting,
        risk_level: RiskLevel,
        priority: i32,
    }

    fn migration_seed_rows() -> HashMap<ChangeType, SeedRow> {
        let sql = include_str!("../../migrations/0001_schema.sql");
        let marker = "INSERT INTO change_routing_rules \
            (id, workspace_id, change_type, routing, risk_level, priority) VALUES";
        let seed_block = sql
            .split_once(marker)
            .expect("change_routing_rules seed insert must exist")
            .1
            .split_once(";\n\n-- ============================================================================\n-- Quality")
            .expect("change_routing_rules seed insert must terminate before quality section")
            .0;

        let mut rows = HashMap::new();
        let mut lines = seed_block.lines().map(str::trim).peekable();
        while let Some(line) = lines.next() {
            if line != "(" {
                continue;
            }

            let header = lines.next().expect("seed row must include change_type");
            let change_type = parse_change_type_from_seed_header(header);

            let routing_line = lines.next().expect("seed row must include routing");
            let routing_json = quoted_segment(routing_line)
                .unwrap_or_else(|| panic!("seed routing line must be quoted: {routing_line}"));
            let routing: ApprovalRouting = serde_json::from_str(routing_json)
                .unwrap_or_else(|e| panic!("seed routing JSON must decode: {e}: {routing_json}"));

            let risk_line = lines.next().expect("seed row must include risk/priority");
            let risk_wire = quoted_segment(risk_line)
                .unwrap_or_else(|| panic!("seed risk line must be quoted: {risk_line}"));
            let risk_level = risk_level_from_str(risk_wire).expect("seed risk must decode");
            let priority = risk_line
                .rsplit_once(',')
                .and_then(|(_, tail)| tail.trim().parse::<i32>().ok())
                .unwrap_or_else(|| panic!("seed priority must be an integer: {risk_line}"));

            assert!(
                rows.insert(
                    change_type,
                    SeedRow {
                        routing,
                        risk_level,
                        priority,
                    },
                )
                .is_none(),
                "duplicate seed row for {change_type:?}"
            );
        }

        rows
    }

    fn parse_change_type_from_seed_header(line: &str) -> ChangeType {
        let change_type_wire = quoted_segment(line)
            .unwrap_or_else(|| panic!("seed header must contain quoted change_type: {line}"));
        change_type_from_str(change_type_wire).expect("seed change_type must decode")
    }

    fn quoted_segment(line: &str) -> Option<&str> {
        let start = line.find('\'')? + 1;
        let end = line[start..].find('\'')? + start;
        Some(&line[start..end])
    }
}

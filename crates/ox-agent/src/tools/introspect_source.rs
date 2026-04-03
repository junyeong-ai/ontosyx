use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::DomainContext;

// ---------------------------------------------------------------------------
// IntrospectSourceTool — progressive schema exploration
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IntrospectSourceInput {
    /// Action: "list_tables" for overview, "table_detail" for specific table info.
    pub action: IntrospectAction,
    /// Table name (required for "table_detail" action).
    #[serde(default)]
    pub table_name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntrospectAction {
    /// List all source tables with column counts and row counts.
    ListTables,
    /// Get detailed schema for a specific table (columns, types, constraints, sample values).
    TableDetail,
}

pub struct IntrospectSourceTool {
    pub domain: Arc<DomainContext>,
}

#[async_trait]
impl SchemaTool for IntrospectSourceTool {
    type Input = IntrospectSourceInput;
    const NAME: &'static str = super::INTROSPECT_SOURCE;
    const DESCRIPTION: &'static str = "Inspect the source database schema. Use 'list_tables' to see all tables with their \
         column counts and row counts. Use 'table_detail' with a table_name to see full column \
         definitions, data types, constraints, and statistics for a specific table. Useful when \
         exploring large schemas progressively or when you need to understand a table's structure \
         before querying or designing.";
    const READ_ONLY: bool = true;

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let schema = match &self.domain.source_schema {
            Some(s) => s,
            None => return ToolResult::error("No source schema available for this project"),
        };

        match input.action {
            IntrospectAction::ListTables => {
                if schema.tables.is_empty() {
                    return ToolResult::error("No tables found in source schema");
                }
                let lines: Vec<String> = schema
                    .tables
                    .iter()
                    .map(|t| format!("{} ({} columns)", t.name, t.columns.len()))
                    .collect();
                let output = serde_json::json!({
                    "table_count": schema.tables.len(),
                    "tables": lines,
                });
                ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
            }
            IntrospectAction::TableDetail => {
                let table_name = match &input.table_name {
                    Some(name) => name,
                    None => {
                        return ToolResult::error("table_name is required for table_detail action");
                    }
                };

                let table = schema.tables.iter().find(|t| t.name == *table_name);

                match table {
                    Some(table) => {
                        let mut result = serde_json::json!({
                            "table_name": table_name,
                            "columns": serde_json::to_value(&table.columns).unwrap_or_default(),
                        });

                        // Include profile data (column statistics) if available
                        if let Some(profile) = &self.domain.source_profile
                            && let Some(tp) = profile
                                .table_profiles
                                .iter()
                                .find(|p| p.table_name == *table_name)
                        {
                            result["profile"] = serde_json::to_value(tp).unwrap_or_default();
                        }

                        ToolResult::success(
                            serde_json::to_string_pretty(&result).unwrap_or_default(),
                        )
                    }
                    None => ToolResult::error(format!(
                        "Table '{}' not found in source schema",
                        table_name
                    )),
                }
            }
        }
    }
}

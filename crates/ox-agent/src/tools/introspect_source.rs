use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::DomainContext;

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

/// Two-shape output discriminated by `action`. Tagged representation
/// keeps the LLM's match arm trivial: `kind: "list" | "detail"` carries
/// the relevant payload so the model never sees an inapplicable field.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntrospectSourceOutput {
    List {
        table_count: usize,
        tables: Vec<String>,
    },
    Detail {
        table_name: String,
        columns: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<serde_json::Value>,
    },
}

pub struct IntrospectSourceTool {
    pub domain: Arc<DomainContext>,
}

#[async_trait]
impl SchemaTool for IntrospectSourceTool {
    type Input = IntrospectSourceInput;
    type Output = IntrospectSourceOutput;
    const NAME: &'static str = super::INTROSPECT_SOURCE;

    fn description(&self) -> &str {
        "Inspect the source database schema. \
         'list_tables' returns tables with column/row counts; \
         'table_detail' with a table_name returns column definitions, types, constraints, stats."
    }

    fn effect(&self) -> ToolEffect {
        ToolEffect::ReadOnly
    }

    async fn execute(
        &self,
        input: Self::Input,
        _ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let schema = self.domain.source_schema.as_ref().ok_or_else(|| {
            entelix::Error::invalid_request(
                "No source schema available — connect a data source first",
            )
        })?;

        match input.action {
            IntrospectAction::ListTables => {
                if schema.tables.is_empty() {
                    return Err(entelix::Error::invalid_request(
                        "No tables found in source schema",
                    ));
                }
                let tables: Vec<String> = schema
                    .tables
                    .iter()
                    .map(|t| format!("{} ({} columns)", t.name, t.columns.len()))
                    .collect();
                Ok(IntrospectSourceOutput::List {
                    table_count: schema.tables.len(),
                    tables,
                })
            }
            IntrospectAction::TableDetail => {
                let table_name = input.table_name.as_deref().ok_or_else(|| {
                    entelix::Error::invalid_request(
                        "table_name is required for table_detail action",
                    )
                })?;

                let table = schema
                    .tables
                    .iter()
                    .find(|t| t.name == table_name)
                    .ok_or_else(|| {
                        entelix::Error::invalid_request(format!(
                            "Table '{}' not found in source schema",
                            table_name
                        ))
                    })?;

                // Include profile data (column statistics) when the
                // source surface produced one — the LLM uses
                // distributions to decide whether a column is worth a
                // GROUP BY or an unnest.
                let profile = self.domain.source_profile.as_ref().and_then(|p| {
                    p.table_profiles
                        .iter()
                        .find(|tp| tp.table_name == table_name)
                        .and_then(|tp| serde_json::to_value(tp).ok())
                });

                Ok(IntrospectSourceOutput::Detail {
                    table_name: table_name.to_owned(),
                    columns: serde_json::to_value(&table.columns).unwrap_or_default(),
                    profile,
                })
            }
        }
    }
}
